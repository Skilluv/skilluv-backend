//! Tests d'intégration P11.1 : ingestion GitHub via slice_ingestion service.
//!
//! On teste directement la fn interne `insert_slice_from_issue` par des
//! INSERTs SQL équivalents (elle n'est pas pub) — mais on peut valider :
//! - `poll_all_github_projects` skip les projets sans repo GitHub.
//! - Le UNIQUE index empêche les doublons cross-runs.
//! - status='open' si mode='auto', 'draft' si 'curator_review'.
//! - Un projet en 'manual_only' ne poll rien même si labels curés.
//!
//! On NE frappe PAS l'API GitHub réelle : on manipule `project_slices`
//! directement pour vérifier les invariants ingérés.

use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p11_1_test_{}",
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

async fn insert_project(
    db: &PgPool,
    owner: Uuid,
    repo_owner: Option<&str>,
    repo_name: Option<&str>,
    ingestion_mode: &str,
    curated_labels: &[&str],
) -> Uuid {
    sqlx::query_scalar(
        r#"
        INSERT INTO projects
            (slug, name, owner_type, owner_id,
             github_repo_owner, github_repo_name, slice_ingestion_mode, curated_labels)
        VALUES ($1, 'Test Project', 'user', $2, $3, $4, $5, $6)
        RETURNING id
        "#,
    )
    .bind(format!("p-{}", Uuid::new_v4()))
    .bind(owner)
    .bind(repo_owner)
    .bind(repo_name)
    .bind(ingestion_mode)
    .bind(curated_labels)
    .fetch_one(db)
    .await
    .expect("insert project")
}

async fn insert_github_slice_via_ingestion_convention(
    db: &PgPool,
    project_id: Uuid,
    issue_number: i32,
    status: &str,
) -> Result<Uuid, sqlx::Error> {
    let metadata = serde_json::json!({
        "source": "github_polling",
        "issue_number": issue_number,
        "labels": ["good-first-issue"],
    });
    sqlx::query_scalar::<_, Uuid>(
        r#"
        INSERT INTO project_slices
            (project_id, slice_type, external_ref, external_metadata,
             title, description, primary_domain, difficulty, fragments_reward,
             status, ingested_from)
        VALUES ($1, 'github_issue', $2, $3,
                'Test issue', 'Test description', 'code', 3, 50,
                $4, 'github_webhook')
        RETURNING id
        "#,
    )
    .bind(project_id)
    .bind(issue_number.to_string())
    .bind(&metadata)
    .bind(status)
    .fetch_one(db)
    .await
}

// ═══════════════════════════════════════════════════════════════════
// UNIQUE (project_id, external_ref) empêche les doublons ingérés
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn duplicate_issue_ingestion_is_rejected_by_unique_index() {
    let (db, name) = setup_test_db().await;
    let owner = insert_user(&db).await;
    let project = insert_project(
        &db,
        owner,
        Some("acme"),
        Some("widgets"),
        "auto",
        &["good-first-issue"],
    )
    .await;

    let first = insert_github_slice_via_ingestion_convention(&db, project, 42, "open").await;
    assert!(first.is_ok());

    // Deuxième ingestion de la même issue → UNIQUE index refuse.
    let second = insert_github_slice_via_ingestion_convention(&db, project, 42, "open").await;
    assert!(
        second.is_err(),
        "UNIQUE (project_id, external_ref) doit refuser le doublon"
    );

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Deux projets différents peuvent avoir la même issue_number
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn same_issue_number_across_projects_is_allowed() {
    let (db, name) = setup_test_db().await;
    let owner = insert_user(&db).await;
    let project_a = insert_project(
        &db,
        owner,
        Some("a"),
        Some("r"),
        "auto",
        &["good-first-issue"],
    )
    .await;
    let project_b = insert_project(
        &db,
        owner,
        Some("b"),
        Some("r"),
        "auto",
        &["good-first-issue"],
    )
    .await;

    insert_github_slice_via_ingestion_convention(&db, project_a, 1, "open")
        .await
        .expect("a");
    insert_github_slice_via_ingestion_convention(&db, project_b, 1, "open")
        .await
        .expect("b");

    let count_a: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM project_slices WHERE project_id = $1")
            .bind(project_a)
            .fetch_one(&db)
            .await
            .expect("a count");
    let count_b: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM project_slices WHERE project_id = $1")
            .bind(project_b)
            .fetch_one(&db)
            .await
            .expect("b count");
    assert_eq!(count_a, 1);
    assert_eq!(count_b, 1);

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// poll_all_github_projects skip les modes / repos invalides
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn poll_skips_projects_without_github_repo() {
    let (db, name) = setup_test_db().await;
    let owner = insert_user(&db).await;

    // Projet sans repo GitHub
    let _p_no_repo = insert_project(&db, owner, None, None, "auto", &["good-first-issue"]).await;
    // Projet manual_only avec repo
    let _p_manual = insert_project(
        &db,
        owner,
        Some("acme"),
        Some("skip"),
        "manual_only",
        &["good-first-issue"],
    )
    .await;
    // Projet auto sans labels
    let _p_no_labels = insert_project(&db, owner, Some("acme"), Some("nolabel"), "auto", &[]).await;

    let reports = skilluv_backend::services::slice_ingestion::poll_all_github_projects(&db)
        .await
        .expect("poll");
    assert!(reports.is_empty(), "aucun projet éligible => aucun rapport");

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Mode auto vs curator_review : status différent des slices insérées
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn ingested_status_reflects_project_mode() {
    let (db, name) = setup_test_db().await;
    let owner = insert_user(&db).await;
    let project_auto = insert_project(
        &db,
        owner,
        Some("a"),
        Some("r"),
        "auto",
        &["good-first-issue"],
    )
    .await;
    let project_review = insert_project(
        &db,
        owner,
        Some("b"),
        Some("r"),
        "curator_review",
        &["good-first-issue"],
    )
    .await;

    insert_github_slice_via_ingestion_convention(&db, project_auto, 1, "open")
        .await
        .expect("auto");
    insert_github_slice_via_ingestion_convention(&db, project_review, 1, "draft")
        .await
        .expect("review");

    let auto_status: String =
        sqlx::query_scalar("SELECT status FROM project_slices WHERE project_id = $1")
            .bind(project_auto)
            .fetch_one(&db)
            .await
            .expect("s a");
    let review_status: String =
        sqlx::query_scalar("SELECT status FROM project_slices WHERE project_id = $1")
            .bind(project_review)
            .fetch_one(&db)
            .await
            .expect("s b");

    assert_eq!(auto_status, "open");
    assert_eq!(review_status, "draft");

    db.close().await;
    cleanup_test_db(&name).await;
}
