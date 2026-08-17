//! The mission board: publishing paid work, applying to it, deciding, and
//! what the money does afterwards.

mod common;
use common::TestApp;
use serde_json::{Value, json};
use uuid::Uuid;

async fn user_id(app: &TestApp, username: &str) -> Uuid {
    sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(username)
        .fetch_one(&app.db)
        .await
        .unwrap()
}

/// An enterprise owner, logged in and through the 2FA gate.
async fn an_enterprise(app: &TestApp, company: &str) -> (String, Uuid) {
    app.register_enterprise(company).await;
    let username = company.to_lowercase().replace(' ', "");
    app.login(&username).await;
    app.enable_totp_for(&username).await;
    let enterprise: Uuid = sqlx::query_scalar(
        "SELECT e.id FROM enterprises e JOIN users u ON u.id = e.owner_id
          WHERE u.username = $1",
    )
    .bind(&username)
    .fetch_one(&app.db)
    .await
    .unwrap();
    (username, enterprise)
}

fn a_mission_body(slug: &str) -> Value {
    json!({
        "slug": slug,
        "mission_type_slug": "backend_service_dev",
        "title": "Une API de facturation",
        "description": "Reprendre un service existant et lui ajouter la facturation.",
        "acceptance_criteria": "Les factures se génèrent, les tests passent, la doc est à jour.",
        "target_languages": ["rust"],
        "target_frameworks": ["axum"],
        "deliverable_format": "github_pr",
        "budget_eur": "4000.00",
    })
}

async fn published(app: &TestApp, slug: &str) -> Value {
    let created = app.post("/api/missions", &a_mission_body(slug)).await;
    assert_eq!(created.status(), 200, "{}", created.text().await.unwrap());
    let live = app
        .post(
            &format!("/api/missions/{slug}/status"),
            &json!({"status": "published"}),
        )
        .await;
    assert_eq!(live.status(), 200, "{}", live.text().await.unwrap());
    live.json().await.unwrap()
}

// ═══════════════════════════════════════════════════════════════════
// Publishing
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn a_draft_is_not_on_the_board() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Draftco").await;
    let created = app
        .post("/api/missions", &a_mission_body("draft-one"))
        .await;
    assert_eq!(created.status(), 200, "{}", created.text().await.unwrap());

    let listed: Value = app.get("/api/missions").await.json().await.unwrap();
    assert!(
        listed["data"]["missions"].as_array().unwrap().is_empty(),
        "a board listing work nobody can take is a board people stop reading"
    );

    // And it is not reachable by its slug either, until it is published.
    assert_eq!(app.get("/api/missions/draft-one").await.status(), 404);
}

#[tokio::test]
async fn a_mission_must_say_what_done_means() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Vagueco").await;
    let mut body = a_mission_body("vague-one");
    body["acceptance_criteria"] = json!("   ");

    let resp = app.post("/api/missions", &body).await;
    assert_eq!(
        resp.status(),
        400,
        "a mission without acceptance criteria ends in an argument about scope"
    );
}

#[tokio::test]
async fn each_payment_model_needs_its_own_number() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Priceco").await;

    // per_hour with a budget and no rate: unpriceable.
    let mut hourly = a_mission_body("hourly-broken");
    hourly["payment_model"] = json!("per_hour");
    assert_eq!(app.post("/api/missions", &hourly).await.status(), 400);

    hourly["slug"] = json!("hourly-fixed");
    hourly["hourly_rate_eur"] = json!("80.00");
    let ok = app.post("/api/missions", &hourly).await;
    assert_eq!(ok.status(), 200, "{}", ok.text().await.unwrap());
}

#[tokio::test]
async fn a_mission_never_goes_backwards() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Flowco").await;
    published(&app, "flow-one").await;

    let back = app
        .post("/api/missions/flow-one/status", &json!({"status": "draft"}))
        .await;
    assert_eq!(back.status(), 400);
}

