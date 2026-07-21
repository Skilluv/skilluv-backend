//! Tests P24.2 : agency_clients + trigger PG + routes CRUD.

use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p24_2_test_{}",
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
async fn insert_ok_for_staffing_agency() {
    let (db, name) = setup_test_db().await;
    let ent = create_enterprise(&db, "agency-1", "staffing_agency").await;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO agency_clients (enterprise_id, client_name)
         VALUES ($1, 'ClientCo') RETURNING id",
    )
    .bind(ent)
    .fetch_one(&db)
    .await
    .expect("insert ok");
    assert!(!id.is_nil());
    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn insert_rejected_for_direct_hire() {
    let (db, name) = setup_test_db().await;
    let ent = create_enterprise(&db, "direct-1", "direct_hire").await;
    let bad = sqlx::query(
        "INSERT INTO agency_clients (enterprise_id, client_name)
         VALUES ($1, 'ClientCo')",
    )
    .bind(ent)
    .execute(&db)
    .await;
    assert!(
        bad.is_err(),
        "direct_hire enterprise cannot own agency_clients"
    );
    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn insert_rejected_for_remote_international() {
    let (db, name) = setup_test_db().await;
    let ent = create_enterprise(&db, "remote-1", "remote_international").await;
    let bad = sqlx::query(
        "INSERT INTO agency_clients (enterprise_id, client_name)
         VALUES ($1, 'ClientCo')",
    )
    .bind(ent)
    .execute(&db)
    .await;
    assert!(
        bad.is_err(),
        "remote_international enterprise cannot own agency_clients"
    );
    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn client_name_length_check_enforced() {
    let (db, name) = setup_test_db().await;
    let ent = create_enterprise(&db, "agency-len", "staffing_agency").await;
    let bad =
        sqlx::query("INSERT INTO agency_clients (enterprise_id, client_name) VALUES ($1, 'X')")
            .bind(ent)
            .execute(&db)
            .await;
    assert!(bad.is_err(), "single-char name rejected");
    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn unique_client_name_per_enterprise() {
    let (db, name) = setup_test_db().await;
    let ent = create_enterprise(&db, "agency-uniq", "staffing_agency").await;
    sqlx::query("INSERT INTO agency_clients (enterprise_id, client_name) VALUES ($1, 'ClientCo')")
        .bind(ent)
        .execute(&db)
        .await
        .unwrap();
    let dup = sqlx::query(
        "INSERT INTO agency_clients (enterprise_id, client_name) VALUES ($1, 'ClientCo')",
    )
    .bind(ent)
    .execute(&db)
    .await;
    assert!(dup.is_err(), "duplicate name per enterprise rejected");
    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn cascade_delete_when_enterprise_deleted() {
    let (db, name) = setup_test_db().await;
    let ent = create_enterprise(&db, "agency-cascade", "staffing_agency").await;
    sqlx::query(
        "INSERT INTO agency_clients (enterprise_id, client_name)
                 VALUES ($1, 'ClientA'), ($1, 'ClientB')",
    )
    .bind(ent)
    .execute(&db)
    .await
    .unwrap();

    sqlx::query("DELETE FROM enterprises WHERE id = $1")
        .bind(ent)
        .execute(&db)
        .await
        .unwrap();

    let remaining: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM agency_clients WHERE enterprise_id = $1")
            .bind(ent)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(remaining, 0, "cascade delete removed all clients");

    db.close().await;
    cleanup_test_db(&name).await;
}
