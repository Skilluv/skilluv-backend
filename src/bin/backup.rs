//! skilluv-backup — Postgres backup management for Skilluv.
//!
//! Run via cron / systemd timer / Docker scheduler. See
//! `docs/runbooks/backup-restore.md` for operational details.

use anyhow::Result;
use clap::{Parser, Subcommand};
use skilluv_backend::services::backup::{self, BackupConfig, NotifyLevel};
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(
    name = "skilluv-backup",
    version,
    about = "Backup / restore tooling for Skilluv"
)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Run a full pg_dump backup and upload it to R2.
    Backup,
    /// List backups currently present in remote storage.
    List,
    /// Apply the retention policy (delete old backups).
    Prune,
    /// Download the latest backup, restore it to an ephemeral DB, and run integrity checks.
    RestoreTest,
    /// Restore a specific backup to a target database (manual operation).
    Restore {
        #[arg(long)]
        backup_key: String,
        #[arg(long)]
        target_db: String,
    },
    /// Mirror MinIO source bucket (avatars) to R2 incrementally.
    MinioMirror,
    /// SKI-29 drill: run a fresh backup, then immediately restore-test it,
    /// and produce a signed JSON report ready to archive as evidence.
    /// Meant to be run monthly (cron / manual) to prove the backup chain
    /// is actually restorable — a backup nobody has ever restored is not
    /// a backup.
    DrillReport,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .compact()
        .init();

    let cli = Cli::parse();
    let cfg = BackupConfig::from_env()?;

    let result = match &cli.command {
        Cmd::Backup => cmd_backup(&cfg).await,
        Cmd::List => cmd_list(&cfg).await,
        Cmd::Prune => cmd_prune(&cfg).await,
        Cmd::RestoreTest => cmd_restore_test(&cfg).await,
        Cmd::Restore {
            backup_key,
            target_db,
        } => cmd_restore(&cfg, backup_key, target_db).await,
        Cmd::MinioMirror => cmd_minio_mirror(&cfg).await,
        Cmd::DrillReport => cmd_drill_report(&cfg).await,
    };

    if let Err(ref err) = result {
        tracing::error!(error = ?err, "command failed");
        let message = format!("{:?} failed: {err:#}", cli.command);
        let _ = backup::notify(&cfg, NotifyLevel::Failure, &message).await;
        std::process::exit(1);
    }
    Ok(())
}

async fn cmd_backup(cfg: &BackupConfig) -> Result<()> {
    let result = backup::run_backup(cfg).await?;
    let pretty = serde_json::to_string_pretty(&result)?;
    println!("{pretty}");
    let message = format!(
        "backup ok: {} ({:.1} MB in {:.1}s)",
        result.key,
        result.size_bytes as f64 / 1024.0 / 1024.0,
        result.duration_seconds
    );
    backup::notify(cfg, NotifyLevel::Success, &message).await?;
    Ok(())
}

async fn cmd_list(cfg: &BackupConfig) -> Result<()> {
    let entries = backup::list_backups(cfg).await?;
    println!("{}", serde_json::to_string_pretty(&entries)?);
    Ok(())
}

async fn cmd_prune(cfg: &BackupConfig) -> Result<()> {
    let report = backup::apply_retention(cfg).await?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    let message = format!("prune ok: kept {}, deleted {}", report.kept, report.deleted);
    backup::notify(cfg, NotifyLevel::Success, &message).await?;
    Ok(())
}

async fn cmd_restore_test(cfg: &BackupConfig) -> Result<()> {
    let report = backup::restore_test(cfg).await?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    let message = format!(
        "restore-test ok on {} (users={}, challenges={}, submissions={})",
        report.backup_key, report.counts.users, report.counts.challenges, report.counts.submissions
    );
    backup::notify(cfg, NotifyLevel::Success, &message).await?;
    Ok(())
}

async fn cmd_restore(cfg: &BackupConfig, backup_key: &str, target_db: &str) -> Result<()> {
    let report = backup::restore_to(cfg, backup_key, target_db).await?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    let message = format!("manual restore ok: {} → {}", backup_key, target_db);
    backup::notify(cfg, NotifyLevel::Warning, &message).await?;
    Ok(())
}

/// SKI-29 — end-to-end backup drill.
///
/// Runs a fresh backup, then a restore-test immediately after, and
/// serialises both reports into a single JSON that operators archive
/// as proof of the restore chain being alive. Emit a Success/Failure
/// notification via the standard `backup::notify` webhook.
async fn cmd_drill_report(cfg: &BackupConfig) -> Result<()> {
    let started_at = chrono::Utc::now();
    tracing::info!("drill: step 1/2 backup");
    let backup_report = backup::run_backup(cfg).await?;
    tracing::info!(key = %backup_report.key, "drill: step 2/2 restore-test");
    let restore_report = backup::restore_test(cfg).await?;
    let ended_at = chrono::Utc::now();

    let drill_report = serde_json::json!({
        "ran_at": started_at.to_rfc3339(),
        "duration_seconds": (ended_at - started_at).num_seconds(),
        "backup": backup_report,
        "restore_test": restore_report,
        "verdict": "restore_chain_alive",
    });
    let pretty = serde_json::to_string_pretty(&drill_report)?;
    println!("{pretty}");

    let message = format!(
        "backup drill OK: backup {} restored + verified (users={}, challenges={}, submissions={}) — {}s total",
        restore_report.backup_key,
        restore_report.counts.users,
        restore_report.counts.challenges,
        restore_report.counts.submissions,
        (ended_at - started_at).num_seconds(),
    );
    backup::notify(cfg, NotifyLevel::Success, &message).await?;
    Ok(())
}

async fn cmd_minio_mirror(cfg: &BackupConfig) -> Result<()> {
    let report = backup::mirror_minio(cfg).await?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    let level = if report.failed > 0 {
        NotifyLevel::Warning
    } else {
        NotifyLevel::Success
    };
    let message = format!(
        "minio-mirror: {} source / +{} uploaded / {} unchanged / {} failed ({:.1} MB in {:.1}s)",
        report.source_objects,
        report.uploaded,
        report.skipped_unchanged,
        report.failed,
        report.total_bytes_uploaded as f64 / 1024.0 / 1024.0,
        report.duration_seconds
    );
    backup::notify(cfg, level, &message).await?;
    Ok(())
}
