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
///
/// The `private_bucket` also needs a CORS rule that exposes the `ETag`, or
/// browser multipart uploads (`design_uploads`) fail at the last step — the
/// browser cannot read the per-part ETag `complete` requires (SKI-309). Once
/// per private bucket, alongside the download policy above:
///
/// ```sh
/// # cors.json: {"CORSRules":[{"AllowedOrigins":["https://skill-uv.com"],
/// #   "AllowedMethods":["PUT","GET"],"AllowedHeaders":["*"],
/// #   "ExposeHeaders":["ETag"]}]}
/// docker exec skilluv-minio mc cors set local/"$MINIO_BUCKET_PRIVATE" cors.json
/// ```
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

    /// Read an object back out of the private bucket.
    ///
    /// Used by the workers that have to look inside a file rather than hand it
    /// to somebody — measuring an audio master, for one. A presigned URL plus
    /// an HTTP client would work and would mean the service talking to itself
    /// through a signature it just made.
    pub async fn get_private(&self, key: &str) -> Result<Vec<u8>, AppError> {
        let response = self
            .private_bucket
            .get_object(key)
            .await
            .map_err(|e| AppError::Internal(format!("read {key} failed: {e}")))?;
        Ok(response.bytes().to_vec())
    }

    pub async fn delete_private(&self, key: &str) -> Result<(), AppError> {
        self.private_bucket
            .delete_object(key)
            .await
            .map_err(|e| AppError::Internal(format!("delete {key} failed: {e}")))?;
        Ok(())
    }

    // ═══════════════════════════════════════════════════════════════
    // Large files, uploaded by the client rather than through us
    // ═══════════════════════════════════════════════════════════════
    //
    // A design deliverable can be a five-gigabyte scene file. Sending that
    // through an axum handler would hold a connection and a buffer for as long
    // as somebody's connection takes, and it would do so for every concurrent
    // upload. So the object store receives it: we hand out presigned PUT URLs
    // for each part, the client uploads straight there, and we only see the
    // part list at the end.
    //
    // All of it lands in the private bucket. A design deliverable can be under
    // NDA, so "readable only through a presigned URL" is the correct default
    // and it is what this bucket already is.

    /// Open a multipart upload and return the store's handle on it.
    pub async fn begin_multipart(&self, key: &str, content_type: &str) -> Result<String, AppError> {
        let started = self
            .private_bucket
            .initiate_multipart_upload(key, content_type)
            .await
            .map_err(|e| AppError::Internal(format!("multipart init for {key} failed: {e}")))?;
        Ok(started.upload_id)
    }

    /// A presigned PUT for one part of an open multipart upload.
    ///
    /// The `uploadId` and `partNumber` travel as query parameters because that
    /// is where S3 expects them, and they are part of what the signature
    /// covers — a client cannot move a part to another upload or another
    /// position without invalidating it.
    pub async fn presign_part_put(
        &self,
        key: &str,
        upload_id: &str,
        part_number: u32,
        expires_seconds: u32,
    ) -> Result<String, AppError> {
        let mut queries = std::collections::HashMap::new();
        queries.insert("uploadId".to_string(), upload_id.to_string());
        queries.insert("partNumber".to_string(), part_number.to_string());

        self.private_bucket
            .presign_put(
                key,
                expires_seconds.min(PRESIGN_MAX_TTL_SECONDS),
                None,
                Some(queries),
            )
            .await
            .map_err(|e| AppError::Internal(format!("presign part {part_number} failed: {e}")))
    }

    /// A presigned PUT for a whole object. Used for the preview that
    /// accompanies an unopenable source file — small enough to arrive in one
    /// request, and separate so it can be replaced without re-sending the
    /// source.
    pub async fn presign_put_url(
        &self,
        key: &str,
        expires_seconds: u32,
    ) -> Result<String, AppError> {
        self.private_bucket
            .presign_put(
                key,
                expires_seconds.min(PRESIGN_MAX_TTL_SECONDS),
                None,
                None,
            )
            .await
            .map_err(|e| AppError::Internal(format!("presign put for {key} failed: {e}")))
    }

    /// Ask the store to assemble the parts.
    pub async fn finish_multipart(
        &self,
        key: &str,
        upload_id: &str,
        parts: &[crate::services::design_uploads::CompletedPart],
    ) -> Result<(), AppError> {
        let parts: Vec<s3::serde_types::Part> = parts
            .iter()
            .map(|p| s3::serde_types::Part {
                part_number: p.part_number as u32,
                etag: p.etag.clone(),
            })
            .collect();

        let response = self
            .private_bucket
            .complete_multipart_upload(key, upload_id, parts)
            .await
            .map_err(|e| AppError::Internal(format!("multipart completion failed: {e}")))?;

        // The store answers 200 with an error document on some failures, so
        // the status is checked rather than assumed.
        if response.status_code() >= 300 {
            return Err(AppError::Validation(format!(
                "the object store refused the assembly ({}) — usually a part                  that was never uploaded, or one under the five-megabyte floor",
                response.status_code()
            )));
        }
        Ok(())
    }

    /// Give up on an unfinished upload, so the store stops billing for the
    /// parts already sent.
    pub async fn abort_multipart(&self, key: &str, upload_id: &str) -> Result<(), AppError> {
        self.private_bucket
            .abort_upload(key, upload_id)
            .await
            .map_err(|e| AppError::Internal(format!("multipart abort failed: {e}")))?;
        Ok(())
    }

    /// Everything under one prefix in the private bucket, with when it was
    /// last modified.
    ///
    /// The only honest source for "what is in the bucket that nothing points
    /// at". A table of uploads would be a second record of what exists, and
    /// the question a cleanup sweep asks is answered wrongly by anything
    /// except the bucket itself.
    ///
    /// Paginated by the client library. The prefix is required rather than
    /// optional: listing a whole bucket is a request nobody here has a use for
    /// and a cost somebody would eventually pay for by accident.
    pub async fn list_private(
        &self,
        prefix: &str,
    ) -> Result<Vec<(String, chrono::DateTime<chrono::Utc>)>, AppError> {
        if prefix.trim().is_empty() {
            return Err(AppError::Internal(
                "listing the private bucket needs a prefix".into(),
            ));
        }

        let pages = self
            .private_bucket
            .list(prefix.to_string(), None)
            .await
            .map_err(|e| AppError::Internal(format!("list {prefix} failed: {e}")))?;

        let mut out = Vec::new();
        for page in pages {
            for object in page.contents {
                // A malformed date is not a reason to skip an object: the
                // sweep's other condition is whether anything references it,
                // and treating an undateable object as brand new means it
                // survives until somebody looks.
                let modified = chrono::DateTime::parse_from_rfc3339(&object.last_modified)
                    .map(|d| d.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|_| chrono::Utc::now());
                out.push((object.key, modified));
            }
        }
        Ok(out)
    }

    /// How many bytes the store actually holds.
    ///
    /// Read back rather than trusted: the ceiling checked before the upload
    /// was checked against a number the client chose.
    pub async fn object_size(&self, key: &str) -> Result<i64, AppError> {
        let (head, status) = self
            .private_bucket
            .head_object(key)
            .await
            .map_err(|e| AppError::Internal(format!("head {key} failed: {e}")))?;
        if status >= 300 {
            return Err(AppError::NotFound(format!(
                "the object store has nothing at {key}"
            )));
        }
        head.content_length.ok_or_else(|| {
            AppError::Internal(format!("the object store did not say how large {key} is"))
        })
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
