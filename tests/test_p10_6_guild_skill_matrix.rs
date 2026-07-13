//! Tests d'intégration P10.6 : guild_skill_matrix agrège par domaine.
//!
//! Vérifie que `guild::guild_skill_matrix` :
//! - Agrège member_count + avg_level par domaine.
//! - top_skills contient au plus 3 slugs, ordonnés par popularité.
//! - Guilde vide → aucun row.
//! - N'inclut PAS les membres sans user_skills.

use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

use skilluv_backend::services::guild::guild_skill_matrix;

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p10_6_test_{}",
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
         VALUES ($1, $2, $3, 'T', 'U', 'Test', 'dummy', TRUE, 0)",
    )
    .bind(user_id)
    .bind(format!("test-{user_id}@example.com"))
    .bind(format!("t{short}"))
    .execute(db)
    .await
    .expect("insert user");
    user_id
}

async fn insert_guild_with_members(db: &PgPool, members: &[Uuid]) -> Uuid {
    let founder = members[0];
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO guilds (slug, tag, name, founder_id)
         VALUES ($1, $2, $3, $4)
         RETURNING id",
    )
    .bind(format!("g-{}", &Uuid::new_v4().to_string()[..8]))
    .bind(format!("T{}", &Uuid::new_v4().to_string()[..3].to_uppercase()))
    .bind("Composition Guild")
    .bind(founder)
    .fetch_one(db)
    .await
    .expect("insert guild");
    for (idx, m) in members.iter().enumerate() {
        let role = if idx == 0 { "founder" } else { "member" };
        sqlx::query("INSERT INTO guild_members (guild_id, user_id, role) VALUES ($1, $2, $3)")
            .bind(id)
            .bind(m)
            .bind(role)
            .execute(db)
            .await
            .expect("insert member");
    }
    id
}

async fn add_user_skill(db: &PgPool, user_id: Uuid, slug: &str, level: i16) {
    let skill_id: Uuid = sqlx::query_scalar("SELECT id FROM skill_nodes WHERE slug = $1")
        .bind(slug)
        .fetch_one(db)
        .await
        .expect("skill_id");
    sqlx::query(
        "INSERT INTO user_skills
            (user_id, skill_id, proven_count, weighted_proven_count,
             proficiency_level, first_proven_at, last_proven_at)
         VALUES ($1, $2, 3, 10, $3, NOW(), NOW())
         ON CONFLICT (user_id, skill_id) DO UPDATE SET proficiency_level = $3",
    )
    .bind(user_id)
    .bind(skill_id)
    .bind(level)
    .execute(db)
    .await
    .expect("insert user_skills");
}

// ═══════════════════════════════════════════════════════════════════
// Guilde vide → matrix vide
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn empty_guild_returns_empty_matrix() {
    let (db, name) = setup_test_db().await;
    let founder = insert_user(&db).await;
    let guild_id = insert_guild_with_members(&db, &[founder]).await;

    let matrix = guild_skill_matrix(&db, guild_id).await.expect("matrix");
    assert!(matrix.is_empty(), "founder sans user_skills → matrix vide");

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Agrégat par domaine avec plusieurs membres
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn matrix_aggregates_members_by_domain() {
    let (db, name) = setup_test_db().await;
    let a = insert_user(&db).await;
    let b = insert_user(&db).await;
    let c = insert_user(&db).await;
    let guild_id = insert_guild_with_members(&db, &[a, b, c]).await;

    // a : rust niveau 3 + python niveau 2 (code)
    add_user_skill(&db, a, "rust", 3).await;
    add_user_skill(&db, a, "python", 2).await;

    // b : rust niveau 4 + figma-craft niveau 2 (code + design)
    add_user_skill(&db, b, "rust", 4).await;
    add_user_skill(&db, b, "figma-craft", 2).await;

    // c : figma-craft niveau 3 + ux niveau 1 (design)
    add_user_skill(&db, c, "figma-craft", 3).await;
    add_user_skill(&db, c, "ux", 1).await;

    let matrix = guild_skill_matrix(&db, guild_id).await.expect("matrix");

    let code = matrix.iter().find(|r| r.domain == "code").expect("code row");
    assert_eq!(code.member_count, 2, "a + b sur code");
    // avg_level(code) = moyenne(3, 2, 4) = 3.0
    assert!((code.avg_level.unwrap() - 3.0).abs() < 0.01);
    assert!(code.top_skills.contains(&"rust".to_string()));

    let design = matrix.iter().find(|r| r.domain == "design").expect("design row");
    assert_eq!(design.member_count, 2, "b + c sur design");
    // top_skills capé à 3
    assert!(design.top_skills.len() <= 3);
    assert!(design.top_skills.contains(&"figma-craft".to_string()));

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// top_skills ordonné par nb de membres pratiquants
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn top_skills_ordered_by_popularity() {
    let (db, name) = setup_test_db().await;
    let a = insert_user(&db).await;
    let b = insert_user(&db).await;
    let c = insert_user(&db).await;
    let guild_id = insert_guild_with_members(&db, &[a, b, c]).await;

    // Tous les 3 pratiquent rust
    add_user_skill(&db, a, "rust", 1).await;
    add_user_skill(&db, b, "rust", 2).await;
    add_user_skill(&db, c, "rust", 3).await;
    // Seul a pratique python
    add_user_skill(&db, a, "python", 4).await;

    let matrix = guild_skill_matrix(&db, guild_id).await.expect("matrix");
    let code = matrix.iter().find(|r| r.domain == "code").expect("code");
    // rust en tête car pratiqué par 3, python par 1
    assert_eq!(code.top_skills[0], "rust");
    if code.top_skills.len() > 1 {
        assert_eq!(code.top_skills[1], "python");
    }

    db.close().await;
    cleanup_test_db(&name).await;
}