#[tokio::test]
async fn cancelling_requires_a_reason() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Cancelco").await;
    published(&app, "cancel-one").await;

    let silent = app
        .post(
            "/api/missions/cancel-one/status",
            &json!({"status": "cancelled"}),
        )
        .await;
    assert_eq!(silent.status(), 400, "applicants are owed a sentence");

    let spoken = app
        .post(
            "/api/missions/cancel-one/status",
            &json!({"status": "cancelled", "reason": "le budget a été retiré"}),
        )
        .await;
    assert_eq!(spoken.status(), 200);
}

// ═══════════════════════════════════════════════════════════════════
// Applying
// ═══════════════════════════════════════════════════════════════════

async fn an_applicant(app: &TestApp, name: &str, slug: &str) -> Uuid {
    app.register_user(name).await;
    app.login(name).await;
    let resp = app
        .post(
            &format!("/api/missions/{slug}/apply"),
            &json!({
                "cover_letter": "J'ai déjà repris deux services de facturation en Rust.",
                "portfolio_urls": ["https://github.com/someone"],
                "expertise": [{"name": "rust", "years": 4}],
                "availability_hours_per_week": 20,
            }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    body["data"]["application"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

#[tokio::test]
async fn an_empty_application_cannot_be_compared() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Emptyco").await;
    published(&app, "empty-one").await;

    app.register_user("empty_applicant").await;
    app.login("empty_applicant").await;
    let resp = app
        .post(
            "/api/missions/empty-one/apply",
            &json!({"cover_letter": "  "}),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn a_closed_mission_stops_taking_applications() {
    let app = TestApp::spawn().await;
    let (owner, _) = an_enterprise(&app, "Closedco").await;
    published(&app, "closed-one").await;

    app.post(
        "/api/missions/closed-one/status",
        &json!({"status": "applications_closed"}),
    )
    .await;

    app.register_user("late_applicant").await;
    app.login("late_applicant").await;
    let resp = app
        .post(
            "/api/missions/closed-one/apply",
            &json!({"cover_letter": "je suis en retard"}),
        )
        .await;
    assert_eq!(resp.status(), 400);

    // The enterprise can still reopen it.
    app.relogin_with_totp(&owner).await;
    let reopened = app
        .post(
            "/api/missions/closed-one/status",
            &json!({"status": "published"}),
        )
        .await;
    assert_eq!(reopened.status(), 200);
}

#[tokio::test]
async fn selecting_one_applicant_answers_all_the_others() {
    let app = TestApp::spawn().await;
    let (owner, _) = an_enterprise(&app, "Selectco").await;
    published(&app, "select-one").await;

    let chosen = an_applicant(&app, "select_chosen", "select-one").await;
    an_applicant(&app, "select_other", "select-one").await;

    app.relogin_with_totp(&owner).await;
    let decided = app
        .post(
            &format!("/api/mission-applications/{chosen}/decision"),
            &json!({"status": "selected"}),
        )
        .await;
    assert_eq!(decided.status(), 200, "{}", decided.text().await.unwrap());

    let statuses: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT a.status, a.decision_reason FROM mission_applications a
           JOIN users u ON u.id = a.user_id
          WHERE u.username = 'select_other'",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();
    assert_eq!(statuses[0].0, "rejected");
    assert!(
        statuses[0].1.is_some(),
        "nobody should be left reading 'submitted' forever"
    );

    // And the mission is now somebody's.
    let mission: (String, Option<Uuid>) =
        sqlx::query_as("SELECT status, assigned_user_id FROM missions WHERE slug = 'select-one'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(mission.0, "in_progress");
    assert_eq!(mission.1, Some(user_id(&app, "select_chosen").await));
}

#[tokio::test]
async fn a_rejection_carries_a_reason() {
    let app = TestApp::spawn().await;
    let (owner, _) = an_enterprise(&app, "Rejectco").await;
    published(&app, "reject-one").await;
    let application = an_applicant(&app, "reject_applicant", "reject-one").await;

    app.relogin_with_totp(&owner).await;
    let silent = app
        .post(
            &format!("/api/mission-applications/{application}/decision"),
            &json!({"status": "rejected"}),
        )
        .await;
    assert_eq!(silent.status(), 400, "somebody spent an hour on this");

    let spoken = app
        .post(
            &format!("/api/mission-applications/{application}/decision"),
            &json!({"status": "rejected", "reason": "nous cherchons quelqu'un sur ce fuseau"}),
        )
        .await;
    assert_eq!(spoken.status(), 200);
}

#[tokio::test]
async fn only_the_publishing_enterprise_reads_the_applications() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Ownerco").await;
    published(&app, "own-one").await;
    an_applicant(&app, "own_applicant", "own-one").await;

    let (_, _) = an_enterprise(&app, "Nosyco").await;
    let resp = app.get("/api/missions/own-one/applications").await;
    assert_eq!(resp.status(), 403);
}

// ═══════════════════════════════════════════════════════════════════
// Filters
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn the_board_filters_on_what_the_work_is_made_of() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Filterco").await;
    published(&app, "filter-rust").await;

    let mut python = a_mission_body("filter-python");
    python["target_languages"] = json!(["python"]);
    python["target_frameworks"] = json!(["django"]);
    python["mission_type_slug"] = json!("consulting_technical");
    app.post("/api/missions", &python).await;
    app.post(
        "/api/missions/filter-python/status",
        &json!({"status": "published"}),
    )
    .await;

    for (query, expected) in [
        ("?language=rust", "filter-rust"),
        ("?language=python", "filter-python"),
        ("?framework=axum", "filter-rust"),
        ("?mission_type=consulting_technical", "filter-python"),
    ] {
        let body: Value = app
            .get(&format!("/api/missions{query}"))
            .await
            .json()
            .await
            .unwrap();
        let slugs: Vec<&str> = body["data"]["missions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["slug"].as_str().unwrap())
            .collect();
        assert_eq!(slugs, vec![expected], "filter {query}");
    }
}

// ═══════════════════════════════════════════════════════════════════
// Money
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn an_invoice_needs_somebody_to_pay() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Invoiceco").await;
    published(&app, "invoice-nobody").await;

    let resp = app
        .post(
            "/api/missions/invoice-nobody/invoices",
            &json!({"label": "Livraison finale"}),
        )
        .await;
    assert_eq!(
        resp.status(),
        400,
        "billing for a mission nobody is on means there is no one to pay"
    );
}

#[tokio::test]
async fn a_retainer_bills_once_a_month_and_they_are_numbered() {
    let app = TestApp::spawn().await;
    let (owner, _) = an_enterprise(&app, "Retainerco").await;

    let mut retainer = a_mission_body("retainer-one");
    retainer["payment_model"] = json!("retainer_monthly");
    retainer["budget_eur"] = json!("2500.00");
    app.post("/api/missions", &retainer).await;
    app.post(
        "/api/missions/retainer-one/status",
        &json!({"status": "published"}),
    )
    .await;

    let application = an_applicant(&app, "retainer_dev", "retainer-one").await;
    app.relogin_with_totp(&owner).await;
    app.post(
        &format!("/api/mission-applications/{application}/decision"),
        &json!({"status": "selected"}),
    )
    .await;

    for label in ["Mars 2026", "Avril 2026"] {
        let resp = app
            .post(
                "/api/missions/retainer-one/invoices",
                &json!({"label": label}),
            )
            .await;
        assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    }

    let sequences: Vec<(i16, String)> = sqlx::query_as(
        "SELECT i.sequence, i.label FROM mission_invoices i
           JOIN missions m ON m.id = i.mission_id
          WHERE m.slug = 'retainer-one' ORDER BY i.sequence",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();
    assert_eq!(sequences.len(), 2, "a retainer is one payment a month");
    assert_eq!(sequences[0].0, 1);
    assert_eq!(sequences[1].0, 2);
}

#[tokio::test]
async fn a_per_hour_invoice_is_derived_from_the_agreed_rate() {
    let app = TestApp::spawn().await;
    let (owner, _) = an_enterprise(&app, "Hourlyco").await;

    let mut hourly = a_mission_body("hourly-one");
    hourly["payment_model"] = json!("per_hour");
    hourly["hourly_rate_eur"] = json!("80.00");
    hourly.as_object_mut().unwrap().remove("budget_eur");
    app.post("/api/missions", &hourly).await;
    app.post(
        "/api/missions/hourly-one/status",
        &json!({"status": "published"}),
    )
    .await;
    let application = an_applicant(&app, "hourly_dev", "hourly-one").await;
    app.relogin_with_totp(&owner).await;
    app.post(
        &format!("/api/mission-applications/{application}/decision"),
        &json!({"status": "selected"}),
    )
    .await;

    // Hours without an amount: the rate does the arithmetic, so the number
    // on the invoice cannot disagree with what was agreed.
    let resp = app
        .post(
            "/api/missions/hourly-one/invoices",
            &json!({"label": "Sprint 1", "hours": "12.5"}),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    // 12.5 hours at 80 euros. Compared as a number: the driver trims the
    // trailing zeros a NUMERIC(12,2) column stores, and the scale is not what
    // this test is about.
    let amount: f64 = body["data"]["invoice"]["amount"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(amount, 1000.0);

    // And an hourly invoice that states no hours is refused.
    let vague = app
        .post(
            "/api/missions/hourly-one/invoices",
            &json!({"label": "Sprint 2"}),
        )
        .await;
    assert_eq!(vague.status(), 400);
}

#[tokio::test]
async fn the_commission_is_frozen_at_selection() {
    let app = TestApp::spawn().await;
    let (owner, _) = an_enterprise(&app, "Commissionco").await;
    published(&app, "commission-one").await;
    let application = an_applicant(&app, "commission_dev", "commission-one").await;

    app.relogin_with_totp(&owner).await;
    app.post(
        &format!("/api/mission-applications/{application}/decision"),
        &json!({"status": "selected"}),
    )
    .await;

    let rate: f64 = sqlx::query_scalar(
        "SELECT commission_percent::FLOAT8 FROM missions WHERE slug = 'commission-one'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(rate, 15.0, "the standard rate applies first");

    let invoice = app
        .post(
            "/api/missions/commission-one/invoices",
            &json!({"label": "Livraison"}),
        )
        .await;
    assert_eq!(invoice.status(), 200);

    // Ten deliveries later, the platform rate for this person drops. The
    // invoice already issued must not silently re-rate.
    for n in 0..12 {
        sqlx::query(
            "INSERT INTO missions
                (slug, enterprise_id, mission_type_id, skill_domain, title, description,
                 acceptance_criteria, deliverable_format, budget_eur, status, assigned_user_id)
             SELECT $1, e.id, mt.id, 'code', 'x', 'x', 'x', 'github_pr', 100, 'closed', $2
               FROM enterprises e, mission_types mt
              WHERE mt.slug = 'backend_service_dev' LIMIT 1",
        )
        .bind(format!("past-{n}"))
        .bind(user_id(&app, "commission_dev").await)
        .execute(&app.db)
        .await
        .unwrap();
    }

    let now = skilluv_backend::services::missions::commission_for(
        &app.db,
        user_id(&app, "commission_dev").await,
    )
    .await
    .unwrap();
    assert_eq!(now, 10.0, "twelve deliveries earn the reduced rate");

    let frozen: f64 = sqlx::query_scalar(
        "SELECT i.commission_percent::FLOAT8 FROM mission_invoices i
           JOIN missions m ON m.id = i.mission_id
          WHERE m.slug = 'commission-one'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(
        frozen, 15.0,
        "what was charged in March must stay readable in November"
    );
}
