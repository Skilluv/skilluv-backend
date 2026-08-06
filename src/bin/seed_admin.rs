//! skilluv-seed-admin — provision or reset a Skilluv admin account.
//!
//! Idempotent: if the target email already exists we UPDATE it (fresh password
//! hash, role forced to 'admin', email_verified forced to true) rather than
//! erroring on the unique constraint. Safe to re-run from `docker compose up`,
//! CI provisioning, or a one-off manual command.
//!
//! Password is MANDATORY (via CLI arg or SEED_ADMIN_PASSWORD env). Minimum
//! 12 characters — the binary refuses to run without one. No auto-generated
//! passwords: an operator must consciously choose a secret.
//!
//! Usage:
//!   SEED_ADMIN_PASSWORD='S3cure!Pass123' cargo run --bin skilluv-seed-admin
//!   cargo run --bin skilluv-seed-admin -- \
//!       --email admin@skill-uv.com --password 'S3cure!Pass123'
//!
//! Env vars (used only when the matching CLI arg is missing):
//!   SEED_ADMIN_EMAIL       default: admin@skill-uv.com
//!   SEED_ADMIN_PASSWORD    REQUIRED — no default (must be ≥12 chars)
//!   SEED_ADMIN_USERNAME    default: admin
//!   SEED_ADMIN_FIRST_NAME  default: Admin
//!   SEED_ADMIN_LAST_NAME   default: Skilluv
//!   DATABASE_URL           standard sqlx connection string
//!
//! After upsert, `recompute_capabilities_for_user` is called defensively to
//! ensure the account has the correct capability set derived from its role /
//! rank / activity (baseline for a fresh admin account).

use anyhow::{Context, Result};
use clap::Parser;
use sqlx::PgPool;
use uuid::Uuid;

use skilluv_backend::services::{AuthService, capabilities_engine};

#[derive(Parser, Debug)]
#[command(
    name = "skilluv-seed-admin",
    about = "Provision or reset a Skilluv admin account (idempotent)"
)]
struct Cli {
    #[arg(long)]
    email: Option<String>,

    #[arg(long)]
    password: Option<String>,

    #[arg(long)]
    username: Option<String>,

    #[arg(long)]
    first_name: Option<String>,

    #[arg(long)]
    last_name: Option<String>,
}

const MIN_PASSWORD_LEN: usize = 12;

fn resolve(cli: Option<String>, env_name: &str, fallback: &str) -> String {
    cli.or_else(|| std::env::var(env_name).ok())
        .unwrap_or_else(|| fallback.to_string())
}

fn resolve_password_or_fail(cli_password: Option<String>) -> Result<String> {
    let password = cli_password.or_else(|| std::env::var("SEED_ADMIN_PASSWORD").ok());
    match password {
        None => anyhow::bail!(
            "SEED_ADMIN_PASSWORD is required. Provide it via --password CLI arg \
             or SEED_ADMIN_PASSWORD env var. Minimum {MIN_PASSWORD_LEN} characters."
        ),
        Some(p) if p.chars().count() < MIN_PASSWORD_LEN => anyhow::bail!(
            "SEED_ADMIN_PASSWORD too short: {} chars, minimum {MIN_PASSWORD_LEN} required.",
            p.chars().count()
        ),
        Some(p) => Ok(p),
    }
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

    let email = resolve(cli.email, "SEED_ADMIN_EMAIL", "admin@skill-uv.com").to_lowercase();
    let username = resolve(cli.username, "SEED_ADMIN_USERNAME", "admin").to_lowercase();
    let first_name = resolve(cli.first_name, "SEED_ADMIN_FIRST_NAME", "Admin");
    let last_name = resolve(cli.last_name, "SEED_ADMIN_LAST_NAME", "Skilluv");

    // Password is MANDATORY — no auto-generation. Force operators to
    // consciously choose a secret they will store.
    let password = resolve_password_or_fail(cli.password)?;

    let display_name = format!("{} {}", first_name.trim(), last_name.trim());
    let password_hash = AuthService::hash_password(&password)
        .map_err(|e| anyhow::anyhow!("hash_password failed: {e}"))?;

    let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL is required")?;
    let db = PgPool::connect(&database_url)
        .await
        .context("failed to connect to Postgres")?;

    // UPSERT by email: existing row → rotate password, force role=admin +
    // email_verified. Fresh row → full insert.
    let row: (Uuid, bool) = sqlx::query_as(
        r#"
        INSERT INTO users
            (email, username, password_hash, first_name, last_name, display_name,
             role, email_verified, terms_accepted_at, password_changed_at)
        VALUES ($1, $2, $3, $4, $5, $6, 'admin', TRUE, NOW(), NOW())
        ON CONFLICT (email) DO UPDATE SET
            password_hash = EXCLUDED.password_hash,
            role = 'admin',
            email_verified = TRUE,
            first_name = EXCLUDED.first_name,
            last_name = EXCLUDED.last_name,
            display_name = EXCLUDED.display_name,
            password_changed_at = NOW(),
            updated_at = NOW()
        RETURNING id, (xmax = 0) AS inserted
        "#,
    )
    .bind(&email)
    .bind(&username)
    .bind(&password_hash)
    .bind(first_name.trim())
    .bind(last_name.trim())
    .bind(&display_name)
    .fetch_one(&db)
    .await
    .context("failed to upsert admin user")?;

    let (user_id, inserted) = row;
    tracing::info!(
        %user_id,
        %email,
        action = if inserted { "created" } else { "updated" },
        "admin account seeded"
    );

    // Ensure derived capabilities are recomputed post-upsert. Defensive: for a
    // fresh admin account this typically results in the baseline capability
    // set (role=admin drives access via AdminGate middleware; caps engine adds
    // any rank/activity-derived caps that a re-run of the seed should refresh).
    match capabilities_engine::recompute_capabilities_for_user(&db, user_id).await {
        Ok(report) => tracing::info!(
            %user_id,
            granted = ?report.granted,
            already_active = ?report.already_active,
            "capabilities recomputed"
        ),
        Err(e) => tracing::warn!(
            %user_id,
            error = %e,
            "recompute_capabilities_for_user failed — role=admin still applied via UPSERT, panel remains accessible"
        ),
    }

    println!();
    println!("═══════════════════════════════════════════════════════════");
    println!(
        "  Admin account {} successfully",
        if inserted { "CREATED" } else { "UPDATED" }
    );
    println!("═══════════════════════════════════════════════════════════");
    println!("  Email:    {email}");
    println!("  Username: {username}");
    println!("  Password: (provided by caller — not echoed)");
    println!("═══════════════════════════════════════════════════════════");

    Ok(())
}
