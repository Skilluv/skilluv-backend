//! The burst limit on the assistant, and the refusal it has to write down.
//!
//! Its own binary, and the only test in it. Every other integration test runs
//! with `SKILLUV_DISABLE_RATELIMIT=1` — a blunt switch the harness sets because
//! several test binaries in parallel used to eat each other's IP buckets. This
//! assertion is about the limiter firing, so it cannot run under that switch,
//! and re-enabling it is a process-global change that would reach every test
//! sharing the binary.
//!
//! It was written in the SKI-295..301 batch and never run: it sat in
//! `test_admin_front_gaps.rs`, where it could only ever have failed.

mod common;

use common::TestApp;
use serde_json::{Value, json};
use uuid::Uuid;

fn user_id_of(register_body: &Value) -> Uuid {
    register_body["data"]["user"]["id"]
        .as_str()
        .expect("register response carries a user id")
        .parse()
        .expect("user id is a uuid")
}

#[tokio::test]
async fn the_burst_limit_is_recorded_as_a_refusal() {
    let app = TestApp::spawn().await;

    // `TestApp::spawn` sets `SKILLUV_DISABLE_RATELIMIT=1`, so this has to be
    // undone after it, and this test has to be the only one in its binary —
    // an env var is process-global and the tests of one binary run in
    // parallel threads.
    //
    // Safe here for two reasons the harness itself relies on: a binary takes
    // its own Redis database (`pid % 16`), and the bucket this exercises is
    // keyed on a user id created a line below. Nothing else can be in it.
    //
    // SAFETY: single-threaded at this point — one test, no other reader.
    unsafe {
        std::env::set_var("SKILLUV_DISABLE_RATELIMIT", "0");
    }

    let me = app.register_user("gapaiburst").await;
    let my_id = user_id_of(&me);
    app.login("gapaiburst").await;

    // The burst window allows three; the fourth is refused. Every call
    // fails on the missing worker, which happens after the limiter.
    for _ in 0..4 {
        let _ = app
            .post(
                "/api/assistant/ask",
                &json!({ "interaction_type": "explain", "prompt": "why does this move?" }),
            )
            .await;
    }

    let refused: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM ai_interactions
          WHERE user_id = $1 AND status = 'rate_limited' AND refusal_kind = 'burst'",
    )
    .bind(my_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(
        refused >= 1,
        "a guard rail whose firing is unobservable cannot be tuned"
    );

    // A refusal is not an AI interaction: it must not eat the quota nor be
    // disclosed on a deliverable.
    let quota: Value = app
        .get("/api/users/me/assistant-quota")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(
        quota["data"]["used"].as_i64(),
        Some(0),
        "nothing reached the worker, so nothing was used"
    );
}
