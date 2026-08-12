//! skilluv-seed-guild — provision a guild whose founder is a given account.
//!
//! SKI-289. Founding a guild through the API requires exactly three
//! co-founders, which makes owner-side flows (Applications / Invitations
//! tabs, invitation revocation) impossible to exercise from a single test
//! account. This binary writes the rows directly so an e2e run has a guild
//! it actually owns.
//!
//! Idempotent: re-running upserts the guild and re-asserts the founder's
//! membership, so it is safe from `docker compose up`, CI provisioning, or
//! a one-off command.
//!
//! The three-co-founder rule is a product rule enforced by
//! `services::guild::create_guild`, not a database constraint. Seeding
//! around it is deliberate and limited to non-production environments —
//! the binary refuses to run against a database whose URL does not look
//! local unless `SEED_GUILD_ALLOW_REMOTE=1` is set.
//!
//! Usage:
//!   cargo run --bin skilluv-seed-guild
//!   cargo run --bin skilluv-seed-guild -- --email e2e@skill-uv.com --slug e2e-guild
//!
//! Env vars (used only when the matching CLI arg is missing):
//!   SEED_GUILD_FOUNDER_EMAIL  founder's email; falls back to E2E_USER_EMAIL
//!   E2E_USER_EMAIL            the account the front-end e2e suite logs in as
//!   SEED_GUILD_SLUG           default: e2e-guild
//!   SEED_GUILD_TAG            default: E2EG   (3-5 chars, uppercase)
//!   SEED_GUILD_NAME           default: E2E Test Guild
//!   SEED_GUILD_ALLOW_REMOTE   set to 1 to allow a non-local DATABASE_URL
//!   DATABASE_URL              standard sqlx connection string

use anyhow::{Context, Result, bail};
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

fn arg(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

/// Refuse to seed anything that does not look like a local or explicitly
/// allowed database. Seeding bypasses a product rule; doing that to
/// production by accident is not a recoverable mistake.
fn assert_seedable(db_url: &str) -> Result<()> {
    if std::env::var("SEED_GUILD_ALLOW_REMOTE").as_deref() == Ok("1") {
        return Ok(());
    }
    let local = db_url.contains("@localhost")
        || db_url.contains("@127.0.0.1")
        || db_url.contains("@postgres")
        || db_url.contains("@db:");
    if !local {
        bail!(
            "DATABASE_URL does not look local. This binary bypasses the \
             three-co-founder rule and is meant for dev/CI only. Set \
             SEED_GUILD_ALLOW_REMOTE=1 if you really mean it."
        );
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "skilluv_backend=info,skilluv_seed_guild=info".into()),
        )
        .init();

    let email = arg("--email")
        .or_else(|| std::env::var("SEED_GUILD_FOUNDER_EMAIL").ok())
        .or_else(|| std::env::var("E2E_USER_EMAIL").ok())
        .context(
            "founder email is required: pass --email, or set \
             SEED_GUILD_FOUNDER_EMAIL / E2E_USER_EMAIL",
        )?;
    let slug = arg("--slug")
        .or_else(|| std::env::var("SEED_GUILD_SLUG").ok())
        .unwrap_or_else(|| "e2e-guild".to_string());
    let tag = arg("--tag")
        .or_else(|| std::env::var("SEED_GUILD_TAG").ok())
        .unwrap_or_else(|| "E2EG".to_string())
        .to_uppercase();
    let name = arg("--name")
        .or_else(|| std::env::var("SEED_GUILD_NAME").ok())
        .unwrap_or_else(|| "E2E Test Guild".to_string());

    // Mirror the CHECK constraints so a bad value fails here with a clear
    // message rather than as a database error.
    if !(3..=5).contains(&tag.len()) {
        bail!("tag must be 3 to 5 characters, got {:?}", tag);
    }
    if !(3..=60).contains(&name.len()) {
        bail!("name must be 3 to 60 characters");
    }

    let db_url = std::env::var("DATABASE_URL").context("DATABASE_URL is required")?;
    assert_seedable(&db_url)?;

    let db = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db_url)
        .await
        .context("failed to connect to database")?;

    let founder_id: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM users WHERE LOWER(email) = LOWER($1)")
            .bind(&email)
            .fetch_optional(&db)
            .await?;
    let founder_id =
        founder_id.with_context(|| format!("no user with email {email} — register it first"))?;

    let mut tx = db.begin().await?;

    // Upsert on slug: re-running must not fail on the unique constraint,
    // and must not orphan a guild whose founder changed.
    let guild_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO guilds (slug, tag, name, description, founder_id, membership_mode)
        VALUES ($1, $2, $3, 'Seeded guild for end-to-end tests.', $4, 'application')
        ON CONFLICT (slug) DO UPDATE SET
            tag         = EXCLUDED.tag,
            name        = EXCLUDED.name,
            founder_id  = EXCLUDED.founder_id,
            disbanded_at = NULL,
            updated_at  = NOW()
        RETURNING id
        "#,
    )
    .bind(&slug)
    .bind(&tag)
    .bind(&name)
    .bind(founder_id)
    .fetch_one(&mut *tx)
    .await
    .context("guild upsert failed — is the tag already taken by another guild?")?;

    sqlx::query(
        r#"
        INSERT INTO guild_members (guild_id, user_id, role)
        VALUES ($1, $2, 'founder')
        ON CONFLICT (guild_id, user_id) DO UPDATE SET role = 'founder'
        "#,
    )
    .bind(guild_id)
    .bind(founder_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    tracing::info!(%guild_id, %slug, %email, "guild seeded with founder");
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "guild_id": guild_id,
            "slug": slug,
            "tag": tag,
            "founder_email": email,
            "founder_id": founder_id,
        }))?
    );

    db.close().await;
    Ok(())
}
