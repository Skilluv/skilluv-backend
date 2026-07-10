//! Tests d'intégration P8.6c (post-P8.7) : aggrégations leaderboard sur
//! `user_skills` + `skill_nodes` (source unique après drop de `skill_fragments`).

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

/// Mirror update_score's domain aggregation (P8.7 : user_skills seul).
async fn compute_domain_total(db: &PgPool, user_id: Uuid, domain: &str) -> i64 {
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
    graph.unwrap_or(0)
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
async fn domain_total_sums_wpc_within_domain() {
    let (db, name) = setup_test_db().await;
    let uid = insert_user(&db).await;

    seed_graph(&db, uid, "python", 20).await;
    seed_graph(&db, uid, "rust", 15).await; // même domaine 'code'
    seed_graph(&db, uid, "figma-craft", 100).await; // domaine 'design'

    assert_eq!(compute_domain_total(&db, uid, "code").await, 35);
    assert_eq!(compute_domain_total(&db, uid, "design").await, 100);
    assert_eq!(compute_domain_total(&db, uid, "game").await, 0);

    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn seed_from_db_query_aggregates_by_domain() {
    let (db, name) = setup_test_db().await;
    let uid = insert_user(&db).await;

    seed_graph(&db, uid, "python", 20).await;
    seed_graph(&db, uid, "rust", 15).await;
    seed_graph(&db, uid, "figma-craft", 100).await;

    let rows: Vec<(Uuid, String, i64)> = sqlx::query_as(
        r#"
        SELECT us.user_id, sn.domain,
               SUM(us.weighted_proven_count)::BIGINT AS total
        FROM user_skills us
        JOIN skill_nodes sn ON sn.id = us.skill_id
        JOIN users u ON u.id = us.user_id
        WHERE u.profile_active = TRUE AND u.is_banned = FALSE
        GROUP BY us.user_id, sn.domain
        HAVING SUM(us.weighted_proven_count) > 0
        "#,
    )
    .fetch_all(&db)
    .await
    .expect("agg");

    let code = rows.iter().find(|(u, d, _)| *u == uid && d == "code").unwrap();
    assert_eq!(code.2, 35);
    let design = rows.iter().find(|(u, d, _)| *u == uid && d == "design").unwrap();
    assert_eq!(design.2, 100);
    let count = rows.iter().filter(|(u, _, _)| *u == uid).count();
    assert_eq!(count, 2, "un row par (user, domain)");

    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn banned_users_excluded_from_seed() {
    let (db, name) = setup_test_db().await;
    let uid = insert_user(&db).await;

    seed_graph(&db, uid, "python", 20).await;
    sqlx::query("UPDATE users SET is_banned = TRUE WHERE id = $1")
        .bind(uid)
        .execute(&db)
        .await
        .expect("ban");

    let rows: Vec<(Uuid, String, i64)> = sqlx::query_as(
        r#"
        SELECT us.user_id, sn.domain,
               SUM(us.weighted_proven_count)::BIGINT AS total
        FROM user_skills us
        JOIN skill_nodes sn ON sn.id = us.skill_id
        JOIN users u ON u.id = us.user_id
        WHERE u.profile_active = TRUE AND u.is_banned = FALSE
        GROUP BY us.user_id, sn.domain
        HAVING SUM(us.weighted_proven_count) > 0
        "#,
    )
    .fetch_all(&db)
    .await
    .expect("agg");

    assert!(rows.iter().all(|(u, _, _)| *u != uid));

    db.close().await;
    cleanup_test_db(&name).await;
}
