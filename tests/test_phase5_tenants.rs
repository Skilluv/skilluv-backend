//! Integration tests — Phase 5.9 White-label tenants.

mod common;

use common::TestApp;
use serde_json::{Value, json};

#[tokio::test]
async fn get_current_tenant_falls_back_to_root() {
    let app = TestApp::spawn().await;
    let resp = app
        .client
        .get(format!("{}/api/tenants/current", app.addr))
        .send()
        .await
        .expect("GET current");
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["slug"], "skilluv");
    assert_eq!(body["data"]["name"], "Skilluv");
    drop(app);
}

#[tokio::test]
async fn get_current_tenant_resolves_by_header() {
    let app = TestApp::spawn().await;
    // Créer un tenant "acme"
    sqlx::query(
        "INSERT INTO tenants (slug, name, subdomain, contact_email, plan, max_users) VALUES ('acme', 'Acme Bootcamp', 'acme', 'admin@acme.io', 'starter', 50)",
    )
    .execute(&app.db)
    .await
    .expect("insert tenant");

    let resp = app
        .client
        .get(format!("{}/api/tenants/current", app.addr))
        .header("X-Skilluv-Tenant", "acme")
        .send()
        .await
        .expect("GET current");
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["slug"], "acme");
    drop(app);
}

#[tokio::test]
async fn admin_only_can_create_tenant() {
    let app = TestApp::spawn().await;
    let _user = app.register_user("nonadmin1").await;
    let resp = app
        .client
        .post(format!("{}/api/admin/tenants", app.addr))
        .json(&json!({
            "slug": "wilcom",
            "name": "Wilcom Bootcamp",
            "contact_email": "hello@wilcom.io"
        }))
        .send()
        .await
        .expect("POST tenant");
    assert_eq!(resp.status(), 403, "non-admin cannot create tenant");
    drop(app);
}
