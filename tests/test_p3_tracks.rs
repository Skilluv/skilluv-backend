//! Tests d'intégration Phase P3 : tracks + DAG des prérequis.
//!
//! Couvre :
//! - Migrations 0066 (DAG + is_capstone) et 0067 (tracks + seed)
//! - Seed initial des 5 tracks Foundations
//! - Enrollment (idempotent), progression, next_challenge
//! - DAG : add_prerequisite avec anti-cycle, self-reference rejetée par CHECK
//! - Eligibility : bloqué si prérequis required manquant, débloqué si complet

use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

use skilluv_backend::services::TracksService;

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p3_test_{}",
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
    .expect("insert user");
}

async fn insert_training_challenge(db: &PgPool, title: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO challenge_templates
            (title, description, instructions, skill_domain, difficulty, is_training)
         VALUES ($1, 'Test description', 'Test instructions', 'code', 1, TRUE)
         RETURNING id",
    )
    .bind(title)
    .fetch_one(db)
    .await
    .expect("insert challenge")
}

// ═══════════════════════════════════════════════════════════════════
// Migrations
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn migration_0066_creates_dag_and_capstone() {
    let (db, db_name) = setup_test_db().await;

    let dag_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM information_schema.tables
                        WHERE table_schema='public' AND table_name='challenge_prerequisites')",
    )
    .fetch_one(&db)
    .await
    .expect("check");
    assert!(dag_exists);

    let capstone_col: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM information_schema.columns
                        WHERE table_name='challenge_templates' AND column_name='is_capstone')",
    )
    .fetch_one(&db)
    .await
    .expect("check");
    assert!(capstone_col);

    db.close().await;
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn dag_check_rejects_self_reference() {
    let (db, db_name) = setup_test_db().await;
    let c1 = insert_training_challenge(&db, "Self-ref test").await;

    let res = sqlx::query(
        "INSERT INTO challenge_prerequisites (challenge_id, depends_on_challenge_id)
         VALUES ($1, $1)",
    )
    .bind(c1)
    .execute(&db)
    .await;
    assert!(res.is_err(), "Self-reference should be rejected by CHECK");

    db.close().await;
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn migration_0067_seeds_five_foundation_tracks() {
    let (db, db_name) = setup_test_db().await;

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tracks WHERE active = TRUE")
        .fetch_one(&db)
        .await
        .expect("count");
    assert_eq!(count, 5, "Expected 5 Foundation tracks seeded");

    let slugs: Vec<String> =
        sqlx::query_scalar("SELECT slug FROM tracks WHERE active = TRUE ORDER BY slug")
            .fetch_all(&db)
            .await
            .expect("slugs");
    assert!(slugs.contains(&"frontend-foundations".to_string()));
    assert!(slugs.contains(&"backend-foundations".to_string()));
    assert!(slugs.contains(&"security-foundations".to_string()));
    assert!(slugs.contains(&"design-foundations".to_string()));
    assert!(slugs.contains(&"game-foundations".to_string()));

    db.close().await;
    cleanup_test_db(&db_name).await;
}

// ═══════════════════════════════════════════════════════════════════
// TracksService : enrollment + progression
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn enroll_creates_user_track_and_is_idempotent() {
    let (db, db_name) = setup_test_db().await;
    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id).await;

    let ut1 = TracksService::enroll(&db, user_id, "frontend-foundations")
        .await
        .expect("first enroll");
    let ut2 = TracksService::enroll(&db, user_id, "frontend-foundations")
        .await
        .expect("second enroll");

    assert_eq!(ut1.user_id, user_id);
    assert_eq!(ut1.track_id, ut2.track_id);
    assert_eq!(
        ut1.started_at, ut2.started_at,
        "Enroll should be idempotent"
    );

    // One row in user_tracks
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM user_tracks WHERE user_id = $1")
        .bind(user_id)
        .fetch_one(&db)
        .await
        .expect("count");
    assert_eq!(count, 1);

    db.close().await;
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn enroll_rejects_unknown_slug() {
    let (db, db_name) = setup_test_db().await;
    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id).await;

    let res = TracksService::enroll(&db, user_id, "does-not-exist").await;
    assert!(res.is_err());

    db.close().await;
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn get_progress_returns_zero_for_new_enrollment() {
    let (db, db_name) = setup_test_db().await;
    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id).await;

    TracksService::enroll(&db, user_id, "backend-foundations")
        .await
        .expect("enroll");

    let progress = TracksService::get_progress(&db, user_id, "backend-foundations")
        .await
        .expect("progress");

    // Aucun track_challenge seedé pour l'instant
    assert_eq!(progress.total_challenges, 0);
    assert_eq!(progress.completed_challenges, 0);
    assert!(progress.completed_at.is_none());

    db.close().await;
    cleanup_test_db(&db_name).await;
}

