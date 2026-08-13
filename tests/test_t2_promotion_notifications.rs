//! Integration tests for SKI-43 — rich notifications on promotion.
//!
//! The central guarantee is that the durable channel always fires: the
//! service-layer proof hooks only hold a `PgPool`, so a promotion reached
//! from a background webhook must still leave a row in `notifications`.

mod common;

use common::TestApp;
use serde_json::Value;
use uuid::Uuid;

use skilluv_backend::services::proof_hooks;

fn user_id_of(register_body: &Value) -> Uuid {
    register_body["data"]["user"]["id"]
        .as_str()
        .expect("register response carries a user id")
        .parse()
        .expect("user id is a uuid")
}

async fn seed_verified_deliverables(app: &TestApp, user_id: Uuid, n: usize) {
    let project_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO projects (id, slug, name, owner_type, owner_id)
         VALUES ($1, $2, 'Notif project', 'user', $3)",
    )
    .bind(project_id)
    .bind(format!("nt-proj-{}", &project_id.to_string()[..8]))
    .bind(user_id)
    .execute(&app.db)
    .await
    .expect("seed project");

    for i in 0..n {
        let slice_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO project_slices
                (id, project_id, slice_type, title, description, primary_domain,
                 difficulty, status, min_rank)
             VALUES ($1, $2, 'github_issue', 'Ranger-only slice', 'desc', 'code',
                     2, 'open', 'ranger')",
        )
        .bind(slice_id)
        .bind(project_id)
        .execute(&app.db)
        .await
        .expect("seed slice");

        sqlx::query(
            "INSERT INTO deliverables
                (slice_id, user_id, artifact_type, artifact_url, verifiable_by,
                 verification_status, verified_at, public)
             VALUES ($1, $2, 'pr_merged', $3, 'human_review', 'verified', NOW(), TRUE)",
        )
        .bind(slice_id)
        .bind(user_id)
        .bind(format!("https://example.test/pr/{i}"))
        .execute(&app.db)
        .await
        .expect("seed deliverable");
    }
}

async fn notifications_of(app: &TestApp, user_id: Uuid, kind: &str) -> Vec<Value> {
    sqlx::query_scalar::<_, Value>(
        "SELECT jsonb_build_object('title', title, 'body', body, 'data', data)
           FROM notifications
          WHERE user_id = $1 AND notification_type = $2
          ORDER BY created_at ASC",
    )
    .bind(user_id)
    .bind(kind)
    .fetch_all(&app.db)
    .await
    .expect("read notifications")
}

#[tokio::test]
async fn rank_promotion_notifies_with_an_unlock_hint() {
    let app = TestApp::spawn().await;
    let me = app.register_user("ntrank").await;
    let my_id = user_id_of(&me);

    // Four verified deliverables promote to Ranger. The slices they were
    // attached to are themselves gated at `ranger`, so the notification
    // has something concrete to point at.
    seed_verified_deliverables(&app, my_id, 4).await;

    let report = proof_hooks::recompute_all_for_user(&app.db, my_id)
        .await
        .expect("recompute");
    assert!(report.rank_promoted, "four deliverables reach Ranger");
    assert_eq!(report.rank_computed, "ranger");

    let notifs = notifications_of(&app, my_id, "rank.promoted").await;
    assert_eq!(notifs.len(), 1, "one notification per promotion");
    assert!(
        notifs[0]["title"].as_str().unwrap().contains("Ranger"),
        "the title names the new rank"
    );
    assert_eq!(notifs[0]["data"]["to_rank"], "ranger");
    assert_eq!(notifs[0]["data"]["from_rank"], "apprenti");
    assert!(
        notifs[0]["data"]["unlock_hint"]["unlocked_slices_count"]
            .as_i64()
            .unwrap()
            > 0,
        "the payoff is spelled out, not implied"
    );
    assert!(notifs[0]["data"]["next_step_cta"]["href"].is_string());
}

