//! The consolidated overview and the internal pipeline.
//!
//! The overview is one query over `enterprise_products`, which is the reason
//! that table exists: every product in the business model registers itself
//! there, so a company's whole file is one join rather than eighteen.

mod common;
use common::TestApp;
use serde_json::{Value, json};
use uuid::Uuid;

async fn an_admin(app: &TestApp, username: &str) {
    // `register_admin`, not `role = 'admin'`: since P21 the admin gate reads
    // `user_capabilities`, and the column on its own opens nothing. The helper
    // grants the capability and enrols the passkey the admin 2FA middleware
    // wants, then logs in.
    app.register_admin(username).await;
}

async fn an_enterprise(app: &TestApp, company: &str) -> String {
    app.register_enterprise(company).await;
    let username = company.to_lowercase().replace(' ', "");
    app.login(&username).await;
    app.enable_totp_for(&username).await;
    username
}

#[tokio::test]
async fn a_company_sees_every_product_it_has_in_one_answer() {
    let app = TestApp::spawn().await;
    let company = an_enterprise(&app, "Overviewco").await;
    let enterprise: Uuid =
        sqlx::query_scalar("SELECT id FROM enterprises WHERE slug LIKE 'overviewco%'")
            .fetch_one(&app.db)
            .await
            .unwrap();

    for product in ["credits_pack", "raas_campaign", "data_room"] {
        sqlx::query(
            // `renews_at` on every row: a data room is a recurring product and
            // the trigger from 0206 refuses one that does not say when. That
            // is the point of the trigger — a renewal nobody was told to ask
            // for lapses — so the test supplies a date rather than working
            // around it.
            "INSERT INTO enterprise_products
                (enterprise_id, product_type, contract_value, currency, renews_at)
             VALUES ($1, $2, 1000.00, 'EUR', NOW() + INTERVAL '1 year')",
        )
        .bind(enterprise)
        .bind(product)
        .execute(&app.db)
        .await
        .unwrap();
    }

    app.login(&company).await;
    let resp = app.get("/api/enterprise/overview").await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();

    let products = body["data"]["products"].as_array().unwrap();
    assert_eq!(products.len(), 3);
    // Each one carries the pillar it belongs to, joined from the catalogue
    // rather than repeated on the row.
    assert!(products.iter().all(|p| p["pillar"].is_string()));
}

#[tokio::test]
async fn the_overview_suggests_only_pillars_the_company_already_buys_from() {
    let app = TestApp::spawn().await;
    let company = an_enterprise(&app, "Suggestco").await;
    let enterprise: Uuid =
        sqlx::query_scalar("SELECT id FROM enterprises WHERE slug LIKE 'suggestco%'")
            .fetch_one(&app.db)
            .await
            .unwrap();

    // One product, in the talent pillar.
    sqlx::query(
        "INSERT INTO enterprise_products (enterprise_id, product_type)
         VALUES ($1, 'raas_campaign')",
    )
    .bind(enterprise)
    .execute(&app.db)
    .await
    .unwrap();

    app.login(&company).await;
    let resp = app.get("/api/enterprise/overview").await;
    let body: Value = resp.json().await.unwrap();
    let suggestions = body["data"]["also_available_in_pillars_you_use"]
        .as_array()
        .unwrap();

    assert!(!suggestions.is_empty());
    // Only the pillar they already buy from, and never something they have.
    assert!(suggestions.iter().all(|s| s["pillar"] == "talent"));
    assert!(
        suggestions
            .iter()
            .all(|s| s["product_type"] != "raas_campaign")
    );
}

#[tokio::test]
async fn spend_is_grouped_by_the_stream_that_produced_it() {
    let app = TestApp::spawn().await;
    let company = an_enterprise(&app, "Spendco").await;
    let enterprise: Uuid =
        sqlx::query_scalar("SELECT id FROM enterprises WHERE slug LIKE 'spendco%'")
            .fetch_one(&app.db)
            .await
            .unwrap();

    for (source, amount) in [
        ("bounty", "800.00"),
        ("bounty", "200.00"),
        ("mentor_session", "50.00"),
    ] {
        sqlx::query(
            "INSERT INTO platform_revenues
                (source, related_enterprise_id, amount_credits, fee_rate_bps)
             VALUES ($1, $2, $3::NUMERIC, 800)",
        )
        .bind(source)
        .bind(enterprise)
        .bind(amount)
        .execute(&app.db)
        .await
        .unwrap();
    }

    app.login(&company).await;
    let resp = app.get("/api/enterprise/overview").await;
    let body: Value = resp.json().await.unwrap();
    let spend = body["data"]["spend_by_stream"].as_array().unwrap();

    let bounty = spend.iter().find(|s| s["stream"] == "bounty").unwrap();
    common::assert_amount(&bounty["total"], "1000.00");
    assert_eq!(bounty["entries"], 2);
}

