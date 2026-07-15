//! Tests P25.1 : extension user_capabilities enum avec 5 caps modération.

use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p25_1_test_{}",
        Uuid::new_v4().to_string().replace('-', "")
    );
    let admin_pool = PgPoolOptions::new().max_connections(2)
        .connect("postgres://skilluv:skilluv_secret@localhost:5433/skilluv")
        .await.expect("admin");
    sqlx::query(&format!("CREATE DATABASE \"{db_name}\""))
        .execute(&admin_pool).await.expect("create");
    admin_pool.close().await;

    let db_url = format!("postgres://skilluv:skilluv_secret@localhost:5433/{db_name}");
    let db = PgPoolOptions::new().max_connections(5)
        .connect(&db_url).await.expect("connect");
    sqlx::migrate!("./migrations").run(&db).await.expect("migrations");
    (db, db_name)
}

async fn cleanup_test_db(db_name: &str) {
    let admin_pool = PgPoolOptions::new().max_connections(2)
        .connect("postgres://skilluv:skilluv_secret@localhost:5433/skilluv")
        .await.expect("admin");
    let _ = sqlx::query(&format!(
        "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '{db_name}'"
    )).execute(&admin_pool).await;
    let _ = sqlx::query(&format!("DROP DATABASE IF EXISTS \"{db_name}\"")).execute(&admin_pool).await;
    admin_pool.close().await;
}

async fn create_user(db: &PgPool) -> Uuid {
    let uid = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, email, username, first_name, last_name, display_name,
             password_hash, profile_active, total_fragments)
         VALUES ($1, $2, $3, 't','u','t','x',TRUE,0)",
    )
    .bind(uid)
    .bind(format!("t-{uid}@ex.io"))
    .bind(format!("t{}", &uid.to_string()[..8]))
    .execute(db).await.expect("u");
    uid
}

#[tokio::test]
async fn all_5_new_moderator_caps_accepted() {
    let (db, name) = setup_test_db().await;
    let u = create_user(&db).await;
    for cap in [
        "community_moderator",
        "forum_moderator",
        "plagiarism_reviewer",
        "kyc_reviewer",
        "community_curator",
    ] {
        sqlx::query("INSERT INTO user_capabilities (user_id, capability) VALUES ($1, $2)")
            .bind(u).bind(cap).execute(&db).await
            .unwrap_or_else(|e| panic!("cap {cap} should be accepted: {e}"));
    }
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_capabilities WHERE user_id = $1 AND revoked_at IS NULL",
    )
    .bind(u).fetch_one(&db).await.unwrap();
    assert_eq!(n, 5, "5 stackées simultanément");
    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn existing_p18_caps_still_accepted() {
    let (db, name) = setup_test_db().await;
    let u = create_user(&db).await;
    for cap in ["challenger", "mentor", "admin", "enterprise_recruiter"] {
        sqlx::query("INSERT INTO user_capabilities (user_id, capability) VALUES ($1, $2)")
            .bind(u).bind(cap).execute(&db).await
            .unwrap_or_else(|e| panic!("legacy cap {cap} regressed: {e}"));
    }
    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn unknown_cap_still_rejected() {
    let (db, name) = setup_test_db().await;
    let u = create_user(&db).await;
    let bad = sqlx::query(
        "INSERT INTO user_capabilities (user_id, capability) VALUES ($1, 'godmode')",
    )
    .bind(u).execute(&db).await;
    assert!(bad.is_err(), "unknown capability must remain rejected");
    db.close().await;
    cleanup_test_db(&name).await;
}
