//! Tests d'intégration pour la Phase P0 du modèle challenges cible.
//!
//! Vérifie que les migrations 0055-0060 :
//! - Montent proprement sur une DB fresh
//! - Créent les tables attendues avec les colonnes/contraintes correctes
//! - Le seed 0057 insère bien 337 skill_nodes (47 catégories + 290 skills)
//! - Les contraintes CHECK / UNIQUE / FK fonctionnent
//!
//! Voir docs/challenges-target-model-and-roadmap.md pour le rationale.

use sqlx::postgres::{PgPool, PgPoolOptions};
use sqlx::Row;
use uuid::Uuid;

/// Setup a fresh test database with all migrations applied.
async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p0_test_{}",
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

/// Insert a minimal valid user row for tests that need a foreign key target.
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

async fn cleanup_test_db(db_name: &str) {
    let admin_pool = PgPoolOptions::new()
        .max_connections(2)
        .connect("postgres://skilluv:skilluv_secret@localhost:5433/skilluv")
        .await
        .expect("Failed to connect to admin DB");

    // Kick any lingering connections
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

#[tokio::test]
async fn migrations_up_to_p0_apply_cleanly() {
    let (db, db_name) = setup_test_db().await;

    // Just landing here means all migrations up through 0060 ran successfully.
    // Cross-check that a few key tables exist.
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT table_name FROM information_schema.tables
         WHERE table_schema = 'public' AND table_name IN
         ('skill_nodes', 'project_slices', 'slice_skills', 'deliverables', 'user_skills')",
    )
    .fetch_all(&db)
    .await
    .expect("Failed to query information_schema");

    assert_eq!(
        tables.len(),
        5,
        "Expected 5 P0 tables, got {}: {:?}",
        tables.len(),
        tables
    );

    db.close().await;
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn skill_nodes_seed_has_expected_counts() {
    let (db, db_name) = setup_test_db().await;

    // 47 catégories (parent_id NULL) + 290 skills atomiques = 337 total
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM skill_nodes")
        .fetch_one(&db)
        .await
        .expect("count skill_nodes failed");

    let categories: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM skill_nodes WHERE parent_id IS NULL")
            .fetch_one(&db)
            .await
            .expect("count categories failed");

    let atomic: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM skill_nodes WHERE parent_id IS NOT NULL")
            .fetch_one(&db)
            .await
            .expect("count atomic skills failed");

    assert_eq!(
        total, 337,
        "Expected 337 total skill_nodes, got {total}"
    );
    assert_eq!(
        categories, 47,
        "Expected 47 categories, got {categories}"
    );
    assert_eq!(atomic, 290, "Expected 290 atomic skills, got {atomic}");

    db.close().await;
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn skill_nodes_cover_all_seven_domains() {
    let (db, db_name) = setup_test_db().await;

    let domains: Vec<(String, i64)> = sqlx::query_as(
        "SELECT domain, COUNT(*) FROM skill_nodes GROUP BY domain ORDER BY domain",
    )
    .fetch_all(&db)
    .await
    .expect("Group by domain failed");

    let domain_names: Vec<String> = domains.iter().map(|(d, _)| d.clone()).collect();

    assert!(domain_names.contains(&"code".to_string()));
    assert!(domain_names.contains(&"design".to_string()));
    assert!(domain_names.contains(&"game".to_string()));
    assert!(domain_names.contains(&"security".to_string()));
    assert!(domain_names.contains(&"soft_skills".to_string()));
    assert!(domain_names.contains(&"ai".to_string()));
    assert!(domain_names.contains(&"ops".to_string()));

    // `code` is the biggest domain (wedge)
    let code_count = domains
        .iter()
        .find(|(d, _)| d == "code")
        .map(|(_, c)| *c)
        .unwrap_or(0);
    assert!(
        code_count > 60,
        "code domain should have > 60 skills, got {code_count}"
    );

    db.close().await;
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn projects_gained_new_columns() {
    let (db, db_name) = setup_test_db().await;

    let columns: Vec<String> = sqlx::query_scalar(
        "SELECT column_name FROM information_schema.columns
         WHERE table_name = 'projects' AND column_name IN
         ('skill_domains', 'lifecycle_status', 'figma_url',
          'github_repo_owner', 'github_repo_name', 'bug_bounty_open',
          'slice_ingestion_mode', 'health_score')",
    )
    .fetch_all(&db)
    .await
    .expect("Failed to query projects columns");

    assert_eq!(
        columns.len(),
        8,
        "Expected 8 new columns on projects, got {}: {:?}",
        columns.len(),
        columns
    );

    db.close().await;
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn slice_ingestion_mode_check_constraint_rejects_bad_values() {
    let (db, db_name) = setup_test_db().await;

    // First we need a valid project — quick insert with minimal fields
    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id).await;

    // Valid insert
    let ok = sqlx::query(
        "INSERT INTO projects (slug, name, owner_type, owner_id, slice_ingestion_mode)
         VALUES ($1, $2, 'user', $3, 'auto')",
    )
    .bind(format!("test-project-{}", Uuid::new_v4()))
    .bind("Test Project")
    .bind(user_id)
    .execute(&db)
    .await;
    assert!(ok.is_ok(), "Valid slice_ingestion_mode='auto' should insert");

    // Invalid slice_ingestion_mode should be rejected
    let bad = sqlx::query(
        "INSERT INTO projects (slug, name, owner_type, owner_id, slice_ingestion_mode)
         VALUES ($1, $2, 'user', $3, 'not_a_valid_mode')",
    )
    .bind(format!("test-project-bad-{}", Uuid::new_v4()))
    .bind("Bad Project")
    .bind(user_id)
    .execute(&db)
    .await;
    assert!(
        bad.is_err(),
        "Invalid slice_ingestion_mode should be rejected by CHECK constraint"
    );

    db.close().await;
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn deliverables_require_slice_or_challenge() {
    let (db, db_name) = setup_test_db().await;

    // A user is needed for user_id FK
    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id).await;

    // Attempting an orphan deliverable (no slice_id, no challenge_id) must fail
    let orphan = sqlx::query(
        "INSERT INTO deliverables
            (user_id, artifact_type, artifact_url, verifiable_by, verification_status)
         VALUES ($1, 'other', 'http://example.com/', 'human_review', 'pending')",
    )
    .bind(user_id)
    .execute(&db)
    .await;

    assert!(
        orphan.is_err(),
        "Deliverable without slice_id and challenge_id should be rejected"
    );

    db.close().await;
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn deliverables_verified_status_requires_signal_fields() {
    let (db, db_name) = setup_test_db().await;

    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id).await;

    // Insert a training challenge (safe: onboarding, no project needed)
    let challenge_id: Uuid = sqlx::query_scalar(
        "INSERT INTO challenge_templates
            (title, description, instructions, skill_domain, difficulty, is_onboarding, is_training)
         VALUES ($1, $2, $3, 'code', 1, TRUE, TRUE)
         RETURNING id",
    )
    .bind("Test onboarding")
    .bind("Test description")
    .bind("Test instructions")
    .fetch_one(&db)
    .await
    .expect("Failed to insert challenge");

    // A verified deliverable with artifact_url and verifiable_by should succeed
    let ok = sqlx::query(
        "INSERT INTO deliverables
            (challenge_id, user_id, artifact_type, artifact_url, verifiable_by, verification_status)
         VALUES ($1, $2, 'other', 'http://example.com/artifact', 'human_review', 'verified')",
    )
    .bind(challenge_id)
    .bind(user_id)
    .execute(&db)
    .await;
    assert!(ok.is_ok(), "Valid verified deliverable should insert");

    db.close().await;
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn slice_skills_unique_primary_per_slice() {
    let (db, db_name) = setup_test_db().await;

    // Fetch two skill_nodes to associate
    let skill_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM skill_nodes WHERE parent_id IS NOT NULL LIMIT 2",
    )
    .fetch_all(&db)
    .await
    .expect("Failed to fetch skill_nodes");
    assert_eq!(skill_ids.len(), 2, "Need at least 2 seeded atomic skills");

    // Set up project + slice
    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id).await;

    let project_id: Uuid = sqlx::query_scalar(
        "INSERT INTO projects (slug, name, owner_type, owner_id)
         VALUES ($1, 'Test Project', 'user', $2)
         RETURNING id",
    )
    .bind(format!("test-project-{}", Uuid::new_v4()))
    .bind(user_id)
    .fetch_one(&db)
    .await
    .expect("Failed to insert project");

    let slice_id: Uuid = sqlx::query_scalar(
        "INSERT INTO project_slices
            (project_id, slice_type, title, description, primary_domain, difficulty, status)
         VALUES ($1, 'github_issue', 'Test', 'Test description', 'code', 3, 'open')
         RETURNING id",
    )
    .bind(project_id)
    .fetch_one(&db)
    .await
    .expect("Failed to insert slice");

    // First primary skill: OK
    let ok = sqlx::query(
        "INSERT INTO slice_skills (slice_id, skill_id, weight, is_primary) VALUES ($1, $2, 3, TRUE)",
    )
    .bind(slice_id)
    .bind(skill_ids[0])
    .execute(&db)
    .await;
    assert!(ok.is_ok(), "First primary skill_id should insert");

    // Second primary skill on same slice: should fail (unique index)
    let bad = sqlx::query(
        "INSERT INTO slice_skills (slice_id, skill_id, weight, is_primary) VALUES ($1, $2, 3, TRUE)",
    )
    .bind(slice_id)
    .bind(skill_ids[1])
    .execute(&db)
    .await;
    assert!(
        bad.is_err(),
        "Second primary=TRUE on same slice should be rejected"
    );

    // Second non-primary is fine
    let ok2 = sqlx::query(
        "INSERT INTO slice_skills (slice_id, skill_id, weight, is_primary) VALUES ($1, $2, 3, FALSE)",
    )
    .bind(slice_id)
    .bind(skill_ids[1])
    .execute(&db)
    .await;
    assert!(ok2.is_ok(), "Second is_primary=FALSE should insert");

    db.close().await;
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn user_skills_proficiency_default_is_one() {
    let (db, db_name) = setup_test_db().await;

    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id).await;

    let skill_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM skill_nodes WHERE parent_id IS NOT NULL LIMIT 1",
    )
    .fetch_one(&db)
    .await
    .expect("Failed to fetch a skill_node");

    sqlx::query(
        "INSERT INTO user_skills (user_id, skill_id) VALUES ($1, $2)",
    )
    .bind(user_id)
    .bind(skill_id)
    .execute(&db)
    .await
    .expect("Failed to insert user_skill");

    let row = sqlx::query(
        "SELECT proven_count, weighted_proven_count, proficiency_level
         FROM user_skills WHERE user_id = $1 AND skill_id = $2",
    )
    .bind(user_id)
    .bind(skill_id)
    .fetch_one(&db)
    .await
    .expect("Failed to fetch user_skill");

    let proven_count: i32 = row.get(0);
    let wpc: i32 = row.get(1);
    let level: i16 = row.get(2);

    assert_eq!(proven_count, 0);
    assert_eq!(wpc, 0);
    assert_eq!(level, 1);

    db.close().await;
    cleanup_test_db(&db_name).await;
}
