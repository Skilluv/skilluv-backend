//! Tests d'intégration P10.5 : bridge Guild ↔ Team.
//!
//! Vérifie que :
//! - `challenge_teams.guild_id` est FK vers guilds, nullable.
//! - `award_bonus_gp_for_team` abonde bien `gp_total` + `gp_season`.
//! - Une guilde disbanded ne reçoit rien.
//! - award_bonus_gp_for_team retourne 0 sur fragments <= 0.

use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

use skilluv_backend::services::guild::award_bonus_gp_for_team;

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p10_5_test_{}",
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

async fn insert_guild(db: &PgPool, founder: Uuid, tag: &str) -> Uuid {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO guilds (slug, tag, name, founder_id)
         VALUES ($1, $2, $3, $4)
         RETURNING id",
    )
    .bind(format!("g-{}", &Uuid::new_v4().to_string()[..8]))
    .bind(tag)
    .bind(format!("Guild {tag}"))
    .bind(founder)
    .fetch_one(db)
    .await
    .expect("insert guild");
    sqlx::query("INSERT INTO guild_members (guild_id, user_id, role) VALUES ($1, $2, 'founder')")
        .bind(id)
        .bind(founder)
        .execute(db)
        .await
        .expect("add founder");
    id
}

async fn insert_persistent_team(db: &PgPool, founder: Uuid, guild_id: Option<Uuid>) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO challenge_teams
            (challenge_id, name, created_by, max_members, is_persistent, status, guild_id)
         VALUES (NULL, $1, $2, 4, TRUE, 'open', $3)
         RETURNING id",
    )
    .bind(format!("team-{}", &Uuid::new_v4().to_string()[..8]))
    .bind(founder)
    .bind(guild_id)
    .fetch_one(db)
    .await
    .expect("insert team")
}

// ═══════════════════════════════════════════════════════════════════
// Schéma : team peut référencer une guilde
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn team_can_reference_a_guild() {
    let (db, name) = setup_test_db().await;
    let founder = insert_user(&db).await;
    let guild_id = insert_guild(&db, founder, "AAA").await;
    let team_id = insert_persistent_team(&db, founder, Some(guild_id)).await;

    let stored: Option<Uuid> =
        sqlx::query_scalar("SELECT guild_id FROM challenge_teams WHERE id = $1")
            .bind(team_id)
            .fetch_one(&db)
            .await
            .expect("fetch");
    assert_eq!(stored, Some(guild_id));

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// award_bonus_gp_for_team abonde bien gp_total + gp_season
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn bonus_gp_credits_guild_totals() {
    let (db, name) = setup_test_db().await;
    let founder = insert_user(&db).await;
    let guild_id = insert_guild(&db, founder, "BBB").await;

    let bonus = award_bonus_gp_for_team(&db, guild_id, 100)
        .await
        .expect("award");
    // 10% de 100 = 10 GP
    assert_eq!(bonus, 10);

    let (gp_total, gp_season): (i64, i64) =
        sqlx::query_as("SELECT gp_total, gp_season FROM guilds WHERE id = $1")
            .bind(guild_id)
            .fetch_one(&db)
            .await
            .expect("gp");
    assert_eq!(gp_total, 10);
    assert_eq!(gp_season, 10);

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// award_bonus_gp_for_team n'abonde rien à une guilde disbanded
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn bonus_gp_ignored_for_disbanded_guild() {
    let (db, name) = setup_test_db().await;
    let founder = insert_user(&db).await;
    let guild_id = insert_guild(&db, founder, "CCC").await;
    sqlx::query("UPDATE guilds SET disbanded_at = NOW() WHERE id = $1")
        .bind(guild_id)
        .execute(&db)
        .await
        .expect("disband");

    let _ = award_bonus_gp_for_team(&db, guild_id, 100)
        .await
        .expect("award noop");

    let gp_total: i64 = sqlx::query_scalar("SELECT gp_total FROM guilds WHERE id = $1")
        .bind(guild_id)
        .fetch_one(&db)
        .await
        .expect("gp");
    assert_eq!(gp_total, 0, "disbanded guild ne reçoit rien");

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Guarde : fragments <= 0 → no-op
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn bonus_gp_noop_when_fragments_non_positive() {
    let (db, name) = setup_test_db().await;
    let founder = insert_user(&db).await;
    let guild_id = insert_guild(&db, founder, "DDD").await;

    assert_eq!(
        award_bonus_gp_for_team(&db, guild_id, 0).await.expect("z"),
        0
    );
    assert_eq!(
        award_bonus_gp_for_team(&db, guild_id, -5).await.expect("n"),
        0
    );

    let gp: i64 = sqlx::query_scalar("SELECT gp_total FROM guilds WHERE id = $1")
        .bind(guild_id)
        .fetch_one(&db)
        .await
        .expect("gp");
    assert_eq!(gp, 0);

    db.close().await;
    cleanup_test_db(&name).await;
}
