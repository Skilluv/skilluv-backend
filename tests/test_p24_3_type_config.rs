//! Tests P24.3 : type_config JSONB + validation par type.
//!
//! Ces tests exercent la contrainte DB (colonne créée + defaults) et la
//! logique de merge JSONB. Les tests HTTP des routes GET/PATCH sont
//! implicites via P24.4 CHANGELOG (contrat testable manuellement).

use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p24_3_test_{}",
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

async fn create_enterprise(db: &PgPool, slug: &str, ent_type: &str) -> Uuid {
    let owner = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, email, username, first_name, last_name, display_name,
             password_hash, profile_active, total_fragments, role)
         VALUES ($1, $2, $3, 't','u','t','x',TRUE,0,'enterprise')",
    )
    .bind(owner)
    .bind(format!("t-{owner}@ex.io"))
    .bind(format!("t{}", &owner.to_string()[..8]))
    .execute(db)
    .await
    .expect("u");
    sqlx::query_scalar(
        "INSERT INTO enterprises (owner_id, company_name, slug, company_size, enterprise_type)
         VALUES ($1, 'Corp', $2, '51-200', $3) RETURNING id",
    )
    .bind(owner)
    .bind(slug)
    .bind(ent_type)
    .fetch_one(db)
    .await
    .expect("ent")
}

#[tokio::test]
async fn default_type_config_is_empty_object() {
    let (db, name) = setup_test_db().await;
    let e = create_enterprise(&db, "cfg-default", "direct_hire").await;
    let cfg: serde_json::Value =
        sqlx::query_scalar("SELECT type_config FROM enterprises WHERE id = $1")
            .bind(e)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(cfg, serde_json::json!({}));
    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn jsonb_merge_preserves_and_overwrites_keys() {
    let (db, name) = setup_test_db().await;
    let e = create_enterprise(&db, "cfg-merge", "remote_international").await;
    // Set initial config
    sqlx::query("UPDATE enterprises SET type_config = $1::jsonb WHERE id = $2")
        .bind(serde_json::json!({"eor_provider":"deel","preferred_currency":"USD"}))
        .bind(e)
        .execute(&db)
        .await
        .unwrap();
    // Merge patch
    sqlx::query("UPDATE enterprises SET type_config = type_config || $1::jsonb WHERE id = $2")
        .bind(serde_json::json!({"preferred_currency":"EUR","timezone_requirement":"UTC±3"}))
        .bind(e)
        .execute(&db)
        .await
        .unwrap();

    let cfg: serde_json::Value =
        sqlx::query_scalar("SELECT type_config FROM enterprises WHERE id = $1")
            .bind(e)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(
        cfg["eor_provider"], "deel",
        "clé absente du patch préservée"
    );
    assert_eq!(cfg["preferred_currency"], "EUR", "clé présente overwritée");
    assert_eq!(cfg["timezone_requirement"], "UTC±3", "nouvelle clé ajoutée");

    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn gin_index_supports_jsonb_query() {
    let (db, name) = setup_test_db().await;
    let e1 = create_enterprise(&db, "cfg-idx-a", "staffing_agency").await;
    let e2 = create_enterprise(&db, "cfg-idx-b", "staffing_agency").await;
    sqlx::query("UPDATE enterprises SET type_config = $1::jsonb WHERE id = $2")
        .bind(serde_json::json!({"commission_rate":0.15,"brand_white_label":true}))
        .bind(e1)
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("UPDATE enterprises SET type_config = $1::jsonb WHERE id = $2")
        .bind(serde_json::json!({"commission_rate":0.20}))
        .bind(e2)
        .execute(&db)
        .await
        .unwrap();

    // Query "quelles agences font du white-label ?" via containment
    let ids: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM enterprises WHERE type_config @> $1::jsonb")
            .bind(serde_json::json!({"brand_white_label":true}))
            .fetch_all(&db)
            .await
            .unwrap();
    assert!(ids.contains(&e1));
    assert!(!ids.contains(&e2));

    db.close().await;
    cleanup_test_db(&name).await;
}
