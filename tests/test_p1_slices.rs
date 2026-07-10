//! Tests d'intégration Phase P1 : service + routes des `project_slices`.
//!
//! Couvre :
//! - Migrations 0062 (bounties.slice_id + projects.curated_labels) et 0063 (backfill)
//! - SlicesService : list_open, get, claim, unclaim, expire_stale_claims, list_claimed_by
//! - Contraintes métier : claim exclusif, unclaim seulement par owner, expiration 7j

use chrono::{Duration, Utc};
use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

use skilluv_backend::services::{SlicesListFilter, SlicesService};

// ═══════════════════════════════════════════════════════════════════
// Helpers de setup / teardown (identique à test_p0_schema)
// ═══════════════════════════════════════════════════════════════════

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p1_test_{}",
        Uuid::new_v4().to_string().replace('-', "")
    );

    let admin_pool = PgPoolOptions::new()
        .max_connections(2)
        .connect("postgres://skilluv:skilluv_secret@localhost:5433/skilluv")
        .await
        .expect("Failed to connect to admin DB");

    sqlx::query(&format!("CREATE DATABASE \"{db_name}\""))
        .execute(&admin_pool)
        .await
        .expect("Failed to create test DB");

    admin_pool.close().await;

    let db_url = format!("postgres://skilluv:skilluv_secret@localhost:5433/{db_name}");
    let db = PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await
        .expect("Failed to connect to test DB");

    sqlx::migrate!("./migrations")
        .run(&db)
        .await
        .expect("Failed to run migrations on test DB");

    (db, db_name)
}

async fn cleanup_test_db(db_name: &str) {
    let admin_pool = PgPoolOptions::new()
        .max_connections(2)
        .connect("postgres://skilluv:skilluv_secret@localhost:5433/skilluv")
        .await
        .expect("Failed to connect to admin DB");

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
         VALUES ($1, $2, $3, $4, $5, $6, $7, FALSE, 0)",
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
    .expect("Failed to insert test user");
}

/// Crée un user + un projet + une slice `open`, retourne (user_id, project_id, slice_id).
async fn setup_project_with_open_slice(db: &PgPool) -> (Uuid, Uuid, Uuid) {
    let user_id = Uuid::new_v4();
    insert_test_user(db, user_id).await;

    let project_id: Uuid = sqlx::query_scalar(
        "INSERT INTO projects (slug, name, owner_type, owner_id)
         VALUES ($1, 'Test Project', 'user', $2)
         RETURNING id",
    )
    .bind(format!("test-project-{}", Uuid::new_v4()))
    .bind(user_id)
    .fetch_one(db)
    .await
    .expect("Failed to insert project");

    let slice_id: Uuid = sqlx::query_scalar(
        "INSERT INTO project_slices
            (project_id, slice_type, title, description, primary_domain, difficulty, status)
         VALUES ($1, 'github_issue', 'Test slice', 'A test slice', 'code', 2, 'open')
         RETURNING id",
    )
    .bind(project_id)
    .fetch_one(db)
    .await
    .expect("Failed to insert slice");

    (user_id, project_id, slice_id)
}

// ═══════════════════════════════════════════════════════════════════
// Migration 0062 : curated_labels sur projects.
// (Le volet oss_bounties.slice_id de 0062 est obsolète depuis P9.2 —
//  la table oss_bounties est droppée en mig 0074.)
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn migration_0062_adds_curated_labels_on_projects() {
    let (db, db_name) = setup_test_db().await;

    let projects_curated: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM information_schema.columns
            WHERE table_name = 'projects' AND column_name = 'curated_labels'
        )",
    )
    .fetch_one(&db)
    .await
    .expect("query");
    assert!(projects_curated, "projects.curated_labels must exist");

    // The default should be the 3 labels
    let default_labels: Vec<String> = sqlx::query_scalar(
        "SELECT UNNEST(column_default::text[])
         FROM information_schema.columns
         WHERE table_name = 'projects' AND column_name = 'curated_labels'",
    )
    .fetch_all(&db)
    .await
    .unwrap_or_default();

    // Check that a fresh project has the default labels
    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id).await;
    let labels: Vec<String> = sqlx::query_scalar(
        "INSERT INTO projects (slug, name, owner_type, owner_id)
         VALUES ($1, 'Curated Labels Test', 'user', $2)
         RETURNING curated_labels",
    )
    .bind(format!("p-{}", Uuid::new_v4()))
    .bind(user_id)
    .fetch_one(&db)
    .await
    .expect("insert");

    assert!(labels.contains(&"good-first-issue".to_string()));
    assert!(labels.contains(&"help-wanted".to_string()));
    assert!(labels.contains(&"skilluv-ready".to_string()));

    let _ = default_labels; // silence warning if unused

    db.close().await;
    cleanup_test_db(&db_name).await;
}

