//! Tests d'intégration P8.6 : consumers legacy skill_fragments retombent sur
//! user_skills.
//!
//! Vérifie les 3 orderings + le comportement legacy-first + le fallback quand
//! aucune ligne legacy n'existe.

use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

use skilluv_backend::services::{SkillFragmentOrder, SkillsService};

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
// Cas 1 : legacy skill_fragments présents → SELECT direct
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn legacy_skill_fragments_prime_over_user_skills() {
    let (db, db_name) = setup_test_db().await;
    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id).await;

    // Insertion legacy
    sqlx::query(
        "INSERT INTO skill_fragments (user_id, skill_domain, sub_skill, fragments)
         VALUES ($1, 'code', 'python', 100),
                ($1, 'design', 'figma', 50)",
    )
    .bind(user_id)
    .execute(&db)
    .await
    .expect("insert legacy");

    // Insertion user_skills (ne devrait pas être lu tant que legacy présent)
    let skill_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM skill_nodes WHERE parent_id IS NOT NULL LIMIT 1",
    )
    .fetch_one(&db)
    .await
    .expect("skill");
    sqlx::query(
        "INSERT INTO user_skills
            (user_id, skill_id, proven_count, weighted_proven_count, proficiency_level,
             first_proven_at, last_proven_at)
         VALUES ($1, $2, 5, 30, 5, NOW(), NOW())",
    )
    .bind(user_id)
    .bind(skill_id)
    .execute(&db)
    .await
    .expect("insert user_skills");

    let fragments = SkillsService::list_user_skill_fragments_or_backfill(
        &db,
        user_id,
        SkillFragmentOrder::ByDomainThenSubskill,
    )
    .await
    .expect("list");

    // 2 fragments legacy attendus (pas les 1 user_skills)
    assert_eq!(fragments.len(), 2);
    // ORDER BY skill_domain, sub_skill : 'code' vient avant 'design' alphabétiquement
    assert_eq!(fragments[0].sub_skill, "python");
    assert_eq!(fragments[1].sub_skill, "figma");

    db.close().await;
    cleanup_test_db(&db_name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Cas 2 : pas de legacy → fallback user_skills
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn user_skills_backfill_when_no_legacy() {
    let (db, db_name) = setup_test_db().await;
    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id).await;

    let (skill_a, skill_b): (Uuid, Uuid) = {
        let ids: Vec<Uuid> = sqlx::query_scalar(
            "SELECT id FROM skill_nodes
             WHERE parent_id IS NOT NULL AND domain = 'code' LIMIT 2",
        )
        .fetch_all(&db)
        .await
        .expect("skills");
        (ids[0], ids[1])
    };

    sqlx::query(
        "INSERT INTO user_skills
            (user_id, skill_id, proven_count, weighted_proven_count, proficiency_level,
             first_proven_at, last_proven_at)
         VALUES ($1, $2, 3, 15, 4, NOW(), NOW()),
                ($1, $3, 1, 3, 2, NOW(), NOW())",
    )
    .bind(user_id)
    .bind(skill_a)
    .bind(skill_b)
    .execute(&db)
    .await
    .expect("insert user_skills");

    let fragments = SkillsService::list_user_skill_fragments_or_backfill(
        &db,
        user_id,
        SkillFragmentOrder::ByFragmentsDesc,
    )
    .await
    .expect("list");

    assert_eq!(fragments.len(), 2);
    // ByFragmentsDesc → WPC 15 avant WPC 3
    assert_eq!(fragments[0].fragments, 15);
    assert_eq!(fragments[1].fragments, 3);
    // Domain hydraté depuis skill_nodes
    assert!(fragments.iter().all(|f| f.skill_domain == "code"));

    db.close().await;
    cleanup_test_db(&db_name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Cas 3 : ni legacy ni user_skills → vide
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn empty_when_no_data() {
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
// Cas 4 : user_skills avec proven_count=0 exclus du fallback
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn zero_proven_count_skills_excluded_from_backfill() {
    let (db, db_name) = setup_test_db().await;
    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id).await;

    let (skill_a, skill_b): (Uuid, Uuid) = {
        let ids: Vec<Uuid> = sqlx::query_scalar(
            "SELECT id FROM skill_nodes WHERE parent_id IS NOT NULL LIMIT 2",
        )
        .fetch_all(&db)
        .await
        .expect("skills");
        (ids[0], ids[1])
    };

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

    db.close().await;
    cleanup_test_db(&db_name).await;
}

// ═══════════════════════════════════════════════════════════════════
// P8.6b : helper list_user_top_skills respecte limit + fallback
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn top_skills_respects_limit_and_ordering() {
    let (db, db_name) = setup_test_db().await;
    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id).await;

    sqlx::query(
        "INSERT INTO skill_fragments (user_id, skill_domain, sub_skill, fragments)
         VALUES ($1, 'code', 'rust', 500),
                ($1, 'code', 'python', 300),
                ($1, 'design', 'figma', 200),
                ($1, 'code', 'go', 100),
                ($1, 'security', 'owasp', 50)",
    )
    .bind(user_id)
    .execute(&db)
    .await
    .expect("insert");

    let top3 = SkillsService::list_user_top_skills(&db, user_id, 3)
        .await
        .expect("top3");
    assert_eq!(top3.len(), 3);
    assert_eq!(top3[0].1, "rust");
    assert_eq!(top3[1].1, "python");
    assert_eq!(top3[2].1, "figma");

    let top10 = SkillsService::list_user_top_skills(&db, user_id, 10)
        .await
        .expect("top10");
    assert_eq!(top10.len(), 5, "capped at total rows count");

    db.close().await;
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn top_skills_fallback_when_no_legacy() {
    let (db, db_name) = setup_test_db().await;
    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id).await;

    let skill_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM skill_nodes WHERE parent_id IS NOT NULL LIMIT 1",
    )
    .fetch_one(&db)
    .await
    .expect("skill");
    sqlx::query(
        "INSERT INTO user_skills
            (user_id, skill_id, proven_count, weighted_proven_count, proficiency_level,
             first_proven_at, last_proven_at)
         VALUES ($1, $2, 3, 12, 3, NOW(), NOW())",
    )
    .bind(user_id)
    .bind(skill_id)
    .execute(&db)
    .await
    .expect("user_skills");

    let top3 = SkillsService::list_user_top_skills(&db, user_id, 3)
        .await
        .expect("top3");
    assert_eq!(top3.len(), 1);
    assert_eq!(top3[0].2, 12, "fragments = weighted_proven_count");

    db.close().await;
    cleanup_test_db(&db_name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Cas 5 : ordering ByDomainThenFragmentsDesc
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn ordering_by_domain_then_fragments_desc() {
    let (db, db_name) = setup_test_db().await;
    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id).await;

    sqlx::query(
        "INSERT INTO skill_fragments (user_id, skill_domain, sub_skill, fragments)
         VALUES ($1, 'design', 'figma', 30),
                ($1, 'code', 'rust', 200),
                ($1, 'code', 'python', 50),
                ($1, 'design', 'penpot', 80)",
    )
    .bind(user_id)
    .execute(&db)
    .await
    .expect("insert");

    let fragments = SkillsService::list_user_skill_fragments_or_backfill(
        &db,
        user_id,
        SkillFragmentOrder::ByDomainThenFragmentsDesc,
    )
    .await
    .expect("list");

    // domains alpha, puis fragments DESC dans chaque
    assert_eq!(fragments.len(), 4);
    assert_eq!(fragments[0].skill_domain, "code");
    assert_eq!(fragments[0].sub_skill, "rust");
    assert_eq!(fragments[1].sub_skill, "python");
    assert_eq!(fragments[2].skill_domain, "design");
    assert_eq!(fragments[2].sub_skill, "penpot");
    assert_eq!(fragments[3].sub_skill, "figma");

    db.close().await;
    cleanup_test_db(&db_name).await;
}
