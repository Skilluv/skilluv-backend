//! Tests d'intégration P11.4 : steward inbox pour valider/rejeter les
//! slices draft ingérées automatiquement.
//!
//! Couvre :
//! - `list_drafts_for_project` renvoie uniquement status='draft'.
//! - `publish_draft` fait draft → open, refuse si pas draft.
//! - `reject_draft` fait draft → closed avec closed_at set.
//! - `is_steward` autorise correctement (steward oui, non-steward non).

use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

use skilluv_backend::services::{SlicesService, StewardsService};

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p11_4_test_{}",
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
    let user_id = Uuid::new_v4();
    let short = &user_id.to_string()[..8];
    sqlx::query(
        "INSERT INTO users
            (id, email, username, first_name, last_name, display_name, password_hash,
             profile_active, total_fragments)
         VALUES ($1, $2, $3, 'T', 'U', 'Test', 'dummy', TRUE, 0)",
    )
    .bind(user_id)
    .bind(format!("test-{user_id}@example.com"))
    .bind(format!("t{short}"))
    .execute(db)
    .await
    .expect("insert user");
    user_id
}

async fn insert_project(db: &PgPool, owner: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO projects (slug, name, owner_type, owner_id)
         VALUES ($1, 'Test Project', 'user', $2)
         RETURNING id",
    )
    .bind(format!("p-{}", Uuid::new_v4()))
    .bind(owner)
    .fetch_one(db)
    .await
    .expect("insert project")
}

async fn insert_draft(db: &PgPool, project_id: Uuid, title: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO project_slices
            (project_id, slice_type, external_ref, title, description,
             primary_domain, difficulty, status)
         VALUES ($1, 'github_issue', $2, $3, 'D', 'code', 3, 'draft')
         RETURNING id",
    )
    .bind(project_id)
    .bind(Uuid::new_v4().to_string())
    .bind(title)
    .fetch_one(db)
    .await
    .expect("insert draft")
}

// ═══════════════════════════════════════════════════════════════════
// list_drafts_for_project renvoie uniquement les drafts
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn list_drafts_returns_only_drafts_of_project() {
    let (db, name) = setup_test_db().await;
    let owner = insert_user(&db).await;
    let project = insert_project(&db, owner).await;

    let _draft_a = insert_draft(&db, project, "Draft A").await;
    let _draft_b = insert_draft(&db, project, "Draft B").await;

    // Une slice open pour vérifier qu'elle est exclue
    sqlx::query(
        "INSERT INTO project_slices
            (project_id, slice_type, title, description, primary_domain, difficulty, status)
         VALUES ($1, 'other', 'Open one', 'D', 'code', 2, 'open')",
    )
    .bind(project)
    .execute(&db)
    .await
    .expect("open slice");

    // Une slice draft mais sur un autre project → exclue
    let other_project = insert_project(&db, owner).await;
    insert_draft(&db, other_project, "Other project draft").await;

    let drafts = SlicesService::list_drafts_for_project(&db, project)
        .await
        .expect("drafts");
    assert_eq!(drafts.len(), 2, "seulement les drafts du project ciblé");
    assert!(drafts.iter().all(|s| s.status == "draft"));

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// publish_draft : draft → open
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn publish_draft_transitions_to_open() {
    let (db, name) = setup_test_db().await;
    let owner = insert_user(&db).await;
    let project = insert_project(&db, owner).await;
    let draft = insert_draft(&db, project, "To publish").await;

    let published = SlicesService::publish_draft(&db, draft)
        .await
        .expect("publish");
    assert_eq!(published.status, "open");

    // Deuxième publish → refuse (plus draft)
    let re = SlicesService::publish_draft(&db, draft).await;
    assert!(re.is_err(), "on ne peut publier qu'une seule fois");

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// reject_draft : draft → closed avec closed_at
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn reject_draft_transitions_to_closed_with_closed_at() {
    let (db, name) = setup_test_db().await;
    let owner = insert_user(&db).await;
    let project = insert_project(&db, owner).await;
    let draft = insert_draft(&db, project, "To reject").await;

    let rejected = SlicesService::reject_draft(&db, draft)
        .await
        .expect("reject");
    assert_eq!(rejected.status, "closed");
    assert!(rejected.closed_at.is_some(), "closed_at doit être set");

    // Deuxième rejet → refuse (plus draft)
    let re = SlicesService::reject_draft(&db, draft).await;
    assert!(re.is_err());

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// is_steward autorise/refuse correctement
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn is_steward_permission_check() {
    let (db, name) = setup_test_db().await;
    let owner = insert_user(&db).await;
    let steward_user = insert_user(&db).await;
    let random_user = insert_user(&db).await;
    let project = insert_project(&db, owner).await;

    // Nomination du steward
    sqlx::query(
        "INSERT INTO project_stewards (project_id, user_id, role, appointed_by_user_id)
         VALUES ($1, $2, 'co_steward', $3)",
    )
    .bind(project)
    .bind(steward_user)
    .bind(owner)
    .execute(&db)
    .await
    .expect("nominate steward");

    assert!(
        StewardsService::is_steward(&db, project, steward_user)
            .await
            .expect("s")
    );
    assert!(
        !StewardsService::is_steward(&db, project, random_user)
            .await
            .expect("r")
    );

    db.close().await;
    cleanup_test_db(&name).await;
}
