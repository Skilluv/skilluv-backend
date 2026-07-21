//! Tests d'intégration P8.6 (post-P8.7) : `list_user_skill_fragments_or_backfill`
//! synthétise désormais toujours depuis `user_skills` + `skill_nodes`.
//!
//! La table `skill_fragments` a été droppée en migration 0071, donc les cas
//! "legacy prime over user_skills" n'existent plus.

use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

use skilluv_backend::services::SkillsService;
use skilluv_backend::services::skills::SkillFragmentOrder;

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p8_6_test_{}",
        Uuid::new_v4().to_string().replace('-', "")
    );
    let admin_pool = PgPoolOptions::new()
        .max_connections(2)
        .connect("postgres://skilluv:skilluv_secret@localhost:5433/skilluv")
        .await
        .expect("admin");
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "CREATE DATABASE \"{db_name}\""
    )))
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
    let _ = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '{db_name}'"
    )))
    .execute(&admin_pool)
    .await;
    let _ = sqlx::query(sqlx::AssertSqlSafe(format!(
        "DROP DATABASE IF EXISTS \"{db_name}\""
    )))
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

async fn insert_user_skill_by_slug(db: &PgPool, user_id: Uuid, slug: &str, wpc: i32) {
    let skill_id: Uuid = sqlx::query_scalar("SELECT id FROM skill_nodes WHERE slug = $1")
        .bind(slug)
        .fetch_one(db)
        .await
        .expect("skill_id");
    sqlx::query(
        "INSERT INTO user_skills
            (user_id, skill_id, proven_count, weighted_proven_count, proficiency_level,
             first_proven_at, last_proven_at)
         VALUES ($1, $2, GREATEST(1, $3), $3, 1, NOW(), NOW())",
    )
    .bind(user_id)
    .bind(skill_id)
    .bind(wpc)
    .execute(db)
    .await
    .expect("user_skills");
}

// ═══════════════════════════════════════════════════════════════════
// Cas 1 : user_skills seul → backfill retourne les fragments synthétiques
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn user_skills_backfill_returns_synthetic_fragments() {
    let (db, db_name) = setup_test_db().await;
    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id).await;

    insert_user_skill_by_slug(&db, user_id, "rust", 15).await;
    insert_user_skill_by_slug(&db, user_id, "python", 3).await;

    let fragments = SkillsService::list_user_skill_fragments_or_backfill(
        &db,
        user_id,
        SkillFragmentOrder::ByFragmentsDesc,
    )
    .await
    .expect("list");

    assert_eq!(fragments.len(), 2);
    assert_eq!(fragments[0].fragments, 15);
    assert_eq!(fragments[0].sub_skill, "rust");
    assert_eq!(fragments[1].fragments, 3);
    assert!(fragments.iter().all(|f| f.skill_domain == "code"));

    db.close().await;
    cleanup_test_db(&db_name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Cas 2 : aucun user_skills → vide
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn empty_when_no_user_skills() {
    let (db, db_name) = setup_test_db().await;
    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id).await;

    let fragments = SkillsService::list_user_skill_fragments_or_backfill(
        &db,
        user_id,
        SkillFragmentOrder::ByDomainThenSubskill,
    )
    .await
    .expect("list");
    assert!(fragments.is_empty());

    db.close().await;
    cleanup_test_db(&db_name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Cas 3 : proven_count = 0 exclu
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn zero_proven_count_skills_excluded_from_backfill() {
    let (db, db_name) = setup_test_db().await;
    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id).await;

    let skill_a: Uuid = sqlx::query_scalar("SELECT id FROM skill_nodes WHERE slug = 'rust'")
        .fetch_one(&db)
        .await
        .expect("rust");
    let skill_b: Uuid = sqlx::query_scalar("SELECT id FROM skill_nodes WHERE slug = 'python'")
        .fetch_one(&db)
        .await
        .expect("python");

    sqlx::query(
        "INSERT INTO user_skills
            (user_id, skill_id, proven_count, weighted_proven_count, proficiency_level)
         VALUES ($1, $2, 0, 0, 1),
                ($1, $3, 2, 4, 2)",
    )
    .bind(user_id)
    .bind(skill_a)
    .bind(skill_b)
    .execute(&db)
    .await
    .expect("insert");

    let fragments = SkillsService::list_user_skill_fragments_or_backfill(
        &db,
        user_id,
        SkillFragmentOrder::ByFragmentsDesc,
    )
    .await
    .expect("list");
    assert_eq!(fragments.len(), 1);
    assert_eq!(fragments[0].fragments, 4);
    assert_eq!(fragments[0].sub_skill, "python");

    db.close().await;
    cleanup_test_db(&db_name).await;
}

// ═══════════════════════════════════════════════════════════════════
// P8.6b : helper list_user_top_skills respecte limit + ordering
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn top_skills_respects_limit_and_ordering() {
    let (db, db_name) = setup_test_db().await;
    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id).await;

    insert_user_skill_by_slug(&db, user_id, "rust", 500).await;
    insert_user_skill_by_slug(&db, user_id, "python", 300).await;
    insert_user_skill_by_slug(&db, user_id, "typescript", 200).await;
    insert_user_skill_by_slug(&db, user_id, "figma-craft", 100).await;

    let top3 = SkillsService::list_user_top_skills(&db, user_id, 3)
        .await
        .expect("top3");
    assert_eq!(top3.len(), 3);
    assert_eq!(top3[0].1, "rust");
    assert_eq!(top3[1].1, "python");
    assert_eq!(top3[2].1, "typescript");

    let top10 = SkillsService::list_user_top_skills(&db, user_id, 10)
        .await
        .expect("top10");
    assert_eq!(top10.len(), 4, "capped at total rows count");

    db.close().await;
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn top_skills_empty_when_no_data() {
    let (db, db_name) = setup_test_db().await;
    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id).await;

    let top3 = SkillsService::list_user_top_skills(&db, user_id, 3)
        .await
        .expect("top3");
    assert!(top3.is_empty());

    db.close().await;
    cleanup_test_db(&db_name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Ordering ByDomainThenFragmentsDesc
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn ordering_by_domain_then_fragments_desc() {
    let (db, db_name) = setup_test_db().await;
    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id).await;

    insert_user_skill_by_slug(&db, user_id, "figma-craft", 30).await;
    insert_user_skill_by_slug(&db, user_id, "rust", 200).await;
    insert_user_skill_by_slug(&db, user_id, "python", 50).await;
    insert_user_skill_by_slug(&db, user_id, "ux", 80).await;

    let fragments = SkillsService::list_user_skill_fragments_or_backfill(
        &db,
        user_id,
        SkillFragmentOrder::ByDomainThenFragmentsDesc,
    )
    .await
    .expect("list");

    assert_eq!(fragments.len(), 4);
    // domain 'code' d'abord (rust=200, python=50), puis 'design' (ux=80, figma-craft=30)
    assert_eq!(fragments[0].skill_domain, "code");
    assert_eq!(fragments[0].sub_skill, "rust");
    assert_eq!(fragments[1].sub_skill, "python");
    assert_eq!(fragments[2].skill_domain, "design");
    assert_eq!(fragments[2].sub_skill, "ux");
    assert_eq!(fragments[3].sub_skill, "figma-craft");

    db.close().await;
    cleanup_test_db(&db_name).await;
}
