//! The applicant tracker.
//!
//! Three things this suite exists to hold, because each of them is a promise
//! made to somebody who is not the customer: a plan's ceiling is real, a
//! refusal carries a reason, and nothing is kept forever.

mod common;
use common::TestApp;
use serde_json::{Value, json};
use uuid::Uuid;

async fn a_company_with_a_tracker(app: &TestApp, name: &str, plan: &str) -> Uuid {
    app.register_enterprise(name).await;
    let resp = app
        .post("/api/ats/subscription", &json!({ "plan": plan }))
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    sqlx::query_scalar("SELECT id FROM enterprises WHERE company_name = $1")
        .bind(name)
        .fetch_one(&app.db)
        .await
        .unwrap()
}

async fn an_opening(app: &TestApp, title: &str) -> Uuid {
    let resp = app
        .post("/api/ats/openings", &json!({ "title": title }))
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let jv: Value = resp.json().await.unwrap();
    Uuid::parse_str(jv["data"]["opening"]["id"].as_str().unwrap()).unwrap()
}

#[tokio::test]
async fn the_free_plan_is_active_the_moment_it_is_chosen() {
    let app = TestApp::spawn().await;
    app.register_enterprise("Freeco").await;

    let resp = app
        .post("/api/ats/subscription", &json!({ "plan": "ats_free" }))
        .await;
    assert_eq!(resp.status(), 200);

    let jv: Value = resp.json().await.unwrap();
    // Nothing to pay, so making them wait for a payment of zero would be
    // theatre.
    assert_eq!(jv["data"]["subscription"]["active"], true);

    drop(app);
}

#[tokio::test]
async fn a_paid_plan_waits_for_its_payment() {
    let app = TestApp::spawn().await;
    app.register_enterprise("Payco").await;

    let resp = app
        .post("/api/ats/subscription", &json!({ "plan": "ats_growth" }))
        .await;
    assert_eq!(resp.status(), 200);

    let jv: Value = resp.json().await.unwrap();
    assert_eq!(jv["data"]["subscription"]["active"], false);
    assert_eq!(jv["data"]["subscription"]["monthly_fee"], "199.00");

    // And the tracker is not usable yet: an upgrade takes effect when it is
    // paid for, which is the only version a company can dispute.
    let opening = app
        .post("/api/ats/openings", &json!({ "title": "Développeur" }))
        .await;
    assert_eq!(opening.status(), 400);

    drop(app);
}

#[tokio::test]
async fn the_plan_ceiling_counts_what_is_open_not_what_was_ever_created() {
    let app = TestApp::spawn().await;
    a_company_with_a_tracker(&app, "Capco", "ats_free").await;

    // ats_free allows three at once.
    let first = an_opening(&app, "Poste 1").await;
    an_opening(&app, "Poste 2").await;
    an_opening(&app, "Poste 3").await;

    let fourth = app
        .post("/api/ats/openings", &json!({ "title": "Poste 4" }))
        .await;
    assert_eq!(fourth.status(), 400);
    let jv: Value = fourth.json().await.unwrap();
    let message = jv["error"]["message"].as_str().unwrap();
    assert!(
        message.contains("Gratuit") || message.contains('3'),
        "the refusal names the plan and its ceiling: {message}"
    );

    // Closing one gives it back. A company that finished hiring has not used
    // up a slot forever.
    assert_eq!(
        app.post(&format!("/api/ats/openings/{first}/close"), &json!({}))
            .await
            .status(),
        200
    );
    assert_eq!(
        app.post("/api/ats/openings", &json!({ "title": "Poste 4" }))
            .await
            .status(),
        200
    );

    drop(app);
}

