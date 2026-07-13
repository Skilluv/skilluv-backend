//! Tests d'intégration P12.2 : user_project_interests.
//!
//! Vérifie :
//! - Migration 0080 crée la table + PK + CHECK constraints.
//! - `mark_interested` upsert idempotent.
//! - `mark_interested_batch` traite N projets d'un coup.
//! - `unmark_interested` met score à 0 sans supprimer la ligne.
//! - `list_interests` filtre score > 0 ET projet non-archivé, trié par score DESC.

use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

use skilluv_backend::services::projects;

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p12_2_test_{}",
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

async fn insert_project(db: &PgPool, owner: Uuid, name: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO projects (slug, name, owner_type, owner_id, skill_domains)
         VALUES ($1, $2, 'user', $3, ARRAY['code'])
         RETURNING id",
    )
    .bind(format!("p-{}", Uuid::new_v4()))
    .bind(name)
    .bind(owner)
    .fetch_one(db)
    .await
    .expect("insert project")
}

// ═══════════════════════════════════════════════════════════════════
// mark_interested est idempotent (upsert)
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn mark_interested_upserts_score() {
    let (db, name) = setup_test_db().await;
    let user = insert_user(&db).await;
    let owner = insert_user(&db).await;
    let project = insert_project(&db, owner, "My Project").await;

    let a = projects::mark_interested(&db, user, project, 50).await.expect("a");
    assert_eq!(a.interest_score, 50);
    let b = projects::mark_interested(&db, user, project, 80).await.expect("b");
    assert_eq!(b.interest_score, 80);

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_project_interests WHERE user_id = $1 AND project_id = $2",
    )
    .bind(user)
    .bind(project)
    .fetch_one(&db)
    .await
    .expect("count");
    assert_eq!(count, 1, "upsert → 1 seule ligne");

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Batch mark N projets
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn mark_batch_marks_all_projects() {
    let (db, name) = setup_test_db().await;
    let user = insert_user(&db).await;
    let owner = insert_user(&db).await;
    let p1 = insert_project(&db, owner, "Project One").await;
    let p2 = insert_project(&db, owner, "Project Two").await;
    let p3 = insert_project(&db, owner, "Project Three").await;

    let count = projects::mark_interested_batch(&db, user, &[p1, p2, p3])
        .await
        .expect("batch");
    assert_eq!(count, 3);

    let stored: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_project_interests
         WHERE user_id = $1 AND interest_score = 50",
    )
    .bind(user)
    .fetch_one(&db)
    .await
    .expect("count");
    assert_eq!(stored, 3);

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// unmark met à 0 sans supprimer la ligne
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn unmark_sets_score_to_zero() {
    let (db, name) = setup_test_db().await;
    let user = insert_user(&db).await;
    let owner = insert_user(&db).await;
    let project = insert_project(&db, owner, "Foo Project").await;

    projects::mark_interested(&db, user, project, 70).await.expect("m");
    let affected = projects::unmark_interested(&db, user, project).await.expect("u");
    assert_eq!(affected, 1);

    let score: i16 = sqlx::query_scalar(
        "SELECT interest_score FROM user_project_interests WHERE user_id = $1 AND project_id = $2",
    )
    .bind(user)
    .bind(project)
    .fetch_one(&db)
    .await
    .expect("s");
    assert_eq!(score, 0);

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// list_interests filtre score > 0 et projet non archivé, tri DESC
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn list_interests_filters_and_sorts() {
    let (db, name) = setup_test_db().await;
    let user = insert_user(&db).await;
    let owner = insert_user(&db).await;
    let p_high = insert_project(&db, owner, "High Interest").await;
    let p_low = insert_project(&db, owner, "Low Interest").await;
    let p_unmarked = insert_project(&db, owner, "Unmarked").await;
    let p_archived = insert_project(&db, owner, "Archived Interesting").await;

    projects::mark_interested(&db, user, p_high, 90).await.expect("h");
    projects::mark_interested(&db, user, p_low, 30).await.expect("l");
    projects::mark_interested(&db, user, p_unmarked, 40).await.expect("u");
    projects::unmark_interested(&db, user, p_unmarked).await.expect("un");
    projects::mark_interested(&db, user, p_archived, 60).await.expect("a");
    sqlx::query("UPDATE projects SET archived_at = NOW() WHERE id = $1")
        .bind(p_archived)
        .execute(&db)
        .await
        .expect("arch");

    let list = projects::list_interests(&db, user).await.expect("list");
    let ids: Vec<Uuid> = list.iter().map(|r| r.project.id).collect();
    assert_eq!(ids, vec![p_high, p_low], "tri par score DESC + filtres OK");

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// CHECK constraint : score entre 0 et 100
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn interest_score_must_be_between_0_and_100() {
    let (db, name) = setup_test_db().await;
    let user = insert_user(&db).await;
    let owner = insert_user(&db).await;
    let project = insert_project(&db, owner, "Constraint Project").await;

    // Insertion directe hors bornes → CHECK refuse
    let res = sqlx::query(
        "INSERT INTO user_project_interests (user_id, project_id, interest_score)
         VALUES ($1, $2, 150)",
    )
    .bind(user)
    .bind(project)
    .execute(&db)
    .await;
    assert!(res.is_err(), "CHECK doit refuser score=150");

    // Le service clamp automatiquement
    let ok = projects::mark_interested(&db, user, project, 200)
        .await
        .expect("service clamp");
    assert_eq!(ok.interest_score, 100, "clamp à 100 côté service");

    db.close().await;
    cleanup_test_db(&name).await;
}
