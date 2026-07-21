//! Tests d'intégration P15.1 : mobile push (FCM + APNS).

use std::str::FromStr;

use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

use skilluv_backend::services::mobile_push::{self, MobilePushMessage, Platform};

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p15_1_test_{}",
        Uuid::new_v4().to_string().replace('-', "")
    );
    let admin_pool = PgPoolOptions::new()
        .max_connections(2)
        .connect("postgres://skilluv:skilluv_secret@localhost:5433/skilluv")
        .await
        .expect("admin");
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "CREATE DATABASE \"{db_name}\""
    )))
    .execute(&admin_pool)
    .await
    .expect("create");
    admin_pool.close().await;

    let db_url = format!("postgres://skilluv:skilluv_secret@localhost:5433/{db_name}");
    let db = PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await
        .expect("connect");
    sqlx::migrate!("./migrations")
        .run(&db)
        .await
        .expect("migrations");
    (db, db_name)
}

async fn cleanup_test_db(db_name: &str) {
    let admin_pool = PgPoolOptions::new()
        .max_connections(2)
        .connect("postgres://skilluv:skilluv_secret@localhost:5433/skilluv")
        .await
        .expect("admin");
    let _ = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '{db_name}'"
    )))
    .execute(&admin_pool)
    .await;
    let _ = sqlx::query(sqlx::AssertSqlSafe(format!(
        "DROP DATABASE IF EXISTS \"{db_name}\""
    )))
    .execute(&admin_pool)
    .await;
    admin_pool.close().await;
}

async fn insert_user(db: &PgPool) -> Uuid {
    let uid = Uuid::new_v4();
    let short = &uid.to_string()[..8];
    sqlx::query(
        "INSERT INTO users
            (id, email, username, first_name, last_name, display_name, password_hash,
             profile_active, total_fragments)
         VALUES ($1, $2, $3, 'T', 'U', 'Test', 'dummy', TRUE, 0)",
    )
    .bind(uid)
    .bind(format!("t-{uid}@ex.io"))
    .bind(format!("t{short}"))
    .execute(db)
    .await
    .expect("u");
    uid
}

// ═══════════════════════════════════════════════════════════════════
// Platform parse
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn platform_from_str_accepts_variants() {
    assert_eq!(Platform::from_str("fcm").unwrap(), Platform::Fcm);
    assert_eq!(Platform::from_str("FCM").unwrap(), Platform::Fcm);
    assert_eq!(Platform::from_str("android").unwrap(), Platform::Fcm);
    assert_eq!(Platform::from_str("apns").unwrap(), Platform::Apns);
    assert_eq!(Platform::from_str("ios").unwrap(), Platform::Apns);
    assert!(Platform::from_str("windows").is_err());
}

// ═══════════════════════════════════════════════════════════════════
// register_token upsert idempotent
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn register_token_upserts_by_device_id() {
    let (db, name) = setup_test_db().await;
    let user = insert_user(&db).await;

    let a = mobile_push::register_token(&db, user, Platform::Fcm, "tok-v1", "dev-1")
        .await
        .expect("a");
    assert_eq!(a.token, "tok-v1");
    let b = mobile_push::register_token(&db, user, Platform::Fcm, "tok-v2", "dev-1")
        .await
        .expect("b");
    assert_eq!(b.token, "tok-v2", "token remplacé sur même device_id");

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM user_push_tokens WHERE user_id = $1")
        .bind(user)
        .fetch_one(&db)
        .await
        .expect("c");
    assert_eq!(count, 1);

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Register refuse token vide
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn register_refuses_empty_fields() {
    let (db, name) = setup_test_db().await;
    let user = insert_user(&db).await;

    let bad_tok = mobile_push::register_token(&db, user, Platform::Fcm, "", "dev").await;
    assert!(bad_tok.is_err());
    let bad_dev = mobile_push::register_token(&db, user, Platform::Fcm, "tok", "").await;
    assert!(bad_dev.is_err());

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// revoke_token supprime la row
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn revoke_token_removes_row() {
    let (db, name) = setup_test_db().await;
    let user = insert_user(&db).await;
    mobile_push::register_token(&db, user, Platform::Apns, "tokA", "dev-ios")
        .await
        .expect("r");
    let n = mobile_push::revoke_token(&db, user, "dev-ios")
        .await
        .expect("d");
    assert_eq!(n, 1);
    let n2 = mobile_push::revoke_token(&db, user, "dev-ios")
        .await
        .expect("d2");
    assert_eq!(n2, 0);

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// push_to_user_mobile : stub OK sans creds, refresh last_seen_at
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn push_delivers_via_stub_and_refreshes_last_seen() {
    // SAFETY: env removal isolated.
    unsafe {
        std::env::remove_var("FCM_SERVER_KEY");
        std::env::remove_var("APNS_KEY_ID");
    }
    let (db, name) = setup_test_db().await;
    let user = insert_user(&db).await;
    mobile_push::register_token(&db, user, Platform::Fcm, "tokf", "d1")
        .await
        .expect("r1");
    mobile_push::register_token(&db, user, Platform::Apns, "toka", "d2")
        .await
        .expect("r2");

    // Force les last_seen_at à hier pour vérifier le refresh
    sqlx::query(
        "UPDATE user_push_tokens SET last_seen_at = NOW() - INTERVAL '1 day' WHERE user_id = $1",
    )
    .bind(user)
    .execute(&db)
    .await
    .expect("age");

    let out = mobile_push::push_to_user_mobile(
        &db,
        user,
        MobilePushMessage {
            title: "Hello",
            body: "World",
            data: None,
        },
    )
    .await
    .expect("p");
    assert_eq!(out.len(), 2, "2 devices");
    assert!(out.iter().all(|o| o.delivered));

    // last_seen_at récent
    let stale: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_push_tokens
         WHERE user_id = $1 AND last_seen_at > NOW() - INTERVAL '1 minute'",
    )
    .bind(user)
    .fetch_one(&db)
    .await
    .expect("s");
    assert_eq!(stale, 2, "les 2 tokens ont un last_seen refresh");

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// purge_stale supprime tokens vieux
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn purge_stale_removes_old_tokens() {
    let (db, name) = setup_test_db().await;
    let user = insert_user(&db).await;
    mobile_push::register_token(&db, user, Platform::Fcm, "recent", "d-recent")
        .await
        .expect("r");
    sqlx::query(
        "INSERT INTO user_push_tokens (user_id, platform, token, device_id, last_seen_at)
         VALUES ($1, 'fcm', 'old', 'd-old', NOW() - INTERVAL '120 days')",
    )
    .bind(user)
    .execute(&db)
    .await
    .expect("old");

    let deleted = mobile_push::purge_stale(&db, 90).await.expect("p");
    assert_eq!(deleted, 1);
    let remaining: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM user_push_tokens WHERE user_id = $1")
            .bind(user)
            .fetch_one(&db)
            .await
            .expect("c");
    assert_eq!(remaining, 1);

    db.close().await;
    cleanup_test_db(&name).await;
}
