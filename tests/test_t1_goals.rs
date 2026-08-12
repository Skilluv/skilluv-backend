//! Integration tests for SKI-38 — measurable personal goals.
//!
//! The interesting surface is the derived progress: a goal must never
//! disagree with the profile it describes, and the percentage must reflect
//! the limiting criterion rather than the most flattering one.

mod common;

use common::TestApp;
use reqwest::StatusCode;
use serde_json::{Value, json};
use uuid::Uuid;

use skilluv_backend::services::goals;

fn user_id_of(register_body: &Value) -> Uuid {
    register_body["data"]["user"]["id"]
        .as_str()
        .expect("register response carries a user id")
        .parse()
        .expect("user id is a uuid")
}

/// Insert `n` verified deliverables for a user, dated `days_ago`.
///
/// Goes straight to SQL rather than through the submission pipeline: this
/// suite is about progress arithmetic, not about how a deliverable gets
/// verified.
async fn seed_verified_deliverables(app: &TestApp, user_id: Uuid, n: usize, days_ago: i64) {
    let project_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO projects (id, slug, name, owner_type, owner_id)
         VALUES ($1, $2, 'Goal project', 'user', $3)",
    )
    .bind(project_id)
    .bind(format!("goal-proj-{}", &project_id.to_string()[..8]))
    .bind(user_id)
    .execute(&app.db)
    .await
    .expect("seed project");

    for i in 0..n {
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

        sqlx::query(
            "INSERT INTO deliverables
                (slice_id, user_id, artifact_type, artifact_url, verifiable_by,
                 verification_status, verified_at, public)
             VALUES ($1, $2, 'pr_merged', $3, 'human_review', 'verified',
                     NOW() - MAKE_INTERVAL(days => $4::INT), TRUE)",
        )
        .bind(slice_id)
        .bind(user_id)
        .bind(format!("https://example.test/pr/{i}"))
        .bind(days_ago as i32)
        .execute(&app.db)
        .await
        .expect("seed deliverable");
    }
}

/// Insert an attestation.
///
/// A `skill` attestation must carry exactly one linked skill
/// (`attestations_skill_has_one_skill`, migration 0068) and a unique
/// `verification_code`, so both are supplied here.
async fn seed_attestation(app: &TestApp, user_id: Uuid, days_ago: i64) {
    let skill_id = seed_skill(app, &format!("attest-skill-{}", Uuid::new_v4())).await;
    let code: String = Uuid::new_v4().simple().to_string()[..12].to_uppercase();
    sqlx::query(
        "INSERT INTO attestations
            (user_id, attestation_type, title, description, issued_at,
             linked_skill_node_ids, verification_code)
         VALUES ($1, 'skill', 'Test attestation', 'desc',
                 NOW() - MAKE_INTERVAL(days => $2::INT), ARRAY[$3::UUID], $4)",
    )
    .bind(user_id)
    .bind(days_ago as i32)
    .bind(skill_id)
    .bind(&code)
    .execute(&app.db)
    .await
    .expect("seed attestation");
}

async fn seed_skill(app: &TestApp, slug: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO skill_nodes (id, slug, display_name, domain)
         VALUES ($1, $2, 'Test skill', 'code')",
    )
    .bind(id)
    .bind(slug)
    .execute(&app.db)
    .await
    .expect("seed skill");
    id
}

