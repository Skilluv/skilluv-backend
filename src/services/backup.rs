//! Backup service.
//!
//! Used by the `skilluv-backup` binary (`src/bin/backup.rs`). Logic split here
//! so it can be unit-tested without spawning processes when possible.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Instant;

use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Datelike, Duration as ChronoDuration, Utc};
use s3::Bucket;
use s3::creds::Credentials;
use s3::region::Region;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::fs;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tracing::{debug, info, warn};

// ─── Configuration ────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct BackupConfig {
    pub database_url: String,
    pub r2_endpoint: String,
    pub r2_bucket: String,
    pub r2_access_key: String,
    pub r2_secret_key: String,
    pub backup_prefix: String,
    pub pg_dump_path: String,
    pub pg_restore_path: String,
    pub psql_path: String,
    pub temp_dir: PathBuf,
    pub notify_webhook_url: Option<String>,
    pub notify_webhook_kind: WebhookKind,
    pub minio_source: Option<MinioSourceConfig>,
    pub minio_mirror_prefix: String,
}

#[derive(Debug, Clone)]
pub struct MinioSourceConfig {
    pub endpoint: String,
    pub bucket: String,
    pub access_key: String,
    pub secret_key: String,
    pub region: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebhookKind {
    Slack,
    Discord,
    Generic,
}

impl WebhookKind {
    pub fn from_env(value: Option<String>) -> Self {
        match value.as_deref().map(str::to_lowercase).as_deref() {
            Some("slack") => Self::Slack,
            Some("discord") => Self::Discord,
            _ => Self::Generic,
        }
    }
}

impl BackupConfig {
    /// Load from environment variables. All vars are documented in `.env.example`.
    pub fn from_env() -> Result<Self> {
        let database_url = required("DATABASE_URL")?;
        let r2_endpoint = required("BACKUP_R2_ENDPOINT")?;
        let r2_bucket = required("BACKUP_R2_BUCKET")?;
        let r2_access_key = required("BACKUP_R2_ACCESS_KEY")?;
        let r2_secret_key = required("BACKUP_R2_SECRET_KEY")?;
        let backup_prefix =
            std::env::var("BACKUP_PREFIX").unwrap_or_else(|_| "skilluv/postgres/".into());
        let pg_dump_path = std::env::var("BACKUP_PG_DUMP").unwrap_or_else(|_| "pg_dump".into());
        let pg_restore_path =
            std::env::var("BACKUP_PG_RESTORE").unwrap_or_else(|_| "pg_restore".into());
        let psql_path = std::env::var("BACKUP_PSQL").unwrap_or_else(|_| "psql".into());
        let temp_dir = std::env::var("BACKUP_TEMP_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| std::env::temp_dir().join("skilluv-backup"));
        let notify_webhook_url = std::env::var("BACKUP_NOTIFY_WEBHOOK_URL").ok();
        let notify_webhook_kind = WebhookKind::from_env(std::env::var("BACKUP_NOTIFY_KIND").ok());

        let minio_mirror_prefix = std::env::var("BACKUP_MINIO_PREFIX")
            .unwrap_or_else(|_| "skilluv/minio-avatars/".into());

        // Source MinIO config. Fall back to MINIO_* if BACKUP_MINIO_SOURCE_* not set —
        // both the backend and the backup tool talk to the same MinIO in most deployments.
        let minio_source = {
            let endpoint = std::env::var("BACKUP_MINIO_SOURCE_ENDPOINT")
                .ok()
                .or_else(|| std::env::var("MINIO_ENDPOINT").ok());
            let bucket = std::env::var("BACKUP_MINIO_SOURCE_BUCKET")
                .ok()
                .or_else(|| std::env::var("MINIO_BUCKET").ok());
            let access_key = std::env::var("BACKUP_MINIO_SOURCE_ACCESS_KEY")
                .ok()
                .or_else(|| std::env::var("MINIO_ACCESS_KEY").ok());
            let secret_key = std::env::var("BACKUP_MINIO_SOURCE_SECRET_KEY")
                .ok()
                .or_else(|| std::env::var("MINIO_SECRET_KEY").ok());
            let region = std::env::var("BACKUP_MINIO_SOURCE_REGION")
                .unwrap_or_else(|_| "us-east-1".into());
            match (endpoint, bucket, access_key, secret_key) {
                (Some(endpoint), Some(bucket), Some(access_key), Some(secret_key)) => {
                    Some(MinioSourceConfig {
                        endpoint,
                        bucket,
                        access_key,
                        secret_key,
                        region,
                    })
                }
                _ => None,
            }
        };

        Ok(Self {
            database_url,
            r2_endpoint,
            r2_bucket,
            r2_access_key,
            r2_secret_key,
            backup_prefix,
            pg_dump_path,
            pg_restore_path,
            psql_path,
            temp_dir,
            notify_webhook_url,
            notify_webhook_kind,
            minio_source,
            minio_mirror_prefix,
        })
    }

    pub fn minio_source_bucket(&self) -> Result<Box<Bucket>> {
        let source = self.minio_source.as_ref().ok_or_else(|| {
            anyhow!("MinIO source not configured (set BACKUP_MINIO_SOURCE_* or MINIO_* env vars)")
        })?;
        let region = Region::Custom {
            region: source.region.clone(),
            endpoint: source.endpoint.clone(),
        };
        let creds = Credentials::new(
            Some(&source.access_key),
            Some(&source.secret_key),
            None,
            None,
            None,
        )
        .map_err(|e| anyhow!("invalid MinIO credentials: {e}"))?;
        let mut bucket = Bucket::new(&source.bucket, region, creds)
            .map_err(|e| anyhow!("MinIO bucket init failed: {e}"))?;
        bucket.set_path_style();
        Ok(bucket)
    }

    pub fn bucket(&self) -> Result<Box<Bucket>> {
        let region = Region::Custom {
            region: "auto".into(),
            endpoint: self.r2_endpoint.clone(),
        };
        let creds = Credentials::new(
            Some(&self.r2_access_key),
            Some(&self.r2_secret_key),
            None,
            None,
            None,
        )
        .map_err(|e| anyhow!("invalid R2 credentials: {e}"))?;
        let mut bucket = Bucket::new(&self.r2_bucket, region, creds)
            .map_err(|e| anyhow!("bucket init failed: {e}"))?;
        bucket.set_path_style();
        Ok(bucket)
    }
}

fn required(key: &str) -> Result<String> {
    std::env::var(key).map_err(|_| anyhow!("missing required env var: {key}"))
}

// ─── Domain types ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct BackupEntry {
    pub key: String,
    pub created_at: DateTime<Utc>,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackupResult {
    pub key: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub duration_seconds: f64,
}

// ─── Backup orchestration ─────────────────────────────────────────

pub async fn run_backup(cfg: &BackupConfig) -> Result<BackupResult> {
    let start = Instant::now();
    let now = Utc::now();
    let stem = format!("skilluv-{}", now.format("%Y-%m-%dT%H-%M-%S"));
    let key = format!("{}{stem}.dump", cfg.backup_prefix);

    fs::create_dir_all(&cfg.temp_dir)
        .await
        .with_context(|| format!("create temp dir {}", cfg.temp_dir.display()))?;
    let local_path = cfg.temp_dir.join(format!("{stem}.dump"));

    info!(target = %key, "starting pg_dump");
    pg_dump_to_file(cfg, &local_path)
        .await
        .context("pg_dump failed")?;

    let metadata = fs::metadata(&local_path).await?;
    let size_bytes = metadata.len();
    if size_bytes == 0 {
        bail!("pg_dump produced an empty file");
    }

    let sha = compute_sha256(&local_path).await?;
    info!(size = size_bytes, sha = %sha, "pg_dump complete, uploading");

    upload_file(cfg, &local_path, &key).await?;

    // Sidecar with checksum, used by restore-test for integrity verification.
    let sidecar_key = format!("{key}.sha256");
    let sidecar_body = format!("{sha}  {stem}.dump\n").into_bytes();
    upload_bytes(cfg, &sidecar_key, &sidecar_body, "text/plain").await?;

    fs::remove_file(&local_path).await.ok();

    let duration_seconds = start.elapsed().as_secs_f64();
    let result = BackupResult {
        key,
        size_bytes,
        sha256: sha,
        duration_seconds,
    };
    info!(?result, "backup complete");
    Ok(result)
}

async fn pg_dump_to_file(cfg: &BackupConfig, path: &Path) -> Result<()> {
    let mut cmd = Command::new(&cfg.pg_dump_path);
    cmd.arg("--format=custom")
        .arg("--compress=9")
        .arg("--no-owner")
        .arg("--no-acl")
        .arg("--file")
        .arg(path)
        .arg(&cfg.database_url)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let output = cmd.output().await.with_context(|| {
        format!(
            "failed to spawn pg_dump (check that `{}` is in PATH)",
            cfg.pg_dump_path
        )
    })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("pg_dump exited with {}: {}", output.status, stderr);
    }
    Ok(())
}

async fn compute_sha256(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 8 * 1024 * 1024]; // 8 MB
    loop {
        let n = file.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

async fn upload_file(cfg: &BackupConfig, path: &Path, key: &str) -> Result<()> {
    let body = fs::read(path).await?;
    upload_bytes(cfg, key, &body, "application/octet-stream").await
}

async fn upload_bytes(
    cfg: &BackupConfig,
    key: &str,
    body: &[u8],
    content_type: &str,
) -> Result<()> {
    let bucket = cfg.bucket()?;
    let response = bucket
        .put_object_with_content_type(key, body, content_type)
        .await
        .with_context(|| format!("upload to {} failed", key))?;
    if response.status_code() / 100 != 2 {
        bail!(
            "upload to {} returned HTTP {}",
            key,
            response.status_code()
        );
    }
    debug!(%key, status = response.status_code(), "uploaded");
    Ok(())
}

// ─── Listing & retention ──────────────────────────────────────────

pub async fn list_backups(cfg: &BackupConfig) -> Result<Vec<BackupEntry>> {
    let bucket = cfg.bucket()?;
    let results = bucket
        .list(cfg.backup_prefix.clone(), Some("/".into()))
        .await
        .context("R2 list failed")?;

    let mut entries = Vec::new();
    for page in results {
        for object in page.contents {
            // Skip sidecars and non-dump files
            if !object.key.ends_with(".dump") {
                continue;
            }
            let created_at = parse_key_timestamp(&object.key)
                .ok_or_else(|| anyhow!("cannot parse timestamp from key: {}", object.key))?;
            entries.push(BackupEntry {
                key: object.key,
                created_at,
                size_bytes: object.size,
            });
        }
    }
    entries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(entries)
}

fn parse_key_timestamp(key: &str) -> Option<DateTime<Utc>> {
    let stem = key.rsplit('/').next()?;
    let stem = stem.strip_prefix("skilluv-")?.strip_suffix(".dump")?;
    let parsed = chrono::NaiveDateTime::parse_from_str(stem, "%Y-%m-%dT%H-%M-%S").ok()?;
    Some(parsed.and_utc())
}

/// Retention rule:
/// - Keep all backups from the last 7 days (daily)
/// - Keep the most recent backup of each ISO week for the last 4 weeks beyond that (weekly)
/// - Keep the most recent backup of each calendar month for the last 12 months beyond that (monthly)
/// - Everything else is candidate for deletion
pub fn classify_for_retention(
    entries: &[BackupEntry],
    now: DateTime<Utc>,
) -> RetentionDecision {
    let mut keep: HashSet<String> = HashSet::new();
    let mut delete: Vec<String> = Vec::new();

    let today = now.date_naive();
    let seven_days_ago = today - ChronoDuration::days(7);
    let weekly_horizon = today - ChronoDuration::days(7 + 28); // 4 weeks beyond the daily window
    let monthly_horizon_year = today.year() - 1;

    // Bucket weekly: ISO year + week → keep most recent
    let mut weekly_best: std::collections::HashMap<(i32, u32), &BackupEntry> =
        std::collections::HashMap::new();
    // Bucket monthly: year + month → keep most recent
    let mut monthly_best: std::collections::HashMap<(i32, u32), &BackupEntry> =
        std::collections::HashMap::new();

    for entry in entries {
        let date = entry.created_at.date_naive();
        if date > seven_days_ago {
            keep.insert(entry.key.clone());
            continue;
        }
        if date > weekly_horizon {
            let iso = date.iso_week();
            let bucket_key = (iso.year(), iso.week());
            weekly_best
                .entry(bucket_key)
                .and_modify(|best| {
                    if entry.created_at > best.created_at {
                        *best = entry;
                    }
                })
                .or_insert(entry);
            continue;
        }
        if date.year() > monthly_horizon_year
            || (date.year() == monthly_horizon_year && date.month() >= today.month())
        {
            let bucket_key = (date.year(), date.month());
            monthly_best
                .entry(bucket_key)
                .and_modify(|best| {
                    if entry.created_at > best.created_at {
                        *best = entry;
                    }
                })
                .or_insert(entry);
            continue;
        }
        delete.push(entry.key.clone());
    }

    for entry in weekly_best.values() {
        keep.insert(entry.key.clone());
    }
    for entry in monthly_best.values() {
        keep.insert(entry.key.clone());
    }

    // Anything in entries that's not in keep and not already in delete becomes delete.
    for entry in entries {
        if !keep.contains(&entry.key) && !delete.iter().any(|k| k == &entry.key) {
            delete.push(entry.key.clone());
        }
    }

    RetentionDecision { keep, delete }
}

#[derive(Debug, Clone, Serialize)]
pub struct RetentionDecision {
    pub keep: HashSet<String>,
    pub delete: Vec<String>,
}

pub async fn apply_retention(cfg: &BackupConfig) -> Result<RetentionReport> {
    let entries = list_backups(cfg).await?;
    let decision = classify_for_retention(&entries, Utc::now());

    let bucket = cfg.bucket()?;
    let mut deleted_keys = Vec::new();
    for key in &decision.delete {
        let response = bucket
            .delete_object(key)
            .await
            .with_context(|| format!("delete {} failed", key))?;
        if response.status_code() / 100 != 2 {
            warn!(%key, status = response.status_code(), "delete returned non-2xx");
            continue;
        }
        // Also delete the sidecar
        let sidecar = format!("{}.sha256", key);
        let _ = bucket.delete_object(&sidecar).await;
        deleted_keys.push(key.clone());
    }

    Ok(RetentionReport {
        kept: decision.keep.len(),
        deleted: deleted_keys.len(),
        deleted_keys,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct RetentionReport {
    pub kept: usize,
    pub deleted: usize,
    pub deleted_keys: Vec<String>,
}

// ─── Restore (test + manual) ──────────────────────────────────────

pub async fn restore_test(cfg: &BackupConfig) -> Result<RestoreReport> {
    let entries = list_backups(cfg).await?;
    let latest = entries
        .first()
        .ok_or_else(|| anyhow!("no backups found to test"))?;

    fs::create_dir_all(&cfg.temp_dir).await?;
    let local_path = cfg.temp_dir.join("restore-test.dump");
    download_to_file(cfg, &latest.key, &local_path).await?;

    let sha_expected_key = format!("{}.sha256", latest.key);
    if let Ok(bytes) = download_bytes(cfg, &sha_expected_key).await {
        let sha_line = String::from_utf8_lossy(&bytes);
        let expected = sha_line
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_string();
        let actual = compute_sha256(&local_path).await?;
        if expected != actual {
            bail!(
                "checksum mismatch for {}: expected {expected}, got {actual}",
                latest.key
            );
        }
        info!(%expected, "checksum match");
    } else {
        warn!(key = %sha_expected_key, "no sidecar checksum found");
    }

    let ephemeral_db = format!(
        "skilluv_restore_test_{}",
        Utc::now().format("%Y%m%d%H%M%S")
    );
    create_database(cfg, &ephemeral_db).await?;
    let restore_result = pg_restore_into(cfg, &local_path, &ephemeral_db).await;
    let count_result = match &restore_result {
        Ok(()) => count_critical_tables(cfg, &ephemeral_db).await,
        Err(_) => Ok(TableCounts::default()),
    };
    drop_database(cfg, &ephemeral_db).await.ok();
    fs::remove_file(&local_path).await.ok();

    restore_result?;
    let counts = count_result?;
    if counts.is_empty() {
        bail!("restore produced an empty database (no critical tables found)");
    }

    Ok(RestoreReport {
        backup_key: latest.key.clone(),
        ephemeral_db,
        counts,
    })
}

pub async fn restore_to(
    cfg: &BackupConfig,
    backup_key: &str,
    target_db: &str,
) -> Result<RestoreReport> {
    fs::create_dir_all(&cfg.temp_dir).await?;
    let local_path = cfg.temp_dir.join("manual-restore.dump");
    download_to_file(cfg, backup_key, &local_path).await?;
    pg_restore_into(cfg, &local_path, target_db).await?;
    let counts = count_critical_tables(cfg, target_db).await?;
    fs::remove_file(&local_path).await.ok();
    Ok(RestoreReport {
        backup_key: backup_key.to_string(),
        ephemeral_db: target_db.to_string(),
        counts,
    })
}

async fn download_to_file(cfg: &BackupConfig, key: &str, path: &Path) -> Result<()> {
    let bucket = cfg.bucket()?;
    let response = bucket
        .get_object(key)
        .await
        .with_context(|| format!("download {} failed", key))?;
    if response.status_code() / 100 != 2 {
        bail!(
            "download {} returned HTTP {}",
            key,
            response.status_code()
        );
    }
    fs::write(path, response.bytes()).await?;
    Ok(())
}

async fn download_bytes(cfg: &BackupConfig, key: &str) -> Result<Vec<u8>> {
    let bucket = cfg.bucket()?;
    let response = bucket.get_object(key).await?;
    if response.status_code() / 100 != 2 {
        bail!("download {} returned HTTP {}", key, response.status_code());
    }
    Ok(response.bytes().to_vec())
}

async fn create_database(cfg: &BackupConfig, db_name: &str) -> Result<()> {
    let admin_url = replace_database_in_url(&cfg.database_url, "postgres");
    let sql = format!("CREATE DATABASE \"{db_name}\"");
    psql_exec(cfg, &admin_url, &sql).await
}

async fn drop_database(cfg: &BackupConfig, db_name: &str) -> Result<()> {
    let admin_url = replace_database_in_url(&cfg.database_url, "postgres");
    // Force-drop any remaining sessions
    let _ = psql_exec(
        cfg,
        &admin_url,
        &format!(
            "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '{db_name}'"
        ),
    )
    .await;
    psql_exec(cfg, &admin_url, &format!("DROP DATABASE IF EXISTS \"{db_name}\"")).await
}

async fn pg_restore_into(cfg: &BackupConfig, dump_path: &Path, db_name: &str) -> Result<()> {
    let target_url = replace_database_in_url(&cfg.database_url, db_name);
    let mut cmd = Command::new(&cfg.pg_restore_path);
    cmd.arg("--no-owner")
        .arg("--no-acl")
        .arg("--single-transaction")
        .arg("--dbname")
        .arg(&target_url)
        .arg(dump_path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = cmd.output().await.context("failed to spawn pg_restore")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("pg_restore failed ({}): {}", output.status, stderr);
    }
    Ok(())
}

async fn psql_exec(cfg: &BackupConfig, url: &str, sql: &str) -> Result<()> {
    let mut cmd = Command::new(&cfg.psql_path);
    cmd.arg("--quiet")
        .arg("--no-psqlrc")
        .arg("-v")
        .arg("ON_ERROR_STOP=1")
        .arg("-c")
        .arg(sql)
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = cmd.output().await.context("failed to spawn psql")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("psql failed ({}): {}", output.status, stderr);
    }
    Ok(())
}

async fn count_critical_tables(cfg: &BackupConfig, db_name: &str) -> Result<TableCounts> {
    let url = replace_database_in_url(&cfg.database_url, db_name);
    let users = scalar_count(cfg, &url, "SELECT COUNT(*) FROM users").await?;
    let challenges = scalar_count(cfg, &url, "SELECT COUNT(*) FROM challenges").await?;
    let submissions =
        scalar_count(cfg, &url, "SELECT COUNT(*) FROM challenge_submissions").await?;
    Ok(TableCounts {
        users,
        challenges,
        submissions,
    })
}

async fn scalar_count(cfg: &BackupConfig, url: &str, sql: &str) -> Result<i64> {
    let mut cmd = Command::new(&cfg.psql_path);
    cmd.arg("--quiet")
        .arg("--no-psqlrc")
        .arg("--tuples-only")
        .arg("--no-align")
        .arg("-c")
        .arg(sql)
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = cmd.output().await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("count query failed: {}", stderr);
    }
    let value = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<i64>()
        .unwrap_or(-1);
    Ok(value)
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct TableCounts {
    pub users: i64,
    pub challenges: i64,
    pub submissions: i64,
}

impl TableCounts {
    pub fn is_empty(&self) -> bool {
        self.users < 0 && self.challenges < 0 && self.submissions < 0
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RestoreReport {
    pub backup_key: String,
    pub ephemeral_db: String,
    pub counts: TableCounts,
}

/// Replace the database name (path) component of a postgres URL.
/// Accepts `postgres://user:pass@host:port/dbname?params` form.
pub fn replace_database_in_url(url: &str, new_db: &str) -> String {
    if let Some(scheme_end) = url.find("://") {
        let after_scheme = &url[scheme_end + 3..];
        if let Some(slash) = after_scheme.find('/') {
            let prefix = &url[..scheme_end + 3 + slash + 1];
            let after_slash = &after_scheme[slash + 1..];
            let after_db = after_slash.find('?').map(|q| &after_slash[q..]).unwrap_or("");
            return format!("{prefix}{new_db}{after_db}");
        } else {
            return format!("{url}/{new_db}");
        }
    }
    url.to_string()
}

// ─── MinIO mirror ─────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize)]
pub struct MirrorReport {
    pub source_objects: usize,
    pub uploaded: usize,
    pub skipped_unchanged: usize,
    pub failed: usize,
    pub total_bytes_uploaded: u64,
    pub duration_seconds: f64,
    pub failures: Vec<MirrorFailure>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MirrorFailure {
    pub key: String,
    pub error: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MirrorAction {
    Upload,
    Skip,
}

fn decide_mirror_action(source_size: u64, target_size: Option<u64>) -> MirrorAction {
    match target_size {
        Some(existing) if existing == source_size => MirrorAction::Skip,
        _ => MirrorAction::Upload,
    }
}

pub async fn mirror_minio(cfg: &BackupConfig) -> Result<MirrorReport> {
    let start = Instant::now();
    let source = cfg.minio_source_bucket()?;
    let target = cfg.bucket()?;
    let prefix = cfg.minio_mirror_prefix.clone();

    info!(prefix = %prefix, "listing MinIO source objects");
    let source_pages = source
        .list("".into(), None)
        .await
        .context("listing MinIO source failed")?;
    let mut source_objects = Vec::new();
    for page in source_pages {
        for object in page.contents {
            source_objects.push(object);
        }
    }

    info!(target_prefix = %prefix, "listing R2 mirror target objects");
    let target_index = list_target_index(&target, &prefix).await?;

    let mut report = MirrorReport::default();
    report.source_objects = source_objects.len();

    for object in source_objects {
        let source_key = object.key.clone();
        let source_size = object.size;
        let mirror_key = format!("{prefix}{source_key}");

        let action = decide_mirror_action(source_size, target_index.get(&source_key).copied());
        if action == MirrorAction::Skip {
            report.skipped_unchanged += 1;
            continue;
        }

        match copy_object(&source, &source_key, &target, &mirror_key).await {
            Ok(bytes) => {
                report.uploaded += 1;
                report.total_bytes_uploaded += bytes;
                debug!(%source_key, bytes, "mirrored");
            }
            Err(err) => {
                report.failed += 1;
                report.failures.push(MirrorFailure {
                    key: source_key,
                    error: format!("{err:#}"),
                });
            }
        }
    }

    report.duration_seconds = start.elapsed().as_secs_f64();
    info!(?report, "mirror complete");
    Ok(report)
}

/// List target objects (R2) with given prefix, return key (without prefix) -> size map.
async fn list_target_index(
    target: &Bucket,
    prefix: &str,
) -> Result<std::collections::HashMap<String, u64>> {
    let pages = target
        .list(prefix.to_string(), None)
        .await
        .context("listing R2 target failed")?;
    let mut index = std::collections::HashMap::new();
    for page in pages {
        for object in page.contents {
            if let Some(rest) = object.key.strip_prefix(prefix) {
                index.insert(rest.to_string(), object.size);
            }
        }
    }
    Ok(index)
}

async fn copy_object(
    source: &Bucket,
    source_key: &str,
    target: &Bucket,
    target_key: &str,
) -> Result<u64> {
    let response = source
        .get_object(source_key)
        .await
        .with_context(|| format!("download {} from source failed", source_key))?;
    if response.status_code() / 100 != 2 {
        bail!(
            "download {} returned HTTP {}",
            source_key,
            response.status_code()
        );
    }
    let body = response.bytes();
    let bytes_len = body.len() as u64;
    let put = target
        .put_object_with_content_type(target_key, body, "application/octet-stream")
        .await
        .with_context(|| format!("upload {} to target failed", target_key))?;
    if put.status_code() / 100 != 2 {
        bail!(
            "upload {} returned HTTP {}",
            target_key,
            put.status_code()
        );
    }
    Ok(bytes_len)
}

// ─── Notifications ────────────────────────────────────────────────

pub async fn notify(cfg: &BackupConfig, level: NotifyLevel, message: &str) -> Result<()> {
    let Some(url) = cfg.notify_webhook_url.as_deref() else {
        return Ok(());
    };
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    let body = match cfg.notify_webhook_kind {
        WebhookKind::Slack => serde_json::json!({
            "text": format!("{} skilluv-backup: {message}", level.emoji()),
        }),
        WebhookKind::Discord => serde_json::json!({
            "content": format!("{} skilluv-backup: {message}", level.emoji()),
        }),
        WebhookKind::Generic => serde_json::json!({
            "level": level.as_str(),
            "service": "skilluv-backup",
            "message": message,
            "timestamp": Utc::now().to_rfc3339(),
        }),
    };
    let response = client.post(url).json(&body).send().await?;
    if !response.status().is_success() {
        warn!(
            status = response.status().as_u16(),
            "notify webhook returned non-2xx"
        );
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
pub enum NotifyLevel {
    Success,
    Warning,
    Failure,
}

impl NotifyLevel {
    fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Warning => "warning",
            Self::Failure => "failure",
        }
    }

    fn emoji(self) -> &'static str {
        match self {
            Self::Success => "✅",
            Self::Warning => "⚠️",
            Self::Failure => "🚨",
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn parse_key_timestamp_roundtrip() {
        let ts = parse_key_timestamp("skilluv/postgres/skilluv-2026-06-26T03-00-00.dump").unwrap();
        assert_eq!(ts.format("%Y-%m-%d %H:%M:%S").to_string(), "2026-06-26 03:00:00");
    }

    #[test]
    fn parse_key_timestamp_rejects_invalid() {
        assert!(parse_key_timestamp("garbage.dump").is_none());
        assert!(parse_key_timestamp("skilluv/postgres/skilluv-2026.dump").is_none());
    }

    #[test]
    fn replace_database_in_url_basic() {
        assert_eq!(
            replace_database_in_url(
                "postgres://u:p@host:5432/skilluv?sslmode=require",
                "skilluv_test"
            ),
            "postgres://u:p@host:5432/skilluv_test?sslmode=require"
        );
        assert_eq!(
            replace_database_in_url("postgres://u:p@host:5432/skilluv", "tmp"),
            "postgres://u:p@host:5432/tmp"
        );
    }

    fn mk_entry(date: &str, key: &str) -> BackupEntry {
        BackupEntry {
            key: key.to_string(),
            created_at: chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .unwrap()
                .and_hms_opt(3, 0, 0)
                .unwrap()
                .and_utc(),
            size_bytes: 100,
        }
    }

    #[test]
    fn retention_keeps_recent_days() {
        let now = NaiveDate::from_ymd_opt(2026, 6, 26)
            .unwrap()
            .and_hms_opt(3, 30, 0)
            .unwrap()
            .and_utc();
        let entries = vec![
            mk_entry("2026-06-25", "skilluv/postgres/skilluv-2026-06-25T03-00-00.dump"),
            mk_entry("2026-06-20", "skilluv/postgres/skilluv-2026-06-20T03-00-00.dump"),
            mk_entry("2026-05-01", "skilluv/postgres/skilluv-2026-05-01T03-00-00.dump"),
            mk_entry("2025-01-15", "skilluv/postgres/skilluv-2025-01-15T03-00-00.dump"),
        ];
        let decision = classify_for_retention(&entries, now);
        assert!(decision.keep.contains(&entries[0].key)); // last 7d
        assert!(decision.keep.contains(&entries[1].key)); // last 7d
        assert!(decision.delete.contains(&entries[3].key)); // older than 12 months
    }

    #[test]
    fn mirror_action_skip_if_same_size() {
        assert_eq!(decide_mirror_action(1024, Some(1024)), MirrorAction::Skip);
    }

    #[test]
    fn mirror_action_upload_if_missing() {
        assert_eq!(decide_mirror_action(1024, None), MirrorAction::Upload);
    }

    #[test]
    fn mirror_action_upload_if_size_differs() {
        assert_eq!(decide_mirror_action(2048, Some(1024)), MirrorAction::Upload);
        assert_eq!(decide_mirror_action(512, Some(1024)), MirrorAction::Upload);
    }

    #[test]
    fn webhook_kind_from_env() {
        assert_eq!(WebhookKind::from_env(Some("slack".into())), WebhookKind::Slack);
        assert_eq!(
            WebhookKind::from_env(Some("DISCORD".into())),
            WebhookKind::Discord
        );
        assert_eq!(WebhookKind::from_env(None), WebhookKind::Generic);
    }
}
