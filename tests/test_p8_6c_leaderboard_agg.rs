//! Tests d'intégration P8.6c : aggrégations leaderboard sur skill_fragments
//! (legacy) UNION user_skills (nouveau graph).
//!
//! Le service `LeaderboardService` fait deux requêtes clés :
//! 1. `update_score` — total par domaine pour un user (MAX legacy vs graph).
//! 2. `seed_from_db` — batch tous users × domaines (MAX legacy vs graph).
//!
//! On mirore ici la logique SQL pour valider :
//! - Legacy seul (skill_fragments présent, user_skills vide) → total legacy.
//! - Graph seul (skill_fragments vide, user_skills présent) → total graph.
//! - Les deux présents (cas dual-write P8.5a+c) → MAX pour éviter le double-comptage.

use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p8_6c_test_{}",
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

async fn insert_user(db: &PgPool) -> Uuid {
    let user_id = Uuid::new_v4();
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
    user_id
}

/// Mirror update_score's domain aggregation logic.
async fn compute_domain_total(db: &PgPool, user_id: Uuid, domain: &str) -> i64 {
    let legacy: Option<i64> = sqlx::query_scalar(
        "SELECT COALESCE(SUM(fragments), 0) FROM skill_fragments WHERE user_id = $1 AND skill_domain = $2",
    )
    .bind(user_id)
    .bind(domain)
    .fetch_one(db)
    .await
    .expect("legacy");

    let graph: Option<i64> = sqlx::query_scalar(
        r#"
        SELECT COALESCE(SUM(us.weighted_proven_count)::BIGINT, 0)
        FROM user_skills us
        JOIN skill_nodes sn ON sn.id = us.skill_id
        WHERE us.user_id = $1 AND sn.domain = $2
        "#,
    )
    .bind(user_id)
    .bind(domain)
    .fetch_one(db)
    .await
    .expect("graph");

    legacy.unwrap_or(0).max(graph.unwrap_or(0))
}

async fn seed_legacy(db: &PgPool, user_id: Uuid, domain: &str, sub: &str, frags: i32) {
    sqlx::query(
        "INSERT INTO skill_fragments (user_id, skill_domain, sub_skill, fragments)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (user_id, skill_domain, sub_skill)
         DO UPDATE SET fragments = skill_fragments.fragments + $4",
    )
    .bind(user_id)
    .bind(domain)
    .bind(sub)
    .bind(frags)
    .execute(db)
    .await
    .expect("skill_fragments");
}

async fn seed_graph(db: &PgPool, user_id: Uuid, slug: &str, wpc: i32) {
    let skill_id: Uuid = sqlx::query_scalar("SELECT id FROM skill_nodes WHERE slug = $1")
        .bind(slug)
        .fetch_one(db)
        .await
        .expect("skill_id");
    sqlx::query(
        "INSERT INTO user_skills
            (user_id, skill_id, proven_count, weighted_proven_count,
             proficiency_level, first_proven_at, last_proven_at)
         VALUES ($1, $2, 1, $3, 1, NOW(), NOW())
         ON CONFLICT (user_id, skill_id) DO UPDATE SET
             weighted_proven_count = user_skills.weighted_proven_count + $3",
    )
    .bind(user_id)
    .bind(skill_id)
    .bind(wpc)
    .execute(db)
    .await
    .expect("user_skills");
}

#[tokio::test]
async fn domain_total_legacy_only() {
    let (db, name) = setup_test_db().await;
    let uid = insert_user(&db).await;

    seed_legacy(&db, uid, "code", "python", 42).await;

    let t = compute_domain_total(&db, uid, "code").await;
    assert_eq!(t, 42);

    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn domain_total_graph_only() {
    let (db, name) = setup_test_db().await;
    let uid = insert_user(&db).await;

    seed_graph(&db, uid, "python", 17).await;

    let t = compute_domain_total(&db, uid, "code").await;
    assert_eq!(t, 17);

    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn domain_total_uses_max_when_both_present() {
    let (db, name) = setup_test_db().await;
    let uid = insert_user(&db).await;

    // Legacy = 30 ; graph = 50 → MAX = 50 (pas 80 — évite double-comptage)
    seed_legacy(&db, uid, "code", "python", 30).await;
    seed_graph(&db, uid, "python", 50).await;

    let t = compute_domain_total(&db, uid, "code").await;
    assert_eq!(t, 50, "max({}, {}) attendu, pas la somme", 30, 50);

    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn seed_from_db_query_deduplicates_domains() {
    let (db, name) = setup_test_db().await;
    let uid = insert_user(&db).await;

    seed_legacy(&db, uid, "code", "python", 30).await;
    seed_graph(&db, uid, "python", 50).await;

    let rows: Vec<(Uuid, String, i64)> = sqlx::query_as(
        r#"
        SELECT user_id, domain, MAX(total)::BIGINT as total
        FROM (
            SELECT sf.user_id, sf.skill_domain AS domain,
                   SUM(sf.fragments)::BIGINT AS total
            FROM skill_fragments sf
            JOIN users u ON u.id = sf.user_id
            WHERE u.profile_active = TRUE AND u.is_banned = FALSE
            GROUP BY sf.user_id, sf.skill_domain
            UNION ALL
            SELECT us.user_id, sn.domain,
                   SUM(us.weighted_proven_count)::BIGINT AS total
            FROM user_skills us
            JOIN skill_nodes sn ON sn.id = us.skill_id
            JOIN users u ON u.id = us.user_id
            WHERE u.profile_active = TRUE AND u.is_banned = FALSE
            GROUP BY us.user_id, sn.domain
        ) merged
        GROUP BY user_id, domain
        HAVING MAX(total) > 0
        "#,
    )
    .fetch_all(&db)
    .await
    .expect("agg");

    let code_row = rows
        .iter()
        .find(|(u, d, _)| *u == uid && d == "code")
        .expect("row for code");
    assert_eq!(code_row.2, 50, "MAX(30, 50) == 50");

    let count_for_code = rows.iter().filter(|(u, d, _)| *u == uid && d == "code").count();
    assert_eq!(count_for_code, 1, "un seul row par (user, domain)");

    db.close().await;
    cleanup_test_db(&name).await;
}
