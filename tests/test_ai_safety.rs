//! Red-team findings, and the order in which they are allowed to come out.

mod common;
use common::TestApp;
use serde_json::json;
use uuid::Uuid;

async fn a_user(app: &TestApp, username: &str) -> Uuid {
    app.register_user(username).await;
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT id FROM users WHERE username = '{username}'"
    )))
    .fetch_one(&app.db)
    .await
    .unwrap()
}

async fn a_safety_slice(app: &TestApp, owner: Uuid) -> Uuid {
    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO projects (slug, name, owner_type, owner_id)
         VALUES ($1, 'Projet sûreté', 'user', $2)
         RETURNING id",
    )
    .bind(format!("proj-{}", Uuid::new_v4().simple()))
    .bind(owner)
    .fetch_one(&app.db)
    .await
    .unwrap();

    let slice: Uuid = sqlx::query_scalar(
        "INSERT INTO project_slices
            (project_id, title, description, primary_domain, slice_type,
             ai_subtype, published_artifact_url, difficulty, orientation_id)
         VALUES ($1, 'Red-team', 'x', 'ai', 'ai_artifact', 'ai_research_paper',
                 'https://arxiv.org/abs/0000.00000', 4,
                 (SELECT id FROM orientations WHERE slug = 'ai-safety-researcher'))
         RETURNING id",
    )
    .bind(project)
    .fetch_one(&app.db)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO deliverables
            (user_id, slice_id, artifact_type, artifact_url, verifiable_by,
             verification_status, verified_at)
         VALUES ($1, $2, 'disclosure', 'https://example.test/report', 'human_review',
                 'verified', NOW())",
    )
    .bind(owner)
    .bind(slice)
    .execute(&app.db)
    .await
    .unwrap();

    slice
}

fn a_finding_body() -> serde_json::Value {
    json!({
        "target_model": "Mistral-7B-Instruct",
        "target_version": "v0.3",
        "attack_type": "jailbreak",
        "reproduction_md": "Conversation en trois tours, la consigne étant \
                            réintroduite comme citation au deuxième tour. \
                            Température 0,7, même graine.",
        "observed_output": "Le modèle produit la consigne interdite mot pour mot.",
        "attempts": 50,
        "successes": 31,
        "severity_tier": "high",
        "severity_rationale_md": "Contournement reproductible sans accès privilégié.",
        "mitigation_proposed_md": "Séparer instruction et donnée dans le gabarit de conversation."
    })
}

#[tokio::test]
async fn a_finding_needs_a_denominator() {
    let app = TestApp::spawn().await;
    let author = a_user(&app, "safe_denom").await;
    let slice = a_safety_slice(&app, author).await;
    app.login("safe_denom").await;

    let mut body = a_finding_body();
    body["attempts"] = json!(0);

    let resp = app
        .post(&format!("/api/slices/{slice}/safety-reports"), &body)
        .await;
    assert!(
        !resp.status().is_success(),
        "seven out of ten and seven out of a thousand are different claims"
    );
}

#[tokio::test]
async fn no_successful_attempt_is_not_a_finding() {
    let app = TestApp::spawn().await;
    let author = a_user(&app, "safe_zero").await;
    let slice = a_safety_slice(&app, author).await;
    app.login("safe_zero").await;

    let mut body = a_finding_body();
    body["successes"] = json!(0);

    let resp = app
        .post(&format!("/api/slices/{slice}/safety-reports"), &body)
        .await;
    assert_eq!(
        resp.status().as_u16(),
        400,
        "a model behaving as it should is not a vulnerability"
    );
}

#[tokio::test]
async fn a_finding_without_a_mitigation_is_refused() {
    let app = TestApp::spawn().await;
    let author = a_user(&app, "safe_nomit").await;
    let slice = a_safety_slice(&app, author).await;
    app.login("safe_nomit").await;

    let mut body = a_finding_body();
    body["mitigation_proposed_md"] = json!("à voir");

    let resp = app
        .post(&format!("/api/slices/{slice}/safety-reports"), &body)
        .await;
    assert!(
        !resp.status().is_success(),
        "reporting without proposing leaves the whole problem with the reader"
    );
}

#[tokio::test]
async fn a_finding_cannot_be_published_before_anyone_is_told() {
    let app = TestApp::spawn().await;
    let author = a_user(&app, "safe_order").await;
    let slice = a_safety_slice(&app, author).await;
    app.login("safe_order").await;

    let created = app
        .post(
            &format!("/api/slices/{slice}/safety-reports"),
            &a_finding_body(),
        )
        .await;
    assert_eq!(created.status().as_u16(), 200, "{:?}", created.text().await);
    let body: serde_json::Value = created.json().await.unwrap();
    let id = body["data"]["id"].as_str().unwrap().to_string();

    // The schema would accept the row. The order is what makes it wrong: this
    // is how a working attack reaches the internet before the person who
    // could fix it has heard of it.
    let refused = app
        .patch(
            &format!("/api/safety-reports/{id}/disclosure"),
            &json!({"status": "published"}),
        )
        .await;
    assert_eq!(refused.status().as_u16(), 400);
}

