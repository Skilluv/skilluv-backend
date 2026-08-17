//! What each enterprise has with us, and the renewal list it exists for.

mod common;
use common::TestApp;
use serde_json::{Value, json};
use uuid::Uuid;

async fn an_admin(app: &TestApp, username: &str) {
    app.register_user(username).await;
    sqlx::query("UPDATE users SET role = 'admin' WHERE username = $1")
        .bind(username)
        .execute(&app.db)
        .await
        .unwrap();
    app.login(username).await;
}

async fn an_enterprise(app: &TestApp, company: &str) -> Uuid {
    app.register_enterprise(company).await;
    sqlx::query_scalar(
        "SELECT e.id FROM enterprises e JOIN users u ON u.id = e.owner_id
          WHERE u.username = $1",
    )
    .bind(company.to_lowercase().replace(' ', ""))
    .fetch_one(&app.db)
    .await
    .unwrap()
}

#[tokio::test]
async fn the_catalogue_says_how_each_product_earns() {
    let app = TestApp::spawn().await;

    let body: Value = app
        .get("/api/enterprise/product-types")
        .await
        .json()
        .await
        .unwrap();
    let types = body["data"]["product_types"].as_array().unwrap();
    assert_eq!(types.len(), 18);

    for product in types {
        assert!(!product["description"].as_str().unwrap().is_empty());
        // The join that turns "what do they have" into "what does it earn",
        // without a mapping written twice.
        assert!(
            product["revenue_stream"].is_string(),
            "{} names no revenue stream",
            product["slug"]
        );
    }
}

#[tokio::test]
async fn a_renewing_product_must_say_when() {
    let app = TestApp::spawn().await;
    let enterprise = an_enterprise(&app, "Renewco").await;
    an_admin(&app, "products_admin").await;

    // Without a date it never appears on a renewal list, and it lapses
    // because nobody was told to ask.
    let silent = app
        .post(
            &format!("/api/admin/enterprises/{enterprise}/products"),
            &json!({"product_type": "subscription_pipeline"}),
        )
        .await;
    assert_eq!(silent.status(), 400, "{}", silent.text().await.unwrap());

    let dated = app
        .post(
            &format!("/api/admin/enterprises/{enterprise}/products"),
            &json!({
                "product_type": "subscription_pipeline",
                "renews_at": "2027-01-15T00:00:00Z",
                "contract_value": "1080.00",
                "currency": "EUR",
            }),
        )
        .await;
    assert_eq!(dated.status(), 200, "{}", dated.text().await.unwrap());
}

#[tokio::test]
async fn a_one_off_product_needs_no_renewal_date() {
    let app = TestApp::spawn().await;
    let enterprise = an_enterprise(&app, "Oneoffco").await;
    an_admin(&app, "oneoff_admin").await;

    let resp = app
        .post(
            &format!("/api/admin/enterprises/{enterprise}/products"),
            &json!({"product_type": "bounty"}),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn stopping_early_requires_a_reason() {
    let app = TestApp::spawn().await;
    let enterprise = an_enterprise(&app, "Stopco").await;
    an_admin(&app, "stop_admin").await;

    let created: Value = app
        .post(
            &format!("/api/admin/enterprises/{enterprise}/products"),
            &json!({"product_type": "studio_engagement"}),
        )
        .await
        .json()
        .await
        .unwrap();
    let id = created["data"]["product_id"].as_str().unwrap();

    // The next person needs the reason exactly when the engagement stopped.
    let silent = app
        .post(
            &format!("/api/admin/enterprise-products/{id}/status"),
            &json!({"status": "cancelled"}),
        )
        .await;
    assert_eq!(silent.status(), 400);

    let spoken = app
        .post(
            &format!("/api/admin/enterprise-products/{id}/status"),
            &json!({"status": "cancelled", "reason": "le budget a été retiré"}),
        )
        .await;
    assert_eq!(spoken.status(), 200);

    let (status, ended): (String, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as("SELECT status, ended_at FROM enterprise_products WHERE id = $1::UUID")
            .bind(id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(status, "cancelled");
    assert!(ended.is_some());
}

#[tokio::test]
async fn lapsed_is_not_the_same_as_cancelled() {
    let app = TestApp::spawn().await;
    let enterprise = an_enterprise(&app, "Lapseco").await;
    an_admin(&app, "lapse_admin").await;

    let created: Value = app
        .post(
            &format!("/api/admin/enterprises/{enterprise}/products"),
            &json!({
                "product_type": "data_licensing",
                "renews_at": "2027-03-01T00:00:00Z",
            }),
        )
        .await
        .json()
        .await
        .unwrap();
    let id = created["data"]["product_id"].as_str().unwrap();

    // One is a decision at the end, the other in the middle. A renewal
    // report that conflates them is useless, so lapsing needs no reason.
    let resp = app
        .post(
            &format!("/api/admin/enterprise-products/{id}/status"),
            &json!({"status": "lapsed"}),
        )
        .await;
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn the_renewal_list_puts_the_overdue_first() {
    let app = TestApp::spawn().await;
    let enterprise = an_enterprise(&app, "Dueco").await;
    an_admin(&app, "renewal_admin").await;

    for (product, renews) in [
        ("subscription_pipeline", "2020-01-01T00:00:00Z"),
        ("data_licensing", "2099-01-01T00:00:00Z"),
    ] {
        app.post(
            &format!("/api/admin/enterprises/{enterprise}/products"),
            &json!({"product_type": product, "renews_at": renews}),
        )
        .await;
    }

    let body: Value = app
        .get("/api/admin/enterprise-products/renewals?within_days=90")
        .await
        .json()
        .await
        .unwrap();
    let renewals = body["data"]["renewals"].as_array().unwrap();

    // The far-future one is outside the horizon; the overdue one is the case
    // somebody most needs to see.
    assert_eq!(renewals.len(), 1);
    assert_eq!(renewals[0]["product_type"], "subscription_pipeline");
    assert_eq!(renewals[0]["overdue"], true);
}

#[tokio::test]
async fn an_enterprise_reads_its_own_and_only_its_own() {
    let app = TestApp::spawn().await;
    let mine = an_enterprise(&app, "Mineco").await;
    let theirs = an_enterprise(&app, "Theirsco").await;
    an_admin(&app, "sep_admin").await;

    for enterprise in [mine, theirs] {
        app.post(
            &format!("/api/admin/enterprises/{enterprise}/products"),
            &json!({"product_type": "bounty"}),
        )
        .await;
    }

    app.relogin_with_totp("mineco").await;
    let body: Value = app
        .get("/api/enterprise/products")
        .await
        .json()
        .await
        .unwrap();
    let products = body["data"]["products"].as_array().unwrap();
    assert_eq!(products.len(), 1);
    assert_eq!(products[0]["enterprise_id"], mine.to_string());
}

#[tokio::test]
async fn the_engagement_list_is_not_public() {
    let app = TestApp::spawn().await;
    let enterprise = an_enterprise(&app, "Privateco").await;
    app.register_user("products_nosy").await;
    app.login("products_nosy").await;

    assert_eq!(
        app.get(&format!("/api/admin/enterprises/{enterprise}/products"))
            .await
            .status(),
        403
    );
}

#[tokio::test]
async fn an_unknown_product_is_refused() {
    let app = TestApp::spawn().await;
    let enterprise = an_enterprise(&app, "Unknownco").await;
    an_admin(&app, "unknown_admin").await;

    let resp = app
        .post(
            &format!("/api/admin/enterprises/{enterprise}/products"),
            &json!({"product_type": "vibes_as_a_service"}),
        )
        .await;
    assert_eq!(resp.status(), 404);
}
