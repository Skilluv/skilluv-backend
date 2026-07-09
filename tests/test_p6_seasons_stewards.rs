//! Tests d'intégration Phase P6 : seasons + project_stewards.

use chrono::{Duration, Utc};
use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

use skilluv_backend::services::{
    CreateSeasonParams, SeasonsService, StewardsService,
};

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p6_test_{}",
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

async fn insert_test_user(db: &PgPool, user_id: Uuid) {
    let short = &user_id.to_string()[..8];
    sqlx::query(
        "INSERT INTO users
            (id, email, username, first_name, last_name, display_name, password_hash,
             profile_active, total_fragments)
         VALUES ($1, $2, $3, $4, $5, $6, $7, TRUE, 0)",
    )
    .bind(user_id)
    .bind(format!("test-{user_id}@example.com"))
    .bind(format!("t{short}"))
    .bind("Test")
    .bind("User")
    .bind("Test User")
    .bind("dummy_hash")
    .execute(db)
    .await
    .expect("insert user");
}

async fn insert_test_project(db: &PgPool, owner: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO projects (slug, name, owner_type, owner_id)
         VALUES ($1, 'P6 Project', 'user', $2)
         RETURNING id",
    )
    .bind(format!("p6-{}", Uuid::new_v4()))
    .bind(owner)
    .fetch_one(db)
    .await
    .expect("project")
}

