//! skilluv-timeline-backfill — rebuild `user_timeline_events` from the
//! source tables (SKI-39).
//!
//! Migration 0142 backfills once at deploy time and installs triggers that
//! keep the timeline current from then on. This binary exists for the cases
//! a migration cannot cover:
//!
//! - a restore run with `--disable-triggers`, which skips row triggers;
//! - triggers temporarily dropped during maintenance;
//! - verifying, on demand, that the timeline is complete.
//!
//! Every insert is `ON CONFLICT DO NOTHING`, so this is idempotent and safe
//! against a live database. A clean run reports `rows_inserted: 0`.
//!
//! Usage:
//!   cargo run --bin skilluv-timeline-backfill              # all users
//!   cargo run --bin skilluv-timeline-backfill -- <user_id> # one user
//!
//! Env:
//!   DATABASE_URL   required (postgres://…)

use anyhow::{Context, Result};
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

use skilluv_backend::services::timeline;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "skilluv_backend=info,skilluv_timeline_backfill=info".into()),
        )
        .init();

    // Optional positional arg: a single user id to scope the rebuild to.
    let only_user = match std::env::args().nth(1) {
        Some(raw) => Some(
            raw.parse::<Uuid>()
                .with_context(|| format!("'{raw}' is not a valid user UUID"))?,
        ),
        None => None,
    };

    let db_url = std::env::var("DATABASE_URL").context("DATABASE_URL is required")?;
    let db = PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await
        .context("failed to connect to database")?;

    let started = std::time::Instant::now();
    let report = timeline::backfill(&db, only_user)
        .await
        .context("timeline backfill failed")?;

    tracing::info!(
        scope = %only_user.map(|u| u.to_string()).unwrap_or_else(|| "all-users".into()),
        rows_inserted = report.total(),
        elapsed_ms = started.elapsed().as_millis() as u64,
        "timeline backfill completed"
    );

    let summary = serde_json::json!({
        "scope": only_user.map(|u| u.to_string()).unwrap_or_else(|| "all-users".into()),
        "rows_inserted": report.total(),
        "detail": report,
        "elapsed_ms": started.elapsed().as_millis() as u64,
    });
    println!("{}", serde_json::to_string_pretty(&summary)?);

    db.close().await;
    Ok(())
}
