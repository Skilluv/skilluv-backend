//! skilluv-seed-admin — provision or reset a Skilluv admin account.
//!
//! Idempotent: if the target email already exists we UPDATE it (fresh password
//! hash, role forced to 'admin', email_verified forced to true) rather than
//! erroring on the unique constraint. Safe to re-run from `docker compose up`,
//! CI provisioning, or a one-off manual command.
//!
//! Usage:
//!   cargo run --bin skilluv-seed-admin
//!       # falls back to env vars, then to safe dev defaults
//!   cargo run --bin skilluv-seed-admin -- --email admin@example.com \
//!       --password 'S3cure!Pass123' --username admin
//!
//! Env vars (used only when the matching CLI arg is missing):
//!   SEED_ADMIN_EMAIL       default: admin@skilluv.local
//!   SEED_ADMIN_PASSWORD    default: a random 20-char password logged to stdout
//!   SEED_ADMIN_USERNAME    default: admin
//!   SEED_ADMIN_FIRST_NAME  default: Admin
//!   SEED_ADMIN_LAST_NAME   default: Skilluv
//!   DATABASE_URL           standard sqlx connection string
//!
//! The generated password (when none is supplied) is printed to stdout ONCE
//! at the end of the run — save it somewhere safe, we don't store it in
//! plaintext anywhere. Re-running the seed with a different password rotates
//! it in place.

use anyhow::{Context, Result};
use clap::Parser;
use sqlx::PgPool;
use uuid::Uuid;

use skilluv_backend::services::AuthService;

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

fn generate_password() -> String {
    // Compose two UUIDv4 hex bodies (~32 chars each) to source ~40 random
    // chars, then decorate with a leading `!` + an embedded uppercase to
    // satisfy the API's password policy (12+ chars, upper, digit, special).
    // Not cryptographically-audited randomness, but it's plenty for a
    // dev/bootstrap password the user is meant to rotate.
    let a = Uuid::new_v4().simple().to_string();
    let b = Uuid::new_v4().simple().to_string();
    format!("!Sk{a}{b}")
        .chars()
        .take(24)
        .collect()
}

fn resolve(cli: Option<String>, env_name: &str, fallback: &str) -> String {
    cli.or_else(|| std::env::var(env_name).ok())
        .unwrap_or_else(|| fallback.to_string())
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .compact()
        .init();

    let cli = Cli::parse();

    let email = resolve(cli.email, "SEED_ADMIN_EMAIL", "admin@skilluv.local").to_lowercase();
    let username = resolve(cli.username, "SEED_ADMIN_USERNAME", "admin").to_lowercase();
    let first_name = resolve(cli.first_name, "SEED_ADMIN_FIRST_NAME", "Admin");
    let last_name = resolve(cli.last_name, "SEED_ADMIN_LAST_NAME", "Skilluv");

    // Password: CLI > env > freshly-generated. When we generate, we surface
    // the plaintext once at the end — the caller MUST capture it, we never
    // print it a second time.
    let (password, generated) = match cli.password.or_else(|| std::env::var("SEED_ADMIN_PASSWORD").ok()) {
        Some(p) => (p, false),
        None => (generate_password(), true),
    };

    let display_name = format!("{} {}", first_name.trim(), last_name.trim());
    let password_hash = AuthService::hash_password(&password)
        .map_err(|e| anyhow::anyhow!("hash_password failed: {e}"))?;

    let database_url =
        std::env::var("DATABASE_URL").context("DATABASE_URL is required")?;
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

    println!();
    println!("═══════════════════════════════════════════════════════════");
    println!("  Admin account {} successfully", if inserted { "CREATED" } else { "UPDATED" });
    println!("═══════════════════════════════════════════════════════════");
    println!("  Email:    {email}");
    println!("  Username: {username}");
    if generated {
        println!("  Password: {password}");
        println!();
        println!("  ⚠  This password was auto-generated and will not be shown");
        println!("     again. Save it in your password manager NOW.");
    } else {
        println!("  Password: (provided by caller — not echoed)");
    }
    println!("═══════════════════════════════════════════════════════════");

    Ok(())
}