// ═══════════════════════════════════════════════════════════════════
// Seasons
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn migration_0069_creates_seasons_tables() {
    let (db, db_name) = setup_test_db().await;
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT table_name FROM information_schema.tables
         WHERE table_schema='public'
           AND table_name IN ('seasons','project_seasons','project_stewards')",
    )
    .fetch_all(&db)
    .await
    .expect("check");
    assert_eq!(tables.len(), 3);
    db.close().await;
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn create_season_rejects_end_before_start() {
    let (db, db_name) = setup_test_db().await;
    let now = Utc::now();
    let bad = SeasonsService::create(
        &db,
        CreateSeasonParams {
            slug: "s0-bad".to_string(),
            name: "Bad".to_string(),
            theme: "Theme".to_string(),
            starts_at: now + Duration::days(1),
            ends_at: now,
        },
    )
    .await;
    assert!(bad.is_err());
    db.close().await;
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn activate_season_deactivates_previous_active() {
    let (db, db_name) = setup_test_db().await;
    let now = Utc::now();

    SeasonsService::create(
        &db,
        CreateSeasonParams {
            slug: "s1-2027".to_string(),
            name: "S1 2027".to_string(),
            theme: "African OS".to_string(),
            starts_at: now - Duration::days(30),
            ends_at: now + Duration::days(60),
        },
    )
    .await
    .expect("s1");
    SeasonsService::create(
        &db,
        CreateSeasonParams {
            slug: "s2-2027".to_string(),
            name: "S2 2027".to_string(),
            theme: "Community first".to_string(),
            starts_at: now + Duration::days(60),
            ends_at: now + Duration::days(150),
        },
    )
    .await
    .expect("s2");

    SeasonsService::activate(&db, "s1-2027").await.expect("act1");
    let current1 = SeasonsService::get_current(&db).await.expect("current1");
    assert_eq!(current1.map(|s| s.slug), Some("s1-2027".to_string()));

    // Activate s2 → s1 should become completed
    SeasonsService::activate(&db, "s2-2027").await.expect("act2");
    let s1_status: String = sqlx::query_scalar(
        "SELECT status FROM seasons WHERE slug = 's1-2027'",
    )
    .fetch_one(&db)
    .await
    .expect("fetch");
    assert_eq!(s1_status, "completed");

    let s2_status: String = sqlx::query_scalar(
        "SELECT status FROM seasons WHERE slug = 's2-2027'",
    )
    .fetch_one(&db)
    .await
    .expect("fetch");
    assert_eq!(s2_status, "active");

    db.close().await;
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn assign_project_to_season_is_idempotent() {
    let (db, db_name) = setup_test_db().await;
    let owner = Uuid::new_v4();
    insert_test_user(&db, owner).await;
    let project_id = insert_test_project(&db, owner).await;

    let season = SeasonsService::create(
        &db,
        CreateSeasonParams {
            slug: "s-assign".to_string(),
            name: "Assign".to_string(),
            theme: "Theme".to_string(),
            starts_at: Utc::now(),
            ends_at: Utc::now() + Duration::days(90),
        },
    )
    .await
    .expect("create");

    SeasonsService::assign_project(&db, season.id, project_id, "primary")
        .await
        .expect("assign");
    // Second assign with different focus_type should update, not error
    SeasonsService::assign_project(&db, season.id, project_id, "featured")
        .await
        .expect("re-assign");

    let projects = SeasonsService::list_projects_in_season(&db, season.id)
        .await
        .expect("list");
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].1, "featured");

    db.close().await;
    cleanup_test_db(&db_name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Stewards
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn add_steward_creates_active_role() {
    let (db, db_name) = setup_test_db().await;
    let admin = Uuid::new_v4();
    insert_test_user(&db, admin).await;
    let steward = Uuid::new_v4();
    insert_test_user(&db, steward).await;
    let project_id = insert_test_project(&db, admin).await;

    let s = StewardsService::add(&db, project_id, steward, "lead_steward", admin)
        .await
        .expect("add");
    assert_eq!(s.role, "lead_steward");
    assert!(s.ended_at.is_none());

    assert!(
        StewardsService::is_steward(&db, project_id, steward)
            .await
            .expect("check")
    );

    db.close().await;
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn add_steward_rejects_invalid_role() {
    let (db, db_name) = setup_test_db().await;
    let admin = Uuid::new_v4();
    insert_test_user(&db, admin).await;
    let user = Uuid::new_v4();
    insert_test_user(&db, user).await;
    let project_id = insert_test_project(&db, admin).await;

    let bad = StewardsService::add(&db, project_id, user, "not_a_role", admin).await;
    assert!(bad.is_err());

    db.close().await;
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn add_steward_is_idempotent_and_reactivates() {
    let (db, db_name) = setup_test_db().await;
    let admin = Uuid::new_v4();
    insert_test_user(&db, admin).await;
    let steward = Uuid::new_v4();
    insert_test_user(&db, steward).await;
    let project_id = insert_test_project(&db, admin).await;

    StewardsService::add(&db, project_id, steward, "co_steward", admin)
        .await
        .expect("first add");

    // Remove
    StewardsService::remove(&db, project_id, steward, "co_steward")
        .await
        .expect("remove");
    assert!(
        !StewardsService::is_steward(&db, project_id, steward)
            .await
            .expect("check")
    );

    // Re-add → reactivate
    StewardsService::add(&db, project_id, steward, "co_steward", admin)
        .await
        .expect("re-add");
    assert!(
        StewardsService::is_steward(&db, project_id, steward)
            .await
            .expect("check")
    );

    db.close().await;
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn list_project_stewards_returns_only_active() {
    let (db, db_name) = setup_test_db().await;
    let owner = Uuid::new_v4();
    insert_test_user(&db, owner).await;
    let user_a = Uuid::new_v4();
    insert_test_user(&db, user_a).await;
    let user_b = Uuid::new_v4();
    insert_test_user(&db, user_b).await;
    let project_id = insert_test_project(&db, owner).await;

    StewardsService::add(&db, project_id, user_a, "lead_steward", owner)
        .await
        .expect("add a");
    StewardsService::add(&db, project_id, user_b, "co_steward", owner)
        .await
        .expect("add b");

    let list1 = StewardsService::list_project_stewards(&db, project_id)
        .await
        .expect("list");
    assert_eq!(list1.len(), 2);

    // Remove user_a
    StewardsService::remove(&db, project_id, user_a, "lead_steward")
        .await
        .expect("remove");

    let list2 = StewardsService::list_project_stewards(&db, project_id)
        .await
        .expect("list");
    assert_eq!(list2.len(), 1);
    assert_eq!(list2[0].user_id, user_b);

    db.close().await;
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn is_steward_returns_false_for_non_steward() {
    let (db, db_name) = setup_test_db().await;
    let owner = Uuid::new_v4();
    insert_test_user(&db, owner).await;
    let stranger = Uuid::new_v4();
    insert_test_user(&db, stranger).await;
    let project_id = insert_test_project(&db, owner).await;

    let res = StewardsService::is_steward(&db, project_id, stranger)
        .await
        .expect("check");
    assert!(!res);

    db.close().await;
    cleanup_test_db(&db_name).await;
}