#[tokio::test]
async fn the_embargo_defaults_to_ninety_days_from_the_notification() {
    let app = TestApp::spawn().await;
    let author = a_user(&app, "safe_embargo").await;
    let slice = a_safety_slice(&app, author).await;
    app.login("safe_embargo").await;

    let created = app
        .post(
            &format!("/api/slices/{slice}/safety-reports"),
            &a_finding_body(),
        )
        .await;
    let body: serde_json::Value = created.json().await.unwrap();
    let id = body["data"]["id"].as_str().unwrap().to_string();

    let notified = app
        .patch(
            &format!("/api/safety-reports/{id}/disclosure"),
            &json!({"status": "vendor_notified"}),
        )
        .await;
    assert_eq!(notified.status().as_u16(), 200);

    let embargoed = app
        .patch(
            &format!("/api/safety-reports/{id}/disclosure"),
            &json!({"status": "embargoed"}),
        )
        .await;
    assert_eq!(embargoed.status().as_u16(), 200);

    let days: Option<f64> = sqlx::query_scalar(
        // EXTRACT returns NUMERIC on PostgreSQL 14+, and sqlx will not decode
        // that into an f64. The cast is the fix, not a wider Rust type.
        "SELECT (EXTRACT(EPOCH FROM (embargo_until - vendor_notified_at)) / 86400)
                    ::DOUBLE PRECISION
           FROM ai_safety_reports WHERE id = $1::uuid",
    )
    .bind(&id)
    .fetch_one(&app.db)
    .await
    .unwrap();

    let days = days.expect("both dates set");
    assert!(
        (days - 90.0).abs() < 1.0,
        "expected ninety days, got {days}"
    );
}

#[tokio::test]
async fn a_vendor_who_fixes_fast_does_not_need_an_embargo() {
    let app = TestApp::spawn().await;
    let author = a_user(&app, "safe_fast").await;
    let slice = a_safety_slice(&app, author).await;
    app.login("safe_fast").await;

    let created = app
        .post(
            &format!("/api/slices/{slice}/safety-reports"),
            &a_finding_body(),
        )
        .await;
    let body: serde_json::Value = created.json().await.unwrap();
    let id = body["data"]["id"].as_str().unwrap().to_string();

    app.patch(
        &format!("/api/safety-reports/{id}/disclosure"),
        &json!({"status": "vendor_notified"}),
    )
    .await;

    let published = app
        .patch(
            &format!("/api/safety-reports/{id}/disclosure"),
            &json!({"status": "published"}),
        )
        .await;
    assert_eq!(published.status().as_u16(), 200);
}

#[tokio::test]
async fn withholding_says_why() {
    let app = TestApp::spawn().await;
    let author = a_user(&app, "safe_withheld").await;
    let slice = a_safety_slice(&app, author).await;
    app.login("safe_withheld").await;

    let created = app
        .post(
            &format!("/api/slices/{slice}/safety-reports"),
            &a_finding_body(),
        )
        .await;
    let body: serde_json::Value = created.json().await.unwrap();
    let id = body["data"]["id"].as_str().unwrap().to_string();

    app.patch(
        &format!("/api/safety-reports/{id}/disclosure"),
        &json!({"status": "vendor_notified"}),
    )
    .await;

    // Withholding with no stated ground is indistinguishable from burying.
    let refused = app
        .patch(
            &format!("/api/safety-reports/{id}/disclosure"),
            &json!({"status": "withheld"}),
        )
        .await;
    assert!(!refused.status().is_success());

    let accepted = app
        .patch(
            &format!("/api/safety-reports/{id}/disclosure"),
            &json!({
                "status": "withheld",
                "withheld_reason_md": "L'exploitation complète apprend plus à un \
                                       attaquant qu'elle n'aide un défenseur."
            }),
        )
        .await;
    assert_eq!(accepted.status().as_u16(), 200);
}

#[tokio::test]
async fn unpublished_findings_are_not_readable_without_an_account() {
    let app = TestApp::spawn().await;
    let author = a_user(&app, "safe_private").await;
    let slice = a_safety_slice(&app, author).await;

    // An unpublished finding is a working attack. A public listing of those
    // would make this the fastest place on the internet to shop for one.
    let anonymous = reqwest::Client::new()
        .get(format!("{}/api/slices/{slice}/safety-reports", app.addr))
        .send()
        .await
        .unwrap();
    assert_eq!(anonymous.status().as_u16(), 401);
}
