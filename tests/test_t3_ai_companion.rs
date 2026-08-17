//! Integration tests for SKI-44 — disclosed AI learning companion.
//!
//! The AI worker is not running in the test environment (`grpc_ai_url` is
//! `None` in the test harness), which is exactly the deployment state the
//! ticket lists as a prerequisite. That makes the unavailable path the one
//! these tests can exercise end to end — and it is the one that matters
//! most, because it is where an interaction could silently go unrecorded.
//!
//! Hashing, caching and quota arithmetic are unit-tested in
//! `services::ai_companion`.

mod common;

use common::TestApp;
use reqwest::StatusCode;
use serde_json::{Value, json};
use uuid::Uuid;

use skilluv_backend::services::ai_companion;

fn user_id_of(register_body: &Value) -> Uuid {
    register_body["data"]["user"]["id"]
        .as_str()
        .expect("register response carries a user id")
        .parse()
        .expect("user id is a uuid")
}

#[tokio::test]
async fn unavailable_worker_still_records_the_interaction() {
    let app = TestApp::spawn().await;
    let me = app.register_user("aicompanion").await;
    let my_id = user_id_of(&me);
    app.login("aicompanion").await;

    let resp = app
        .post(
            "/api/assistant/ask",
            &json!({
                "interaction_type": "explain",
                "prompt": "Explique-moi le borrow checker",
            }),
        )
        .await;
    assert_eq!(
        resp.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "a missing AI worker is a 503, not a 500"
    );

    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT interaction_type, status FROM ai_interactions WHERE user_id = $1")
            .bind(my_id)
            .fetch_all(&app.db)
            .await
            .unwrap();
    assert_eq!(
        rows.len(),
        1,
        "an interaction that produced no answer is still an interaction that happened"
    );
    assert_eq!(rows[0].0, "explain");
    assert_eq!(rows[0].1, "unavailable");
}

#[tokio::test]
async fn failed_calls_do_not_consume_quota() {
    let app = TestApp::spawn().await;
    let me = app.register_user("aiquota").await;
    let my_id = user_id_of(&me);
    app.login("aiquota").await;

    // The worker is down, so every call fails. Quota counts only calls
    // that actually reached it.
    for i in 0..3 {
        let resp = app
            .post(
                "/api/assistant/ask",
                &json!({
                    "interaction_type": "debug_help",
                    "prompt": format!("question numero {i}"),
                }),
            )
            .await;
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    let body: Value = app
        .get("/api/users/me/assistant-quota")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(
        body["data"]["used"], 0,
        "a learner must not be billed an allowance for an answer they never got"
    );
    assert_eq!(body["data"]["remaining"], ai_companion::DAILY_QUOTA);

    // The ledger still shows the attempts.
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ai_interactions WHERE user_id = $1")
        .bind(my_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(count, 3);
}

#[tokio::test]
async fn quota_blocks_once_the_daily_allowance_is_spent() {
    let app = TestApp::spawn().await;
    let me = app.register_user("aispent").await;
    let my_id = user_id_of(&me);
    app.login("aispent").await;

    // Simulate a day's worth of successful calls directly, since the
    // worker is unavailable in tests.
    for i in 0..ai_companion::DAILY_QUOTA {
        sqlx::query(
            "INSERT INTO ai_interactions (user_id, interaction_type, prompt, status)
             VALUES ($1, 'explain', $2, 'ok')",
        )
        .bind(my_id)
        .bind(format!("prompt {i}"))
        .execute(&app.db)
        .await
        .unwrap();
    }

    let body: Value = app
        .get("/api/users/me/assistant-quota")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["remaining"], 0);

    let resp = app
        .post(
            "/api/assistant/ask",
            &json!({ "interaction_type": "explain", "prompt": "one more please" }),
        )
        .await;
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "the quota is refused before the worker is contacted"
    );

    // Old interactions fall out of the rolling window.
    sqlx::query(
        "UPDATE ai_interactions SET created_at = NOW() - INTERVAL '25 hours'
          WHERE user_id = $1",
    )
    .bind(my_id)
    .execute(&app.db)
    .await
    .unwrap();
    let body: Value = app
        .get("/api/users/me/assistant-quota")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["remaining"], ai_companion::DAILY_QUOTA);
}

