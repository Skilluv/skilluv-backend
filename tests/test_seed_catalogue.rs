//! A fresh database seeds itself.
//!
//! The failure this file exists to catch is not a crash. It is a seed that runs
//! to completion, reports success and inserts nothing — which is what four of
//! the seven SQL scripts did for as long as they existed, because they resolved
//! their owner as `admin@skilluv.local` while the admin seeder creates
//! `admin@skill-uv.com`. An empty `INSERT ... SELECT` is not an error in
//! Postgres and `psql` exits 0.
//!
//! So every assertion here counts rows. "The seeder returned Ok" is precisely
//! the thing that was already true while the catalogue was empty.

mod common;
use common::TestApp;

use skilluv_backend::services::seed;

/// The seeder reads its administrator out of the environment, and the
/// environment is global to this binary while its tests run in parallel. Two
/// of these tests are *about* the variable being absent or wrong, so they
/// cannot share the process with one that needs it set.
///
/// So every test in this file takes this lock for its whole body, which makes
/// them serial. Five tests, and the alternative is a suite that fails once a
/// fortnight for a reason nobody can reproduce.
static ENV: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Set the password a seeded administrator gets, for the tests that want one.
///
/// SAFETY: every caller holds `ENV`, so no other test in this binary is
/// reading the environment while this writes it.
fn arm_the_admin_password() {
    unsafe {
        std::env::set_var("SEED_ADMIN_PASSWORD", "seed-catalogue-test-password");
        std::env::set_var("SEED_ADMIN_EMAIL", "admin@skill-uv.com");
    }
}

async fn count(app: &TestApp, sql: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_string()))
        .fetch_one(&app.db)
        .await
        .unwrap()
}

#[tokio::test]
async fn a_fresh_database_seeds_its_whole_catalogue() {
    let _env = ENV.lock().await;
    arm_the_admin_password();
    let app = TestApp::spawn().await;

    // Before: migrations only.
    assert_eq!(count(&app, "SELECT count(*) FROM seed_runs").await, 0);

    let report = seed::run(&app.db).await.expect("the seed run");

    assert!(
        !report.blocked_on_owner,
        "the admin step did not produce an owner: {:#?}",
        report.steps
    );
    assert_eq!(
        report.applied,
        seed::step_names().len(),
        "not every step ran: {:#?}",
        report.steps
    );

    // The administrator every other step hangs off.
    assert_eq!(
        count(
            &app,
            "SELECT count(*) FROM users WHERE role = 'admin' AND email = 'admin@skill-uv.com'"
        )
        .await,
        1
    );

    // The repositories work is drawn from. Fifty-odd across five catalogues;
    // the assertion is a floor rather than an exact number so that adding one
    // does not fail the test that adding none should fail.
    assert!(
        count(
            &app,
            "SELECT count(*) FROM projects WHERE curated_by_admin OR is_flagship"
        )
        .await
            >= 40,
        "the project catalogue did not land"
    );
    // Our own repositories specifically: these are what the GitHub ingestor
    // polls, so an empty set means no slices are ever materialised.
    assert!(
        count(
            &app,
            "SELECT count(*) FROM projects WHERE slug IN
                 ('skilluv-backend', 'skilluv-frontend', 'skilluv-admin', 'skilluv-ia')"
        )
        .await
            == 4
    );

    // The two we steward ourselves.
    assert_eq!(
        count(&app, "SELECT count(*) FROM projects WHERE is_flagship").await,
        2,
        "the flagships did not land — this is the script that carried a \
         hard-coded owner UUID"
    );

    // The onboarding challenges. These are what a new account is offered on
    // its first visit; zero of them is a signup that leads nowhere.
    assert!(
        count(
            &app,
            "SELECT count(*) FROM challenge_templates
              WHERE is_onboarding AND status = 'published'"
        )
        .await
            >= 10,
        "the onboarding challenges did not land"
    );

    // The badge the first merged pull request awards.
    assert_eq!(
        count(
            &app,
            "SELECT count(*) FROM badge_rules WHERE slug = 'bonjour_skilluv'"
        )
        .await,
        1
    );

    // The seasons and their deliverables.
    assert!(count(&app, "SELECT count(*) FROM seasons").await >= 2);

    // Design work on our own surfaces.
    assert!(
        count(
            &app,
            "SELECT count(*) FROM project_slices WHERE slice_type = 'design_artifact'"
        )
        .await
            > 0,
        "the design canvas did not land"
    );

    // And the ledger says so, once per step.
    assert_eq!(
        count(&app, "SELECT count(*) FROM seed_runs").await,
        seed::step_names().len() as i64
    );
}

