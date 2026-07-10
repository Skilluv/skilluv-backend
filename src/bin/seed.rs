//! skilluv-seed — populate a database with deterministic fake data for staging / dev.
//!
//! Idempotent : safe to run multiple times. Records flagged with `email` ending in
//! `@seed.skilluv.local` are reserved for seed.
//!
//! Usage:
//!   skilluv-seed                  # defaults: 20 users, 8 challenges, 3 submissions/user
//!   skilluv-seed --users 50 --challenges 30
//!   skilluv-seed --wipe           # delete previously seeded data first

use anyhow::{Context, Result};
use clap::Parser;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

const SEED_EMAIL_DOMAIN: &str = "@seed.skilluv.local";
const DOMAINS: &[&str] = &["code", "design", "game", "security"];

#[derive(Parser, Debug)]
#[command(name = "skilluv-seed", about = "Populate staging/dev DB with fake data")]
struct Cli {
    /// Number of fake users to create
    #[arg(long, default_value_t = 20)]
    users: usize,

    /// Number of fake challenges to create
    #[arg(long, default_value_t = 8)]
    challenges: usize,

    /// Number of submissions per user
    #[arg(long, default_value_t = 3)]
    submissions_per_user: usize,

    /// Delete existing seed data first
    #[arg(long, default_value_t = false)]
    wipe: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "info".into()))
        .compact()
        .init();

    let cli = Cli::parse();

    let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL not set")?;
    let db = PgPool::connect(&database_url)
        .await
        .context("failed to connect to PostgreSQL")?;

    if cli.wipe {
        wipe_seed_data(&db).await?;
    }

    let report = run_seed(&db, &cli).await?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

async fn wipe_seed_data(db: &PgPool) -> Result<()> {
    tracing::info!("wiping previously seeded data");
    let pattern = format!("%{SEED_EMAIL_DOMAIN}");
    // submissions/user_skills are cascaded via foreign keys
    let res = sqlx::query("DELETE FROM users WHERE email LIKE $1")
        .bind(&pattern)
        .execute(db)
        .await?;
    tracing::info!(rows = res.rows_affected(), "seed users deleted");
    sqlx::query("DELETE FROM challenges WHERE slug LIKE 'seed-%'")
        .execute(db)
        .await?;
    Ok(())
}

#[derive(serde::Serialize)]
struct SeedReport {
    users_created: usize,
    users_skipped: usize,
    challenges_created: usize,
    challenges_skipped: usize,
    submissions_created: usize,
}

async fn run_seed(db: &PgPool, cli: &Cli) -> Result<SeedReport> {
    let mut report = SeedReport {
        users_created: 0,
        users_skipped: 0,
        challenges_created: 0,
        challenges_skipped: 0,
        submissions_created: 0,
    };

    let user_ids = seed_users(db, cli.users, &mut report).await?;
    let challenge_ids = seed_challenges(db, cli.challenges, &mut report).await?;

    if !user_ids.is_empty() && !challenge_ids.is_empty() {
        report.submissions_created = seed_submissions(
            db,
            &user_ids,
            &challenge_ids,
            cli.submissions_per_user,
        )
        .await?;
    }

    Ok(report)
}

async fn seed_users(
    db: &PgPool,
    count: usize,
    report: &mut SeedReport,
) -> Result<Vec<Uuid>> {
    let mut ids = Vec::with_capacity(count);
    for i in 0..count {
        let username = format!("seed_user_{i:04}");
        let email = format!("{username}{SEED_EMAIL_DOMAIN}");
        let domain = DOMAINS[i % DOMAINS.len()];
        let total_fragments = (i as i32) * 137 % 6000;
        let title = match total_fragments {
            0..=499 => "apprenti",
            500..=1999 => "artisan",
            2000..=4999 => "maitre",
            _ => "legende",
        };
        let golden_stars = if title == "legende" {
            (total_fragments - 5000).max(0) / 100
        } else {
            0
        };

        let existing: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM users WHERE email = $1")
            .bind(&email)
            .fetch_optional(db)
            .await?;
        if let Some((id,)) = existing {
            ids.push(id);
            report.users_skipped += 1;
            continue;
        }

        let inserted: (Uuid,) = sqlx::query_as(
            r#"
            INSERT INTO users (
                email, username, password_hash, first_name, last_name, display_name,
                skill_domain, role, title, golden_stars, total_fragments,
                profile_active, email_verified
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, 'user', $8, $9, $10, TRUE, TRUE)
            RETURNING id
            "#,
        )
        .bind(&email)
        .bind(&username)
        .bind("$argon2id$v=19$m=19456,t=2,p=1$seed-placeholder$seed-placeholder")
        .bind(format!("Seed{i}"))
        .bind("User")
        .bind(format!("Seed User {i}"))
        .bind(domain)
        .bind(title)
        .bind(golden_stars)
        .bind(total_fragments)
        .fetch_one(db)
        .await?;
        ids.push(inserted.0);
        report.users_created += 1;
    }
    Ok(ids)
}

