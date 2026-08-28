//! skilluv-seed-projects — provision the repositories Skilluv draws work from.
//!
//! The catalogue itself lives in `services::seed::projects`, because the server
//! seeds itself on boot and a binary's private constant is not reachable from
//! there. This is the same data applied by hand, for a targeted re-run.
//!
//! Usage:
//!   cargo run --bin skilluv-seed-projects
//!   cargo run --bin skilluv-seed-projects -- --owner-email admin@example.com
//!
//! Env vars (used only when the matching CLI arg is missing):
//!   SEED_ADMIN_EMAIL       default: admin@skill-uv.com
//!   DATABASE_URL           standard sqlx connection string
//!
//! To re-run it as part of the whole catalogue instead:
//!   cargo run --bin skilluv-seed-all -- --forget projects

use anyhow::{Context, Result};
use clap::Parser;
use sqlx::PgPool;
use uuid::Uuid;

use skilluv_backend::services::seed;

#[derive(Parser, Debug)]
#[command(
    name = "skilluv-seed-projects",
    about = "Seed the repositories Skilluv draws work from (idempotent)"
)]
struct Cli {
    /// Email of the admin user that will own the seeded projects.
    /// Defaults to SEED_ADMIN_EMAIL env, then to admin@skill-uv.com.
    #[arg(long)]
    owner_email: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .compact()
        .init();

    let cli = Cli::parse();
    let owner_email = cli
        .owner_email
        .or_else(|| std::env::var("SEED_ADMIN_EMAIL").ok())
        .unwrap_or_else(|| seed::admin_account::DEFAULT_EMAIL.to_string())
        .to_lowercase();

    let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL is required")?;
    let db = PgPool::connect(&database_url)
        .await
        .context("failed to connect to Postgres")?;

    let owner_id: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM users WHERE email = $1 AND role = 'admin'")
            .bind(&owner_email)
            .fetch_optional(&db)
            .await
            .context("failed to look up the admin user")?;

    let owner_id = owner_id.ok_or_else(|| {
        anyhow::anyhow!(
            "admin user {owner_email} not found. Run `cargo run --bin skilluv-seed-admin` \
             first (with SEED_ADMIN_PASSWORD set)."
        )
    })?;

    let detail = seed::projects::run(&db, owner_id).await?;

    println!();
    println!("═══════════════════════════════════════════════════════════");
    println!("  Projects seeded — {detail}");
    println!("  Owner: {owner_email}");
    println!();
    println!("  Ecosystem projects carry no labels on purpose: their issue");
    println!("  volume would bury the partner repositories. Enable one");
    println!("  deliberately when somebody is ready to steward it.");
    println!("═══════════════════════════════════════════════════════════");

    Ok(())
}
