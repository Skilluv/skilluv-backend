//! Tests P24.1 : enterprises.enterprise_type + backfill.

use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p24_1_test_{}",
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

async fn create_owner(db: &PgPool) -> Uuid {
    let uid = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, email, username, first_name, last_name, display_name,
             password_hash, profile_active, total_fragments, role)
         VALUES ($1, $2, $3, 't','u','t','x',TRUE,0,'enterprise')",
    )
    .bind(uid)
    .bind(format!("t-{uid}@ex.io"))
    .bind(format!("t{}", &uid.to_string()[..8]))
    .execute(db)
    .await
    .expect("u");
    uid
}

async fn create_enterprise(db: &PgPool, slug: &str, ent_type: Option<&str>) -> Uuid {
    let owner = create_owner(db).await;
    let query = if let Some(t) = ent_type {
        format!(
            "INSERT INTO enterprises (owner_id, company_name, slug, company_size, enterprise_type)
             VALUES ($1, 'Corp', $2, '51-200', '{t}') RETURNING id"
        )
    } else {
        "INSERT INTO enterprises (owner_id, company_name, slug, company_size)
         VALUES ($1, 'Corp', $2, '51-200') RETURNING id"
            .to_string()
    };
    sqlx::query_scalar(sqlx::AssertSqlSafe(query))
        .bind(owner)
        .bind(slug)
        .fetch_one(db)
        .await
        .expect("ent")
}

#[tokio::test]
async fn default_type_is_direct_hire() {
    let (db, name) = setup_test_db().await;
    let e = create_enterprise(&db, "acme-default", None).await;
    let t: String = sqlx::query_scalar("SELECT enterprise_type FROM enterprises WHERE id = $1")
        .bind(e)
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(t, "direct_hire");
    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn all_three_types_accepted() {
    let (db, name) = setup_test_db().await;
    for (slug, t) in [
        ("acme-direct", "direct_hire"),
        ("acme-agency", "staffing_agency"),
        ("acme-remote", "remote_international"),
    ] {
        let e = create_enterprise(&db, slug, Some(t)).await;
        let stored: String =
            sqlx::query_scalar("SELECT enterprise_type FROM enterprises WHERE id = $1")
                .bind(e)
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(stored, t);
    }
    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn invalid_type_rejected() {
    let (db, name) = setup_test_db().await;
    let owner = create_owner(&db).await;
    let bad = sqlx::query(
        "INSERT INTO enterprises (owner_id, company_name, slug, company_size, enterprise_type)
         VALUES ($1, 'Corp', 'bad', '51-200', 'freelancer')",
    )
    .bind(owner)
    .execute(&db)
    .await;
    assert!(bad.is_err(), "invalid enterprise_type must be rejected");
    db.close().await;
    cleanup_test_db(&name).await;
}
