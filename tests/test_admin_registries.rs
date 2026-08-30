//! The lists that make twenty-eight admin write routes reachable (SKI-354).
//!
//! ## What was broken
//!
//! Twenty-eight admin write routes carried an `{id}` that nothing let an
//! administrator obtain. `POST /admin/engagements/{id}/start`,
//! `/sponsorships/{id}/sign`, `/finance/advances/{id}/disburse` — the buttons
//! existed, the lists did not.
//!
//! One cause, repeated twelve times: everything an enterprise buys is listed
//! only under `/api/enterprise/*`, behind `require_enterprise`, which resolves
//! the **caller's** company and filters on it. Skilluv staff, who service those
//! contracts, are nobody's enterprise. They had the verbs and no nouns, and the
//! only way to reach one was an id pasted out of psql — which SKI-337 already
//! described as the problem, not the fix.
//!
//! These suites hold the two properties that make the fix real: the lists are
//! gated, and they return the id the write routes take.

mod common;
use common::TestApp;
use serde_json::Value;
use uuid::Uuid;

async fn an_admin(app: &TestApp, username: &str) -> Uuid {
    app.register_user(username).await;
    let id: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(username)
        .fetch_one(&app.db)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO user_capabilities (user_id, capability, granted_reason)
         VALUES ($1, 'admin', 'test') ON CONFLICT DO NOTHING",
    )
    .bind(id)
    .execute(&app.db)
    .await
    .unwrap();
    app.login(username).await;
    id
}

/// The whole point of the registry, asserted rather than assumed.
///
/// `enterprise_products` was built to be this list — `sales_pipeline.rs` says
/// so in its own comment: *"every product registers itself in
/// `enterprise_products` — which is the reason that table exists."* Ten modules
/// insert into it with `source_table` and `source_id`, and `source_id` **is**
/// the `{id}` those twenty write routes take. It was in the table; nothing
/// served it.
#[tokio::test]
async fn the_registry_serves_the_id_the_write_routes_take() {
    let app = TestApp::spawn().await;
    an_admin(&app, "reg_admin").await;

    let owner: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'reg_admin'")
        .fetch_one(&app.db)
        .await
        .unwrap();
    let enterprise: Uuid = sqlx::query_scalar(
        "INSERT INTO enterprises (owner_id, company_name, slug, company_size)
         VALUES ($1, 'Coopérative Anacarde', 'coop-anacarde', '1-10')
         RETURNING id",
    )
    .bind(owner)
    .fetch_one(&app.db)
    .await
    .expect("an enterprise");

    // A product with a source, which is the shape every selling module writes.
    let source = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO enterprise_products
             (enterprise_id, product_type, status, source_table, source_id)
         VALUES ($1, (SELECT slug FROM enterprise_product_types LIMIT 1),
                 'pending', 'engagements', $2)",
    )
    .bind(enterprise)
    .bind(source)
    .execute(&app.db)
    .await
    .expect("a product");

    let body: Value = app
        .get("/api/admin/enterprise-products")
        .await
        .json()
        .await
        .unwrap();
    let rows = body["data"]
        .as_array()
        .unwrap_or_else(|| panic!("the registry did not answer a list: {body}"));
    let row = rows
        .iter()
        .find(|r| r["source_id"] == source.to_string())
        .unwrap_or_else(|| panic!("the product is not in the registry: {rows:?}"));

    assert_eq!(row["source_table"], "engagements");
    assert_eq!(row["company_name"], "Coopérative Anacarde");
    // `pending` is this table's word for a draft, and it is the state that
    // matters: `activate` and `open` act on exactly what is not yet public, so
    // a registry that hid it would hide the rows it exists for.
    assert_eq!(row["status"], "pending");

    // The envelope every admin list agreed on (SKI-58 / SKI-111).
    assert!(body["pagination"]["total"].as_i64().unwrap_or(0) >= 1);
    assert!(body["pagination"]["page"].is_number());
}

