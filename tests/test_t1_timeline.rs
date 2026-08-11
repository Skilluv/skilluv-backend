//! Integration tests for SKI-39 — profile timeline.
//!
//! The timeline is written by database triggers (migration 0142), so most
//! of these tests assert on what plain SQL writes produce, not on what a
//! Rust code path does. That is the point: the triggers exist precisely so
//! that writers which bypass the Rust hooks still land on the timeline.

mod common;

use common::TestApp;
use reqwest::StatusCode;
use serde_json::{Value, json};
use uuid::Uuid;

use skilluv_backend::services::timeline;

fn user_id_of(register_body: &Value) -> Uuid {
    register_body["data"]["user"]["id"]
        .as_str()
        .expect("register response carries a user id")
        .parse()
        .expect("user id is a uuid")
}

async fn event_types_for(app: &TestApp, user_id: Uuid) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT event_type FROM user_timeline_events
          WHERE user_id = $1 ORDER BY event_at ASC, event_type ASC",
    )
    .bind(user_id)
    .fetch_all(&app.db)
    .await
    .expect("read timeline")
}

async fn seed_slice(app: &TestApp, owner: Uuid) -> Uuid {
    let project_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO projects (id, slug, name, owner_type, owner_id)
         VALUES ($1, $2, 'Timeline project', 'user', $3)",
    )
    .bind(project_id)
    .bind(format!("tl-proj-{}", &project_id.to_string()[..8]))
    .bind(owner)
    .execute(&app.db)
    .await
    .expect("seed project");

    let slice_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO project_slices
            (id, project_id, slice_type, title, description, primary_domain,
             difficulty, status)
         VALUES ($1, $2, 'github_issue', 'Slice', 'desc', 'code', 2, 'open')",
    )
    .bind(slice_id)
    .bind(project_id)
    .execute(&app.db)
    .await
    .expect("seed slice");
    slice_id
}

#[tokio::test]
async fn signup_is_recorded_by_the_trigger() {
    let app = TestApp::spawn().await;
    let me = app.register_user("tlsignup").await;
    let my_id = user_id_of(&me);

    let types = event_types_for(&app, my_id).await;
    assert_eq!(
        types,
        vec!["signup".to_string()],
        "registering must produce exactly one signup event"
    );

    // event_at is the account creation instant, not the insert instant.
    let (event_at, created_at): (chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>) =
        sqlx::query_as(
            "SELECT e.event_at, u.created_at
           FROM user_timeline_events e JOIN users u ON u.id = e.user_id
          WHERE e.user_id = $1 AND e.event_type = 'signup'",
        )
        .bind(my_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(event_at, created_at);
}

#[tokio::test]
async fn deliverable_verification_is_recorded_once() {
    let app = TestApp::spawn().await;
    let me = app.register_user("tldeliv").await;
    let my_id = user_id_of(&me);
    let slice_id = seed_slice(&app, my_id).await;

    // Inserted as pending: no timeline event yet.
    let deliverable_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO deliverables
            (id, slice_id, user_id, artifact_type, artifact_url, verifiable_by,
             verification_status)
         VALUES ($1, $2, $3, 'pr_merged', 'https://example.test/pr/1',
                 'human_review', 'pending')",
    )
    .bind(deliverable_id)
    .bind(slice_id)
    .bind(my_id)
    .execute(&app.db)
    .await
    .unwrap();
    assert!(
        !event_types_for(&app, my_id)
            .await
            .contains(&"deliverable_verified".to_string())
    );

    sqlx::query(
        "UPDATE deliverables SET verification_status = 'verified', verified_at = NOW()
          WHERE id = $1",
    )
    .bind(deliverable_id)
    .execute(&app.db)
    .await
    .unwrap();

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_timeline_events
          WHERE user_id = $1 AND event_type = 'deliverable_verified'",
    )
    .bind(my_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(count, 1);

    // Touching an already-verified deliverable must not re-stamp.
    sqlx::query("UPDATE deliverables SET verification_status = 'verified' WHERE id = $1")
        .bind(deliverable_id)
        .execute(&app.db)
        .await
        .unwrap();
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_timeline_events
          WHERE user_id = $1 AND event_type = 'deliverable_verified'",
    )
    .bind(my_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(
        count, 1,
        "re-saving a verified deliverable is not a new event"
    );
}

