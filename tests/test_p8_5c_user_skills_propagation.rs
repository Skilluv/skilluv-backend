//! Tests d'intégration P8.5c : propagation legacy challenge → user_skills.
//!
//! Vérifie que `SkillsService::propagate_legacy_challenge_success_to_user_skills` :
//! - Upsert `user_skills` quand `language` matche un slug de `skill_nodes`
//! - Recalcule `proficiency_level` via la formule log2
//! - Skip silencieusement (Ok(None)) quand aucun slug ne matche
//! - Est cumulatif (2 appels → proven_count = 2)

use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

use skilluv_backend::services::SkillsService;

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p8_5c_test_{}",
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

// ═══════════════════════════════════════════════════════════════════
// language = "python" → upsert user_skills sur skill "python"
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn propagates_when_language_matches_slug() {
    let (db, db_name) = setup_test_db().await;

    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id).await;

    let result = SkillsService::propagate_legacy_challenge_success_to_user_skills(
        &db,
        user_id,
        Some("python"),
        "code",
        5,
    )
    .await
    .expect("propagate");

    let skill_id = result.expect("python skill_node should exist in seed 0057");

    let (proven_count, wpc, level): (i32, i32, i16) = sqlx::query_as(
        "SELECT proven_count, weighted_proven_count, proficiency_level
         FROM user_skills WHERE user_id = $1 AND skill_id = $2",
    )
    .bind(user_id)
    .bind(skill_id)
    .fetch_one(&db)
    .await
    .expect("row");

    assert_eq!(proven_count, 1);
    assert_eq!(wpc, 5);
    assert!(level >= 1 && level <= 5);

    db.close().await;
    cleanup_test_db(&db_name).await;
}

// ═══════════════════════════════════════════════════════════════════
// language absent ou pas de match → Ok(None), pas d'insert
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn skips_when_no_slug_match() {
    let (db, db_name) = setup_test_db().await;

    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id).await;

    let none_lang = SkillsService::propagate_legacy_challenge_success_to_user_skills(
        &db, user_id, None, "code", 5,
    )
    .await
    .expect("propagate");
    assert!(none_lang.is_none());

    let unknown = SkillsService::propagate_legacy_challenge_success_to_user_skills(
        &db,
        user_id,
        Some("cobol-hyperion-9000"),
        "code",
        5,
    )
    .await
    .expect("propagate");
    assert!(unknown.is_none());

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_skills WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_one(&db)
    .await
    .expect("count");
    assert_eq!(count, 0, "aucune ligne créée quand pas de match");

    db.close().await;
    cleanup_test_db(&db_name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Appels multiples → proven_count et wpc s'incrémentent
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn is_cumulative_on_repeat() {
    let (db, db_name) = setup_test_db().await;

    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id).await;

    for _ in 0..3 {
        SkillsService::propagate_legacy_challenge_success_to_user_skills(
            &db,
            user_id,
            Some("rust"),
            "code",
            2,
        )
        .await
        .expect("propagate");
    }

    let (proven, wpc): (i32, i32) = sqlx::query_as(
        "SELECT us.proven_count, us.weighted_proven_count
         FROM user_skills us JOIN skill_nodes sn ON sn.id = us.skill_id
         WHERE us.user_id = $1 AND sn.slug = 'rust'",
    )
    .bind(user_id)
    .fetch_one(&db)
    .await
    .expect("row");

    assert_eq!(proven, 3);
    assert_eq!(wpc, 6);

    db.close().await;
    cleanup_test_db(&db_name).await;
}

// ═══════════════════════════════════════════════════════════════════
// weight <= 0 → skip
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn skips_when_weight_is_non_positive() {
    let (db, db_name) = setup_test_db().await;

    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id).await;

    let out = SkillsService::propagate_legacy_challenge_success_to_user_skills(
        &db,
        user_id,
        Some("python"),
        "code",
        0,
    )
    .await
    .expect("propagate");
    assert!(out.is_none());

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_skills WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_one(&db)
    .await
    .expect("count");
    assert_eq!(count, 0);

    db.close().await;
    cleanup_test_db(&db_name).await;
}