#[tokio::test]
async fn request_validation_happens_before_anything_is_spent() {
    let app = TestApp::spawn().await;
    let me = app.register_user("aivalid").await;
    let my_id = user_id_of(&me);
    app.login("aivalid").await;

    let cases = [
        json!({ "interaction_type": "write_my_homework", "prompt": "do it" }),
        json!({ "interaction_type": "explain", "prompt": "" }),
        json!({ "interaction_type": "explain", "prompt": "   " }),
        json!({ "interaction_type": "explain", "prompt": "x".repeat(4001) }),
        json!({ "interaction_type": "pre_review", "prompt": "check this",
                "code": "x".repeat(20_001) }),
    ];

    for case in cases {
        let resp = app.post("/api/assistant/ask", &case).await;
        assert_eq!(
            resp.status(),
            StatusCode::BAD_REQUEST,
            "payload should be rejected on shape alone"
        );
    }

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ai_interactions WHERE user_id = $1")
        .bind(my_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(
        count, 0,
        "a malformed request is not an interaction and must not enter the ledger"
    );
}

#[tokio::test]
async fn the_ledger_is_readable_by_its_own_user() {
    let app = TestApp::spawn().await;
    let me = app.register_user("ailedger").await;
    let my_id = user_id_of(&me);

    sqlx::query(
        "INSERT INTO ai_interactions
            (user_id, interaction_type, prompt, status, disclosure_label)
         VALUES ($1, 'explain', 'ma question', 'ok', 'Assistance IA disclosed')",
    )
    .bind(my_id)
    .execute(&app.db)
    .await
    .unwrap();

    app.login("ailedger").await;
    let body: Value = app
        .get("/api/users/me/assistant-interactions")
        .await
        .json()
        .await
        .unwrap();
    let interactions = body["data"]["interactions"].as_array().unwrap();
    assert_eq!(interactions.len(), 1);
    assert_eq!(interactions[0]["prompt"], "ma question");
    assert_eq!(
        interactions[0]["disclosure_label"], "Assistance IA disclosed",
        "disclosure is something the user can inspect, not only something done to them"
    );

    // Another user sees nothing of it.
    app.register_user("aisnooper").await;
    app.login("aisnooper").await;
    let body: Value = app
        .get("/api/users/me/assistant-interactions")
        .await
        .json()
        .await
        .unwrap();
    assert!(body["data"]["interactions"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn disclosure_attaches_recent_interactions_to_a_deliverable() {
    let app = TestApp::spawn().await;
    let me = app.register_user("aidisclose").await;
    let my_id = user_id_of(&me);

    // Two recent interactions, one stale beyond the window.
    for (kind, age_days) in [("explain", 1), ("pre_review", 2), ("debug_help", 60)] {
        sqlx::query(
            "INSERT INTO ai_interactions
                (user_id, interaction_type, prompt, status, created_at)
             VALUES ($1, $2, 'q', 'ok', NOW() - MAKE_INTERVAL(days => $3::INT))",
        )
        .bind(my_id)
        .bind(kind)
        .bind(age_days)
        .execute(&app.db)
        .await
        .unwrap();
    }
    // A failed one, which is not disclosable — no help was received.
    sqlx::query(
        "INSERT INTO ai_interactions (user_id, interaction_type, prompt, status)
         VALUES ($1, 'explain', 'q', 'unavailable')",
    )
    .bind(my_id)
    .execute(&app.db)
    .await
    .unwrap();

    // A deliverable to attach them to.
    let project_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO projects (id, slug, name, owner_type, owner_id)
         VALUES ($1, 'ai-disclose-project', 'AI project', 'user', $2)",
    )
    .bind(project_id)
    .bind(my_id)
    .execute(&app.db)
    .await
    .unwrap();
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
    .unwrap();
    let deliverable_id: Uuid = sqlx::query_scalar(
        "INSERT INTO deliverables
            (slice_id, user_id, artifact_type, artifact_url, verifiable_by,
             verification_status)
         VALUES ($1, $2, 'pr_merged', 'https://example.test/pr/1',
                 'human_review', 'pending')
         RETURNING id",
    )
    .bind(slice_id)
    .bind(my_id)
    .fetch_one(&app.db)
    .await
    .unwrap();

    let report = ai_companion::disclose_on_deliverable(&app.db, my_id, deliverable_id)
        .await
        .expect("disclosure sweep");
    assert_eq!(
        report.interactions_attached, 2,
        "only successful interactions inside the window are attached"
    );
    assert_eq!(report.types, vec!["explain", "pre_review"]);

    // The artifact itself carries the disclosure, so a reviewer sees it
    // without needing to know the ledger exists.
    let signal: Value =
        sqlx::query_scalar("SELECT verification_signal FROM deliverables WHERE id = $1")
            .bind(deliverable_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(signal["ai_companion"]["interactions"], 2);
    assert_eq!(
        signal["ai_companion"]["window_days"],
        ai_companion::DISCLOSURE_WINDOW_DAYS
    );

    // Idempotent: a second sweep attaches nothing.
    let report = ai_companion::disclose_on_deliverable(&app.db, my_id, deliverable_id)
        .await
        .expect("second sweep");
    assert_eq!(report.interactions_attached, 0);

    // The stale interaction is still undisclosed and shows up as such.
    app.login("aidisclose").await;
    let body: Value = app
        .get("/api/users/me/assistant-interactions?undisclosed_only=true")
        .await
        .json()
        .await
        .unwrap();
    let undisclosed = body["data"]["interactions"].as_array().unwrap();
    assert_eq!(undisclosed.len(), 2, "the stale one plus the failed one");
}

#[tokio::test]
async fn companion_requires_authentication() {
    let app = TestApp::spawn().await;
    let resp = app
        .post(
            "/api/assistant/ask",
            &json!({ "interaction_type": "explain", "prompt": "hello" }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    let resp = app.get("/api/users/me/assistant-quota").await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