#[tokio::test]
async fn rank_promotion_records_one_event_per_rank_reached() {
    let app = TestApp::spawn().await;
    let me = app.register_user("tlrank").await;
    let my_id = user_id_of(&me);

    sqlx::query(
        "INSERT INTO user_rank_history (user_id, from_rank, to_rank, reason)
         VALUES ($1, 'apprenti', 'ranger', 'test')",
    )
    .bind(my_id)
    .execute(&app.db)
    .await
    .unwrap();

    // A demotion + re-promotion to the same rank is still one milestone.
    sqlx::query(
        "INSERT INTO user_rank_history (user_id, from_rank, to_rank, reason)
         VALUES ($1, 'ranger', 'apprenti', 'admin override'),
                ($1, 'apprenti', 'ranger', 'restored')",
    )
    .bind(my_id)
    .execute(&app.db)
    .await
    .unwrap();

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_timeline_events
          WHERE user_id = $1 AND event_type = 'rank_promoted' AND dedup_key = 'ranger'",
    )
    .bind(my_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(count, 1, "reaching Ranger twice is one timeline entry");
}

#[tokio::test]
async fn capability_and_attestation_are_recorded() {
    let app = TestApp::spawn().await;
    let me = app.register_user("tlcapattest").await;
    let my_id = user_id_of(&me);

    sqlx::query(
        "INSERT INTO user_capabilities (user_id, capability, granted_reason)
         VALUES ($1, 'mentor', 'three sessions')",
    )
    .bind(my_id)
    .execute(&app.db)
    .await
    .unwrap();

    // A `skill` attestation must carry exactly one linked skill and a
    // unique verification code (migration 0068).
    let skill_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO skill_nodes (id, slug, display_name, domain)
         VALUES ($1, $2, 'Rust', 'code')",
    )
    .bind(skill_id)
    .bind(format!("tl-skill-{}", &skill_id.to_string()[..8]))
    .execute(&app.db)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO attestations
            (user_id, attestation_type, title, description,
             linked_skill_node_ids, verification_code)
         VALUES ($1, 'skill', 'Rust fundamentals', 'desc', ARRAY[$2::UUID], $3)",
    )
    .bind(my_id)
    .bind(skill_id)
    .bind(Uuid::new_v4().simple().to_string()[..12].to_uppercase())
    .execute(&app.db)
    .await
    .unwrap();

    let types = event_types_for(&app, my_id).await;
    assert!(types.contains(&"capability_granted".to_string()));
    assert!(types.contains(&"attestation_received".to_string()));

    // Metadata carries what the front end needs to render the entry.
    let metadata: Value = sqlx::query_scalar(
        "SELECT metadata FROM user_timeline_events
          WHERE user_id = $1 AND event_type = 'attestation_received'",
    )
    .bind(my_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(metadata["title"], "Rust fundamentals");
    assert_eq!(metadata["attestation_type"], "skill");
}

