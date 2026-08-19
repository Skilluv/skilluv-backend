//! Ops missions: on-call, production access, and the two positions the
//! schema now refuses to publish without.

mod common;
use common::TestApp;
use serde_json::{Value, json};
use uuid::Uuid;

async fn an_enterprise(app: &TestApp, company: &str) -> Uuid {
    app.register_enterprise(company).await;
    // `register_enterprise` verifies the e-mail and stops there. Every
    // `/enterprise/*` route also wants a session and a second factor, so
    // without these two lines the gate answers 403 and every assertion in this
    // file reads as an authorisation failure rather than the thing it tests.
    let username = company.to_lowercase().replace(' ', "");
    app.login(&username).await;
    app.enable_totp_for(&username).await;
    sqlx::query_scalar(
        "SELECT e.id FROM enterprises e JOIN users u ON u.id = e.owner_id
          WHERE e.company_name = $1",
    )
    .bind(company)
    .fetch_one(&app.db)
    .await
    .unwrap()
}

fn an_ops_mission(slug: &str) -> Value {
    json!({
        "slug": slug,
        "mission_type_slug": "ops_infra_build",
        "title": "Reprendre le cluster",
        "description": "Un cluster monté à la main, à décrire en manifestes.",
        "acceptance_criteria": "Le chart s'applique deux fois sans différence, et le README suffit.",
        "deliverable_format": "iac_repository",
        "target_platforms": ["aws"],
        "budget_eur": "6000.00",
    })
}

#[tokio::test]
async fn an_ops_deliverable_is_not_a_pull_request() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Clusterco").await;

    let created = app
        .post("/api/missions", &an_ops_mission("cluster-one"))
        .await;
    assert_eq!(created.status(), 200, "{}", created.text().await.unwrap());

    drop(app);
}

#[tokio::test]
async fn on_call_without_a_retainer_cannot_be_published() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Pagerco").await;

    // Everything about this mission is fine except that it asks somebody to
    // be reachable and pays them per deliverable for it.
    let mut body = an_ops_mission("pager-unpaid");
    body["includes_oncall"] = json!(true);
    body["oncall_window"] = json!("18h-08h, Africa/Abidjan");
    body["oncall_response_minutes"] = json!(30);

    let resp = app.post("/api/missions", &body).await;
    assert_eq!(resp.status(), 400);

    let jv: Value = resp.json().await.unwrap();
    let message = jv["error"]["message"].as_str().unwrap();
    assert!(
        message.contains("retainer") || message.contains("unpaid"),
        "the refusal says why rather than naming a constraint: {message}"
    );

    // With the retainer, the same mission is publishable.
    body["payment_model"] = json!("retainer_monthly");
    let resp = app.post("/api/missions", &body).await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    drop(app);
}

#[tokio::test]
async fn on_call_without_a_window_or_a_response_time_is_not_on_call() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Vagueops").await;

    let mut body = an_ops_mission("pager-vague");
    body["includes_oncall"] = json!(true);
    body["payment_model"] = json!("retainer_monthly");
    // No window, no response time: "be available" with no hours is a
    // twenty-four hour obligation nobody named.
    let resp = app.post("/api/missions", &body).await;
    assert_eq!(resp.status(), 400);

    drop(app);
}

#[tokio::test]
async fn production_access_forces_an_nda() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Prodco").await;

    let mut body = an_ops_mission("prod-access");
    body["production_access_required"] = json!(true);
    body["nda_required"] = json!(false);

    let resp = app.post("/api/missions", &body).await;
    assert_eq!(resp.status(), 400);

    body["nda_required"] = json!(true);
    let resp = app.post("/api/missions", &body).await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    drop(app);
}

#[tokio::test]
async fn applying_to_an_on_call_mission_means_answering_about_on_call() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Rotationco").await;

    let mut body = an_ops_mission("rotation-one");
    body["includes_oncall"] = json!(true);
    body["oncall_window"] = json!("nuits en semaine, Europe/Paris");
    body["oncall_response_minutes"] = json!(15);
    body["payment_model"] = json!("retainer_monthly");

    assert_eq!(app.post("/api/missions", &body).await.status(), 200);
    assert_eq!(
        app.post(
            "/api/missions/rotation-one/status",
            &json!({"status": "published"})
        )
        .await
        .status(),
        200
    );

    app.register_user("ops_applicant").await;
    app.login("ops_applicant").await;

    let resp = app
        .post(
            "/api/missions/rotation-one/apply",
            &json!({ "cover_letter": "Je connais ce genre de cluster et je suis disponible." }),
        )
        .await;
    assert_eq!(
        resp.status(),
        400,
        "answering after selection is how somebody agrees to a rotation they cannot hold"
    );

    let resp = app
        .post(
            "/api/missions/rotation-one/apply",
            &json!({
                "cover_letter": "Je connais ce genre de cluster et je suis disponible.",
                "oncall_available": false,
                "oncall_experience": "never",
            }),
        )
        .await;
    // Saying no is an answer, not a disqualification: the enterprise decides,
    // and it decides knowing.
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    drop(app);
}