// ═══════════════════════════════════════════════════════════════════
// DAG : add_prerequisite avec anti-cycle
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn add_prerequisite_stores_edge() {
    let (db, db_name) = setup_test_db().await;
    let a = insert_training_challenge(&db, "Challenge A").await;
    let b = insert_training_challenge(&db, "Challenge B").await;

    // B depends on A
    TracksService::add_prerequisite(&db, b, a, true)
        .await
        .expect("add");

    let required: bool = sqlx::query_scalar(
        "SELECT required FROM challenge_prerequisites
         WHERE challenge_id = $1 AND depends_on_challenge_id = $2",
    )
    .bind(b)
    .bind(a)
    .fetch_one(&db)
    .await
    .expect("fetch");
    assert!(required);

    db.close().await;
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn add_prerequisite_rejects_direct_cycle() {
    let (db, db_name) = setup_test_db().await;
    let a = insert_training_challenge(&db, "Challenge A").await;
    let b = insert_training_challenge(&db, "Challenge B").await;

    TracksService::add_prerequisite(&db, b, a, true)
        .await
        .expect("A→B ok");

    // Now try A depends on B → creates cycle
    let res = TracksService::add_prerequisite(&db, a, b, true).await;
    assert!(res.is_err(), "Direct cycle should be rejected");

    db.close().await;
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn add_prerequisite_rejects_transitive_cycle() {
    let (db, db_name) = setup_test_db().await;
    let a = insert_training_challenge(&db, "A").await;
    let b = insert_training_challenge(&db, "B").await;
    let c = insert_training_challenge(&db, "C").await;

    // A → B → C (A depends on B, B depends on C)
    TracksService::add_prerequisite(&db, a, b, true)
        .await
        .expect("A depends on B");
    TracksService::add_prerequisite(&db, b, c, true)
        .await
        .expect("B depends on C");

    // Now try C depends on A → transitive cycle C → A → B → C
    let res = TracksService::add_prerequisite(&db, c, a, true).await;
    assert!(res.is_err(), "Transitive cycle should be rejected");

    db.close().await;
    cleanup_test_db(&db_name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Eligibility
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn eligibility_true_when_no_prerequisites() {
    let (db, db_name) = setup_test_db().await;
    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id).await;

    let c = insert_training_challenge(&db, "No prereqs").await;

    let check = TracksService::check_eligibility(&db, user_id, c)
        .await
        .expect("check");
    assert!(check.eligible);
    assert!(check.missing_required_prerequisites.is_empty());

    db.close().await;
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn eligibility_false_when_required_prereq_not_completed() {
    let (db, db_name) = setup_test_db().await;
    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id).await;

    let a = insert_training_challenge(&db, "Prereq A").await;
    let b = insert_training_challenge(&db, "Target B").await;

    TracksService::add_prerequisite(&db, b, a, true)
        .await
        .expect("add");

    let check = TracksService::check_eligibility(&db, user_id, b)
        .await
        .expect("check");
    assert!(!check.eligible);
    assert_eq!(check.missing_required_prerequisites, vec![a]);

    db.close().await;
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn eligibility_true_when_required_prereq_completed_via_deliverable() {
    let (db, db_name) = setup_test_db().await;
    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id).await;

    let a = insert_training_challenge(&db, "Prereq A").await;
    let b = insert_training_challenge(&db, "Target B").await;

    TracksService::add_prerequisite(&db, b, a, true)
        .await
        .expect("add");

    // Simulate a verified deliverable for A
    sqlx::query(
        "INSERT INTO deliverables
            (challenge_id, user_id, artifact_type, artifact_url,
             verifiable_by, verification_status, verified_at)
         VALUES ($1, $2, 'other', 'http://x/', 'human_review', 'verified', NOW())",
    )
    .bind(a)
    .bind(user_id)
    .execute(&db)
    .await
    .expect("insert deliverable");

    let check = TracksService::check_eligibility(&db, user_id, b)
        .await
        .expect("check");
    assert!(check.eligible, "reason: {:?}", check.reason);

    db.close().await;
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn eligibility_recommended_prereqs_dont_block() {
    let (db, db_name) = setup_test_db().await;
    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id).await;

    let a = insert_training_challenge(&db, "Recommended A").await;
    let b = insert_training_challenge(&db, "Target B").await;

    TracksService::add_prerequisite(&db, b, a, false)
        .await
        .expect("add recommended");

    let check = TracksService::check_eligibility(&db, user_id, b)
        .await
        .expect("check");
    assert!(check.eligible, "recommended shouldn't block");
    assert_eq!(check.missing_recommended_prerequisites, vec![a]);
    assert!(check.missing_required_prerequisites.is_empty());

    db.close().await;
    cleanup_test_db(&db_name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Track progress avec challenges
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn progress_reflects_completed_challenges() {
    let (db, db_name) = setup_test_db().await;
    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id).await;

    // Add 3 challenges to the frontend-foundations track
    let track_id: Uuid =
        sqlx::query_scalar("SELECT id FROM tracks WHERE slug = 'frontend-foundations'")
            .fetch_one(&db)
            .await
            .expect("track");

    let c1 = insert_training_challenge(&db, "Track C1").await;
    let c2 = insert_training_challenge(&db, "Track C2").await;
    let c3 = insert_training_challenge(&db, "Track C3").await;

    for (pos, c) in [c1, c2, c3].iter().enumerate() {
        sqlx::query(
            "INSERT INTO track_challenges (track_id, challenge_id, position) VALUES ($1, $2, $3)",
        )
        .bind(track_id)
        .bind(c)
        .bind(pos as i32)
        .execute(&db)
        .await
        .expect("insert track_challenge");
    }

    TracksService::enroll(&db, user_id, "frontend-foundations")
        .await
        .expect("enroll");

    // Complete c1 and c2 (verified deliverables)
    for c in &[c1, c2] {
        sqlx::query(
            "INSERT INTO deliverables
                (challenge_id, user_id, artifact_type, artifact_url,
                 verifiable_by, verification_status, verified_at)
             VALUES ($1, $2, 'other', 'http://x/', 'human_review', 'verified', NOW())",
        )
        .bind(c)
        .bind(user_id)
        .execute(&db)
        .await
        .expect("insert deliverable");
    }

    let progress = TracksService::get_progress(&db, user_id, "frontend-foundations")
        .await
        .expect("progress");
    assert_eq!(progress.total_challenges, 3);
    assert_eq!(progress.completed_challenges, 2);

    // next_challenge_in_track should return c3
    let next = TracksService::next_challenge_in_track(&db, user_id, track_id)
        .await
        .expect("next");
    assert_eq!(next, Some(c3));

    db.close().await;
    cleanup_test_db(&db_name).await;
}