#[tokio::test]
async fn timeline_endpoint_is_public_and_paginates() {
    let app = TestApp::spawn().await;
    let me = app.register_user("tlpublic").await;
    let my_id = user_id_of(&me);

    for rank in ["ranger", "artisan", "maitre"] {
        sqlx::query(
            "INSERT INTO user_rank_history (user_id, from_rank, to_rank, reason)
             VALUES ($1, 'apprenti', $2, 'test')",
        )
        .bind(my_id)
        .bind(rank)
        .execute(&app.db)
        .await
        .unwrap();
    }

    // Anonymous read: no login before this call.
    let resp = app.get(&format!("/api/users/{my_id}/timeline")).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["total"], 4, "signup + three promotions");

    let body: Value = app
        .get(&format!("/api/users/{my_id}/timeline?limit=2"))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["events"].as_array().unwrap().len(), 2);
    assert_eq!(body["data"]["total"], 4, "total ignores the page size");

    let body: Value = app
        .get(&format!(
            "/api/users/{my_id}/timeline?event_type=rank_promoted"
        ))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["total"], 3);

    let resp = app
        .get(&format!("/api/users/{my_id}/timeline?event_type=nonsense"))
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn hidden_profile_timeline_is_404_for_others_and_visible_to_the_owner() {
    let app = TestApp::spawn().await;
    let me = app.register_user("tlhidden").await;
    let my_id = user_id_of(&me);

    sqlx::query("UPDATE users SET profile_hidden = TRUE WHERE id = $1")
        .bind(my_id)
        .execute(&app.db)
        .await
        .unwrap();

    // A fresh client with no cookie jar entry: `register_user` leaves the
    // caller logged in, so reusing `app`'s client here would test the
    // owner's view, not an anonymous one.
    let anon = reqwest::Client::new();
    let resp = anon
        .get(format!("{}/api/users/{my_id}/timeline", app.addr))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "hidden profiles answer 404, same as a nonexistent user"
    );

    // Another authenticated user gets the same answer.
    app.register_user("tlonlooker").await;
    app.login("tlonlooker").await;
    let resp = app.get(&format!("/api/users/{my_id}/timeline")).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    app.login("tlhidden").await;
    let resp = app.get(&format!("/api/users/{my_id}/timeline")).await;
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "the owner always sees their own timeline"
    );

    let resp = app
        .get(&format!("/api/users/{}/timeline", Uuid::new_v4()))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn backfill_is_idempotent_and_rebuilds_dropped_rows() {
    let app = TestApp::spawn().await;
    let me = app.register_user("tlbackfill").await;
    let my_id = user_id_of(&me);
    let slice_id = seed_slice(&app, my_id).await;

    sqlx::query(
        "INSERT INTO deliverables
            (slice_id, user_id, artifact_type, artifact_url, verifiable_by,
             verification_status, verified_at)
         VALUES ($1, $2, 'pr_merged', 'https://example.test/pr/9',
                 'human_review', 'verified', NOW())",
    )
    .bind(slice_id)
    .bind(my_id)
    .execute(&app.db)
    .await
    .unwrap();

    // Everything is already there thanks to the triggers, so a backfill is
    // a no-op — which is the cheapest possible completeness check.
    let report = timeline::backfill(&app.db, Some(my_id)).await.unwrap();
    assert_eq!(report.total(), 0, "triggers already recorded everything");

    // Simulate a restore that skipped triggers.
    sqlx::query("DELETE FROM user_timeline_events WHERE user_id = $1")
        .bind(my_id)
        .execute(&app.db)
        .await
        .unwrap();

    let report = timeline::backfill(&app.db, Some(my_id)).await.unwrap();
    assert_eq!(report.signup, 1);
    assert_eq!(report.deliverable_verified, 1);
    assert_eq!(report.total(), 2);

    // And running it again still changes nothing.
    let report = timeline::backfill(&app.db, Some(my_id)).await.unwrap();
    assert_eq!(report.total(), 0);
}

#[tokio::test]
async fn admin_backfill_endpoint_reports_insertions() {
    let app = TestApp::spawn().await;
    let target = app.register_user("tltarget").await;
    let target_id = user_id_of(&target);

    sqlx::query("DELETE FROM user_timeline_events WHERE user_id = $1")
        .bind(target_id)
        .execute(&app.db)
        .await
        .unwrap();

    app.register_admin("tladmin").await;
    app.login("tladmin").await;

    let resp = app
        .post(
            &format!("/api/admin/users/{target_id}/backfill-timeline"),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(
        body["data"]["rows_inserted"], 1,
        "the signup event is rebuilt"
    );

    let resp = app
        .post(
            &format!("/api/admin/users/{}/backfill-timeline", Uuid::new_v4()),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn admin_backfill_requires_admin() {
    let app = TestApp::spawn().await;
    let target = app.register_user("tlplain").await;
    let target_id = user_id_of(&target);
    app.login("tlplain").await;

    let resp = app
        .post(
            &format!("/api/admin/users/{target_id}/backfill-timeline"),
            &json!({}),
        )
        .await;
    assert!(
        resp.status() == StatusCode::FORBIDDEN || resp.status() == StatusCode::UNAUTHORIZED,
        "a plain user must not reach the admin backfill, got {}",
        resp.status()
    );
}
