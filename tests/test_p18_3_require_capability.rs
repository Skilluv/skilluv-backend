//! Tests P18.3 : middleware require_capability.

use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

use skilluv_backend::errors::AppError;
use skilluv_backend::middleware::capabilities::{list_active_capabilities, require_capability};

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p18_3_test_{}",
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

async fn create_user(db: &PgPool, role: &str) -> Uuid {
    let uid = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, email, username, first_name, last_name, display_name,
                             password_hash, profile_active, total_fragments, role)
         VALUES ($1, $2, $3, 't','u','t','x',TRUE,0,$4)",
    )
    .bind(uid)
    .bind(format!("t-{uid}@ex.io"))
    .bind(format!("t{}", &uid.to_string()[..8]))
    .bind(role)
    .execute(db)
    .await
    .expect("u");
    // Ne s'appuie pas sur le backfill (fait au moment de la migration).
    // On simule le grant manuel selon le rôle.
    match role {
        "admin" => {
            sqlx::query("INSERT INTO user_capabilities (user_id, capability) VALUES ($1,'admin')")
                .bind(uid)
                .execute(db)
                .await
                .unwrap();
        }
        "mentor" => {
            sqlx::query("INSERT INTO user_capabilities (user_id, capability) VALUES ($1,'mentor')")
                .bind(uid)
                .execute(db)
                .await
                .unwrap();
        }
        _ => {}
    }
    sqlx::query(
        "INSERT INTO user_capabilities (user_id, capability) VALUES ($1,'challenger')
                 ON CONFLICT DO NOTHING",
    )
    .bind(uid)
    .execute(db)
    .await
    .unwrap();
    uid
}

#[tokio::test]
async fn require_capability_passes_when_active() {
    let (db, name) = setup_test_db().await;
    let admin = create_user(&db, "admin").await;
    assert!(require_capability(&db, admin, "admin").await.is_ok());
    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn require_capability_forbidden_when_absent() {
    let (db, name) = setup_test_db().await;
    let u = create_user(&db, "user").await;
    let r = require_capability(&db, u, "admin").await;
    assert!(matches!(r, Err(AppError::Forbidden)));
    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn require_capability_forbidden_when_revoked() {
    let (db, name) = setup_test_db().await;
    let u = create_user(&db, "admin").await;
    sqlx::query(
        "UPDATE user_capabilities SET revoked_at = NOW()
                 WHERE user_id = $1 AND capability = 'admin'",
    )
    .bind(u)
    .execute(&db)
    .await
    .unwrap();
    let r = require_capability(&db, u, "admin").await;
    assert!(matches!(r, Err(AppError::Forbidden)));
    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn require_capability_forbidden_when_expired() {
    let (db, name) = setup_test_db().await;
    let u = create_user(&db, "user").await;
    sqlx::query(
        "INSERT INTO user_capabilities (user_id, capability, expires_at)
         VALUES ($1, 'jury_tournament', NOW() - INTERVAL '1 day')",
    )
    .bind(u)
    .execute(&db)
    .await
    .unwrap();
    let r = require_capability(&db, u, "jury_tournament").await;
    assert!(
        matches!(r, Err(AppError::Forbidden)),
        "expired capability rejected"
    );
    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn list_active_capabilities_excludes_revoked_and_expired() {
    let (db, name) = setup_test_db().await;
    let u = create_user(&db, "mentor").await;

    // Ajoute une révoquée + une expirée + une active supplémentaire.
    sqlx::query(
        "INSERT INTO user_capabilities (user_id, capability, revoked_at)
                 VALUES ($1, 'pr_reviewer', NOW())",
    )
    .bind(u)
    .execute(&db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO user_capabilities (user_id, capability, expires_at)
                 VALUES ($1, 'jury_tournament', NOW() - INTERVAL '1 day')",
    )
    .bind(u)
    .execute(&db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO user_capabilities (user_id, capability)
                 VALUES ($1, 'issue_proposer')",
    )
    .bind(u)
    .execute(&db)
    .await
    .unwrap();

    let caps = list_active_capabilities(&db, u).await.unwrap();
    // Expected: challenger + mentor + issue_proposer (3), sorted alpha.
    assert_eq!(
        caps,
        vec![
            "challenger".to_string(),
            "issue_proposer".to_string(),
            "mentor".to_string(),
        ]
    );

    db.close().await;
    cleanup_test_db(&name).await;
}
