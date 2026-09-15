//! skilluv-reset - throw this deployment's database away and build it back.
//!
//! For a staging box you drive through Coolify, where the only shell you have
//! is one inside the container. That shell has no `psql` and no `docker`, so
//! the usual recipe - drop from the host, restart the service, let it migrate
//! on boot - is not available from where you are standing. This is that recipe
//! as a binary that ships in the image.
//!
//! ## What it does
//!
//! Connects to the `postgres` maintenance database with the same credentials,
//! because a database cannot be dropped by a connection that is inside it.
//! Drops, recreates, then applies the migrations and the seed catalogue - the
//! same two calls the server makes on boot, so a reset cannot succeed on
//! something a real deployment would fail on.
//!
//! ## The backend does not need restarting
//!
//! `AppState` holds no data read from the database: a pool, a Redis handle and
//! configuration. `DROP DATABASE ... WITH (FORCE)` kills the pool's
//! connections, sqlx opens new ones, and the name has not changed - so it
//! reconnects to what this rebuilt. A restart is a belt-and-braces choice, not
//! a requirement.
//!
//! Two things do outlive the drop, and neither is the Rust process:
//!
//!   * Redis - sessions, OAuth state, rate-limit counters and the one-hour
//!     open-slices cache. Flushed here unless `--keep-redis`.
//!   * The browser holding a session cookie for a user that no longer exists.
//!     Nothing in this process can reach that; clear the cookies afterwards or
//!     every page will answer 401 and look broken.
//!
//! ## Usage
//!
//!   skilluv-reset            # asks for the database name first
//!   skilluv-reset --yes      # no prompt
//!   skilluv-reset --keep-redis
//!
//! Refuses when `ENVIRONMENT` is a production one. Staging is disposable and
//! resetting it is the point; production is not.

use anyhow::{Context, Result, bail};
use clap::Parser;
use sqlx::{Connection, PgConnection, PgPool};
use std::io::{IsTerminal, Write};

use skilluv_backend::services::seed;

#[derive(Parser, Debug)]
#[command(
    name = "skilluv-reset",
    about = "Drop this deployment's database, migrate it and seed it (staging only)"
)]
struct Cli {
    /// Do not ask for the database name before dropping it.
    #[arg(long)]
    yes: bool,

    /// Leave Redis alone. Sessions and cached feeds from before the reset
    /// survive, which is rarely what you want.
    #[arg(long)]
    keep_redis: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .compact()
        .init();

    let cli = Cli::parse();

    let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL is required")?;
    let environment = std::env::var("ENVIRONMENT").unwrap_or_else(|_| "dev".into());

    // The one refusal. Everything else - staging, a remote host, a database
    // with production-looking data in it - is the operator's call; this is
    // not, because there is no undo and no flag that should make it feel
    // routine.
    match environment.to_lowercase().as_str() {
        "prod" | "production" => {
            bail!(
                "ENVIRONMENT is '{environment}'. This drops a database; staging is \
                 disposable and production is not."
            )
        }
        _ => {}
    }

    // A database cannot be dropped from inside itself, so everything below
    // talks to `postgres` - same host, same credentials, different database.
    let mut target = url::Url::parse(&database_url).context("DATABASE_URL is not a URL")?;
    let db_name = target.path().trim_start_matches('/').to_string();
    if db_name.is_empty() {
        bail!("DATABASE_URL names no database");
    }
    target.set_path("/postgres");
    let maintenance_url = target.to_string();

    let host = target.host_str().unwrap_or("?");
    let redis_note = if cli.keep_redis {
        "left alone (--keep-redis)"
    } else {
        "flushed"
    };
    println!();
    println!("  {:<11} {db_name} on {host}", "database");
    println!("  {:<11} {environment}", "environment");
    println!("  {:<11} {redis_note}", "redis");
    println!();
    println!("  Everything in it is destroyed. There is no undo.");
    println!();

    if !cli.yes {
        if !std::io::stdin().is_terminal() {
            bail!("not a terminal, and --yes was not given; refusing to guess");
        }
        print!("  Type the database name ({db_name}) to go ahead: ");
        std::io::stdout().flush().ok();
        let mut answer = String::new();
        std::io::stdin().read_line(&mut answer)?;
        if answer.trim() != db_name {
            println!("  Nothing was touched.");
            std::process::exit(1);
        }
        println!();
    }

    // ── Drop and recreate ────────────────────────────────────────────
    // `WITH (FORCE)` terminates whatever is still connected - the server's own
    // pool, most of the time. Without it the drop fails on a running
    // deployment, which is every time this is worth running.
    println!("▶ dropping {db_name}");
    let mut admin = PgConnection::connect(&maintenance_url)
        .await
        .context("could not reach the `postgres` maintenance database")?;

    // The name comes from this deployment's own DATABASE_URL, not from a
    // caller, and an identifier cannot be a bind parameter. Quoted so a name
    // with a hyphen in it still parses.
    let quoted = format!("\"{}\"", db_name.replace('"', "\"\""));
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "DROP DATABASE IF EXISTS {quoted} WITH (FORCE)"
    )))
    .execute(&mut admin)
    .await
    .context("DROP DATABASE failed")?;
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE DATABASE {quoted}")))
        .execute(&mut admin)
        .await
        .context("CREATE DATABASE failed")?;
    admin.close().await.ok();
    println!("  recreated, empty");

    // ── Migrations ───────────────────────────────────────────────────
    // The same migrator the server runs on boot, embedded at compile time, so
    // this works in an image that carries no migration files of its own.
    println!("▶ migrations");
    let db = PgPool::connect(&database_url)
        .await
        .context("could not connect to the database just created")?;
    sqlx::migrate!("./migrations")
        .run(&db)
        .await
        .context("a migration failed")?;
    println!("  applied");

    // ── Seeds ────────────────────────────────────────────────────────
    println!("▶ seeds");
    let report = seed::run(&db).await.context("the seed catalogue failed")?;
    for step in &report.steps {
        println!(
            "  {} {:<28} {}",
            if step.ran { "•" } else { " " },
            step.name,
            step.detail
        );
    }
    println!(
        "  {} applied, {} already up to date",
        report.applied, report.skipped
    );

    // ── Redis ────────────────────────────────────────────────────────
    // A fresh database read through the previous run's cache is a half-state
    // that reads as a bug in the code rather than as leftover data.
    if !cli.keep_redis {
        println!("▶ redis");
        match std::env::var("REDIS_URL") {
            Ok(redis_url) => match flush_redis(&redis_url).await {
                Ok(()) => println!("  flushed"),
                Err(e) => println!("  not flushed: {e}"),
            },
            Err(_) => println!("  REDIS_URL is not set; nothing flushed"),
        }
    }

    println!();
    if report.blocked_on_owner {
        println!("  Some steps were skipped: this database has no administrator.");
        println!("  Set SEED_ADMIN_PASSWORD (12+ characters - the .env.example");
        println!("  placeholder is 9) and run `skilluv-seed-all`.");
        println!();
    }
    println!("Done. {db_name} is what a new deployment would have.");
    println!("Your browser still holds a cookie for a user that no longer exists:");
    println!("clear it, or every page will answer 401 and look broken.");

    Ok(())
}

async fn flush_redis(redis_url: &str) -> Result<()> {
    let client = redis::Client::open(redis_url)?;
    let mut conn = client.get_multiplexed_async_connection().await?;
    let _: () = redis::cmd("FLUSHALL").query_async(&mut conn).await?;
    Ok(())
}
