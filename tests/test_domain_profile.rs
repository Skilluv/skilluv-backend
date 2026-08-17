//! The domain wizard: closed vocabularies, and one table instead of thirty
//! columns.

mod common;
use common::TestApp;
use serde_json::json;

#[tokio::test]
async fn an_unanswered_wizard_reads_as_an_empty_object() {
    let app = TestApp::spawn().await;
    app.register_user("wiz_empty").await;

    let resp = app.get("/api/users/me/domain-profile/ai").await;
    assert_eq!(resp.status().as_u16(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["answers"], json!({}));
}

#[tokio::test]
async fn the_answers_come_back_as_they_went_in() {
    let app = TestApp::spawn().await;
    app.register_user("wiz_full").await;

    let resp = app
        .put(
            "/api/users/me/domain-profile/ai",
            &json!({
                "level": "practitioner",
                "weekly_hours": "3_10",
                "goal": "portfolio",
                "compute": "none",
                "main_framework": "pytorch",
                "huggingface_username": "quelquun"
            }),
        )
        .await;
    assert_eq!(resp.status().as_u16(), 200, "{:?}", resp.text().await);

    let body: serde_json::Value = app
        .get("/api/users/me/domain-profile/ai")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["answers"]["compute"], "none");
    assert_eq!(body["data"]["answers"]["main_framework"], "pytorch");
    assert_eq!(body["data"]["answers"]["huggingface_username"], "quelquun");
}

#[tokio::test]
async fn an_answer_outside_the_vocabulary_is_refused() {
    let app = TestApp::spawn().await;
    app.register_user("wiz_bad").await;

    // A recommender reading an unknown value has no branch for it and
    // silently recommends nothing, which looks like an empty platform rather
    // than a bad answer.
    let resp = app
        .put(
            "/api/users/me/domain-profile/ai",
            &json!({"compute": "un gros ordinateur"}),
        )
        .await;
    assert_eq!(resp.status().as_u16(), 400);
}

#[tokio::test]
async fn an_unknown_domain_is_refused_by_the_handler_not_the_database() {
    let app = TestApp::spawn().await;
    app.register_user("wiz_domain").await;

    // A 400 naming the seven, not a 500 from a CHECK violation.
    let resp = app
        .put(
            "/api/users/me/domain-profile/marketing",
            &json!({"level": "senior"}),
        )
        .await;
    assert_eq!(resp.status().as_u16(), 400);
}

#[tokio::test]
async fn a_cleared_answer_is_actually_cleared() {
    let app = TestApp::spawn().await;
    app.register_user("wiz_clear").await;

    app.put(
        "/api/users/me/domain-profile/ai",
        &json!({"level": "senior", "compute": "cloud_large"}),
    )
    .await;

    // Somebody who lost access to a GPU must stop being shown challenges they
    // can no longer run. Merging would have kept the old answer.
    app.put(
        "/api/users/me/domain-profile/ai",
        &json!({"level": "senior"}),
    )
    .await;

    let body: serde_json::Value = app
        .get("/api/users/me/domain-profile/ai")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["answers"]["level"], "senior");
    assert!(body["data"]["answers"].get("compute").is_none());
}

#[tokio::test]
async fn two_domains_do_not_overwrite_each_other() {
    let app = TestApp::spawn().await;
    app.register_user("wiz_two").await;

    app.put(
        "/api/users/me/domain-profile/ai",
        &json!({"main_framework": "jax"}),
    )
    .await;
    app.put(
        "/api/users/me/domain-profile/code",
        &json!({"level": "debutant"}),
    )
    .await;

    let ai: serde_json::Value = app
        .get("/api/users/me/domain-profile/ai")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(ai["data"]["answers"]["main_framework"], "jax");
}

#[tokio::test]
async fn the_wizard_needs_an_account() {
    let app = TestApp::spawn().await;
    let resp = reqwest::Client::new()
        .get(format!("{}/api/users/me/domain-profile/ai", app.addr))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 401);
}

#[tokio::test]
async fn skipping_is_not_the_same_as_answering_nothing() {
    let app = TestApp::spawn().await;
    app.register_user("wiz_skip").await;

    let before: serde_json::Value = app
        .get("/api/users/me/domain-profile/ai")
        .await
        .json()
        .await
        .unwrap();
    assert!(before["data"]["skipped_at"].is_null());
    assert!(before["data"]["completed_at"].is_null());

    let resp = app
        .post("/api/users/me/domain-profile/ai/skip", &json!({}))
        .await;
    assert_eq!(resp.status().as_u16(), 200, "{:?}", resp.text().await);

    // Without this distinction the wizard reappears forever for exactly the
    // people who least wanted it.
    let after: serde_json::Value = app
        .get("/api/users/me/domain-profile/ai")
        .await
        .json()
        .await
        .unwrap();
    assert!(!after["data"]["skipped_at"].is_null());
    assert!(after["data"]["completed_at"].is_null());
}

#[tokio::test]
async fn answering_after_skipping_undoes_the_skip() {
    let app = TestApp::spawn().await;
    app.register_user("wiz_unskip").await;

    app.post("/api/users/me/domain-profile/ai/skip", &json!({}))
        .await;
    app.put(
        "/api/users/me/domain-profile/ai",
        &json!({"level": "practitioner"}),
    )
    .await;

    // Somebody who said "stop asking" and then answered has changed their
    // mind; leaving the old timestamp would keep the profile reading as
    // skipped forever.
    let after: serde_json::Value = app
        .get("/api/users/me/domain-profile/ai")
        .await
        .json()
        .await
        .unwrap();
    assert!(after["data"]["skipped_at"].is_null());
    assert!(!after["data"]["completed_at"].is_null());
}

#[tokio::test]
async fn the_two_wizards_do_not_share_a_row() {
    let app = TestApp::spawn().await;
    app.register_user("wiz_both").await;

    app.put(
        "/api/users/me/domain-profile/ai",
        &json!({"main_framework": "jax"}),
    )
    .await;
    app.post("/api/users/me/domain-profile/code/skip", &json!({}))
        .await;

    // Migration 0235 brought the code answers into this table. One row per
    // person per domain: answering one wizard must not mark the other done.
    let ai: serde_json::Value = app
        .get("/api/users/me/domain-profile/ai")
        .await
        .json()
        .await
        .unwrap();
    let code: serde_json::Value = app
        .get("/api/users/me/domain-profile/code")
        .await
        .json()
        .await
        .unwrap();

    assert_eq!(ai["data"]["answers"]["main_framework"], "jax");
    assert!(!ai["data"]["completed_at"].is_null());
    assert!(code["data"]["completed_at"].is_null());
    assert!(!code["data"]["skipped_at"].is_null());
}
