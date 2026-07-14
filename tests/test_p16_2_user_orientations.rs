//! Tests P16.2 : user_orientations + backfill + contraintes.

use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p16_2_test_{}",
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

async fn create_user_with_domain(db: &PgPool, domain: Option<&str>) -> Uuid {
    let uid = Uuid::new_v4();
    let short = &uid.to_string()[..8];
    sqlx::query(
        "INSERT INTO users
            (id, email, username, first_name, last_name, display_name, password_hash,
             profile_active, total_fragments, skill_domain)
         VALUES ($1, $2, $3, 'T', 'U', 'T', 'x', TRUE, 0, $4)",
    )
    .bind(uid)
    .bind(format!("t-{uid}@ex.io"))
    .bind(format!("t{short}"))
    .bind(domain)
    .execute(db)
    .await
    .expect("u");
    uid
}

async fn orientation_id(db: &PgPool, slug: &str) -> Uuid {
    sqlx::query_scalar("SELECT id FROM orientations WHERE slug = $1")
        .bind(slug)
        .fetch_one(db)
        .await
        .expect("o")
}

#[tokio::test]
async fn only_one_primary_orientation_per_user_when_active() {
    let (db, name) = setup_test_db().await;
    let u = create_user_with_domain(&db, None).await;
    let a = orientation_id(&db, "dev-frontend").await;
    let b = orientation_id(&db, "dev-backend").await;

    sqlx::query(
        "INSERT INTO user_orientations (user_id, orientation_id, is_primary) VALUES ($1, $2, TRUE)",
    )
    .bind(u).bind(a).execute(&db).await.expect("first");

    let dup = sqlx::query(
        "INSERT INTO user_orientations (user_id, orientation_id, is_primary) VALUES ($1, $2, TRUE)",
    )
    .bind(u).bind(b).execute(&db).await;
    assert!(dup.is_err(), "two primary orientations rejected");

    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn ended_primary_frees_slot_for_new_primary() {
    let (db, name) = setup_test_db().await;
    let u = create_user_with_domain(&db, None).await;
    let a = orientation_id(&db, "dev-frontend").await;
    let b = orientation_id(&db, "pentester-web").await;

    sqlx::query(
        "INSERT INTO user_orientations (user_id, orientation_id, is_primary, ended_at)
         VALUES ($1, $2, TRUE, NOW())",
    )
    .bind(u).bind(a).execute(&db).await.expect("historical");

    let ok = sqlx::query(
        "INSERT INTO user_orientations (user_id, orientation_id, is_primary) VALUES ($1, $2, TRUE)",
    )
    .bind(u).bind(b).execute(&db).await;
    assert!(ok.is_ok(), "new primary allowed once previous is ended");

    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn ended_before_start_rejected() {
    let (db, name) = setup_test_db().await;
    let u = create_user_with_domain(&db, None).await;
    let a = orientation_id(&db, "dev-frontend").await;

    let bad = sqlx::query(
        "INSERT INTO user_orientations (user_id, orientation_id, started_at, ended_at)
         VALUES ($1, $2, NOW(), NOW() - INTERVAL '1 day')",
    )
    .bind(u).bind(a).execute(&db).await;
    assert!(bad.is_err(), "ended_at < started_at must be rejected");

    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn backfill_assigns_mapped_orientation_from_skill_domain() {
    let (db, name) = setup_test_db().await;
    // Simule un user créé APRÈS la migration en insérant manuellement.
    // Le backfill de la migration ne joue que sur ce qui existait au moment
    // où elle a tourné — ici on vérifie qu'un user pré-existant serait pris.
    // On simule ça en insérant user + skill_domain, puis en ré-appliquant
    // l'INSERT ON CONFLICT DO NOTHING (idempotent).
    let u_code   = create_user_with_domain(&db, Some("code")).await;
    let u_design = create_user_with_domain(&db, Some("design")).await;
    let u_game   = create_user_with_domain(&db, Some("game")).await;
    let u_sec    = create_user_with_domain(&db, Some("security")).await;
    let _u_none  = create_user_with_domain(&db, None).await;

    sqlx::query(
        "INSERT INTO user_orientations (user_id, orientation_id, mode, is_primary, started_at)
         SELECT u.id, o.id, 'learning', TRUE, NOW()
         FROM users u
         JOIN orientations o ON o.slug = CASE u.skill_domain
             WHEN 'code'     THEN 'dev-fullstack'
             WHEN 'design'   THEN 'web-designer'
             WHEN 'game'     THEN 'game-programmer'
             WHEN 'security' THEN 'pentester-web'
         END
         WHERE u.skill_domain IN ('code','design','game','security')
           AND NOT EXISTS (
             SELECT 1 FROM user_orientations uo WHERE uo.user_id = u.id
           )
         ON CONFLICT DO NOTHING",
    )
    .execute(&db)
    .await
    .expect("backfill");

    let mapping: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT uo.user_id, o.slug
         FROM user_orientations uo JOIN orientations o ON o.id = uo.orientation_id
         WHERE uo.user_id = ANY($1)
         ORDER BY o.slug",
    )
    .bind(&vec![u_code, u_design, u_game, u_sec])
    .fetch_all(&db)
    .await
    .expect("m");
    assert_eq!(mapping.len(), 4);
    let by_uid: std::collections::HashMap<_, _> = mapping.into_iter().collect();
    assert_eq!(by_uid[&u_code],   "dev-fullstack");
    assert_eq!(by_uid[&u_design], "web-designer");
    assert_eq!(by_uid[&u_game],   "game-programmer");
    assert_eq!(by_uid[&u_sec],    "pentester-web");

    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn user_can_have_multiple_orientations_but_only_one_primary() {
    let (db, name) = setup_test_db().await;
    let u = create_user_with_domain(&db, None).await;
    let a = orientation_id(&db, "dev-frontend").await;
    let b = orientation_id(&db, "web-designer").await;
    let c = orientation_id(&db, "pentester-web").await;

    sqlx::query(
        "INSERT INTO user_orientations (user_id, orientation_id, mode, is_primary)
         VALUES ($1, $2, 'active', TRUE), ($1, $3, 'active', FALSE), ($1, $4, 'learning', FALSE)",
    )
    .bind(u).bind(a).bind(b).bind(c)
    .execute(&db).await.expect("multi");

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_orientations WHERE user_id = $1 AND ended_at IS NULL",
    )
    .bind(u).fetch_one(&db).await.expect("cnt");
    assert_eq!(count, 3);

    let primaries: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_orientations
         WHERE user_id = $1 AND is_primary = TRUE AND ended_at IS NULL",
    )
    .bind(u).fetch_one(&db).await.expect("prim");
    assert_eq!(primaries, 1);

    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn mode_check_rejects_invalid_value() {
    let (db, name) = setup_test_db().await;
    let u = create_user_with_domain(&db, None).await;
    let a = orientation_id(&db, "dev-frontend").await;

    let bad = sqlx::query(
        "INSERT INTO user_orientations (user_id, orientation_id, mode) VALUES ($1, $2, 'expert')",
    )
    .bind(u).bind(a).execute(&db).await;
    assert!(bad.is_err(), "mode='expert' rejected");
    db.close().await;
    cleanup_test_db(&name).await;
}
