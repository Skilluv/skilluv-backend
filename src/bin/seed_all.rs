//! skilluv-seed-all — apply every seed this database has not had.
//!
//! The same thing the server does after its migrations on every boot, as a
//! command, for the times you want it without a restart: after restoring a
//! dump, after `--forget`, or when `SEED_ADMIN_PASSWORD` was set late.
//!
//! Idempotent and cheap. On a database that is up to date it is one `SELECT`
//! against `seed_runs` and nothing else.
//!
//! Usage:
//!   cargo run --bin skilluv-seed-all
//!   cargo run --bin skilluv-seed-all -- --forget design_canvas
//!   cargo run --bin skilluv-seed-all -- --list
//!
//! Env:
//!   DATABASE_URL           standard sqlx connection string
//!   SEED_ADMIN_PASSWORD    required the first time, >= 12 characters
//!   SEED_ADMIN_EMAIL       default: admin@skill-uv.com

use anyhow::{Context, Result};
use clap::Parser;
use sqlx::PgPool;

use skilluv_backend::services::seed;

#[derive(Parser, Debug)]
#[command(
    name = "skilluv-seed-all",
    about = "Apply every seed step this database has not had (idempotent)"
)]
struct Cli {
    /// Forget one step so it is applied again. Repeatable.
    #[arg(long)]
    forget: Vec<String>,

    /// Forget every step. The next run applies the whole catalogue; every step
    /// is idempotent, so this rewrites rather than duplicates.
    #[arg(long)]
    forget_all: bool,

    /// Print the step names and exit.
    #[arg(long)]
    list: bool,
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

    if cli.list {
        for name in seed::step_names() {
            println!("{name}");
        }
        return Ok(());
    }

    let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL is required")?;
    let db = PgPool::connect(&database_url)
        .await
        .context("failed to connect to Postgres")?;

    if cli.forget_all {
        for name in seed::step_names() {
            seed::forget(&db, name).await?;
        }
        println!("Every step forgotten; all of them will be applied below.");
    }

    for name in &cli.forget {
        // Checked against the catalogue rather than silently accepted: a typo
        // would otherwise delete nothing and report that it had.
        if !seed::step_names().contains(&name.as_str()) {
            anyhow::bail!("no seed step called {name}. Run with --list to see the ten there are.");
        }
        let existed = seed::forget(&db, name).await?;
        println!(
            "{name}: {}",
            if existed {
                "forgotten, will be applied below"
            } else {
                "was not in the ledger, will be applied below"
            }
        );
    }

    let report = seed::run(&db).await?;

    println!();
    println!("═══════════════════════════════════════════════════════════");
    println!(
        "  Seed complete — {} applied, {} already up to date",
        report.applied, report.skipped
    );
    println!("═══════════════════════════════════════════════════════════");
    for step in &report.steps {
        println!(
            "  {} {:<28} {}",
            if step.ran { "•" } else { " " },
            step.name,
            step.detail
        );
    }
    println!("═══════════════════════════════════════════════════════════");

    if report.blocked_on_owner {
        println!();
        println!("  Some steps were skipped: this database has no administrator.");
        println!("  Set SEED_ADMIN_PASSWORD (12+ characters) and run this again;");
        println!("  nothing already applied is repeated.");
        // A non-zero exit, so a deployment pipeline notices. A partially
        // seeded database that reports success is the failure this whole
        // module was written to end.
        std::process::exit(1);
    }

    Ok(())
}
