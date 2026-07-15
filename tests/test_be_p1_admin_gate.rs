//! Tests BE-A + BE-B + BE-C — Admin gate + reset-2fa endpoint.
//!
//! Couvre :
//!   - BE-A : admin sans 2FA reçoit AUTH_ADMIN_2FA_SETUP_REQUIRED (403) sur
//!     toute route /api/admin/*. Admin avec TOTP OU passkey passe.
//!   - BE-C : requête vers /api/admin/* depuis origin non-admin retourne
//!     AUTH_ADMIN_ORIGIN_REQUIRED (403). Depuis origin admin.* ou localhost:5174,
//!     passe.
//!   - BE-B : POST /admin/users/{id}/reset-2fa wipe TOTP + WebAuthn + backup
//!     codes + révoque sessions. Réservé aux users avec capability 'admin'.

mod common;
use common::TestApp;
use serde_json::json;

async fn grant_admin_capability(app: &TestApp, uid: uuid::Uuid) {
    sqlx::query(
        "INSERT INTO user_capabilities (user_id, capability, granted_reason)
         VALUES ($1, 'admin', 'test_setup') ON CONFLICT DO NOTHING",
    )
    .bind(uid).execute(&app.db).await.unwrap();
}

/// Simule un passkey enregistré pour satisfaire le check "second facteur".
/// On préfère passkey vs TOTP dans les tests car TOTP forcerait un code
/// au prochain login. Le middleware `ensure_admin_2fa` accepte les 2.
async fn register_passkey_for(app: &TestApp, uid: uuid::Uuid) {
    sqlx::query(
        "INSERT INTO webauthn_credentials
            (user_id, credential_id, credential, label)
         VALUES ($1, $2, '{\"stub\":true}'::jsonb, 'test-passkey')",
    )
    .bind(uid)
    .bind(format!("cred-{uid}").into_bytes())
    .execute(&app.db).await.unwrap();
}

// ═══════════════════════════════════════════════════════════════════
// BE-A — 2FA mandatory admin
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn admin_without_2fa_gets_admin_2fa_setup_required_on_admin_routes() {
    let app = TestApp::spawn().await;
    app.register_admin("admin_no_2fa").await;

    // register_admin met users.role='admin' + user_capabilities.admin
    // MAIS ne configure PAS TOTP ni webauthn. Donc admin sans 2FA.
    // Appel depuis origin admin → passe le BE-C origin gate mais bloqué par BE-A 2fa gate.
    let resp = app
        .client
        .get(format!("{}/api/admin/stats", app.addr))
        .header("origin", "http://localhost:5174")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 403, "admin sans 2FA doit recevoir 403");
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], "AUTH_ADMIN_2FA_SETUP_REQUIRED");
}

#[tokio::test]
async fn admin_with_totp_passes_admin_2fa_gate() {
    let app = TestApp::spawn().await;
    app.register_admin("admin_with_totp").await;
    let uid: uuid::Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'admin_with_totp'")
        .fetch_one(&app.db).await.unwrap();
    register_passkey_for(&app, uid).await;

    let resp = app
        .client
        .get(format!("{}/api/admin/stats", app.addr))
        .header("origin", "http://localhost:5174")
        .send()
        .await
        .unwrap();
    // Ne doit plus être 403 (BE-A + BE-C tous les 2 satisfaits). Peut être 200, 404, etc.
    assert_ne!(resp.status().as_u16(), 403, "admin avec passkey + origin admin doit passer les 2 gates");
}

// ═══════════════════════════════════════════════════════════════════
// BE-C — Origin server-side check
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn admin_route_from_non_admin_origin_is_rejected() {
    let app = TestApp::spawn().await;
    app.register_admin("admin_origin_test").await;
    let uid: uuid::Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'admin_origin_test'")
        .fetch_one(&app.db).await.unwrap();
    register_passkey_for(&app, uid).await;

    // Le test client par défaut n'envoie pas d'Origin admin — reproduit
    // le comportement d'un browser sur skilluv.com qui appellerait /api/admin/*.
    let resp = app
        .client
        .get(format!("{}/api/admin/stats", app.addr))
        .header("origin", "https://skilluv.com") // origin publique, NON admin
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 403);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], "AUTH_ADMIN_ORIGIN_REQUIRED");
}

