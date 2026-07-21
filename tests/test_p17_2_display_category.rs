//! Tests P17.2 : skill_nodes.display_category + backfill deterministic.

use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p17_2_test_{}",
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

#[tokio::test]
async fn backfill_maps_every_seeded_skill_to_a_valid_category() {
    let (db, name) = setup_test_db().await;
    let unmapped: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM skill_nodes
         WHERE display_category NOT IN ('craft','create','understand','operate','share','meta')",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(unmapped, 0);
    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn code_domain_maps_to_craft() {
    let (db, name) = setup_test_db().await;
    let bad: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM skill_nodes WHERE domain = 'code' AND display_category <> 'craft'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(bad, 0);
    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn design_and_game_domains_map_to_create() {
    let (db, name) = setup_test_db().await;
    let bad: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM skill_nodes
         WHERE domain IN ('design','game') AND display_category <> 'create'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(bad, 0);
    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn security_and_ops_domains_map_to_operate() {
    let (db, name) = setup_test_db().await;
    let bad: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM skill_nodes
         WHERE domain IN ('security','ops') AND display_category <> 'operate'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(bad, 0);
    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn ai_maps_to_understand_soft_skills_to_share() {
    let (db, name) = setup_test_db().await;
    let bad_ai: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM skill_nodes WHERE domain = 'ai' AND display_category <> 'understand'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(bad_ai, 0);
    let bad_soft: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM skill_nodes WHERE domain = 'soft_skills' AND display_category <> 'share'",
    ).fetch_one(&db).await.unwrap();
    assert_eq!(bad_soft, 0);
    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn display_category_check_rejects_invalid() {
    let (db, name) = setup_test_db().await;
    let bad = sqlx::query(
        "INSERT INTO skill_nodes (slug, display_name, domain, display_category)
         VALUES ('p17-bad-cat', 'Bad', 'code', 'legendary')",
    )
    .execute(&db)
    .await;
    assert!(bad.is_err());
    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn admin_can_override_to_meta() {
    let (db, name) = setup_test_db().await;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO skill_nodes (slug, display_name, domain, display_category)
         VALUES ('p17-oss-gov', 'OSS Gov', 'code', 'meta') RETURNING id",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    let cat: String = sqlx::query_scalar("SELECT display_category FROM skill_nodes WHERE id = $1")
        .bind(id)
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(cat, "meta");
    db.close().await;
    cleanup_test_db(&name).await;
}
