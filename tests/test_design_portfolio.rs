//! The public design profile — everything a stranger needs in one call.
//!
//! Nine sections, and the rule that holds them together: what is shown is
//! either verified work or a declaration clearly marked as one. Nothing in
//! between, and nothing a recruiter could mistake for the other.

mod common;
use common::TestApp;
use serde_json::Value;
use uuid::Uuid;

async fn a_public_user(app: &TestApp, username: &str) -> Uuid {
    app.register_user(username).await;
    let id: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(username)
        .fetch_one(&app.db)
        .await
        .unwrap();
    // Registration leaves the profile inactive until somebody fills it in.
    sqlx::query("UPDATE users SET profile_active = TRUE WHERE id = $1")
        .bind(id)
        .execute(&app.db)
        .await
        .unwrap();
    id
}

async fn profile(app: &TestApp, username: &str) -> Value {
    let resp = app.get(&format!("/api/users/{username}/design-profile")).await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    resp.json().await.unwrap()
}

#[tokio::test]
async fn an_unfinished_profile_is_absent_rather_than_empty() {
    let app = TestApp::spawn().await;
    // Registered and never filled in. Serving this reads as "this person has
    // done nothing" rather than "this person has not started", which is a
    // different and much worse claim.
    app.register_user("dp_inactive").await;

    let resp = app.get("/api/users/dp_inactive/design-profile").await;
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn a_hidden_profile_reads_as_absent_not_as_forbidden() {
    let app = TestApp::spawn().await;
    let id = a_public_user(&app, "dp_hidden").await;
    sqlx::query("UPDATE users SET profile_hidden = TRUE WHERE id = $1")
        .bind(id)
        .execute(&app.db)
        .await
        .unwrap();

    // "This person exists but you may not see them" leaks the thing being
    // hidden.
    let resp = app.get("/api/users/dp_hidden/design-profile").await;
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn an_empty_profile_answers_with_empty_sections_not_with_zeroes() {
    let app = TestApp::spawn().await;
    a_public_user(&app, "dp_empty").await;

    let body = profile(&app, "dp_empty").await;
    let data = &body["data"];

    assert_eq!(data["username"], "dp_empty");
    assert_eq!(data["missions"]["delivered"], 0);
    // Never rated is null, not zero. A zero on a profile says the opposite of
    // what is true.
    assert!(data["missions"]["rating_average"].is_null(), "{body}");
    assert!(data["portfolios"].as_array().unwrap().is_empty());
    assert!(data["iteration_stories"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn only_a_confirmed_portfolio_is_shown() {
    let app = TestApp::spawn().await;
    let id = a_public_user(&app, "dp_portfolio").await;

    sqlx::query(
        "INSERT INTO external_signals (user_id, provider, url, title, verified_at,
                                       verification_method)
         VALUES ($1, 'behance', 'https://behance.net/confirme', 'Portfolio',
                 NOW(), 'manual_review'),
                ($1, 'dribbble', 'https://dribbble.com/affirme', 'Portfolio', NULL, NULL)",
    )
    .bind(id)
    .execute(&app.db)
    .await
    .unwrap();

    let body = profile(&app, "dp_portfolio").await;
    let providers: Vec<&str> = body["data"]["portfolios"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["provider"].as_str().unwrap())
        .collect();

    // A URL somebody typed is not evidence, and this is a page a recruiter
    // reads.
    assert_eq!(providers, vec!["behance"], "{body}");
}

#[tokio::test]
async fn availability_is_a_declaration_and_sits_apart_from_the_proofs() {
    let app = TestApp::spawn().await;
    let id = a_public_user(&app, "dp_available").await;
    sqlx::query(
        "UPDATE users SET available_for_hire = TRUE, looking_for = 'freelance' WHERE id = $1",
    )
    .bind(id)
    .execute(&app.db)
    .await
    .unwrap();

    let body = profile(&app, "dp_available").await;
    assert_eq!(body["data"]["availability"]["available_for_missions"], true);
    assert_eq!(body["data"]["availability"]["looking_for"], "freelance");

    // It is its own section, not folded into the score or the artefacts:
    // saying you are available is not a thing anybody verified.
    assert!(body["data"]["craft_score"].get("available").is_none());
}

#[tokio::test]
async fn a_profile_that_does_not_exist_is_a_404() {
    let app = TestApp::spawn().await;
    assert_eq!(
        app.get("/api/users/dp_nobody/design-profile").await.status(),
        404
    );
}
