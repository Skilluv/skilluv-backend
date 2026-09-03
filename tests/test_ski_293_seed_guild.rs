//! SKI-293 point 4 — `skilluv-seed-guild` provisions an owner-side guild.
//!
//! Founding a guild through the API requires exactly three co-founders, so a
//! single e2e account cannot create one. Without a seed, `/guilds` is an
//! empty page on the test backend and the Applications / Invitations tabs and
//! revocation are never exercised end to end.
//!
//! These tests run the real binary as a subprocess against a migrated
//! database rather than re-implementing its SQL — re-implementing it would
//! test the copy, not the thing that ships.

mod common;
use common::TestApp;
use std::process::Command;
use uuid::Uuid;

/// Where the seed binary actually is.
///
/// `env!("CARGO_BIN_EXE_...")` is baked in at compile time and points at the
/// machine that compiled it. CI now builds the suite once into a nextest
/// archive and runs it on twelve other runners, where that path does not
/// exist. nextest ships non-test binaries inside the archive and publishes
/// their relocated path in `NEXTEST_BIN_EXE_<name>` — hyphens become
/// underscores. The compile-time path stays as the fallback, which is what a
/// plain `cargo test` on a developer machine uses.
fn seed_binary() -> String {
    std::env::var("NEXTEST_BIN_EXE_skilluv_seed_guild")
        .unwrap_or_else(|_| env!("CARGO_BIN_EXE_skilluv-seed-guild").to_string())
}

/// Runs the seed binary against this test app's database.
/// Returns `(success, stdout, stderr)`.
fn run_seed(app: &TestApp, args: &[&str]) -> (bool, String, String) {
    let out = Command::new(seed_binary())
        .args(args)
        .env("DATABASE_URL", app.database_url())
        // The binary refuses a non-local URL unless this is set. The test
        // database is local, but the guard also trips on hostnames like
        // `postgres`, so be explicit rather than depend on the host name.
        .env("SEED_GUILD_ALLOW_REMOTE", "1")
        .output()
        .expect("failed to run skilluv-seed-guild");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

async fn make_founder(app: &TestApp, email: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO users (username, email, password_hash, display_name, first_name, last_name)
         VALUES ('seed_founder', $1, 'x', 'Seed Founder', 'Seed', 'Founder')
         RETURNING id",
    )
    .bind(email)
    .fetch_one(&app.db)
    .await
    .unwrap()
}

#[tokio::test]
async fn it_seeds_a_guild_with_a_founder_an_application_and_an_invitation() {
    let app = TestApp::spawn().await;
    make_founder(&app, "founder@seed.test").await;

    let (ok, stdout, stderr) = run_seed(&app, &["--email", "founder@seed.test"]);
    assert!(ok, "seed failed.\nstdout: {stdout}\nstderr: {stderr}");

    let guild_id: Uuid = sqlx::query_scalar("SELECT id FROM guilds WHERE slug = 'e2e-guild'")
        .fetch_one(&app.db)
        .await
        .expect("guild was created");

    let founder_role: String =
        sqlx::query_scalar("SELECT role FROM guild_members WHERE guild_id = $1")
            .bind(guild_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(founder_role, "founder");

    let pending_applications: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM guild_applications WHERE guild_id = $1 AND status = 'pending'",
    )
    .bind(guild_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(
        pending_applications, 1,
        "the Applications tab needs a row to act on"
    );

    let pending_invitations: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM guild_invitations
         WHERE guild_id = $1 AND accepted_at IS NULL AND revoked_at IS NULL",
    )
    .bind(guild_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(
        pending_invitations, 1,
        "revocation cannot be exercised without a live invitation"
    );
}

#[tokio::test]
async fn running_it_twice_changes_nothing() {
    let app = TestApp::spawn().await;
    make_founder(&app, "twice@seed.test").await;

    let (first, _, err1) = run_seed(&app, &["--email", "twice@seed.test"]);
    assert!(first, "first run failed: {err1}");
    let (second, _, err2) = run_seed(&app, &["--email", "twice@seed.test"]);
    assert!(
        second,
        "a second run must succeed — this is provisioning, it will be \
         re-run on every deploy: {err2}"
    );

    let guilds: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM guilds")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(guilds, 1);

    let applications: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM guild_applications")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(applications, 1, "the application must not be duplicated");

    let invitations: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM guild_invitations")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(invitations, 1, "the invitation must not be duplicated");
}

#[tokio::test]
async fn it_refuses_an_unknown_founder_email() {
    let app = TestApp::spawn().await;

    let (ok, _, stderr) = run_seed(&app, &["--email", "ghost@seed.test"]);
    assert!(!ok, "seeding onto a missing account must fail");
    assert!(
        stderr.contains("no user with email"),
        "the error should name the cause, got: {stderr}"
    );
}

#[tokio::test]
async fn it_refuses_a_remote_database_unless_told_otherwise() {
    let out = Command::new(seed_binary())
        .args(["--email", "whoever@seed.test"])
        .env(
            "DATABASE_URL",
            "postgres://u:p@db.production.example/skilluv",
        )
        .env_remove("SEED_GUILD_ALLOW_REMOTE")
        .output()
        .expect("failed to run skilluv-seed-guild");

    assert!(!out.status.success(), "a remote URL must be refused");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("does not look local"),
        "the refusal should explain itself, got: {stderr}"
    );
}

#[tokio::test]
async fn it_rejects_a_tag_the_database_would_reject() {
    let app = TestApp::spawn().await;
    make_founder(&app, "badtag@seed.test").await;

    let (ok, _, stderr) = run_seed(&app, &["--email", "badtag@seed.test", "--tag", "XY"]);
    assert!(!ok, "a two-character tag violates the CHECK constraint");
    assert!(
        stderr.contains("tag must be 3 to 5 characters"),
        "the guard should fire before the database does, got: {stderr}"
    );
}
