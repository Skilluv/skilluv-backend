//! Tests d'intégration P15.2 : LLM verifier (wrapper skilluv-ia gRPC).
//!
//! On teste les chemins qui ne nécessitent PAS un serveur gRPC live :
//!   - Deliverable inexistant → NotFound.
//!   - Deliverable avec verifiable_by != 'llm_evaluation' → Validation error.
//!   - Code content vide → Validation error.
//!   - ai_client=None → pending_manual_review + signal `llm_verifier.status = skipped`.
//!
//! Le chemin nominal (score >= 0.7 → verified) nécessite un mock gRPC ;
//! il est couvert manuellement en environnement dev avec skilluv-ia lancé.

use serde_json::json;
use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

use skilluv_backend::services::llm_verifier;

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p15_2_test_{}",
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

async fn create_user(db: &PgPool) -> Uuid {
    let uid = Uuid::new_v4();
    let short = &uid.to_string()[..8];
    sqlx::query(
        "INSERT INTO users
            (id, email, username, first_name, last_name, display_name, password_hash,
             profile_active, total_fragments)
         VALUES ($1, $2, $3, 'T', 'U', 'Test', 'dummy', TRUE, 0)",
    )
    .bind(uid)
    .bind(format!("t-{uid}@ex.io"))
    .bind(format!("t{short}"))
    .execute(db)
    .await
    .expect("u");
    uid
}

/// Crée un template + deliverable avec metadata donnée, retourne l'id du deliverable.
async fn create_deliverable(
    db: &PgPool,
    user_id: Uuid,
    verifiable_by: &str,
    metadata: Option<serde_json::Value>,
) -> Uuid {
    let cid: Uuid = sqlx::query_scalar(
        "INSERT INTO challenge_templates
            (title, description, instructions, skill_domain, difficulty,
             is_training, status, evaluation_rubric)
         VALUES ('T', 'D', 'Do X', 'code', 2, TRUE, 'published',
                 '{\"criteria\":[\"clarity\",\"tests\"]}'::jsonb)
         RETURNING id",
    )
    .fetch_one(db)
    .await
    .expect("ch");
    sqlx::query_scalar(
        "INSERT INTO deliverables
            (challenge_id, user_id, artifact_type, artifact_url,
             verifiable_by, verification_status, artifact_metadata)
         VALUES ($1, $2, 'other', $3, $4, 'pending', $5)
         RETURNING id",
    )
    .bind(cid)
    .bind(user_id)
    .bind(format!("skilluv:t:{}", Uuid::new_v4()))
    .bind(verifiable_by)
    .bind(metadata)
    .fetch_one(db)
    .await
    .expect("d")
}

// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn returns_not_found_when_deliverable_missing() {
    let (db, name) = setup_test_db().await;
    let res = llm_verifier::evaluate_deliverable(&db, None, Uuid::new_v4()).await;
    assert!(res.is_err(), "missing id must fail");
    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn rejects_deliverable_not_marked_for_llm_evaluation() {
    let (db, name) = setup_test_db().await;
    let user = create_user(&db).await;
    let d = create_deliverable(&db, user, "human_review", Some(json!({"code_content": "x"}))).await;
    let res = llm_verifier::evaluate_deliverable(&db, None, d).await;
    assert!(res.is_err(), "human_review deliverable must be rejected");
    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn rejects_when_code_content_empty() {
    let (db, name) = setup_test_db().await;
    let user = create_user(&db).await;
    let d = create_deliverable(
        &db,
        user,
        "llm_evaluation",
        Some(json!({"code_content": "   "})),
    )
    .await;
    let res = llm_verifier::evaluate_deliverable(&db, None, d).await;
    assert!(res.is_err(), "empty code content must fail");
    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn falls_back_to_manual_review_when_ai_client_absent() {
    let (db, name) = setup_test_db().await;
    let user = create_user(&db).await;
    let d = create_deliverable(
        &db,
        user,
        "llm_evaluation",
        Some(json!({
            "code_content": "fn add(a: i32, b: i32) -> i32 { a + b }",
            "language": "rust"
        })),
    )
    .await;

    let outcome = llm_verifier::evaluate_deliverable(&db, None, d).await.expect("outcome");
    assert_eq!(outcome.new_status, "pending_manual_review");
    assert!(!outcome.llm_reachable);
    assert!(outcome.score.is_none());

    // Vérifie la persistance : status + signal.llm_verifier.status = 'skipped'
    let (status, signal): (String, Option<serde_json::Value>) = sqlx::query_as(
        "SELECT verification_status, verification_signal FROM deliverables WHERE id = $1",
    )
    .bind(d)
    .fetch_one(&db)
    .await
    .expect("row");
    assert_eq!(status, "pending_manual_review");
    let sig = signal.expect("signal set");
    assert_eq!(sig["llm_verifier"]["status"], "skipped");
    assert_eq!(sig["llm_verifier"]["reason"], "ai_client_not_connected");

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Migration 0087 : 'llm_evaluation' est bien accepté par le CHECK constraint
// et evaluation_rubric est bien un JSONB queryable.
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn migration_0087_accepts_llm_evaluation_verifiable_by() {
    let (db, name) = setup_test_db().await;
    let user = create_user(&db).await;

    let d = create_deliverable(
        &db,
        user,
        "llm_evaluation",
        Some(json!({"code_content": "fn x() {}"})),
    )
    .await;
    assert!(!d.is_nil());

    let rubric: Option<serde_json::Value> = sqlx::query_scalar(
        "SELECT ct.evaluation_rubric
         FROM deliverables d JOIN challenge_templates ct ON ct.id = d.challenge_id
         WHERE d.id = $1",
    )
    .bind(d)
    .fetch_one(&db)
    .await
    .expect("rubric");
    let r = rubric.expect("rubric set by default");
    assert!(r["criteria"].is_array());

    db.close().await;
    cleanup_test_db(&name).await;
}