#[tokio::test]
async fn goal_crud_roundtrip() {
    let app = TestApp::spawn().await;
    app.register_user("goaluser").await;
    app.login("goaluser").await;

    let resp = app
        .post(
            "/api/users/me/goals",
            &json!({ "kind": "artifact_count", "target_value": "5" }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body: Value = resp.json().await.unwrap();
    let goal_id = body["data"]["goal"]["goal"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(body["data"]["goal"]["progress_pct"], 0.0);
    assert_eq!(body["data"]["goal"]["achieved"], false);

    let body: Value = app.get("/api/users/me/goals").await.json().await.unwrap();
    assert_eq!(body["data"]["goals"].as_array().unwrap().len(), 1);

    // PATCH moves the deadline; the response carries fresh progress.
    let future = (chrono::Utc::now() + chrono::Duration::days(30))
        .date_naive()
        .to_string();
    let resp = app
        .put(&format!("/api/users/me/goals/{goal_id}"), &json!({}))
        .await;
    // PUT is not routed for this path — PATCH is. Confirms we did not
    // accidentally widen the verb set.
    assert_eq!(resp.status(), StatusCode::METHOD_NOT_ALLOWED);

    let resp = app
        .client
        .patch(format!("{}/api/users/me/goals/{goal_id}", app.addr))
        .json(&json!({ "deadline": future }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["goal"]["goal"]["deadline"], future);

    let resp = app.delete(&format!("/api/users/me/goals/{goal_id}")).await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    let resp = app.get(&format!("/api/users/me/goals/{goal_id}")).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn artifact_count_progress_tracks_verified_deliverables() {
    let app = TestApp::spawn().await;
    let me = app.register_user("goalcount").await;
    app.login("goalcount").await;
    let my_id = user_id_of(&me);

    seed_verified_deliverables(&app, my_id, 3, 10).await;

    let body: Value = app
        .post(
            "/api/users/me/goals",
            &json!({ "kind": "artifact_count", "target_value": "6" }),
        )
        .await
        .json()
        .await
        .unwrap();

    assert_eq!(body["data"]["goal"]["progress_pct"], 50.0, "3 of 6");
    assert_eq!(body["data"]["goal"]["achieved"], false);
    // 3 deliverables in a 90-day window = 1 per 30 days; 3 remaining = 90 days.
    assert_eq!(body["data"]["goal"]["eta_days_at_current_pace"], 90);
}

#[tokio::test]
async fn rank_goal_progress_is_limited_by_the_missing_criterion() {
    let app = TestApp::spawn().await;
    let me = app.register_user("goalrank").await;
    app.login("goalrank").await;
    let my_id = user_id_of(&me);

    // artisan needs 11 deliverables AND 1 attestation. Give plenty of the
    // former and none of the latter: the surplus must not mask the gap.
    seed_verified_deliverables(&app, my_id, 20, 5).await;

    let body: Value = app
        .post(
            "/api/users/me/goals",
            &json!({ "kind": "rank", "target_value": "artisan" }),
        )
        .await
        .json()
        .await
        .unwrap();

    assert_eq!(
        body["data"]["goal"]["progress_pct"], 50.0,
        "deliverables capped at 100%, attestations at 0% -> mean 50%"
    );
    assert_eq!(body["data"]["goal"]["achieved"], false);
    assert!(
        body["data"]["goal"]["eta_days_at_current_pace"].is_null(),
        "no attestation has ever been issued, so there is no pace to extrapolate"
    );

    let criteria = body["data"]["goal"]["criteria"].as_array().unwrap();
    assert_eq!(criteria.len(), 2);
    let attest = criteria
        .iter()
        .find(|c| c["name"] == "attestations")
        .expect("attestation criterion is reported");
    assert_eq!(attest["current"], 0);
    assert_eq!(attest["required"], 1);
}

#[tokio::test]
async fn rank_goal_reaches_100_when_every_criterion_is_met() {
    let app = TestApp::spawn().await;
    let me = app.register_user("goalranked").await;
    app.login("goalranked").await;
    let my_id = user_id_of(&me);

    seed_verified_deliverables(&app, my_id, 11, 5).await;
    seed_attestation(&app, my_id, 4).await;

    let body: Value = app
        .post(
            "/api/users/me/goals",
            &json!({ "kind": "rank", "target_value": "artisan" }),
        )
        .await
        .json()
        .await
        .unwrap();

    assert_eq!(body["data"]["goal"]["progress_pct"], 100.0);
    assert_eq!(body["data"]["goal"]["achieved"], true);
    assert_eq!(body["data"]["goal"]["eta_days_at_current_pace"], 0);
}

#[tokio::test]
async fn skill_level_goal_reads_proficiency() {
    let app = TestApp::spawn().await;
    let me = app.register_user("goalskill").await;
    app.login("goalskill").await;
    let my_id = user_id_of(&me);
    let skill_id = seed_skill(&app, "goal-test-skill").await;

    // No user_skills row: unproven is level 0, not the DB default of 1.
    let body: Value = app
        .post(
            "/api/users/me/goals",
            &json!({
                "kind": "skill_level",
                "target_value": "4",
                "target_skill_id": skill_id,
            }),
        )
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["goal"]["progress_pct"], 0.0);

    sqlx::query(
        "INSERT INTO user_skills (user_id, skill_id, proven_count, proficiency_level)
         VALUES ($1, $2, 4, 2)",
    )
    .bind(my_id)
    .bind(skill_id)
    .execute(&app.db)
    .await
    .unwrap();

    let body: Value = app.get("/api/users/me/goals").await.json().await.unwrap();
    assert_eq!(
        body["data"]["goals"][0]["progress_pct"], 50.0,
        "level 2 of 4"
    );
}

#[tokio::test]
async fn capability_goal_is_binary() {
    let app = TestApp::spawn().await;
    let me = app.register_user("goalcap").await;
    app.login("goalcap").await;
    let my_id = user_id_of(&me);

    let body: Value = app
        .post(
            "/api/users/me/goals",
            &json!({ "kind": "capability", "target_value": "mentor" }),
        )
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["goal"]["progress_pct"], 0.0);
    assert!(
        body["data"]["goal"]["eta_days_at_current_pace"].is_null(),
        "a capability is granted, not accumulated — no ETA is honest here"
    );

    sqlx::query(
        "INSERT INTO user_capabilities (user_id, capability, granted_reason)
         VALUES ($1, 'mentor', 'test')",
    )
    .bind(my_id)
    .execute(&app.db)
    .await
    .unwrap();

    let body: Value = app.get("/api/users/me/goals").await.json().await.unwrap();
    assert_eq!(body["data"]["goals"][0]["progress_pct"], 100.0);
    assert_eq!(body["data"]["goals"][0]["achieved"], true);
}

#[tokio::test]
async fn goal_validation_rejects_incoherent_targets() {
    let app = TestApp::spawn().await;
    app.register_user("goalvalid").await;
    app.login("goalvalid").await;

    let cases = [
        // Unknown kind.
        json!({ "kind": "vibes", "target_value": "5" }),
        // apprenti is granted at signup — a no-op goal.
        json!({ "kind": "rank", "target_value": "apprenti" }),
        json!({ "kind": "rank", "target_value": "legende" }),
        // skill_level without a skill.
        json!({ "kind": "skill_level", "target_value": "3" }),
        // Out-of-range level.
        json!({ "kind": "skill_level", "target_value": "9",
                "target_skill_id": Uuid::new_v4() }),
        // Non-numeric count.
        json!({ "kind": "artifact_count", "target_value": "many" }),
        json!({ "kind": "artifact_count", "target_value": "0" }),
        // Unknown capability.
        json!({ "kind": "capability", "target_value": "supreme_leader" }),
        // target_skill_id on a kind that has no skill.
        json!({ "kind": "artifact_count", "target_value": "5",
                "target_skill_id": Uuid::new_v4() }),
    ];

    for case in cases {
        let resp = app.post("/api/users/me/goals", &case).await;
        assert!(
            resp.status() == StatusCode::BAD_REQUEST || resp.status() == StatusCode::NOT_FOUND,
            "payload {case} should be rejected, got {}",
            resp.status()
        );
    }

    // A past deadline is refused too.
    let past = (chrono::Utc::now() - chrono::Duration::days(1))
        .date_naive()
        .to_string();
    let resp = app
        .post(
            "/api/users/me/goals",
            &json!({ "kind": "artifact_count", "target_value": "5", "deadline": past }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn duplicate_live_goal_is_a_conflict() {
    let app = TestApp::spawn().await;
    app.register_user("goaldup").await;
    app.login("goaldup").await;

    let payload = json!({ "kind": "rank", "target_value": "ranger" });
    let resp = app.post("/api/users/me/goals", &payload).await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app.post("/api/users/me/goals", &payload).await;
    assert_eq!(
        resp.status(),
        StatusCode::CONFLICT,
        "the partial unique index must surface as a 409, not a 500"
    );
}

#[tokio::test]
async fn archival_sweep_stamps_and_archives() {
    let app = TestApp::spawn().await;
    let me = app.register_user("goalsweep").await;
    app.login("goalsweep").await;
    let my_id = user_id_of(&me);

    seed_verified_deliverables(&app, my_id, 5, 10).await;

    // Achieved goal: 5 verified deliverables against a target of 5.
    app.post(
        "/api/users/me/goals",
        &json!({ "kind": "artifact_count", "target_value": "5" }),
    )
    .await;

    // Unachieved goal whose deadline has lapsed. Inserted directly because
    // the API refuses past deadlines.
    sqlx::query(
        "INSERT INTO user_goals (user_id, kind, target_value, deadline)
         VALUES ($1, 'artifact_count', '999', CURRENT_DATE - 1)",
    )
    .bind(my_id)
    .execute(&app.db)
    .await
    .unwrap();

    let report = goals::run_archival_sweep(&app.db).await.expect("sweep");
    assert_eq!(report.newly_achieved, 1);
    assert_eq!(report.archived_achieved, 1);
    assert_eq!(report.archived_expired, 1);

    // Default listing is now empty; both goals remain readable on request.
    let body: Value = app.get("/api/users/me/goals").await.json().await.unwrap();
    assert!(body["data"]["goals"].as_array().unwrap().is_empty());

    let body: Value = app
        .get("/api/users/me/goals?include_archived=true")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["goals"].as_array().unwrap().len(), 2);

    // Idempotent: a second sweep changes nothing.
    let report = goals::run_archival_sweep(&app.db).await.expect("sweep");
    assert_eq!(report.newly_achieved, 0);
    assert_eq!(report.archived_achieved, 0);
    assert_eq!(report.archived_expired, 0);
}

#[tokio::test]
async fn goals_are_scoped_to_their_owner() {
    let app = TestApp::spawn().await;
    app.register_user("goalmine").await;
    app.login("goalmine").await;
    let created: Value = app
        .post(
            "/api/users/me/goals",
            &json!({ "kind": "artifact_count", "target_value": "3" }),
        )
        .await
        .json()
        .await
        .unwrap();
    let goal_id = created["data"]["goal"]["goal"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    app.register_user("goaltheirs").await;
    app.login("goaltheirs").await;

    let resp = app.get(&format!("/api/users/me/goals/{goal_id}")).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    let resp = app.delete(&format!("/api/users/me/goals/{goal_id}")).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    let body: Value = app.get("/api/users/me/goals").await.json().await.unwrap();
    assert!(body["data"]["goals"].as_array().unwrap().is_empty());
}
