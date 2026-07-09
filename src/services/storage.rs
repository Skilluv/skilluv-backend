use s3::creds::Credentials;
use s3::{Bucket, Region};
use uuid::Uuid;

use crate::config::AppConfig;
use crate::errors::AppError;

pub struct StorageService {
    bucket: Box<Bucket>,
    endpoint: String,
    bucket_name: String,
    cdn_base_url: Option<String>,
}

/// One-shot MinIO/S3 setup required for avatars + enterprise logos to render in
/// the browser: the bucket must allow anonymous downloads.
///
/// For the bundled `docker-compose` dev stack, run once after first bucket
/// creation (the setting is persisted in the MinIO data volume so it survives
/// container restarts):
///
/// ```sh
/// docker exec skilluv-minio mc alias set local http://localhost:9000 "$MINIO_ACCESS_KEY" "$MINIO_SECRET_KEY"
/// docker exec skilluv-minio mc anonymous set download local/"$MINIO_BUCKET"
/// ```
///
/// In production this is provisioned as part of the object-storage IaC — the
/// bucket must expose GetObject anonymously (or be fronted by a CDN with an
/// origin identity that can). The `rust-s3` crate we use (0.35) has no
/// `put_bucket_policy` helper, so we don't try to enforce it from code.
impl StorageService {
    pub async fn new(config: &AppConfig) -> Self {
        let region = Region::Custom {
            region: "us-east-1".to_string(),
            endpoint: config.minio_endpoint.clone(),
        };

        let credentials = Credentials::new(
            Some(&config.minio_access_key),
            Some(&config.minio_secret_key),
            None,
            None,
            None,
        )
        .expect("Failed to create S3 credentials");

        let bucket = Bucket::new(&config.minio_bucket, region, credentials)
            .expect("Failed to create S3 bucket handle")
            .with_path_style();

        // Try to create bucket if it doesn't exist
        let _ = s3::Bucket::create_with_path_style(
            &config.minio_bucket,
            Region::Custom {
                region: "us-east-1".to_string(),
                endpoint: config.minio_endpoint.clone(),
            },
            Credentials::new(
                Some(&config.minio_access_key),
                Some(&config.minio_secret_key),
                None,
                None,
                None,
            )
            .unwrap(),
            s3::BucketConfiguration::default(),
        )
        .await;

        tracing::info!(
            bucket = config.minio_bucket,
            endpoint = config.minio_endpoint,
            "Storage service initialized"
        );

        Self {
            bucket,
            endpoint: config.minio_endpoint.clone(),
            bucket_name: config.minio_bucket.clone(),
            cdn_base_url: config.avatar_cdn_base_url.clone(),
        }
    }

    /// Upload avatar image. Returns the storage key.
    pub async fn upload_avatar(
        &self,
        user_id: Uuid,
        data: &[u8],
        content_type: &str,
    ) -> Result<String, AppError> {
        let ext = match content_type {
            "image/jpeg" => "jpg",
            "image/png" => "png",
            "image/webp" => "webp",
            _ => {
                return Err(AppError::Validation(
                    "Unsupported image format. Use JPEG, PNG, or WebP.".to_string(),
                ));
            }
        };

        let key = format!("{user_id}.{ext}");

        self.bucket
            .put_object_with_content_type(&key, data, content_type)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to upload avatar: {e}")))?;

        Ok(key)
    }

    /// Delete avatar by key prefix (user_id).
    pub async fn delete_avatar(&self, user_id: Uuid) -> Result<(), AppError> {
        // Try all possible extensions
        for ext in &["jpg", "png", "webp"] {
            let key = format!("{user_id}.{ext}");
            let _ = self.bucket.delete_object(&key).await;
        }
        Ok(())
    }

    /// Get public URL for an avatar key. Uses CDN when configured, otherwise direct MinIO URL.
    pub fn avatar_url(&self, key: &str) -> String {
        match &self.cdn_base_url {
            Some(cdn) => format!("{}/{}", cdn.trim_end_matches('/'), key),
            None => format!("{}/{}/{}", self.endpoint, self.bucket_name, key),
        }
    }

    /// Upload enterprise logo. Namespaced under `enterprise-logos/` so avatars
    /// and logos never collide in the shared bucket. Returns the storage key.
    pub async fn upload_enterprise_logo(
        &self,
        enterprise_id: Uuid,
        data: &[u8],
        content_type: &str,
    ) -> Result<String, AppError> {
        let ext = match content_type {
            "image/jpeg" => "jpg",
            "image/png" => "png",
            "image/webp" => "webp",
            _ => {
                return Err(AppError::Validation(
                    "Unsupported image format. Use JPEG, PNG, or WebP.".to_string(),
                ));
            }
        };

        let key = format!("enterprise-logos/{enterprise_id}.{ext}");

        self.bucket
            .put_object_with_content_type(&key, data, content_type)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to upload enterprise logo: {e}")))?;

        Ok(key)
    }

    /// Delete every extension variant so a re-upload with a different format
    /// doesn't leave a dangling object behind.
    pub async fn delete_enterprise_logo(&self, enterprise_id: Uuid) -> Result<(), AppError> {
        for ext in &["jpg", "png", "webp"] {
            let key = format!("enterprise-logos/{enterprise_id}.{ext}");
            let _ = self.bucket.delete_object(&key).await;
        }
        Ok(())
    }

    /// Public URL for an enterprise logo key. Shares the avatar CDN when set.
    pub fn enterprise_logo_url(&self, key: &str) -> String {
        match &self.cdn_base_url {
            Some(cdn) => format!("{}/{}", cdn.trim_end_matches('/'), key),
            None => format!("{}/{}/{}", self.endpoint, self.bucket_name, key),
        }
    }

    /// Generic upload to an arbitrary key (used by data exports, etc.).
    pub async fn upload_generic(
        &self,
        key: &str,
        data: &[u8],
        content_type: &str,
    ) -> Result<(), AppError> {
        self.bucket
            .put_object_with_content_type(key, data, content_type)
            .await
            .map_err(|e| AppError::Internal(format!("upload {key} failed: {e}")))?;
        Ok(())
    }

    /// Generate a presigned GET URL valid for `expires_seconds`.
    pub async fn presigned_get_url(
        &self,
        key: &str,
        expires_seconds: u32,
    ) -> Result<String, AppError> {
        self.bucket
            .presign_get(key, expires_seconds, None)
            .await
            .map_err(|e| AppError::Internal(format!("presign failed: {e}")))
    }

    pub async fn delete_generic(&self, key: &str) -> Result<(), AppError> {
        self.bucket
            .delete_object(key)
            .await
            .map_err(|e| AppError::Internal(format!("delete {key} failed: {e}")))?;
        Ok(())
    }
}
