//! Tests P25.3 : helper require_any_capability.

use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

use skilluv_backend::errors::AppError;
use skilluv_backend::middleware::capabilities::require_any_capability;

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p25_3_test_{}",
        Uuid::new_v4().to_string().replace('-', "")
    );
    let admin_pool = PgPoolOptions::new()
        .max_connections(2)
        .connect("postgres://skilluv:skilluv_secret@localhost:5433/skilluv")
        .await
        .expect("admin");
    sqlx::query(&format!("CREATE DATABASE \"{db_name}\""))
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
    let _ = sqlx::query(&format!(
        "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '{db_name}'"
    ))
    .execute(&admin_pool)
    .await;
    let _ = sqlx::query(&format!("DROP DATABASE IF EXISTS \"{db_name}\""))
        .execute(&admin_pool)
        .await;
    admin_pool.close().await;
}

async fn create_user_with_cap(db: &PgPool, cap: Option<&str>) -> Uuid {
    let uid = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, email, username, first_name, last_name, display_name,
             password_hash, profile_active, total_fragments)
         VALUES ($1, $2, $3, 't','u','t','x',TRUE,0)",
    )
    .bind(uid)
    .bind(format!("t-{uid}@ex.io"))
    .bind(format!("t{}", &uid.to_string()[..8]))
    .execute(db)
    .await
    .expect("u");
    if let Some(c) = cap {
        sqlx::query("INSERT INTO user_capabilities (user_id, capability) VALUES ($1, $2)")
            .bind(uid)
            .bind(c)
            .execute(db)
            .await
            .unwrap();
    }
    uid
}

#[tokio::test]
async fn passes_when_user_has_first_cap() {
    let (db, name) = setup_test_db().await;
    let u = create_user_with_cap(&db, Some("plagiarism_reviewer")).await;
    let res = require_any_capability(&db, u, &["plagiarism_reviewer", "admin"]).await;
    assert!(res.is_ok());
    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn passes_when_user_has_second_cap() {
    let (db, name) = setup_test_db().await;
    let u = create_user_with_cap(&db, Some("admin")).await;
    let res = require_any_capability(&db, u, &["plagiarism_reviewer", "admin"]).await;
    assert!(res.is_ok());
    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn forbidden_when_none_match() {
    let (db, name) = setup_test_db().await;
    let u = create_user_with_cap(&db, Some("challenger")).await;
    let res = require_any_capability(&db, u, &["plagiarism_reviewer", "admin"]).await;
    assert!(matches!(res, Err(AppError::Forbidden)));
    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn forbidden_when_empty_capabilities_list() {
    let (db, name) = setup_test_db().await;
    let u = create_user_with_cap(&db, Some("admin")).await;
    let res = require_any_capability(&db, u, &[]).await;
    assert!(
        matches!(res, Err(AppError::Forbidden)),
        "empty list is a nonsensical query → refuse par sécurité"
    );
    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn forbidden_when_matched_cap_is_revoked() {
    let (db, name) = setup_test_db().await;
    let u = create_user_with_cap(&db, Some("forum_moderator")).await;
    sqlx::query(
        "UPDATE user_capabilities SET revoked_at = NOW()
                 WHERE user_id = $1 AND capability = 'forum_moderator'",
    )
    .bind(u)
    .execute(&db)
    .await
    .unwrap();
    let res = require_any_capability(&db, u, &["forum_moderator", "plagiarism_reviewer"]).await;
    assert!(matches!(res, Err(AppError::Forbidden)));
    db.close().await;
    cleanup_test_db(&name).await;
}