#[tokio::test]
async fn a_recompute_that_changes_nothing_stays_silent() {
    let app = TestApp::spawn().await;
    let me = app.register_user("ntquiet").await;
    let my_id = user_id_of(&me);

    // The first recompute is not a no-op even for a brand-new account:
    // `challenger` is granted to every registered user, and announcing it
    // is correct. What must not happen is announcing it twice.
    let first = proof_hooks::recompute_all_for_user(&app.db, my_id)
        .await
        .expect("first recompute");
    assert!(!first.rank_promoted, "no proofs, no rank promotion");
    assert!(
        first
            .capabilities_granted
            .contains(&"challenger".to_string()),
        "every registered user gets the challenger capability"
    );

    let after_first: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM notifications WHERE user_id = $1")
            .bind(my_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(after_first, first.capabilities_granted.len() as i64);

    // Second pass over unchanged state: nothing was granted, so nothing is
    // announced.
    let second = proof_hooks::recompute_all_for_user(&app.db, my_id)
        .await
        .expect("second recompute");
    assert!(second.capabilities_granted.is_empty());
    assert!(!second.rank_promoted);

    let after_second: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM notifications WHERE user_id = $1")
            .bind(my_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(
        after_second, after_first,
        "a recompute that changes nothing must stay silent"
    );
}

#[tokio::test]
async fn promotion_notification_is_not_repeated_on_a_second_recompute() {
    let app = TestApp::spawn().await;
    let me = app.register_user("ntonce").await;
    let my_id = user_id_of(&me);
    seed_verified_deliverables(&app, my_id, 4).await;

    proof_hooks::recompute_all_for_user(&app.db, my_id)
        .await
        .expect("first recompute");
    proof_hooks::recompute_all_for_user(&app.db, my_id)
        .await
        .expect("second recompute");

    let notifs = notifications_of(&app, my_id, "rank.promoted").await;
    assert_eq!(
        notifs.len(),
        1,
        "rank promotion is unidirectional, so the second recompute promotes nothing"
    );
}

#[tokio::test]
async fn first_verified_deliverable_is_celebrated_exactly_once() {
    let app = TestApp::spawn().await;
    let me = app.register_user("ntfirst").await;
    let my_id = user_id_of(&me);

    seed_verified_deliverables(&app, my_id, 1).await;
    proof_hooks::recompute_all_for_user(&app.db, my_id)
        .await
        .expect("recompute");

    let notifs = notifications_of(&app, my_id, "deliverable.first_verified").await;
    assert_eq!(notifs.len(), 1);
    assert_eq!(notifs[0]["data"]["verified_count"], 1);

    // A second recompute at the same count must not re-celebrate...
    proof_hooks::recompute_all_for_user(&app.db, my_id)
        .await
        .expect("recompute");
    let notifs = notifications_of(&app, my_id, "deliverable.first_verified").await;
    assert_eq!(notifs.len(), 1);

    // ...and neither must a later one, once the count has moved on.
    seed_verified_deliverables(&app, my_id, 3).await;
    proof_hooks::recompute_all_for_user(&app.db, my_id)
        .await
        .expect("recompute");
    let notifs = notifications_of(&app, my_id, "deliverable.first_verified").await;
    assert_eq!(notifs.len(), 1, "'first' means first, forever");
}

#[tokio::test]
async fn capability_grant_produces_its_own_notification() {
    let app = TestApp::spawn().await;
    let me = app.register_user("ntcap").await;
    let my_id = user_id_of(&me);

    // Four verified deliverables also grant the `challenger` capability.
    seed_verified_deliverables(&app, my_id, 4).await;
    let report = proof_hooks::recompute_all_for_user(&app.db, my_id)
        .await
        .expect("recompute");

    let notifs = notifications_of(&app, my_id, "capability.granted").await;
    assert_eq!(
        notifs.len(),
        report.capabilities_granted.len(),
        "one notification per newly granted capability"
    );
    for n in &notifs {
        assert!(n["data"]["capability"].is_string());
        assert!(n["data"]["next_step_cta"]["label"].is_string());
    }
}

#[tokio::test]
async fn achieved_goal_emits_a_milestone_notification() {
    let app = TestApp::spawn().await;
    let me = app.register_user("ntgoal").await;
    let my_id = user_id_of(&me);

    sqlx::query(
        "INSERT INTO user_goals (user_id, kind, target_value)
         VALUES ($1, 'artifact_count', '2')",
    )
    .bind(my_id)
    .execute(&app.db)
    .await
    .unwrap();

    seed_verified_deliverables(&app, my_id, 2).await;
    proof_hooks::recompute_all_for_user(&app.db, my_id)
        .await
        .expect("recompute");

    let notifs = notifications_of(&app, my_id, "goal.reached").await;
    assert_eq!(notifs.len(), 1);
    assert_eq!(notifs[0]["data"]["kind"], "artifact_count");
    assert_eq!(notifs[0]["data"]["target_value"], "2");

    // achieved_at is stamped, so the milestone cannot fire twice.
    proof_hooks::recompute_all_for_user(&app.db, my_id)
        .await
        .expect("recompute");
    let notifs = notifications_of(&app, my_id, "goal.reached").await;
    assert_eq!(notifs.len(), 1);

    let achieved: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT achieved_at FROM user_goals WHERE user_id = $1")
            .bind(my_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(achieved.is_some());
}

#[tokio::test]
async fn live_variant_also_writes_the_durable_row() {
    let app = TestApp::spawn().await;
    let me = app.register_user("ntlive").await;
    let my_id = user_id_of(&me);
    seed_verified_deliverables(&app, my_id, 4).await;

    // The live path adds Redis and WebSocket delivery on top; the database
    // row must be identical either way, since that is what the
    // notifications endpoint reads.
    let mut redis = redis::aio::ConnectionManager::new(
        redis::Client::open(format!(
            "redis://localhost:6379/{}",
            (std::process::id() as usize) % 16
        ))
        .expect("redis client"),
    )
    .await
    .expect("redis connect");
    let ws = skilluv_backend::websocket::WsManager::new();

    let report = proof_hooks::recompute_all_for_user_live(&app.db, &mut redis, &ws, my_id)
        .await
        .expect("recompute live");
    assert!(report.rank_promoted);

    let notifs = notifications_of(&app, my_id, "rank.promoted").await;
    assert_eq!(notifs.len(), 1);
}