// (Le test du backfill 0063 est retiré en P9.2 — oss_bounties est droppée
// en mig 0074, on ne peut plus insérer dans cette table pour re-jouer 0063.
// Les invariants qu'il vérifiait sont désormais couverts par test_phase5_bounties
// via l'API bounties qui écrit directement dans project_slices.)

// ═══════════════════════════════════════════════════════════════════
// SlicesService::list_open
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn list_open_returns_only_open_slices() {
    let (db, db_name) = setup_test_db().await;
    let (_user, project_id, open_slice) = setup_project_with_open_slice(&db).await;

    // Add a claimed slice (should not appear)
    let other_user = Uuid::new_v4();
    insert_test_user(&db, other_user).await;

    sqlx::query(
        "INSERT INTO project_slices
            (project_id, slice_type, title, description, primary_domain, difficulty,
             status, claimed_by_user_id, claimed_at, claim_expires_at)
         VALUES ($1, 'github_issue', 'Claimed', 'X', 'code', 3, 'claimed', $2, NOW(),
                 NOW() + INTERVAL '7 days')",
    )
    .bind(project_id)
    .bind(other_user)
    .execute(&db)
    .await
    .expect("insert claimed slice");

    let filter = SlicesListFilter {
        page: 1,
        per_page: 100,
        ..Default::default()
    };
    let (slices, total) = SlicesService::list_open(&db, &filter).await.expect("list");

    assert_eq!(total, 1);
    assert_eq!(slices.len(), 1);
    assert_eq!(slices[0].id, open_slice);

    db.close().await;
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn list_open_respects_domain_filter() {
    let (db, db_name) = setup_test_db().await;
    let (_user, project_id, _slice) = setup_project_with_open_slice(&db).await;

    // Add a design slice
    sqlx::query(
        "INSERT INTO project_slices
            (project_id, slice_type, title, description, primary_domain, difficulty, status)
         VALUES ($1, 'figma_frame', 'Design', 'X', 'design', 2, 'open')",
    )
    .bind(project_id)
    .execute(&db)
    .await
    .expect("insert design slice");

    let filter = SlicesListFilter {
        domain: Some("design".to_string()),
        page: 1,
        per_page: 100,
        ..Default::default()
    };
    let (slices, total) = SlicesService::list_open(&db, &filter).await.expect("list");

    assert_eq!(total, 1);
    assert_eq!(slices[0].primary_domain, "design");

    db.close().await;
    cleanup_test_db(&db_name).await;
}

// ═══════════════════════════════════════════════════════════════════
// SlicesService::claim / unclaim
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn claim_open_slice_sets_expiration_seven_days_out() {
    let (db, db_name) = setup_test_db().await;
    let (user_id, _project, slice_id) = setup_project_with_open_slice(&db).await;

    let before = Utc::now();
    let slice = SlicesService::claim(&db, slice_id, user_id)
        .await
        .expect("claim");
    let after = Utc::now();

    assert_eq!(slice.status, "claimed");
    assert_eq!(slice.claimed_by_user_id, Some(user_id));
    let expires = slice.claim_expires_at.expect("expires_at should be set");

    let expected_min = before + Duration::days(7) - Duration::seconds(2);
    let expected_max = after + Duration::days(7) + Duration::seconds(2);
    assert!(
        expires >= expected_min && expires <= expected_max,
        "claim_expires_at should be ~7 days out, got {expires}"
    );

    db.close().await;
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn claim_rejects_already_claimed_slice() {
    let (db, db_name) = setup_test_db().await;
    let (user_id, _project, slice_id) = setup_project_with_open_slice(&db).await;

    SlicesService::claim(&db, slice_id, user_id)
        .await
        .expect("first claim");

    let user2 = Uuid::new_v4();
    insert_test_user(&db, user2).await;

    let res = SlicesService::claim(&db, slice_id, user2).await;
    assert!(res.is_err(), "Second claim on same slice must fail");

    db.close().await;
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn unclaim_returns_slice_to_open() {
    let (db, db_name) = setup_test_db().await;
    let (user_id, _project, slice_id) = setup_project_with_open_slice(&db).await;

    SlicesService::claim(&db, slice_id, user_id)
        .await
        .expect("claim");

    let slice = SlicesService::unclaim(&db, slice_id, user_id)
        .await
        .expect("unclaim");

    assert_eq!(slice.status, "open");
    assert!(slice.claimed_by_user_id.is_none());
    assert!(slice.claim_expires_at.is_none());

    db.close().await;
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn unclaim_by_non_owner_fails() {
    let (db, db_name) = setup_test_db().await;
    let (owner, _project, slice_id) = setup_project_with_open_slice(&db).await;

    SlicesService::claim(&db, slice_id, owner)
        .await
        .expect("claim by owner");

    let stranger = Uuid::new_v4();
    insert_test_user(&db, stranger).await;

    let res = SlicesService::unclaim(&db, slice_id, stranger).await;
    assert!(res.is_err(), "Unclaim by non-owner must fail");

    db.close().await;
    cleanup_test_db(&db_name).await;
}

// ═══════════════════════════════════════════════════════════════════
// SlicesService::expire_stale_claims
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn expire_stale_claims_returns_expired_to_pool() {
    let (db, db_name) = setup_test_db().await;
    let (user_id, project_id, _slice) = setup_project_with_open_slice(&db).await;

    // Insert a slice claimed with an already-past expiration
    let expired_slice_id: Uuid = sqlx::query_scalar(
        "INSERT INTO project_slices
            (project_id, slice_type, title, description, primary_domain, difficulty,
             status, claimed_by_user_id, claimed_at, claim_expires_at)
         VALUES ($1, 'github_issue', 'Expired', 'X', 'code', 2, 'claimed', $2,
                 NOW() - INTERVAL '10 days', NOW() - INTERVAL '1 hour')
         RETURNING id",
    )
    .bind(project_id)
    .bind(user_id)
    .fetch_one(&db)
    .await
    .expect("insert expired slice");

    let expired_count = SlicesService::expire_stale_claims(&db)
        .await
        .expect("expire");

    assert_eq!(expired_count, 1);

    // The slice should now be open again
    let status: String = sqlx::query_scalar(
        "SELECT status FROM project_slices WHERE id = $1",
    )
    .bind(expired_slice_id)
    .fetch_one(&db)
    .await
    .expect("query");
    assert_eq!(status, "open");

    db.close().await;
    cleanup_test_db(&db_name).await;
}

// ═══════════════════════════════════════════════════════════════════
// SlicesService::list_claimed_by
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn list_claimed_by_returns_only_my_active_slices() {
    let (db, db_name) = setup_test_db().await;
    let (user_id, _project, slice_id) = setup_project_with_open_slice(&db).await;

    SlicesService::claim(&db, slice_id, user_id)
        .await
        .expect("claim");

    let slices = SlicesService::list_claimed_by(&db, user_id)
        .await
        .expect("list_claimed_by");

    assert_eq!(slices.len(), 1);
    assert_eq!(slices[0].id, slice_id);

    // A different user sees nothing
    let other_user = Uuid::new_v4();
    insert_test_user(&db, other_user).await;

    let other_slices = SlicesService::list_claimed_by(&db, other_user)
        .await
        .expect("list_claimed_by other");
    assert!(other_slices.is_empty());

    db.close().await;
    cleanup_test_db(&db_name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Contrainte de cohérence : claimed_by ↔ claimed_at
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn claim_coherence_constraint_rejects_partial_state() {
    let (db, db_name) = setup_test_db().await;
    let (user_id, project_id, _slice) = setup_project_with_open_slice(&db).await;

    // Try to insert a slice with claimed_by but no claimed_at → should be rejected
    let bad = sqlx::query(
        "INSERT INTO project_slices
            (project_id, slice_type, title, description, primary_domain, difficulty,
             status, claimed_by_user_id)
         VALUES ($1, 'github_issue', 'Bad', 'X', 'code', 2, 'claimed', $2)",
    )
    .bind(project_id)
    .bind(user_id)
    .execute(&db)
    .await;

    assert!(bad.is_err(), "Partial claim state should be rejected");

    db.close().await;
    cleanup_test_db(&db_name).await;
}
