//! Tests d'intégration Phase P8.2 : DAG check dans /api/challenges/{id}/start.
//!
//! Vérifie que TracksService::check_eligibility fonctionne côté service pour
//! les deux cas d'usage :
//! - Challenge avec prérequis DAG → check via deliverables verified
//! - Challenge sans prérequis DAG → autorisé sans check (P8.3 : gate 100% DAG)
//!
//! Le endpoint HTTP n'est pas testé directement ici car il demande le stack
//! auth complet ; le comportement est testé au niveau service.

use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

use skilluv_backend::services::TracksService;

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p8_2_test_{}",
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

async fn insert_test_user(db: &PgPool, user_id: Uuid, total_fragments: i32) {
    let short = &user_id.to_string()[..8];
    sqlx::query(
        "INSERT INTO users
            (id, email, username, first_name, last_name, display_name, password_hash,
             profile_active, total_fragments)
         VALUES ($1, $2, $3, $4, $5, $6, $7, TRUE, $8)",
    )
    .bind(user_id)
    .bind(format!("test-{user_id}@example.com"))
    .bind(format!("t{short}"))
    .bind("Test")
    .bind("User")
    .bind("Test User")
    .bind("dummy_hash")
    .bind(total_fragments)
    .execute(db)
    .await
    .expect("user");
}

async fn insert_training_challenge(db: &PgPool, title: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO challenge_templates
            (title, description, instructions, skill_domain, difficulty,
             reward_fragments, is_onboarding, is_training, status)
         VALUES ($1, 'D', 'I', 'code', 1, 10, TRUE, TRUE, 'published')
         RETURNING id",
    )
    .bind(title)
    .fetch_one(db)
    .await
    .expect("challenge")
}

async fn add_verified_deliverable_for_challenge(
    db: &PgPool,
    user_id: Uuid,
    challenge_id: Uuid,
) {
    sqlx::query(
        "INSERT INTO deliverables
            (challenge_id, user_id, artifact_type, artifact_url, verifiable_by,
             verification_status)
         VALUES ($1, $2, 'other', 'http://x/', 'human_review', 'verified')",
    )
    .bind(challenge_id)
    .bind(user_id)
    .execute(db)
    .await
    .expect("deliverable");
}

// ═══════════════════════════════════════════════════════════════════
// Cas 1 : challenge sans DAG → fallback fragments
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn no_dag_entries_uses_legacy_fragments_check() {
    let (db, db_name) = setup_test_db().await;

    let challenge_id = insert_training_challenge(&db, "Legacy Challenge").await;
    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id, 0).await;

    // Sans DAG, check_eligibility retourne toujours true (aucun prereq à vérifier).
    // C'est le fallback legacy fragments qui bloque l'user côté route handler.
    let eligibility =
        TracksService::check_eligibility(&db, user_id, challenge_id)
            .await
            .expect("elig");
    assert!(eligibility.eligible, "no DAG entries → service says eligible");
    assert!(eligibility.missing_required_prerequisites.is_empty());

    // Détection de la présence de DAG côté SQL (miroir du has_dag_prereqs du route)
    let has_dag: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM challenge_prerequisites WHERE challenge_id = $1)",
    )
    .bind(challenge_id)
    .fetch_one(&db)
    .await
    .expect("query");
    assert!(!has_dag, "no DAG entries in fresh setup");

    db.close().await;
    cleanup_test_db(&db_name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Cas 2 : challenge avec DAG, prereq NON complété → bloque
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn dag_check_blocks_when_required_prereq_not_completed() {
    let (db, db_name) = setup_test_db().await;

    let prereq_id = insert_training_challenge(&db, "Prereq").await;
    let target_id = insert_training_challenge(&db, "Target").await;

    // Le target dépend du prereq
    TracksService::add_prerequisite(&db, target_id, prereq_id, true)
        .await
        .expect("add prereq");

    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id, 0).await;

    // User n'a pas complété le prereq → check_eligibility retourne false
    let eligibility =
        TracksService::check_eligibility(&db, user_id, target_id)
            .await
            .expect("elig");
    assert!(!eligibility.eligible);
    assert_eq!(eligibility.missing_required_prerequisites, vec![prereq_id]);

    // has_dag = true (la route handler prendra la branche DAG et bloquera)
    let has_dag: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM challenge_prerequisites WHERE challenge_id = $1)",
    )
    .bind(target_id)
    .fetch_one(&db)
    .await
    .expect("query");
    assert!(has_dag);

    db.close().await;
    cleanup_test_db(&db_name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Cas 3 : DAG + prereq complété → autorise
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn dag_check_allows_when_required_prereq_completed_via_deliverable() {
    let (db, db_name) = setup_test_db().await;

    let prereq_id = insert_training_challenge(&db, "Prereq").await;
    let target_id = insert_training_challenge(&db, "Target").await;
    TracksService::add_prerequisite(&db, target_id, prereq_id, true)
        .await
        .expect("add prereq");

    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id, 0).await;

    // User complète le prereq via un deliverable verified
    add_verified_deliverable_for_challenge(&db, user_id, prereq_id).await;

    let eligibility =
        TracksService::check_eligibility(&db, user_id, target_id)
            .await
            .expect("elig");
    assert!(eligibility.eligible);
    assert!(eligibility.missing_required_prerequisites.is_empty());

    db.close().await;
    cleanup_test_db(&db_name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Cas 4 : DAG existe mais prereq est optionnel → autorise même si non fait
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn dag_optional_prereq_does_not_block() {
    let (db, db_name) = setup_test_db().await;

    let recommended_id = insert_training_challenge(&db, "Recommended").await;
    let target_id = insert_training_challenge(&db, "Target").await;
    // required=false → recommandé, ne bloque pas
    TracksService::add_prerequisite(&db, target_id, recommended_id, false)
        .await
        .expect("add optional prereq");

    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id, 0).await;

    let eligibility =
        TracksService::check_eligibility(&db, user_id, target_id)
            .await
            .expect("elig");
    assert!(eligibility.eligible);
    assert!(eligibility.missing_required_prerequisites.is_empty());
    assert_eq!(
        eligibility.missing_recommended_prerequisites,
        vec![recommended_id]
    );

    db.close().await;
    cleanup_test_db(&db_name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Cas 5 : le check est 100% DAG en P8.3 — un user avec 0 fragments peut
// démarrer un challenge dès que le DAG est satisfait
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn dag_only_gating_allows_zero_fragments_when_dag_satisfied() {
    let (db, db_name) = setup_test_db().await;

    let prereq_id = insert_training_challenge(&db, "Prereq").await;
    let target_id = insert_training_challenge(&db, "Target").await;

    TracksService::add_prerequisite(&db, target_id, prereq_id, true)
        .await
        .expect("add prereq");

    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id, 0).await;
    add_verified_deliverable_for_challenge(&db, user_id, prereq_id).await;

    let eligibility =
        TracksService::check_eligibility(&db, user_id, target_id)
            .await
            .expect("elig");
    assert!(eligibility.eligible);

    db.close().await;
    cleanup_test_db(&db_name).await;
}
