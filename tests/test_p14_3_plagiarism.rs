//! Tests d'intégration P14.3 : détection anti-plagiat via cosine similarity.

use bigdecimal::BigDecimal;
use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

use skilluv_backend::services::plagiarism;

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p14_3_test_{}",
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

async fn create_user(db: &PgPool, tenant: Option<Uuid>) -> Uuid {
    let uid = Uuid::new_v4();
    let short = &uid.to_string()[..8];
    sqlx::query(
        "INSERT INTO users
            (id, email, username, first_name, last_name, display_name, password_hash,
             profile_active, total_fragments, primary_tenant_id)
         VALUES ($1, $2, $3, 'T', 'U', 'Test', 'dummy', TRUE, 0, $4)",
    )
    .bind(uid)
    .bind(format!("t-{uid}@ex.io"))
    .bind(format!("t{short}"))
    .bind(tenant)
    .execute(db)
    .await
    .expect("u");
    uid
}

async fn create_deliverable(db: &PgPool, user_id: Uuid, tenant: Option<Uuid>) -> Uuid {
    let challenge_id: Uuid = sqlx::query_scalar(
        "INSERT INTO challenge_templates
            (title, description, instructions, skill_domain, difficulty,
             is_training, status, tenant_id)
         VALUES ('T', 'D', 'I', 'code', 2, TRUE, 'published', $1) RETURNING id",
    )
    .bind(tenant)
    .fetch_one(db)
    .await
    .expect("ch");
    sqlx::query_scalar(
        "INSERT INTO deliverables
            (challenge_id, user_id, artifact_type, artifact_url,
             verifiable_by, verification_status, tenant_id)
         VALUES ($1, $2, 'other', $3, 'human_review', 'verified', $4)
         RETURNING id",
    )
    .bind(challenge_id)
    .bind(user_id)
    .bind(format!("skilluv:t:{}", Uuid::new_v4()))
    .bind(tenant)
    .fetch_one(db)
    .await
    .expect("d")
}

// ═══════════════════════════════════════════════════════════════════
// Cosine unit
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn cosine_similarity_identical_vectors_is_one() {
    let a = vec![1.0f32, 2.0, 3.0];
    let sim = plagiarism::cosine_similarity(&a, &a);
    assert!((sim - 1.0).abs() < 1e-6);
}

#[tokio::test]
async fn cosine_similarity_orthogonal_is_zero() {
    let a = vec![1.0f32, 0.0];
    let b = vec![0.0f32, 1.0];
    let sim = plagiarism::cosine_similarity(&a, &b);
    assert!((sim - 0.0).abs() < 1e-6);
}

#[tokio::test]
async fn cosine_similarity_mismatched_length_is_zero() {
    let a = vec![1.0f32, 2.0];
    let b = vec![1.0f32, 2.0, 3.0];
    assert_eq!(plagiarism::cosine_similarity(&a, &b), 0.0);
}

