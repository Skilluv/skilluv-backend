//! SCIM 2.0 provisioning end-to-end tests.
//!
//! Covers the full IdP flow: token generation → auth → User CRUD →
//! deprovisioning (PATCH active=false + DELETE) → Group CRUD + membership ops.

mod common;

use reqwest::header::AUTHORIZATION;
use reqwest::{Client, StatusCode};
use serde_json::{Value, json};

/// Set up an enterprise + SSO config + SCIM token. Returns (bearer token, enterprise slug).
async fn setup_scim(app: &common::TestApp, slug_input: &str) -> String {
    let company = slug_input.to_string();
    app.register_enterprise(&company).await;
    let username = company.to_lowercase();
    app.login(&username).await;
    app.enable_totp_for(&username).await;

    // The SCIM token setter requires an existing SSO config row.
    let resp = app
        .post(
            "/api/enterprise/sso/config",
            &json!({
                "issuer": "https://accounts.google.com",
                "client_id": "cid",
                "client_secret": "csec",
                "email_domains": [format!("{username}.example")],
                "default_role": "recruiter",
            }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::OK, "SSO setup failed");

    let token_resp = app
        .post("/api/enterprise/sso/scim/token", &json!({}))
        .await;
    assert_eq!(token_resp.status(), StatusCode::OK, "token gen failed");
    let body: Value = token_resp.json().await.unwrap();
    let token = body["data"]["token"].as_str().unwrap().to_string();
    assert!(token.starts_with("scim_"));
    token
}

fn scim_client() -> Client {
    // Fresh client — the SCIM API is stateless and cookie-less.
    Client::builder().build().unwrap()
}

async fn scim_get(app: &common::TestApp, token: &str, path: &str) -> reqwest::Response {
    scim_client()
        .get(format!("{}{}", app.addr, path))
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .send()
        .await
        .unwrap()
}

async fn scim_post(app: &common::TestApp, token: &str, path: &str, body: &Value) -> reqwest::Response {
    scim_client()
        .post(format!("{}{}", app.addr, path))
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .json(body)
        .send()
        .await
        .unwrap()
}

async fn scim_patch(app: &common::TestApp, token: &str, path: &str, body: &Value) -> reqwest::Response {
    scim_client()
        .patch(format!("{}{}", app.addr, path))
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .json(body)
        .send()
        .await
        .unwrap()
}

async fn scim_delete(app: &common::TestApp, token: &str, path: &str) -> reqwest::Response {
    scim_client()
        .delete(format!("{}{}", app.addr, path))
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn test_scim_bearer_auth_required() {
    let app = common::TestApp::spawn().await;
    // No token — all endpoints must 401.
    let resp = scim_get(&app, "invalid-token", "/api/scim/v2/Users").await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    let no_header = scim_client()
        .get(format!("{}/api/scim/v2/Users", app.addr))
        .send()
        .await
        .unwrap();
    assert_eq!(no_header.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_scim_service_provider_config() {
    let app = common::TestApp::spawn().await;
    let token = setup_scim(&app, "SpConfigCorp").await;

    let resp = scim_get(&app, &token, "/api/scim/v2/ServiceProviderConfig").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["patch"]["supported"], true);
    assert_eq!(body["filter"]["supported"], true);
    assert_eq!(body["bulk"]["supported"], false);
}

#[tokio::test]
async fn test_scim_user_lifecycle() {
    let app = common::TestApp::spawn().await;
    let token = setup_scim(&app, "LifecycleCorp").await;

    // POST /Users
    let create = scim_post(
        &app,
        &token,
        "/api/scim/v2/Users",
        &json!({
            "userName": "alice_scim",
            "externalId": "okta-user-123",
            "name": { "givenName": "Alice", "familyName": "Doe" },
            "displayName": "Alice Doe",
            "emails": [{ "value": "alice@lifecyclecorp.example", "primary": true }],
            "active": true,
        }),
    )
    .await;
    assert_eq!(create.status(), StatusCode::CREATED);
    let created: Value = create.json().await.unwrap();
    let user_id = created["id"].as_str().unwrap().to_string();
    assert_eq!(created["userName"], "alice_scim");
    assert_eq!(created["active"], true);
    assert_eq!(created["externalId"], "okta-user-123");

    // GET /Users/{id}
    let get_one = scim_get(&app, &token, &format!("/api/scim/v2/Users/{user_id}")).await;
    assert_eq!(get_one.status(), StatusCode::OK);
    let one: Value = get_one.json().await.unwrap();
    assert_eq!(one["userName"], "alice_scim");

    // GET /Users?filter=userName eq "alice_scim"
    let list = scim_get(
        &app,
        &token,
        "/api/scim/v2/Users?filter=userName%20eq%20%22alice_scim%22",
    )
    .await;
    assert_eq!(list.status(), StatusCode::OK);
    let list_body: Value = list.json().await.unwrap();
    assert_eq!(list_body["totalResults"], 1);

    // POST duplicate externalId → 400 with "already exists"
    let dup = scim_post(
        &app,
        &token,
        "/api/scim/v2/Users",
        &json!({
            "userName": "another",
            "externalId": "okta-user-123",
            "emails": [{ "value": "other@lifecyclecorp.example" }],
        }),
    )
    .await;
    assert_eq!(dup.status(), StatusCode::BAD_REQUEST);

    // PATCH active=false → membership revoked + sessions killed
    // Insert a fake SSO session first so we can verify it gets revoked.
    let session_before: (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO user_sessions (user_id, refresh_hash, login_method) VALUES ($1, $2, 'sso') RETURNING id",
    )
    .bind(user_id.parse::<uuid::Uuid>().unwrap())
    .bind(vec![0u8; 32])
    .fetch_one(&app.db)
    .await
    .unwrap();

    let patch = scim_patch(
        &app,
        &token,
        &format!("/api/scim/v2/Users/{user_id}"),
        &json!({
            "schemas": ["urn:ietf:params:scim:api:messages:2.0:PatchOp"],
            "Operations": [
                { "op": "replace", "path": "active", "value": false }
            ]
        }),
    )
    .await;
    assert_eq!(patch.status(), StatusCode::OK);
    let patched: Value = patch.json().await.unwrap();
    assert_eq!(patched["active"], false);

    // Membership status flipped
    let member_status: (String,) = sqlx::query_as(
        "SELECT status FROM enterprise_members WHERE user_id = $1",
    )
    .bind(user_id.parse::<uuid::Uuid>().unwrap())
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(member_status.0, "revoked");

    // Session revoked
    let revoked: (Option<chrono::DateTime<chrono::Utc>>,) = sqlx::query_as(
        "SELECT revoked_at FROM user_sessions WHERE id = $1",
    )
    .bind(session_before.0)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(revoked.0.is_some());

    // DELETE /Users/{id} — idempotent soft delete
    let del = scim_delete(&app, &token, &format!("/api/scim/v2/Users/{user_id}")).await;
    assert_eq!(del.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn test_scim_group_crud_and_membership() {
    let app = common::TestApp::spawn().await;
    let token = setup_scim(&app, "GroupCorp").await;

    // Provision a couple of users first.
    let u1 = scim_post(
        &app,
        &token,
        "/api/scim/v2/Users",
        &json!({
            "userName": "bob",
            "emails": [{ "value": "bob@groupcorp.example" }],
        }),
    )
    .await;
    let u1_id = u1.json::<Value>().await.unwrap()["id"].as_str().unwrap().to_string();
    let u2 = scim_post(
        &app,
        &token,
        "/api/scim/v2/Users",
        &json!({
            "userName": "carol",
            "emails": [{ "value": "carol@groupcorp.example" }],
        }),
    )
    .await;
    let u2_id = u2.json::<Value>().await.unwrap()["id"].as_str().unwrap().to_string();

    // POST /Groups with initial member.
    let create = scim_post(
        &app,
        &token,
        "/api/scim/v2/Groups",
        &json!({
            "displayName": "Recruiters",
            "externalId": "okta-group-42",
            "members": [{ "value": u1_id }],
        }),
    )
    .await;
    assert_eq!(create.status(), StatusCode::CREATED);
    let group: Value = create.json().await.unwrap();
    let group_id = group["id"].as_str().unwrap().to_string();
    assert_eq!(group["displayName"], "Recruiters");
    assert_eq!(group["members"].as_array().unwrap().len(), 1);

    // PATCH add second member.
    let add = scim_patch(
        &app,
        &token,
        &format!("/api/scim/v2/Groups/{group_id}"),
        &json!({
            "schemas": ["urn:ietf:params:scim:api:messages:2.0:PatchOp"],
            "Operations": [
                { "op": "add", "path": "members", "value": [{ "value": u2_id }] }
            ]
        }),
    )
    .await;
    assert_eq!(add.status(), StatusCode::OK);
    let after_add: Value = add.json().await.unwrap();
    assert_eq!(after_add["members"].as_array().unwrap().len(), 2);

    // PATCH remove using Okta-style filter path.
    let rm = scim_patch(
        &app,
        &token,
        &format!("/api/scim/v2/Groups/{group_id}"),
        &json!({
            "schemas": ["urn:ietf:params:scim:api:messages:2.0:PatchOp"],
            "Operations": [
                { "op": "remove", "path": format!("members[value eq \"{u1_id}\"]") }
            ]
        }),
    )
    .await;
    assert_eq!(rm.status(), StatusCode::OK);
    let after_rm: Value = rm.json().await.unwrap();
    assert_eq!(after_rm["members"].as_array().unwrap().len(), 1);
    assert_eq!(after_rm["members"][0]["value"], u2_id);

    // List groups.
    let list = scim_get(&app, &token, "/api/scim/v2/Groups").await;
    let body: Value = list.json().await.unwrap();
    assert_eq!(body["totalResults"], 1);

    // DELETE /Groups/{id}
    let del = scim_delete(&app, &token, &format!("/api/scim/v2/Groups/{group_id}")).await;
    assert_eq!(del.status(), StatusCode::NO_CONTENT);
    let get_after = scim_get(&app, &token, &format!("/api/scim/v2/Groups/{group_id}")).await;
    assert_eq!(get_after.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_scim_token_revoke_disables_access() {
    let app = common::TestApp::spawn().await;
    let token = setup_scim(&app, "RevokeTokCorp").await;

    // Sanity: token works before revoke.
    let ok = scim_get(&app, &token, "/api/scim/v2/Users").await;
    assert_eq!(ok.status(), StatusCode::OK);

    // Owner revokes.
    let revoke = app.delete("/api/enterprise/sso/scim/token").await;
    assert_eq!(revoke.status(), StatusCode::OK);

    // Same token no longer authenticates.
    let after = scim_get(&app, &token, "/api/scim/v2/Users").await;
    assert_eq!(after.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_group_role_mapping_promotes_and_demotes_members() {
    let app = common::TestApp::spawn().await;
    let token = setup_scim(&app, "RoleMapCorp").await;

    // Provision a user (starts as recruiter — the default_role from setup_scim).
    let u = scim_post(
        &app,
        &token,
        "/api/scim/v2/Users",
        &json!({
            "userName": "dave",
            "emails": [{ "value": "dave@rolemapcorp.example" }],
        }),
    )
    .await;
    let user_id = u.json::<Value>().await.unwrap()["id"].as_str().unwrap().to_string();
    let user_uuid: uuid::Uuid = user_id.parse().unwrap();

    // Create a group and put the user in it.
    let g = scim_post(
        &app,
        &token,
        "/api/scim/v2/Groups",
        &json!({
            "displayName": "Executives",
            "members": [{ "value": user_id }],
        }),
    )
    .await;
    let group_id = g.json::<Value>().await.unwrap()["id"].as_str().unwrap().to_string();

    // Owner marks the group as conferring the "enterprise" role.
    let map = app
        .client
        .put(format!(
            "{}/api/enterprise/sso/scim/groups/{group_id}/mapped-role",
            app.addr
        ))
        .json(&json!({ "mapped_role": "enterprise" }))
        .send()
        .await
        .unwrap();
    let status = map.status();
    let raw = map.text().await.unwrap();
    assert_eq!(status, StatusCode::OK, "body: {}", raw);
    let body: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(body["data"]["affected_users"], 1);

    // User's enterprise_members.role is now "enterprise".
    let role: (String,) =
        sqlx::query_as("SELECT role FROM enterprise_members WHERE user_id = $1")
            .bind(user_uuid)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(role.0, "enterprise");

    // Clear the mapping — user falls back to config default (recruiter).
    let unmap = app
        .client
        .put(format!(
            "{}/api/enterprise/sso/scim/groups/{group_id}/mapped-role",
            app.addr
        ))
        .json(&json!({ "mapped_role": null }))
        .send()
        .await
        .unwrap();
    assert_eq!(unmap.status(), StatusCode::OK);

    let role_after: (String,) =
        sqlx::query_as("SELECT role FROM enterprise_members WHERE user_id = $1")
            .bind(user_uuid)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(role_after.0, "recruiter");
}

#[tokio::test]
async fn test_group_removal_demotes_user() {
    let app = common::TestApp::spawn().await;
    let token = setup_scim(&app, "DemoteCorp").await;

    let u = scim_post(
        &app,
        &token,
        "/api/scim/v2/Users",
        &json!({
            "userName": "eve",
            "emails": [{ "value": "eve@demotecorp.example" }],
        }),
    )
    .await;
    let user_id = u.json::<Value>().await.unwrap()["id"].as_str().unwrap().to_string();

    let g = scim_post(
        &app,
        &token,
        "/api/scim/v2/Groups",
        &json!({
            "displayName": "Admins",
            "members": [{ "value": user_id }],
        }),
    )
    .await;
    let group_id = g.json::<Value>().await.unwrap()["id"].as_str().unwrap().to_string();

    // Mark group as enterprise-role.
    app.client
        .put(format!(
            "{}/api/enterprise/sso/scim/groups/{group_id}/mapped-role",
            app.addr
        ))
        .json(&json!({ "mapped_role": "enterprise" }))
        .send()
        .await
        .unwrap();

    // Confirm elevated.
    let role: (String,) =
        sqlx::query_as("SELECT role FROM enterprise_members WHERE user_id = $1::UUID")
            .bind(&user_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(role.0, "enterprise");

    // Remove user from group via Azure-style PATCH (path=members + value=[...]).
    let rm = scim_patch(
        &app,
        &token,
        &format!("/api/scim/v2/Groups/{group_id}"),
        &json!({
            "schemas": ["urn:ietf:params:scim:api:messages:2.0:PatchOp"],
            "Operations": [
                { "op": "remove", "path": "members", "value": [{ "value": user_id }] }
            ]
        }),
    )
    .await;
    assert_eq!(rm.status(), StatusCode::OK);

    // Role falls back to the config default ("recruiter").
    let role_after: (String,) =
        sqlx::query_as("SELECT role FROM enterprise_members WHERE user_id = $1::UUID")
            .bind(&user_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(role_after.0, "recruiter");
}

#[tokio::test]
async fn test_token_rotation_grace_period() {
    let app = common::TestApp::spawn().await;
    let old_token = setup_scim(&app, "RotateCorp").await;

    // Sanity: old token works.
    assert_eq!(
        scim_get(&app, &old_token, "/api/scim/v2/Users").await.status(),
        StatusCode::OK
    );

    // Owner rotates by POSTing again — a new token is minted, the old one is
    // supposed to still work for 24h.
    let rotate = app.post("/api/enterprise/sso/scim/token", &json!({})).await;
    assert_eq!(rotate.status(), StatusCode::OK);
    let new_token = rotate.json::<Value>().await.unwrap()["data"]["token"]
        .as_str()
        .unwrap()
        .to_string();
    assert_ne!(old_token, new_token);

    // Both tokens accepted during the grace window.
    assert_eq!(
        scim_get(&app, &new_token, "/api/scim/v2/Users").await.status(),
        StatusCode::OK
    );
    assert_eq!(
        scim_get(&app, &old_token, "/api/scim/v2/Users").await.status(),
        StatusCode::OK
    );

    // Simulate the grace period expiring — force previous_token_expires_at
    // into the past and the old token must stop working.
    sqlx::query(
        "UPDATE enterprise_sso_configs SET previous_scim_token_expires_at = NOW() - INTERVAL '1 hour'
         WHERE previous_scim_token_hash IS NOT NULL",
    )
    .execute(&app.db)
    .await
    .unwrap();

    assert_eq!(
        scim_get(&app, &old_token, "/api/scim/v2/Users").await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        scim_get(&app, &new_token, "/api/scim/v2/Users").await.status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn test_scim_audit_trail() {
    let app = common::TestApp::spawn().await;
    let token = setup_scim(&app, "AuditCorp").await;

    // Provisioning a user writes an audit row.
    let u = scim_post(
        &app,
        &token,
        "/api/scim/v2/Users",
        &json!({
            "userName": "audited",
            "emails": [{ "value": "audited@auditcorp.example" }],
        }),
    )
    .await;
    let user_id = u.json::<Value>().await.unwrap()["id"].as_str().unwrap().to_string();

    let provisioned: (String,) = sqlx::query_as(
        "SELECT action FROM audit_log WHERE action = 'scim.user.provisioned' AND target_id = $1::UUID",
    )
    .bind(&user_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(provisioned.0, "scim.user.provisioned");

    // DELETE writes another row.
    scim_delete(&app, &token, &format!("/api/scim/v2/Users/{user_id}"))
        .await;

    let deleted: (String,) = sqlx::query_as(
        "SELECT action FROM audit_log WHERE action = 'scim.user.deleted' AND target_id = $1::UUID",
    )
    .bind(&user_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(deleted.0, "scim.user.deleted");
}

#[tokio::test]
async fn test_scim_token_requires_prior_sso_config() {
    let app = common::TestApp::spawn().await;
    app.register_enterprise("NoSsoYetCorp").await;
    app.login("nossoyetcorp").await;
    app.enable_totp_for("nossoyetcorp").await;

    // No SSO config → generating a SCIM token must fail cleanly (400), not panic.
    let resp = app.post("/api/enterprise/sso/scim/token", &json!({})).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}