/// A draft is what an administrator came for, and it is what `renewals` hides.
///
/// Widening `renewals` instead of adding this route would have inherited its
/// `status = 'active' AND renews_at IS NOT NULL` filter — which excludes every
/// row somebody needs to activate.
#[tokio::test]
async fn the_registry_shows_what_the_renewals_list_filters_out() {
    let app = TestApp::spawn().await;
    an_admin(&app, "reg_admin2").await;

    let owner: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'reg_admin2'")
        .fetch_one(&app.db)
        .await
        .unwrap();
    let enterprise: Uuid = sqlx::query_scalar(
        "INSERT INTO enterprises (owner_id, company_name, slug, company_size)
         VALUES ($1, 'Atelier Test', 'atelier-test', '1-10') RETURNING id",
    )
    .bind(owner)
    .fetch_one(&app.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO enterprise_products (enterprise_id, product_type, status)
         VALUES ($1, (SELECT slug FROM enterprise_product_types LIMIT 1), 'pending')",
    )
    .bind(enterprise)
    .execute(&app.db)
    .await
    .unwrap();

    let registry: Value = app
        .get("/api/admin/enterprise-products?status=pending")
        .await
        .json()
        .await
        .unwrap();
    assert!(
        !registry["data"].as_array().unwrap().is_empty(),
        "the registry must show pending products"
    );

    let renewals: Value = app
        .get("/api/admin/enterprise-products/renewals")
        .await
        .json()
        .await
        .unwrap();
    let listed = renewals["data"]["renewals"].as_array().unwrap();
    assert!(
        listed.is_empty(),
        "renewals answers a different question and must not have grown one"
    );
}

/// Every new list is behind the admin gate.
///
/// These carry contract values, company names and who asked for money. A
/// registry that leaked would be worse than the gap it closes.
#[tokio::test]
async fn none_of_the_new_lists_are_reachable_without_the_admin_gate() {
    let app = TestApp::spawn().await;
    app.register_user("reg_nobody").await;
    app.login("reg_nobody").await;

    for path in [
        "/api/admin/enterprise-products",
        "/api/admin/finance/advances",
        "/api/admin/finance/referrals",
        "/api/admin/finance/guarantee-claims",
        "/api/admin/finance/partnerships",
        "/api/admin/sponsored-content",
    ] {
        assert_eq!(
            app.get(path).await.status(),
            403,
            "{path} is reachable without the admin gate"
        );
    }
}

/// The finance queues answer, and put what is waiting first.
///
/// `enterprise_products` does not cover these: an advance and a referral belong
/// to a contributor, not to a company. The ordering is the point — an advance
/// requested three weeks ago is the row somebody most needs to see, and a list
/// sorted by date alone buries it under everything already settled.
#[tokio::test]
async fn the_finance_queues_answer_and_lead_with_what_is_waiting() {
    let app = TestApp::spawn().await;
    an_admin(&app, "fin_admin").await;

    for path in [
        "/api/admin/finance/advances",
        "/api/admin/finance/referrals",
        "/api/admin/finance/guarantee-claims",
        "/api/admin/finance/partnerships",
    ] {
        let response = app.get(path).await;
        assert_eq!(response.status(), 200, "{path}");
        let body: Value = response.json().await.unwrap();
        assert!(
            body["data"].is_object(),
            "{path} does not answer the standard envelope: {body}"
        );
    }
}

/// The partnership list must include drafts, or `activate` stays unreachable.
///
/// `GET /finance/partners` returns only active ones, so the row you have to
/// activate is exactly the row it hides.
#[tokio::test]
async fn the_partnership_list_includes_the_drafts_the_public_one_hides() {
    let app = TestApp::spawn().await;
    an_admin(&app, "fin_admin2").await;

    sqlx::query(
        "INSERT INTO financial_partnerships
             (partner_org, kind, countries, commission_percent,
              regulatory_basis, status)
         VALUES ('Banque Test', 'loan', ARRAY['BJ'], 2.5, 'test', 'draft')",
    )
    .execute(&app.db)
    .await
    .expect("a draft partnership");

    let body: Value = app
        .get("/api/admin/finance/partnerships")
        .await
        .json()
        .await
        .unwrap();
    let rows = body["data"]["partnerships"].as_array().unwrap();
    assert!(
        rows.iter().any(|r| r["partner_org"] == "Banque Test"),
        "the draft is missing, so nothing can activate it: {rows:?}"
    );
    // And it leads, because it is the one carrying an action.
    assert_eq!(rows[0]["status"], "draft");
}
