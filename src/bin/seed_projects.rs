//! skilluv-seed-projects — provision the 4 Skilluv public repos as Projects
//! in the DB, wired to the P11 GitHubIngestor via label `skilluv-challenge`.
//!
//! Phase 1 dogfooding (P26 v2 SKI-74): these 4 rows are what makes the P11
//! ingestor pick up issues from our own repos and materialise them as slices.
//!
//! Idempotent: uses `INSERT ... ON CONFLICT (slug) DO UPDATE` to refresh
//! `curated_labels` and `slice_ingestion_mode` on re-runs. Safe from CI /
//! Coolify post-deploy / manual command.
//!
//! Requires the admin user (see `bin/seed_admin.rs`) to already exist —
//! the projects are owned by that user. If the admin isn't found, the
//! binary fails with a clear error pointing to seed_admin.
//!
//! Usage:
//!   cargo run --bin skilluv-seed-projects
//!       # uses SEED_ADMIN_EMAIL or defaults to admin@skill-uv.com
//!   cargo run --bin skilluv-seed-projects -- --owner-email admin@example.com
//!
//! Env vars (used only when the matching CLI arg is missing):
//!   SEED_ADMIN_EMAIL       default: admin@skill-uv.com
//!   DATABASE_URL           standard sqlx connection string

use anyhow::{Context, Result};
use clap::Parser;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Parser, Debug)]
#[command(
    name = "skilluv-seed-projects",
    about = "Seed the 4 Skilluv public repos as Projects (idempotent)"
)]
struct Cli {
    /// Email of the admin user that will own the seeded projects.
    /// Defaults to SEED_ADMIN_EMAIL env, then to admin@skill-uv.com.
    #[arg(long)]
    owner_email: Option<String>,
}

/// A Skilluv-owned public repo to seed as a Project.
struct SkilluvRepo {
    slug: &'static str,
    name: &'static str,
    description: &'static str,
    github_owner: &'static str,
    github_repo: &'static str,
    skill_domains: &'static [&'static str],
    tech_stack: &'static [&'static str],
}

/// The 4 repos that make up Phase 1 dogfooding — see docs/DOGFOODING-PHASE-1.md.
/// Extending this list means adding repos to the ingestion pool; the binary is
/// idempotent so re-runs pick up the new ones without touching existing rows.
const REPOS: &[SkilluvRepo] = &[
    SkilluvRepo {
        slug: "skilluv-backend",
        name: "Skilluv Backend",
        description: "Rust/axum backend powering the Skilluv platform.",
        github_owner: "skilluv",
        github_repo: "skilluv-backend",
        skill_domains: &["code", "ops"],
        tech_stack: &["rust", "axum", "postgres", "sqlx"],
    },
    SkilluvRepo {
        slug: "skilluv-frontend",
        name: "Skilluv Frontend",
        description: "SvelteKit frontend for the Skilluv user experience.",
        github_owner: "skilluv",
        github_repo: "skilluv-frontend",
        skill_domains: &["code", "design"],
        tech_stack: &["typescript", "sveltekit", "tailwind"],
    },
    SkilluvRepo {
        slug: "skilluv-admin",
        name: "Skilluv Admin Panel",
        description: "SvelteKit admin panel (moderation, ops, analytics).",
        github_owner: "skilluv",
        github_repo: "skilluv-admin",
        skill_domains: &["code", "ops"],
        tech_stack: &["typescript", "sveltekit"],
    },
    SkilluvRepo {
        slug: "skilluv-ia",
        name: "Skilluv IA",
        description: "AI/ML services (verifier, coach, embeddings).",
        github_owner: "skilluv",
        github_repo: "skilluv-ia",
        skill_domains: &["ai", "code"],
        tech_stack: &["python", "fastapi", "grpc"],
    },
];

