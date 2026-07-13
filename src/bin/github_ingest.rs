//! skilluv-github-ingest — Poll les issues GitHub avec labels curés et
//! matérialise chaque nouvelle issue en `project_slice`.
//!
//! Design (P11.1) :
//! - Se connecte à la DB via `DATABASE_URL`.
//! - Appelle `services::slice_ingestion::poll_all_github_projects` — parcourt
//!   tous les projets avec `slice_ingestion_mode IN ('auto','curator_review')`.
//! - Sort avec un rapport agrégé (JSON) sur stdout — utilisable en cron.
//! - Le mode `auto` publie direct (status='open') ; `curator_review` crée en
//!   draft, attente validation steward (endpoint P11.4).
//!
//! Usage :
//!   # One-shot (cron hourly recommandé)
//!   cargo run --bin skilluv-github-ingest
//!
//! Env :
//!   DATABASE_URL   requis (postgres://…)
//!   GITHUB_TOKEN   optionnel — augmente le rate-limit (5000/h vs 60/h anonyme)

use anyhow::{Context, Result};
use sqlx::postgres::PgPoolOptions;

use skilluv_backend::services::slice_ingestion::poll_all_github_projects;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "skilluv_backend=info,skilluv_github_ingest=info".into()),
        )
        .init();

    let db_url = std::env::var("DATABASE_URL")
        .context("DATABASE_URL is required")?;
    let db = PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await
        .context("failed to connect to database")?;

    let started = std::time::Instant::now();
    let reports = poll_all_github_projects(&db)
        .await
        .context("poll_all_github_projects failed")?;

    let projects = reports.len();
    let created: u32 = reports.iter().map(|r| r.slices_created).sum();
    let duplicates: u32 = reports.iter().map(|r| r.slices_skipped_duplicate).sum();
    let errors: u32 = reports.iter().map(|r| r.errors).sum();

    tracing::info!(
        projects_polled = projects,
        slices_created = created,
        slices_skipped_duplicate = duplicates,
        errors,
        elapsed_ms = started.elapsed().as_millis() as u64,
        "github ingestion cycle completed"
    );

    // Rapport JSON sur stdout — utilisable par un wrapper de cron pour alerting.
    let summary = serde_json::json!({
        "projects_polled": projects,
        "slices_created": created,
        "slices_skipped_duplicate": duplicates,
        "errors": errors,
        "elapsed_ms": started.elapsed().as_millis() as u64,
        "reports": reports,
    });
    println!("{}", serde_json::to_string_pretty(&summary)?);

    db.close().await;
    Ok(())
}