// ═══════════════════════════════════════════════════════════════════
// store_embedding upsert
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn store_embedding_upserts() {
    let (db, name) = setup_test_db().await;
    let user = create_user(&db, None).await;
    let d = create_deliverable(&db, user, None).await;

    let v1 = vec![0.5f32; 8];
    plagiarism::store_embedding(&db, d, None, &v1)
        .await
        .expect("s1");
    let v2 = vec![0.9f32; 8];
    plagiarism::store_embedding(&db, d, None, &v2)
        .await
        .expect("s2");

    let stored: Vec<f32> = sqlx::query_scalar(
        "SELECT embedding FROM deliverable_embeddings WHERE deliverable_id = $1",
    )
    .bind(d)
    .fetch_one(&db)
    .await
    .expect("f");
    assert_eq!(stored, v2);

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// scan_deliverable détecte le match cross-user > threshold
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn scan_flags_high_similarity_across_users() {
    let (db, name) = setup_test_db().await;
    let user_a = create_user(&db, None).await;
    let user_b = create_user(&db, None).await;
    let d_a = create_deliverable(&db, user_a, None).await;
    let d_b = create_deliverable(&db, user_b, None).await;

    let v = vec![1.0f32, 0.5, 0.25, 0.125];
    let close = vec![1.0f32, 0.5, 0.25, 0.13];
    plagiarism::store_embedding(&db, d_a, None, &v)
        .await
        .expect("a");
    plagiarism::store_embedding(&db, d_b, None, &close)
        .await
        .expect("b");

    let res = plagiarism::scan_deliverable(&db, d_b, 0.9, 30)
        .await
        .expect("scan");
    assert_eq!(res.best_match_id, Some(d_a));
    assert!(res.best_score >= 0.9);
    assert_eq!(res.compared_count, 1);

    let (score, similar_to): (Option<BigDecimal>, Option<Uuid>) = sqlx::query_as(
        "SELECT plagiarism_score, plagiarism_similar_to
             FROM deliverables WHERE id = $1",
    )
    .bind(d_b)
    .fetch_one(&db)
    .await
    .expect("row");
    assert!(score.is_some(), "score set");
    assert_eq!(similar_to, Some(d_a));

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// scan_deliverable ne flag pas si similarité < threshold
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn scan_does_not_flag_when_below_threshold() {
    let (db, name) = setup_test_db().await;
    let user_a = create_user(&db, None).await;
    let user_b = create_user(&db, None).await;
    let d_a = create_deliverable(&db, user_a, None).await;
    let d_b = create_deliverable(&db, user_b, None).await;

    // Vecteurs assez différents
    let v_a = vec![1.0f32, 0.0, 0.0, 0.0];
    let v_b = vec![0.0f32, 1.0, 0.0, 0.0];
    plagiarism::store_embedding(&db, d_a, None, &v_a)
        .await
        .expect("a");
    plagiarism::store_embedding(&db, d_b, None, &v_b)
        .await
        .expect("b");

    let res = plagiarism::scan_deliverable(&db, d_b, 0.9, 30)
        .await
        .expect("scan");
    assert!(res.best_score < 0.5);

    let (score, similar_to): (Option<BigDecimal>, Option<Uuid>) = sqlx::query_as(
        "SELECT plagiarism_score, plagiarism_similar_to
             FROM deliverables WHERE id = $1",
    )
    .bind(d_b)
    .fetch_one(&db)
    .await
    .expect("row");
    assert!(score.is_none(), "score reste NULL car < threshold");
    assert!(similar_to.is_none());

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Scan isolé par tenant : ne compare que dans le même tenant
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn scan_only_compares_within_same_tenant() {
    let (db, name) = setup_test_db().await;
    let tenant_a: Uuid = sqlx::query_scalar(
        "INSERT INTO tenants (slug, name, contact_email)
         VALUES ('corpP3', 'P3', 'p3@ex.io') RETURNING id",
    )
    .fetch_one(&db)
    .await
    .expect("t");
    let user_a = create_user(&db, Some(tenant_a)).await;
    let user_pub = create_user(&db, None).await;
    let d_a = create_deliverable(&db, user_a, Some(tenant_a)).await;
    let d_pub = create_deliverable(&db, user_pub, None).await;

    // Même embedding sur tenant_a et pub
    let same = vec![1.0f32, 0.5, 0.25];
    plagiarism::store_embedding(&db, d_a, Some(tenant_a), &same)
        .await
        .expect("a");
    plagiarism::store_embedding(&db, d_pub, None, &same)
        .await
        .expect("pub");

    // Scan de d_a : ne doit PAS voir d_pub (tenant différent).
    let res = plagiarism::scan_deliverable(&db, d_a, 0.9, 30)
        .await
        .expect("s");
    assert_eq!(res.compared_count, 0, "pas de match cross-tenant");

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// list_flagged retourne les deliverables >= threshold
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn list_flagged_returns_sorted_by_score_desc() {
    let (db, name) = setup_test_db().await;
    let user = create_user(&db, None).await;
    let d1 = create_deliverable(&db, user, None).await;
    let d2 = create_deliverable(&db, user, None).await;
    let d3 = create_deliverable(&db, user, None).await;

    sqlx::query("UPDATE deliverables SET plagiarism_score = 0.95 WHERE id = $1")
        .bind(d1)
        .execute(&db)
        .await
        .expect("u1");
    sqlx::query("UPDATE deliverables SET plagiarism_score = 0.92 WHERE id = $1")
        .bind(d2)
        .execute(&db)
        .await
        .expect("u2");
    sqlx::query("UPDATE deliverables SET plagiarism_score = 0.5 WHERE id = $1")
        .bind(d3)
        .execute(&db)
        .await
        .expect("u3");

    let list = plagiarism::list_flagged(&db, BigDecimal::try_from(0.9).unwrap(), 10)
        .await
        .expect("l");
    assert_eq!(list.len(), 2, "d1 + d2 >= 0.9");
    assert_eq!(list[0].0, d1, "d1 first (score 0.95)");
    assert_eq!(list[1].0, d2);

    db.close().await;
    cleanup_test_db(&name).await;
}
