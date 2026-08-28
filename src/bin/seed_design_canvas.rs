//! skilluv-seed-design-canvas — design work on Skilluv's own surfaces.
//!
//! The briefs live in `services::seed::design_canvas`, because the server
//! seeds itself on boot and a binary's private constant is not reachable from
//! there. This is the same data applied by hand, for a targeted re-run.
//!
//! Requires the projects to exist: a brief with no project to land in is an
//! error rather than a skip, because a partially seeded canvas that reports
//! success is the kind of thing somebody discovers a month later.
//!
//! Usage:
//!   cargo run --bin skilluv-seed-design-canvas
//!   cargo run --bin skilluv-seed-design-canvas -- --owner-email admin@example.com
//!
//! To re-run it as part of the whole catalogue instead:
//!   cargo run --bin skilluv-seed-all -- --forget design_canvas

use anyhow::{Context, Result};
use clap::Parser;
use sqlx::PgPool;
use uuid::Uuid;

use skilluv_backend::services::seed;

#[derive(Parser, Debug)]
#[command(
    name = "skilluv-seed-design-canvas",
    about = "Seed the design challenges on Skilluv's own surfaces (idempotent)"
)]
struct Cli {
    /// Email of the admin the seeded slices are attributed to.
    /// Defaults to SEED_ADMIN_EMAIL, then to admin@skill-uv.com.
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

    let detail = seed::design_canvas::run(&db, owner_id).await?;

    println!();
    println!("═══════════════════════════════════════════════════════════");
    println!("  Design canvas seeded — {detail}");
    println!("═══════════════════════════════════════════════════════════");

    Ok(())
}