#[tokio::test]
async fn a_second_run_does_nothing_and_costs_nothing() {
    let _env = ENV.lock().await;
    arm_the_admin_password();
    let app = TestApp::spawn().await;

    seed::run(&app.db).await.expect("first run");
    let projects_after_first = count(&app, "SELECT count(*) FROM projects").await;

    let second = seed::run(&app.db).await.expect("second run");
    assert_eq!(second.applied, 0, "{:#?}", second.steps);
    assert_eq!(second.skipped, seed::step_names().len());

    // Not a single duplicated row. The ledger is what makes the second run
    // cheap; idempotency is what makes it safe, and both have to hold.
    assert_eq!(
        count(&app, "SELECT count(*) FROM projects").await,
        projects_after_first
    );
}

#[tokio::test]
async fn forgetting_a_step_applies_it_again_without_duplicating_it() {
    let _env = ENV.lock().await;
    arm_the_admin_password();
    let app = TestApp::spawn().await;
    seed::run(&app.db).await.expect("first run");

    let before = count(&app, "SELECT count(*) FROM projects").await;
    assert!(seed::forget(&app.db, "projects").await.unwrap());

    let again = seed::run(&app.db).await.expect("second run");
    assert_eq!(again.applied, 1, "{:#?}", again.steps);
    assert_eq!(
        count(&app, "SELECT count(*) FROM projects").await,
        before,
        "re-applying a step duplicated its rows"
    );

    // Forgetting something that was never there is not an error — it is the
    // shape of `--forget` on a step that has not run yet.
    assert!(!seed::forget(&app.db, "no-such-step").await.unwrap());
}

#[tokio::test]
async fn without_a_password_the_catalogue_declines_instead_of_half_seeding() {
    let _env = ENV.lock().await;
    // SAFETY: the lock is held, so nothing else in this binary is reading the
    // environment while the variable this test is about is taken away.
    unsafe {
        std::env::remove_var("SEED_ADMIN_PASSWORD");
    }
    let app = TestApp::spawn().await;

    let report = seed::run(&app.db)
        .await
        .expect("the run itself must not fail");

    assert!(
        report.blocked_on_owner,
        "a database with no administrator reported a complete seed"
    );
    // Nothing owned was written. A half-seeded catalogue that says it worked
    // is the failure mode this module was built to end.
    assert_eq!(count(&app, "SELECT count(*) FROM projects").await, 0);
    assert_eq!(
        count(&app, "SELECT count(*) FROM users WHERE role = 'admin'").await,
        0
    );

    // The admin step itself did run, and recorded why it produced nothing —
    // so an operator reading `seed_runs` finds the answer rather than a gap.
    let detail: String =
        sqlx::query_scalar("SELECT detail FROM seed_runs WHERE name = 'admin_account'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(detail.contains("SEED_ADMIN_PASSWORD"), "{detail}");

    // Restored for whatever runs next.
    arm_the_admin_password();
}

#[tokio::test]
async fn a_short_password_is_refused_rather_than_accepted_quietly() {
    let _env = ENV.lock().await;
    // SAFETY: the lock is held — same reasoning as the test above.
    unsafe {
        std::env::set_var("SEED_ADMIN_PASSWORD", "short");
    }
    let app = TestApp::spawn().await;

    let report = seed::run(&app.db)
        .await
        .expect("the run itself must not fail");
    assert!(report.blocked_on_owner);
    assert_eq!(
        count(&app, "SELECT count(*) FROM users WHERE role = 'admin'").await,
        0,
        "a five-character password provisioned an administrator"
    );

    arm_the_admin_password();
}