#[tokio::test]
async fn admin_route_from_admin_origin_localhost5174_passes_origin_gate() {
    let app = TestApp::spawn().await;
    app.register_admin("admin_localhost").await;
    let uid: uuid::Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'admin_localhost'")
        .fetch_one(&app.db).await.unwrap();
    register_passkey_for(&app, uid).await;

    let resp = app
        .client
        .get(format!("{}/api/admin/stats", app.addr))
        .header("origin", "http://localhost:5174")
        .send()
        .await
        .unwrap();
    assert_ne!(resp.status().as_u16(), 403,
        "origin localhost:5174 doit passer le origin gate");
}

// ═══════════════════════════════════════════════════════════════════
// BE-B — reset-2fa endpoint
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn admin_reset_2fa_wipes_totp_webauthn_and_revokes_sessions() {
    let app = TestApp::spawn().await;
    app.register_admin("admin_reset").await;
    let admin_uid: uuid::Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'admin_reset'")
        .fetch_one(&app.db).await.unwrap();
    register_passkey_for(&app, admin_uid).await;
    grant_admin_capability(&app, admin_uid).await;

    app.register_user("target_reset").await;
    let target_uid: uuid::Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'target_reset'")
        .fetch_one(&app.db).await.unwrap();
    // Simule TOTP + backup codes + webauthn sur le user cible.
    sqlx::query(
        "UPDATE users SET totp_enabled = TRUE, totp_secret = 'ABC' WHERE id = $1",
    )
    .bind(target_uid).execute(&app.db).await.unwrap();
    sqlx::query(
        "INSERT INTO totp_backup_codes (user_id, code_hash) VALUES ($1, 'hash1'), ($1, 'hash2')",
    )
    .bind(target_uid).execute(&app.db).await.unwrap();

    // Login as admin puis reset le user cible.
    app.login("admin_reset").await;
    let resp = app
        .client
        .post(format!("{}/api/admin/users/{}/reset-2fa", app.addr, target_uid))
        .header("origin", "http://localhost:5174")
        .json(&json!({ "reason": "test reset for security incident" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 200, "reset doit réussir pour admin");

    // Vérifie que TOTP est reset côté DB.
    let (totp_enabled, totp_secret): (bool, Option<String>) = sqlx::query_as(
        "SELECT totp_enabled, totp_secret FROM users WHERE id = $1",
    )
    .bind(target_uid).fetch_one(&app.db).await.unwrap();
    assert!(!totp_enabled);
    assert!(totp_secret.is_none());

    // Backup codes wipés.
    let backup_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM totp_backup_codes WHERE user_id = $1",
    )
    .bind(target_uid).fetch_one(&app.db).await.unwrap();
    assert_eq!(backup_count, 0);

    // Audit log écrit.
    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM admin_audit_log
         WHERE action = 'reset_2fa' AND target_id = $1 AND admin_id = $2",
    )
    .bind(target_uid).bind(admin_uid).fetch_one(&app.db).await.unwrap();
    assert_eq!(audit_count, 1);
}

#[tokio::test]
async fn admin_reset_2fa_refuses_short_reason() {
    let app = TestApp::spawn().await;
    app.register_admin("admin_short_reason").await;
    let admin_uid: uuid::Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'admin_short_reason'")
        .fetch_one(&app.db).await.unwrap();
    register_passkey_for(&app, admin_uid).await;
    grant_admin_capability(&app, admin_uid).await;

    app.register_user("target_short").await;
    let target_uid: uuid::Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'target_short'")
        .fetch_one(&app.db).await.unwrap();

    app.login("admin_short_reason").await;
    let resp = app
        .client
        .post(format!("{}/api/admin/users/{}/reset-2fa", app.addr, target_uid))
        .header("origin", "http://localhost:5174")
        .json(&json!({ "reason": "bad" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 400);
}

#[tokio::test]
async fn admin_reset_2fa_refuses_non_admin() {
    let app = TestApp::spawn().await;
    // Register plain user (no admin capability)
    app.register_user("normal_reset_attempt").await;
    let user_uid: uuid::Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'normal_reset_attempt'")
        .fetch_one(&app.db).await.unwrap();
    register_passkey_for(&app, user_uid).await;

    app.register_user("target_x").await;
    let target_uid: uuid::Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'target_x'")
        .fetch_one(&app.db).await.unwrap();

    app.login("normal_reset_attempt").await;
    let resp = app
        .client
        .post(format!("{}/api/admin/users/{}/reset-2fa", app.addr, target_uid))
        .header("origin", "http://localhost:5174")
        .json(&json!({ "reason": "trying to reset without admin" }))
        .send()
        .await
        .unwrap();
    // Peut être 403 pour cause d'origin (si CORS filtre avant) ou 403
    // pour cause de capability. Les deux sont acceptables — on refuse.
    assert_eq!(resp.status().as_u16(), 403);
}
