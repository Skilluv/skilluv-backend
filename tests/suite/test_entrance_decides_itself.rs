//! The code entrance is decided without a reviewer, and earns nobody the right
//! to review.
//!
//! The decision itself - five mechanical checks on the diff - lives in
//! `services::hello_check` and is tested there, as pure functions, because
//! every one of its inputs is a string. What this file holds is the two
//! consequences that reach the database:
//!
//!   * a rite is finished by one function whoever finishes it, so the
//!     automatic path and a person's verdict cannot drift on what "finished"
//!     means - that drift is how the founding badge went unreachable once;
//!   * passing the code entrance no longer grants `rite_reviewer:code`,
//!     because a capability to witness others, granted by a process nobody
//!     witnessed, anchors the chain of trust to nothing.
//!
//! The webhook that calls these reads HELLO.md off GitHub, so, like the
//! existing code-rite test, the rows are written here directly.

use skilluv_backend::services::{capabilities_engine, reviews};
use uuid::Uuid;

/// A code rite sitting at `pr_opened`, with its deliverable, as the webhook
/// leaves it.
async fn a_code_rite_awaiting_its_verdict(db: &sqlx::PgPool, user_id: Uuid, fork_id: i64) -> Uuid {
    let fork = format!("user{fork_id}/starter-fullstack-node");
    let challenge_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM challenge_templates
          WHERE is_domain_rite AND skill_domain = 'code' AND status = 'published'",
    )
    .fetch_one(db)
    .await
    .unwrap();

    let deliverable_id: Uuid = sqlx::query_scalar(
        "INSERT INTO deliverables
            (challenge_id, user_id, artifact_type, artifact_url, artifact_hash,
             verifiable_by, verification_status, fragments_awarded, public,
             submitted_at, created_at)
         VALUES ($1, $2, 'other', 'https://github.com/x/starter-fullstack-node/pull/1',
                 $3, 'automated_diff', 'verified', 0, TRUE, NOW(), NOW())
         RETURNING id",
    )
    .bind(challenge_id)
    .bind(user_id)
    .bind(format!("hash-{fork_id}"))
    .fetch_one(db)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO onboarding_bonjour_skilluv
            (user_id, skill_domain, rite_form, challenge_id, deliverable_id,
             starter_slug, fork_full_name, fork_html_url, github_fork_id,
             status, pr_number, pr_url, pr_opened_at)
         VALUES ($1, 'code', 'fork', $2, $3,
                 'starter-fullstack-node', $4, $5, $6,
                 'pr_opened', 1, $7, NOW())",
    )
    .bind(user_id)
    .bind(challenge_id)
    .bind(deliverable_id)
    .bind(&fork)
    // The table checks this is a whole `owner/repo` GitHub URL, so the
    // fixture has to be one.
    .bind(format!("https://github.com/{fork}"))
    .bind(fork_id)
    .bind(format!("https://github.com/{fork}/pull/1"))
    .execute(db)
    .await
    .unwrap();

    deliverable_id
}

async fn user_id(app: &crate::common::TestApp, username: &str) -> Uuid {
    sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(username)
        .fetch_one(&app.db)
        .await
        .unwrap()
}

/// One function finishes a fork rite, and it finishes it once.
///
/// There are two callers - a person's verdict and the automatic one - and
/// the second copy of this logic is exactly where the badge would stop firing
/// without anybody noticing. So both call this, and this is what it does.
#[tokio::test]
async fn finishing_a_fork_rite_is_one_function_and_it_is_idempotent() {
    let app = crate::common::TestApp::spawn().await;
    app.register_user("auto_done").await;
    let uid = user_id(&app, "auto_done").await;
    let deliverable_id = a_code_rite_awaiting_its_verdict(&app.db, uid, 910001).await;

    let mut tx = app.db.begin().await.unwrap();
    let first = reviews::complete_fork_rite(&mut tx, deliverable_id)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(first, Some(uid), "the rite belongs to this person");

    let (status, completed): (String, bool) = sqlx::query_as(
        "SELECT status, completed_at IS NOT NULL
           FROM onboarding_bonjour_skilluv WHERE user_id = $1",
    )
    .bind(uid)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(status, "completed");
    // `badge_rules.bonjour_skilluv` fires on exactly this.
    assert!(completed, "completed_at is what the founding badge reads");

    // A redelivered webhook, or a verdict arriving after the automatic one,
    // must not finish it a second time.
    let mut tx = app.db.begin().await.unwrap();
    let second = reviews::complete_fork_rite(&mut tx, deliverable_id)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(second, None, "a finished rite is not finished again");
}

/// Passing the code entrance grants no reviewer rung.
///
/// The code entrance is decided mechanically now, so passing it means nobody
/// read anything - and the right to witness other people cannot be earned
/// from a process in which nobody witnessed you.
#[tokio::test]
async fn passing_the_code_entrance_grants_no_reviewer_rung() {
    let app = crate::common::TestApp::spawn().await;
    app.register_user("auto_norung").await;
    let uid = user_id(&app, "auto_norung").await;
    let deliverable_id = a_code_rite_awaiting_its_verdict(&app.db, uid, 910002).await;

    let mut tx = app.db.begin().await.unwrap();
    reviews::complete_fork_rite(&mut tx, deliverable_id)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    capabilities_engine::recompute_capabilities_for_user(&app.db, uid)
        .await
        .unwrap();

    let held: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM user_capabilities
          WHERE user_id = $1 AND capability = 'rite_reviewer:code'
            AND revoked_at IS NULL",
    )
    .bind(uid)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(
        held, 0,
        "the code entrance earns nobody the right to judge it"
    );
}

/// The rung survives where a person still reads the entrance.
///
/// Removing it everywhere would reopen the circle migration 0624 closed: no
/// reviewer, no completed rite, no attestation, no mentor, no reviewer. The
/// eleven domains whose entrance ends in a verdict keep it, and this is the
/// test that says the code change did not reach them.
#[tokio::test]
async fn the_rung_survives_for_a_rite_a_person_decided() {
    let app = crate::common::TestApp::spawn().await;
    app.register_user("design_rung").await;
    let uid = user_id(&app, "design_rung").await;

    // A completed design rite, in the shape a design rite actually has: a
    // submission a person reviewed, with no fork and no GitHub account
    // (`onboarding_bonjour_submission_shape` refuses the fork columns). The
    // engine reads only the domain and `completed_at`.
    sqlx::query(
        "INSERT INTO onboarding_bonjour_skilluv
            (user_id, skill_domain, rite_form, status, completed_at)
         VALUES ($1, 'design', 'submission', 'completed', NOW())",
    )
    .bind(uid)
    .execute(&app.db)
    .await
    .unwrap();

    capabilities_engine::recompute_capabilities_for_user(&app.db, uid)
        .await
        .unwrap();

    let held: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM user_capabilities
          WHERE user_id = $1 AND capability = 'rite_reviewer:design'
            AND revoked_at IS NULL",
    )
    .bind(uid)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(held, 1, "a rite somebody read still earns its rung");
}
