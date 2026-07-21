//! Tests IA-D — Table ai_call_log + helper record.

use sqlx::postgres::{PgPool, PgPoolOptions};
use std::time::Duration;
use uuid::Uuid;

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_ia_d_test_{}",
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
async fn ai_call_log_table_exists_after_migration_0101() {
    let (db, name) = setup_test_db().await;
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM information_schema.tables WHERE table_name = 'ai_call_log')",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert!(exists);
    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn status_check_accepts_5_values() {
    let (db, name) = setup_test_db().await;
    for status in [
        "ok",
        "unavailable",
        "internal",
        "business_failure",
        "timeout",
    ] {
        let res = sqlx::query(
            "INSERT INTO ai_call_log (method, latency_ms, status)
             VALUES ('TestMethod', 100, $1)",
        )
        .bind(status)
        .execute(&db)
        .await;
        assert!(res.is_ok(), "status {status} rejected: {res:?}");
    }
    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn status_check_rejects_invalid() {
    let (db, name) = setup_test_db().await;
    let res = sqlx::query(
        "INSERT INTO ai_call_log (method, latency_ms, status)
         VALUES ('X', 100, 'godmode')",
    )
    .execute(&db)
    .await;
    assert!(res.is_err());
    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn record_helper_inserts_ok_row_and_metrics() {
    let (db, name) = setup_test_db().await;

    // Simule un appel gRPC réussi (Ok(())).
    let result: Result<(), tonic::Status> = Ok(());
    skilluv_backend::services::ai_log::record(
        &db,
        "ReviewCode",
        Some(Uuid::new_v4()),
        None,
        Duration::from_millis(1234),
        &result,
        Some("claude-opus-4-7"),
    )
    .await;

    let (method, latency, status, model): (String, i32, String, Option<String>) = sqlx::query_as(
        "SELECT method, latency_ms, status, model_version FROM ai_call_log ORDER BY called_at DESC LIMIT 1",
    )
    .fetch_one(&db).await.unwrap();
    assert_eq!(method, "ReviewCode");
    assert_eq!(latency, 1234);
    assert_eq!(status, "ok");
    assert_eq!(model.as_deref(), Some("claude-opus-4-7"));

    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn record_helper_maps_grpc_status_to_error_string() {
    let (db, name) = setup_test_db().await;

    // Simule DeadlineExceeded → 'timeout'.
    let result: Result<(), tonic::Status> = Err(tonic::Status::deadline_exceeded("60s exceeded"));
    skilluv_backend::services::ai_log::record(
        &db,
        "AnalyzePerformance",
        None,
        None,
        Duration::from_secs(60),
        &result,
        None,
    )
    .await;

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ai_call_log")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(
        count, 1,
        "record() should have inserted 1 row (best-effort)"
    );

    let (status, err_msg): (String, Option<String>) = sqlx::query_as(
        "SELECT status, error_message FROM ai_call_log ORDER BY called_at DESC LIMIT 1",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(status, "timeout");
    assert!(err_msg.unwrap_or_default().contains("60s exceeded"));

    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn record_helper_maps_unavailable_grpc_status() {
    let (db, name) = setup_test_db().await;
    let unavail: Result<(), tonic::Status> = Err(tonic::Status::unavailable("Claude down"));
    skilluv_backend::services::ai_log::record(
        &db,
        "ReviewCode",
        None,
        None,
        Duration::from_millis(50),
        &unavail,
        None,
    )
    .await;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ai_call_log")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(count, 1, "un seul row attendu");
    let (s, m): (String, String) = sqlx::query_as("SELECT status, method FROM ai_call_log LIMIT 1")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(s, "unavailable");
    assert_eq!(m, "ReviewCode");
    db.close().await;
    cleanup_test_db(&name).await;
}
