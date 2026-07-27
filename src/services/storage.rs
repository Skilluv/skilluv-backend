use s3::creds::Credentials;
use s3::{Bucket, Region};
use uuid::Uuid;

use crate::config::AppConfig;
use crate::errors::AppError;

/// Split storage across two buckets so we never mix files intended for the
/// browser with private documents. Any mix would force the public bucket to
/// leak KYC / RGPD material via `GetObject` — a hard no for compliance.
///
/// - `public_bucket` (`MINIO_BUCKET`, default `avatars`) — avatars + enterprise
///   logos. Must allow anonymous `GetObject` in production so `<img src>` works
///   without pre-signing every URL. `ListObjects` MUST stay denied.
/// - `private_bucket` (`MINIO_BUCKET_PRIVATE`, default `documents`) — data
///   exports, KYC docs, anything else. Zero anonymous access ; every read goes
///   through a short-TTL presigned URL.
pub struct StorageService {
    public_bucket: Box<Bucket>,
    private_bucket: Box<Bucket>,
    endpoint: String,
    public_bucket_name: String,
    cdn_base_url: Option<String>,
}

/// Hard cap on presigned URL lifetime. 7 days = the longest defensible window
/// (data exports emailed to users). Any caller asking for more is capped +
/// warned in logs.
const PRESIGN_MAX_TTL_SECONDS: u32 = 7 * 24 * 3600;

/// One-shot MinIO/S3 policy required for avatars + enterprise logos to render
/// in the browser: the *public* bucket must allow anonymous `GetObject`. Run
/// once after first bucket creation (persisted in the MinIO data volume, so it
/// survives restarts):
///
/// ```sh
/// docker exec skilluv-minio mc alias set local http://localhost:9000 "$MINIO_ROOT_USER" "$MINIO_ROOT_PASSWORD"
/// docker exec skilluv-minio mc anonymous set download local/"$MINIO_BUCKET"
/// ```
///
/// The `private_bucket` gets NO anonymous access — presigned URLs only. The
/// `rust-s3` crate we use (0.35) has no `put_bucket_policy` helper, so this
/// policy step remains manual / IaC.
impl StorageService {
    pub async fn new(config: &AppConfig) -> Self {
        let region = || Region::Custom {
            region: "us-east-1".to_string(),
            endpoint: config.minio_endpoint.clone(),
        };
        let creds = || {
            Credentials::new(
                Some(&config.minio_access_key),
                Some(&config.minio_secret_key),
                None,
                None,
                None,
            )
            .expect("Failed to create S3 credentials")
        };

        // Public bucket handle
        let public_bucket = Bucket::new(&config.minio_bucket, region(), creds())
            .expect("Failed to create public bucket handle")
            .with_path_style();

        // Private bucket handle
        let private_bucket = Bucket::new(&config.minio_bucket_private, region(), creds())
            .expect("Failed to create private bucket handle")
            .with_path_style();

        // Auto-provision both buckets if missing. Errors ignored: idempotent
        // (bucket already exists) or MinIO not ready yet (will retry on first
        // upload). Never fatal at startup so the API still boots for /health.
        let _ = s3::Bucket::create_with_path_style(
            &config.minio_bucket,
            region(),
            creds(),
            s3::BucketConfiguration::default(),
        )
        .await;
        let _ = s3::Bucket::create_with_path_style(
            &config.minio_bucket_private,
            region(),
            creds(),
            s3::BucketConfiguration::default(),
        )
        .await;

        tracing::info!(
            public_bucket = config.minio_bucket,
            private_bucket = config.minio_bucket_private,
            endpoint = config.minio_endpoint,
            "Storage service initialized"
        );

        Self {
            public_bucket,
            private_bucket,
            endpoint: config.minio_endpoint.clone(),
            public_bucket_name: config.minio_bucket.clone(),
            cdn_base_url: config.avatar_cdn_base_url.clone(),
        }
    }

    // ─────────────────────────── PUBLIC bucket ───────────────────────────

