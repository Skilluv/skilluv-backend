//! Tests d'intégration P15.3 : marketplace de team slots + notif skill-match.

use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

use skilluv_backend::services::TeamRolesService;

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p15_3_test_{}",
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

async fn create_skill(db: &PgPool, slug: &str, domain: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO skill_nodes (slug, display_name, domain) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(slug)
    .bind(slug)
    .bind(domain)
    .fetch_one(db)
    .await
    .expect("skill")
}

async fn create_challenge_and_team(db: &PgPool, creator: Uuid) -> (Uuid, Uuid) {
    let cid: Uuid = sqlx::query_scalar(
        "INSERT INTO challenge_templates
            (title, description, instructions, skill_domain, difficulty, is_training, status)
         VALUES ('Game Jam', 'D', 'I', 'game', 3, TRUE, 'published') RETURNING id",
    )
    .fetch_one(db)
    .await
    .expect("ch");
    let tid: Uuid = sqlx::query_scalar(
        "INSERT INTO challenge_teams (challenge_id, name, created_by, max_members)
         VALUES ($1, 'Alpha', $2, 4) RETURNING id",
    )
    .bind(cid)
    .bind(creator)
    .fetch_one(db)
    .await
    .expect("t");
    sqlx::query("INSERT INTO team_members (team_id, user_id) VALUES ($1, $2)")
        .bind(tid)
        .bind(creator)
        .execute(db)
        .await
        .expect("mem");
    (cid, tid)
}

// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn marketplace_lists_only_open_slots_enriched() {
    let (db, name) = setup_test_db().await;
    let u = create_user(&db).await;
    let (_cid, tid) = create_challenge_and_team(&db, u).await;
    let skill = create_skill(&db, "p15-godot", "game").await;

    // 1 open, 1 filled
    let open = TeamRolesService::create_slot(
        &db,
        skilluv_backend::services::CreateSlotParams {
            team_id: tid,
            role_slug: "musician",
            role_display_name: Some("Musician"),
            required_skill_slug: None,
            min_proficiency_level: 1,
        },
    )
    .await
    .expect("open slot");

    let filled = TeamRolesService::create_slot(
        &db,
        skilluv_backend::services::CreateSlotParams {
            team_id: tid,
            role_slug: "coder",
            role_display_name: Some("Coder"),
            required_skill_slug: Some("p15-godot"),
            min_proficiency_level: 1,
        },
    )
    .await
    .expect("coder slot");
    // Fill it
    sqlx::query(
        "UPDATE user_skills SET proficiency_level = 3 WHERE user_id = $1 AND skill_id = $2",
    )
    .bind(u)
    .bind(skill)
    .execute(&db)
    .await
    .ok();
    // Ensure user_skills row exists so fill_slot passes
    sqlx::query(
        "INSERT INTO user_skills (user_id, skill_id, proficiency_level) VALUES ($1, $2, 3)
         ON CONFLICT DO NOTHING",
    )
    .bind(u)
    .bind(skill)
    .execute(&db)
    .await
    .expect("us");
    TeamRolesService::fill_slot(&db, filled.id, u)
        .await
        .expect("fill");

    let slots = TeamRolesService::marketplace_open_slots(&db, None, None, 50)
        .await
        .expect("list");
    assert_eq!(slots.len(), 1, "only the open slot is returned");
    assert_eq!(slots[0].slot_id, open.id);
    assert_eq!(slots[0].team_name, "Alpha");
    assert_eq!(slots[0].challenge_title, "Game Jam");

    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn marketplace_filters_by_role_and_skill() {
    let (db, name) = setup_test_db().await;
    let u = create_user(&db).await;
    let (_c, tid) = create_challenge_and_team(&db, u).await;
    create_skill(&db, "p15-figma", "design").await;
    create_skill(&db, "p15-rust", "code").await;

    TeamRolesService::create_slot(
        &db,
        skilluv_backend::services::CreateSlotParams {
            team_id: tid,
            role_slug: "designer",
            role_display_name: None,
            required_skill_slug: Some("p15-figma"),
            min_proficiency_level: 2,
        },
    )
    .await
    .expect("s1");
    TeamRolesService::create_slot(
        &db,
        skilluv_backend::services::CreateSlotParams {
            team_id: tid,
            role_slug: "coder",
            role_display_name: None,
            required_skill_slug: Some("p15-rust"),
            min_proficiency_level: 2,
        },
    )
    .await
    .expect("s2");

    let by_role = TeamRolesService::marketplace_open_slots(&db, Some("designer"), None, 50)
        .await
        .expect("role");
    assert_eq!(by_role.len(), 1);
    assert_eq!(by_role[0].role_slug, "designer");

    let by_skill = TeamRolesService::marketplace_open_slots(&db, None, Some("p15-rust"), 50)
        .await
        .expect("skill");
    assert_eq!(by_skill.len(), 1);
    assert_eq!(by_skill[0].required_skill_slug.as_deref(), Some("p15-rust"));

    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn notify_eligible_users_inserts_notifications_for_skill_matched_only() {
    let (db, name) = setup_test_db().await;
    let creator = create_user(&db).await;
    let (_c, tid) = create_challenge_and_team(&db, creator).await;
    let skill = create_skill(&db, "p15-godot", "game").await;

    let matched_1 = create_user(&db).await;
    let matched_2 = create_user(&db).await;
    let too_low = create_user(&db).await;
    let unrelated = create_user(&db).await;
    let _ = unrelated;
    for (uid, lvl) in [(matched_1, 3), (matched_2, 4), (too_low, 1)] {
        sqlx::query(
            "INSERT INTO user_skills (user_id, skill_id, proficiency_level) VALUES ($1, $2, $3)",
        )
        .bind(uid)
        .bind(skill)
        .bind(lvl as i16)
        .execute(&db)
        .await
        .expect("us");
    }

    let slot = TeamRolesService::create_slot(
        &db,
        skilluv_backend::services::CreateSlotParams {
            team_id: tid,
            role_slug: "coder",
            role_display_name: Some("Godot Coder"),
            required_skill_slug: Some("p15-godot"),
            min_proficiency_level: 3,
        },
    )
    .await
    .expect("slot");

    let n = TeamRolesService::notify_eligible_users_for_slot(&db, slot.id)
        .await
        .expect("notify");
    assert_eq!(n, 2, "matched_1 + matched_2 (too_low excluded)");

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM notifications WHERE notification_type = 'team_slot_open'",
    )
    .fetch_one(&db)
    .await
    .expect("cnt");
    assert_eq!(count, 2);

    // too_low doit ne pas être notifié
    let n_low: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM notifications WHERE user_id = $1")
        .bind(too_low)
        .fetch_one(&db)
        .await
        .expect("n_low");
    assert_eq!(n_low, 0);

    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn notify_does_nothing_when_no_required_skill() {
    let (db, name) = setup_test_db().await;
    let u = create_user(&db).await;
    let (_c, tid) = create_challenge_and_team(&db, u).await;

    let slot = TeamRolesService::create_slot(
        &db,
        skilluv_backend::services::CreateSlotParams {
            team_id: tid,
            role_slug: "any",
            role_display_name: None,
            required_skill_slug: None,
            min_proficiency_level: 1,
        },
    )
    .await
    .expect("slot");

    let n = TeamRolesService::notify_eligible_users_for_slot(&db, slot.id)
        .await
        .expect("notify");
    assert_eq!(n, 0, "no skill filter → no broadcast (avoid spam)");
    db.close().await;
    cleanup_test_db(&name).await;
}