// ═══════════════════════════════════════════════════════════════════
// The pipeline
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn a_lost_deal_has_to_say_why() {
    let app = TestApp::spawn().await;
    an_admin(&app, "pipelineadmin").await;

    let resp = app
        .post(
            "/api/admin/sales/opportunities",
            &json!({
                "org_name": "Banque régionale",
                "product_type": "raas_campaign",
                "estimated_value": "12000.00",
            }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let created: Value = resp.json().await.unwrap();
    let id = created["data"]["opportunity"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // A pipeline that records wins and shrugs at losses teaches nothing.
    let resp = app
        .post(
            &format!("/api/admin/sales/opportunities/{id}/stage"),
            &json!({ "stage": "lost" }),
        )
        .await;
    assert_eq!(resp.status(), 400);

    let resp = app
        .post(
            &format!("/api/admin/sales/opportunities/{id}/stage"),
            &json!({ "stage": "lost", "lost_reason": "Budget gelé jusqu'en 2028." }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn the_weighted_total_is_smaller_than_the_face_value() {
    let app = TestApp::spawn().await;
    an_admin(&app, "weightadmin").await;

    for (org, stage) in [("Une", "lead"), ("Deux", "negotiation")] {
        let resp = app
            .post(
                "/api/admin/sales/opportunities",
                &json!({ "org_name": org, "estimated_value": "10000.00" }),
            )
            .await;
        let created: Value = resp.json().await.unwrap();
        let id = created["data"]["opportunity"]["id"]
            .as_str()
            .unwrap()
            .to_string();
        app.post(
            &format!("/api/admin/sales/opportunities/{id}/stage"),
            &json!({ "stage": stage }),
        )
        .await;
    }

    let resp = app.get("/api/admin/sales/opportunities").await;
    let body: Value = resp.json().await.unwrap();

    // 10% of ten thousand plus 75% of ten thousand.
    common::assert_amount(&body["data"]["weighted_value"], "8500.00");
    // And it says out loud that the weights are guesses.
    assert!(body["data"]["weighted_value_note"].is_string());
}

#[tokio::test]
async fn an_overdue_next_step_surfaces_on_its_own() {
    let app = TestApp::spawn().await;
    an_admin(&app, "overdueadmin").await;

    let resp = app
        .post(
            "/api/admin/sales/opportunities",
            &json!({ "org_name": "Télécom", "estimated_value": "40000.00" }),
        )
        .await;
    let created: Value = resp.json().await.unwrap();
    let id = created["data"]["opportunity"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let resp = app
        .post(
            &format!("/api/admin/sales/opportunities/{id}/activities"),
            &json!({
                "kind": "meeting",
                "summary_md": "Ils veulent un pilote sur une équipe avant d'engager.",
                "next_step": "Envoyer la proposition de pilote.",
                "next_step_due_on": "2020-01-01",
            }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    // A CRM without this is a diary.
    let resp = app.get("/api/admin/sales/overdue").await;
    let body: Value = resp.json().await.unwrap();
    let overdue = body["data"]["overdue"].as_array().unwrap();
    assert_eq!(overdue.len(), 1);
    assert_eq!(overdue[0]["org_name"], "Télécom");
}

#[tokio::test]
async fn a_closed_opportunity_leaves_the_overdue_list() {
    let app = TestApp::spawn().await;
    an_admin(&app, "closedadmin").await;

    let resp = app
        .post(
            "/api/admin/sales/opportunities",
            &json!({ "org_name": "Fermée SA" }),
        )
        .await;
    let created: Value = resp.json().await.unwrap();
    let id = created["data"]["opportunity"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    app.post(
        &format!("/api/admin/sales/opportunities/{id}/activities"),
        &json!({
            "kind": "call",
            "summary_md": "Premier contact, intéressés par le recrutement.",
            "next_step": "Relancer.",
            "next_step_due_on": "2020-01-01",
        }),
    )
    .await;

    app.post(
        &format!("/api/admin/sales/opportunities/{id}/stage"),
        &json!({ "stage": "won" }),
    )
    .await;

    let resp = app.get("/api/admin/sales/overdue").await;
    let body: Value = resp.json().await.unwrap();
    assert!(body["data"]["overdue"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn renewals_are_read_from_the_products_themselves() {
    let app = TestApp::spawn().await;
    an_admin(&app, "renewaladmin").await;
    let owner: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'renewaladmin'")
        .fetch_one(&app.db)
        .await
        .unwrap();

    let enterprise: Uuid = sqlx::query_scalar(
        "INSERT INTO enterprises (owner_id, company_name, slug, company_size)
         VALUES ($1, 'Renouvelle SA', 'renouvelle-sa', '11-50') RETURNING id",
    )
    .bind(owner)
    .fetch_one(&app.db)
    .await
    .unwrap();

    // A learning subscription lapsing in a fortnight. No renewal_date column
    // anywhere: the view reads the subscription's own period end.
    sqlx::query(
        "INSERT INTO corporate_learning_subscriptions
            (enterprise_id, plan, seats, monthly_fee_per_seat, current_period_end)
         VALUES ($1, 'professional', 20, 30.00, NOW() + INTERVAL '14 days')",
    )
    .bind(enterprise)
    .execute(&app.db)
    .await
    .unwrap();

    let resp = app.get("/api/admin/sales/renewals?within_days=30").await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    let renewals = body["data"]["renewals"].as_array().unwrap();

    let learning = renewals
        .iter()
        .find(|r| r["product"] == "corporate_learning")
        .unwrap();
    common::assert_amount(&learning["value"], "600.00");

    // And moving the subscription moves the renewal, because there is only
    // one place the date lives.
    sqlx::query(
        "UPDATE corporate_learning_subscriptions
            SET current_period_end = NOW() + INTERVAL '200 days'",
    )
    .execute(&app.db)
    .await
    .unwrap();

    let resp = app.get("/api/admin/sales/renewals?within_days=30").await;
    let body: Value = resp.json().await.unwrap();
    assert!(
        !body["data"]["renewals"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["product"] == "corporate_learning")
    );
}

#[tokio::test]
async fn the_pipeline_is_not_readable_without_admin() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Nosypipeco").await;

    let resp = app.get("/api/admin/sales/opportunities").await;
    assert!(resp.status().is_client_error());
}