async fn seed_challenges(
    db: &PgPool,
    count: usize,
    report: &mut SeedReport,
) -> Result<Vec<Uuid>> {
    let mut ids = Vec::with_capacity(count);
    for i in 0..count {
        let slug = format!("seed-challenge-{i:03}");
        let domain = DOMAINS[i % DOMAINS.len()];
        let difficulty: i16 = (i % 5) as i16 + 1;
        let reward_fragments = 50 + (i as i32) * 10;

        let existing: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM challenges WHERE slug = $1")
            .bind(&slug)
            .fetch_optional(db)
            .await?;
        if let Some((id,)) = existing {
            ids.push(id);
            report.challenges_skipped += 1;
            continue;
        }

        // P8.3 : prerequisite_fragments retiré, la progression est gérée via
        // challenge_prerequisites (DAG) + tracks. Les seeds restent sans DAG
        // pour l'instant — à populer manuellement par un admin si besoin.
        let inserted: (Uuid,) = sqlx::query_as(
            r#"
            INSERT INTO challenges (
                slug, title, description, instructions, skill_domain,
                difficulty, reward_fragments,
                expected_output, language, status, is_onboarding
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'published', FALSE)
            RETURNING id
            "#,
        )
        .bind(&slug)
        .bind(format!("Seed Challenge #{i} ({domain})"))
        .bind(format!("Seed challenge for staging in domain {domain}."))
        .bind("Print 'Hello, Skilluv!' to stdout.")
        .bind(domain)
        .bind(difficulty)
        .bind(reward_fragments)
        .bind("Hello, Skilluv!")
        .bind(if domain == "code" { Some("python") } else { None })
        .fetch_one(db)
        .await?;
        ids.push(inserted.0);
        report.challenges_created += 1;
    }
    Ok(ids)
}

async fn seed_submissions(
    db: &PgPool,
    user_ids: &[Uuid],
    challenge_ids: &[Uuid],
    per_user: usize,
) -> Result<usize> {
    let mut created = 0;
    for (uidx, user_id) in user_ids.iter().enumerate() {
        for i in 0..per_user.min(challenge_ids.len()) {
            let challenge_id = challenge_ids[(uidx + i) % challenge_ids.len()];
            let status = if i == 0 { "success" } else { "in_progress" };
            // skip if a submission already exists for this user+challenge+attempt
            let existing: Option<(Uuid,)> = sqlx::query_as(
                "SELECT id FROM challenge_submissions WHERE user_id = $1 AND challenge_id = $2 AND attempt_number = 1",
            )
            .bind(user_id)
            .bind(challenge_id)
            .fetch_optional(db)
            .await?;
            if existing.is_some() {
                continue;
            }
            sqlx::query(
                r#"
                INSERT INTO challenge_submissions (
                    challenge_id, user_id, attempt_number, status, code, language,
                    fragments_earned, stdout, submitted_at, evaluated_at
                )
                VALUES ($1, $2, 1, $3, 'print("Hello, Skilluv!")', 'python', $4, $5,
                        NOW() - INTERVAL '1 day' * $6, NOW() - INTERVAL '1 day' * $6)
                "#,
            )
            .bind(challenge_id)
            .bind(user_id)
            .bind(status)
            .bind(if status == "success" { 50 } else { 0 })
            .bind(json!({"stdout": "Hello, Skilluv!"}).to_string())
            .bind((uidx as i32) % 30)
            .execute(db)
            .await?;
            created += 1;
        }
    }
    Ok(created)
}