#[tokio::test]
async fn a_candidate_is_somebody_or_the_row_is_refused() {
    let app = TestApp::spawn().await;
    a_company_with_a_tracker(&app, "Ghostco", "ats_free").await;
    let opening = an_opening(&app, "Développeuse").await;

    // No account, no name: a pipeline entry nobody can contact.
    let resp = app
        .post(
            &format!("/api/ats/openings/{opening}/candidates"),
            &json!({ "external_email": "personne@example.com" }),
        )
        .await;
    assert_eq!(resp.status(), 400);

    let resp = app
        .post(
            &format!("/api/ats/openings/{opening}/candidates"),
            &json!({ "external_name": "Awa Diop", "external_email": "awa@example.com" }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    // The same person twice is a data-entry mistake, not two candidates.
    let again = app
        .post(
            &format!("/api/ats/openings/{opening}/candidates"),
            &json!({ "external_name": "Awa Diop", "external_email": "AWA@example.com" }),
        )
        .await;
    assert_eq!(again.status(), 400);

    drop(app);
}

#[tokio::test]
async fn a_refusal_carries_a_reason() {
    let app = TestApp::spawn().await;
    a_company_with_a_tracker(&app, "Silentco", "ats_free").await;
    let opening = an_opening(&app, "Ingénieur").await;

    let created = app
        .post(
            &format!("/api/ats/openings/{opening}/candidates"),
            &json!({ "external_name": "Koffi Mensah" }),
        )
        .await;
    let jv: Value = created.json().await.unwrap();
    let candidate = jv["data"]["candidate_id"].as_str().unwrap().to_string();

    let (rejecting, advancing): (Uuid, Uuid) = sqlx::query_as(
        "SELECT
             (SELECT id FROM ats_stages WHERE opening_id = $1 AND is_terminal_rejected),
             (SELECT id FROM ats_stages WHERE opening_id = $1 AND position = 1)",
    )
    .bind(opening)
    .fetch_one(&app.db)
    .await
    .unwrap();

    // Advancing needs no reason: nobody is owed an explanation for good news.
    assert_eq!(
        app.post(
            &format!("/api/ats/candidates/{candidate}/move"),
            &json!({ "to_stage_id": advancing })
        )
        .await
        .status(),
        200
    );

    // Refusing does.
    let silent = app
        .post(
            &format!("/api/ats/candidates/{candidate}/move"),
            &json!({ "to_stage_id": rejecting }),
        )
        .await;
    assert_eq!(silent.status(), 400);

    let too_short = app
        .post(
            &format!("/api/ats/candidates/{candidate}/move"),
            &json!({ "to_stage_id": rejecting, "reason": "non" }),
        )
        .await;
    assert_eq!(too_short.status(), 400, "'non' is not a reason");

    let resp = app
        .post(
            &format!("/api/ats/candidates/{candidate}/move"),
            &json!({
                "to_stage_id": rejecting,
                "reason": "Profil intéressant mais pas assez d'expérience Kubernetes pour ce poste.",
            }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    // And the reason survives on the move, where the transition is.
    let reason: Option<String> = sqlx::query_scalar(
        "SELECT reason FROM ats_candidate_moves
          WHERE candidate_id = $1::UUID ORDER BY moved_at DESC LIMIT 1",
    )
    .bind(Uuid::parse_str(&candidate).unwrap())
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(reason.unwrap().contains("Kubernetes"));

    drop(app);
}

#[tokio::test]
async fn every_candidate_carries_an_erasure_date() {
    let app = TestApp::spawn().await;
    a_company_with_a_tracker(&app, "Forgetco", "ats_free").await;
    let opening = an_opening(&app, "Analyste").await;

    app.post(
        &format!("/api/ats/openings/{opening}/candidates"),
        &json!({ "external_name": "Fatou Sow" }),
    )
    .await;

    let erase_after: chrono::NaiveDate =
        sqlx::query_scalar("SELECT erase_after FROM ats_candidates WHERE opening_id = $1")
            .bind(opening)
            .fetch_one(&app.db)
            .await
            .unwrap();

    // 180 days on the free plan. An ATS that never forgets is a CV database
    // nobody consented to.
    let expected = (chrono::Utc::now() + chrono::Duration::days(180)).date_naive();
    assert_eq!(erase_after, expected);

    drop(app);
}

#[tokio::test]
async fn a_pipeline_belongs_to_the_company_that_entered_it() {
    let app = TestApp::spawn().await;
    a_company_with_a_tracker(&app, "Mineco", "ats_free").await;
    let opening = an_opening(&app, "Poste privé").await;

    // A second company, logged in over the first.
    a_company_with_a_tracker(&app, "Nosyco", "ats_free").await;

    let resp = app
        .get(&format!("/api/ats/openings/{opening}/pipeline"))
        .await;
    assert_eq!(
        resp.status(),
        404,
        "Skilluv holding these rows does not make them anybody else's to read"
    );

    drop(app);
}

#[tokio::test]
async fn the_product_type_that_named_nothing_is_gone() {
    let app = TestApp::spawn().await;

    let (dead, alive): (i64, i64) = sqlx::query_as(
        "SELECT
             (SELECT count(*) FROM enterprise_product_types WHERE slug = 'subscription_pipeline'),
             (SELECT count(*) FROM enterprise_product_types WHERE slug = 'ats_subscription')",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    // A registry row is what makes a product sellable, and this one named a
    // monthly access to candidate tracking that did not exist.
    assert_eq!(dead, 0);
    assert_eq!(alive, 1);

    drop(app);
}