/// Curated labels the P11 GitHubIngestor will filter on. Only issues carrying
/// one of these labels are ingested as slices. `skilluv-challenge` is added by
/// the Linear→GitHub bot (SKI-72) when a Linear ticket is tagged
/// `challenge-ready`.
const CURATED_LABELS: &[&str] = &["skilluv-challenge"];

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
        .unwrap_or_else(|| "admin@skill-uv.com".to_string())
        .to_lowercase();

    let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL is required")?;
    let db = PgPool::connect(&database_url)
        .await
        .context("failed to connect to Postgres")?;

    let owner_id: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM users WHERE email = $1 AND role = 'admin'")
            .bind(&owner_email)
            .fetch_optional(&db)
            .await
            .context("failed to look up admin user")?;

    let owner_id = owner_id
        .ok_or_else(|| {
            anyhow::anyhow!(
                "admin user {owner_email} not found. Run `cargo run --bin skilluv-seed-admin` \
                 first (with SEED_ADMIN_PASSWORD set)."
            )
        })?
        .0;

    let curated_labels: Vec<String> = CURATED_LABELS.iter().map(|s| s.to_string()).collect();
    let mut created = 0usize;
    let mut updated = 0usize;

    for repo in REPOS {
        let skill_domains: Vec<String> = repo.skill_domains.iter().map(|s| s.to_string()).collect();
        let tech_stack: Vec<String> = repo.tech_stack.iter().map(|s| s.to_string()).collect();
        let repo_url = format!(
            "https://github.com/{}/{}",
            repo.github_owner, repo.github_repo
        );

        let row: (bool,) = sqlx::query_as(
            r#"
            INSERT INTO projects
                (slug, name, description, repo_url, tech_stack, is_oss,
                 looking_for_contributors, owner_type, owner_id, curated_by_admin,
                 skill_domains, lifecycle_status,
                 github_repo_owner, github_repo_name,
                 slice_ingestion_mode, curated_labels)
            VALUES ($1, $2, $3, $4, $5, TRUE,
                    TRUE, 'user', $6, TRUE,
                    $7, 'active',
                    $8, $9,
                    'auto', $10)
            ON CONFLICT (slug) DO UPDATE SET
                name = EXCLUDED.name,
                description = EXCLUDED.description,
                repo_url = EXCLUDED.repo_url,
                tech_stack = EXCLUDED.tech_stack,
                looking_for_contributors = TRUE,
                curated_by_admin = TRUE,
                skill_domains = EXCLUDED.skill_domains,
                github_repo_owner = EXCLUDED.github_repo_owner,
                github_repo_name = EXCLUDED.github_repo_name,
                slice_ingestion_mode = EXCLUDED.slice_ingestion_mode,
                curated_labels = EXCLUDED.curated_labels,
                updated_at = NOW()
            RETURNING (xmax = 0) AS inserted
            "#,
        )
        .bind(repo.slug)
        .bind(repo.name)
        .bind(repo.description)
        .bind(&repo_url)
        .bind(&tech_stack)
        .bind(owner_id)
        .bind(&skill_domains)
        .bind(repo.github_owner)
        .bind(repo.github_repo)
        .bind(&curated_labels)
        .fetch_one(&db)
        .await
        .with_context(|| format!("failed to upsert project {}", repo.slug))?;

        if row.0 {
            created += 1;
            tracing::info!(slug = repo.slug, "project created");
        } else {
            updated += 1;
            tracing::info!(slug = repo.slug, "project updated");
        }
    }

    println!();
    println!("═══════════════════════════════════════════════════════════");
    println!("  Skilluv public repos seeded ({created} created, {updated} updated)");
    println!("═══════════════════════════════════════════════════════════");
    for repo in REPOS {
        println!(
            "  • {} → github.com/{}/{}",
            repo.slug, repo.github_owner, repo.github_repo
        );
    }
    println!();
    println!(
        "  Curated labels (P11 ingest filter): {}",
        curated_labels.join(", ")
    );
    println!("  Ingestion mode: auto (issues appear directly as 'open' slices)");
    println!("  Owner: {owner_email}");
    println!("═══════════════════════════════════════════════════════════");

    Ok(())
}