    /// Upload avatar image to the public bucket. Returns the storage key.
    pub async fn upload_avatar(
        &self,
        user_id: Uuid,
        data: &[u8],
        content_type: &str,
    ) -> Result<String, AppError> {
        let ext = image_ext(content_type)?;
        let key = format!("{user_id}.{ext}");
        self.public_bucket
            .put_object_with_content_type(&key, data, content_type)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to upload avatar: {e}")))?;
        Ok(key)
    }

    /// Delete avatar by key prefix (user_id). Iterates all supported extensions
    /// because the DB doesn't remember the exact one.
    pub async fn delete_avatar(&self, user_id: Uuid) -> Result<(), AppError> {
        for ext in &["jpg", "png", "webp"] {
            let key = format!("{user_id}.{ext}");
            let _ = self.public_bucket.delete_object(&key).await;
        }
        Ok(())
    }

    /// Get public URL for an avatar key. Uses CDN when configured, otherwise
    /// direct MinIO URL against the *public* bucket.
    pub fn avatar_url(&self, key: &str) -> String {
        match &self.cdn_base_url {
            Some(cdn) => format!("{}/{}", cdn.trim_end_matches('/'), key),
            None => format!("{}/{}/{}", self.endpoint, self.public_bucket_name, key),
        }
    }

    /// Upload enterprise logo. Namespaced under `enterprise-logos/` so avatars
    /// and logos never collide. Returns the storage key.
    pub async fn upload_enterprise_logo(
        &self,
        enterprise_id: Uuid,
        data: &[u8],
        content_type: &str,
    ) -> Result<String, AppError> {
        let ext = image_ext(content_type)?;
        let key = format!("enterprise-logos/{enterprise_id}.{ext}");
        self.public_bucket
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
            let _ = self.public_bucket.delete_object(&key).await;
        }
        Ok(())
    }

    /// Public URL for an enterprise logo key. Shares the avatar CDN when set.
    pub fn enterprise_logo_url(&self, key: &str) -> String {
        match &self.cdn_base_url {
            Some(cdn) => format!("{}/{}", cdn.trim_end_matches('/'), key),
            None => format!("{}/{}/{}", self.endpoint, self.public_bucket_name, key),
        }
    }

    // ─────────────────────────── PRIVATE bucket ──────────────────────────

    /// Upload arbitrary bytes to the private bucket. Used for data exports,
    /// KYC documents and anything else that must not be reachable via a
    /// stable URL.
    pub async fn upload_private(
        &self,
        key: &str,
        data: &[u8],
        content_type: &str,
    ) -> Result<(), AppError> {
        self.private_bucket
            .put_object_with_content_type(key, data, content_type)
            .await
            .map_err(|e| AppError::Internal(format!("upload {key} failed: {e}")))?;
        Ok(())
    }

    /// Generate a presigned GET URL against the *private* bucket. TTL is capped
    /// at [`PRESIGN_MAX_TTL_SECONDS`] (7 days) — callers asking for more get
    /// silently capped and a warning is logged.
    pub async fn presigned_get_url(
        &self,
        key: &str,
        expires_seconds: u32,
    ) -> Result<String, AppError> {
        let ttl = if expires_seconds > PRESIGN_MAX_TTL_SECONDS {
            tracing::warn!(
                requested_ttl = expires_seconds,
                capped_ttl = PRESIGN_MAX_TTL_SECONDS,
                %key,
                "presigned URL TTL capped"
            );
            PRESIGN_MAX_TTL_SECONDS
        } else {
            expires_seconds
        };
        self.private_bucket
            .presign_get(key, ttl, None)
            .await
            .map_err(|e| AppError::Internal(format!("presign failed: {e}")))
    }

    pub async fn delete_private(&self, key: &str) -> Result<(), AppError> {
        self.private_bucket
            .delete_object(key)
            .await
            .map_err(|e| AppError::Internal(format!("delete {key} failed: {e}")))?;
        Ok(())
    }
}

fn image_ext(content_type: &str) -> Result<&'static str, AppError> {
    match content_type {
        "image/jpeg" => Ok("jpg"),
        "image/png" => Ok("png"),
        "image/webp" => Ok("webp"),
        _ => Err(AppError::Validation(
            "Unsupported image format. Use JPEG, PNG, or WebP.".to_string(),
        )),
    }
}
