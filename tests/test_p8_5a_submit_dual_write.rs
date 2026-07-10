//! Tests d'intégration P8.5a : dual-write challenge_submissions → deliverables.
//!
//! Vérifie que DeliverablesService::create_from_challenge_submission :
//! - Crée un deliverable verified avec les bons champs
//! - Est idempotent (même code de submission → même deliverable)
//! - Génère un artifact_hash SHA-256 stable

use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

use skilluv_backend::services::DeliverablesService;

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p8_5a_test_{}",
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
    .bind("T")
    .bind("U")
    .bind("Test")
    .bind("dummy")
    .execute(db)
    .await
    .expect("user");
}

async fn insert_training_challenge(db: &PgPool) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO challenges
            (title, description, instructions, skill_domain, difficulty,
             reward_fragments, is_onboarding, is_training, status)
         VALUES ('P8.5a', 'Test', 'Test', 'code', 1, 10, TRUE, TRUE, 'published')
         RETURNING id",
    )
    .fetch_one(db)
    .await
    .expect("challenge")
}

// ═══════════════════════════════════════════════════════════════════
// Création de deliverable depuis submission
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn create_from_submission_produces_verified_deliverable() {
    let (db, db_name) = setup_test_db().await;

    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id).await;
    let challenge_id = insert_training_challenge(&db).await;
    let submission_id = Uuid::new_v4();

    let deliverable_id = DeliverablesService::create_from_challenge_submission(
        &db,
        user_id,
        challenge_id,
        submission_id,
        "print('Hello, Skilluv!')",
        50,
    )
    .await
    .expect("create");

    let (
        artifact_type,
        artifact_url,
        artifact_hash,
        verifiable_by,
        verification_status,
        fragments_awarded,
        stored_challenge_id,
        slice_id,
    ): (String, String, Option<String>, String, String, i32, Option<Uuid>, Option<Uuid>) =
        sqlx::query_as(
            "SELECT artifact_type, artifact_url, artifact_hash, verifiable_by,
                    verification_status, fragments_awarded, challenge_id, slice_id
             FROM deliverables WHERE id = $1",
        )
        .bind(deliverable_id)
        .fetch_one(&db)
        .await
        .expect("fetch");

    assert_eq!(artifact_type, "other");
    assert_eq!(artifact_url, format!("skilluv:submission:{submission_id}"));
    assert!(artifact_hash.is_some());
    assert_eq!(artifact_hash.as_deref().unwrap().len(), 64, "SHA-256 hex = 64 chars");
    assert_eq!(verifiable_by, "automated_diff");
    assert_eq!(verification_status, "verified");
    assert_eq!(fragments_awarded, 50);
    assert_eq!(stored_challenge_id, Some(challenge_id));
    assert!(slice_id.is_none(), "challenge submission has no slice");

    db.close().await;
    cleanup_test_db(&db_name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Idempotence via UNIQUE (user_id, artifact_hash)
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn same_submission_code_is_idempotent() {
    let (db, db_name) = setup_test_db().await;

    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id).await;
    let challenge_id = insert_training_challenge(&db).await;
    let submission_id = Uuid::new_v4();
    let code = "return 42;";

    let first = DeliverablesService::create_from_challenge_submission(
        &db,
        user_id,
        challenge_id,
        submission_id,
        code,
        10,
    )
    .await
    .expect("first");

    let second = DeliverablesService::create_from_challenge_submission(
        &db,
        user_id,
        challenge_id,
        submission_id,
        code, // même code → même hash → même deliverable
        10,
    )
    .await
    .expect("second");

    assert_eq!(first, second, "same code → same deliverable_id");

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM deliverables WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_one(&db)
    .await
    .expect("count");
    assert_eq!(count, 1);

    db.close().await;
    cleanup_test_db(&db_name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Code différent → deliverables distincts
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn different_code_creates_distinct_deliverables() {
    let (db, db_name) = setup_test_db().await;

    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id).await;
    let challenge_id = insert_training_challenge(&db).await;

    let d1 = DeliverablesService::create_from_challenge_submission(
        &db,
        user_id,
        challenge_id,
        Uuid::new_v4(),
        "first attempt",
        5,
    )
    .await
    .expect("d1");

    let d2 = DeliverablesService::create_from_challenge_submission(
        &db,
        user_id,
        challenge_id,
        Uuid::new_v4(),
        "second attempt",
        10,
    )
    .await
    .expect("d2");

    assert_ne!(d1, d2);

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM deliverables WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_one(&db)
    .await
    .expect("count");
    assert_eq!(count, 2);

    db.close().await;
    cleanup_test_db(&db_name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Hash SHA-256 est déterministe pour le même code
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn artifact_hash_is_deterministic() {
    let (db, db_name) = setup_test_db().await;

    let user_a = Uuid::new_v4();
    insert_test_user(&db, user_a).await;
    let user_b = Uuid::new_v4();
    insert_test_user(&db, user_b).await;
    let challenge_id = insert_training_challenge(&db).await;
    let code = "same code";

    let d_a = DeliverablesService::create_from_challenge_submission(
        &db,
        user_a,
        challenge_id,
        Uuid::new_v4(),
        code,
        1,
    )
    .await
    .expect("a");

    let d_b = DeliverablesService::create_from_challenge_submission(
        &db,
        user_b,
        challenge_id,
        Uuid::new_v4(),
        code,
        1,
    )
    .await
    .expect("b");

    // 2 deliverables distincts (users différents) mais même hash
    let hash_a: String = sqlx::query_scalar("SELECT artifact_hash FROM deliverables WHERE id = $1")
        .bind(d_a)
        .fetch_one(&db)
        .await
        .expect("h_a");
    let hash_b: String = sqlx::query_scalar("SELECT artifact_hash FROM deliverables WHERE id = $1")
        .bind(d_b)
        .fetch_one(&db)
        .await
        .expect("h_b");
    assert_eq!(hash_a, hash_b, "same code → same SHA-256 hash across users");

    db.close().await;
    cleanup_test_db(&db_name).await;
}
