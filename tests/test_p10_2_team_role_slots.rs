//! Tests d'intégration P10.2 : team_role_slots multidisciplinaires.
//!
//! Couvre :
//! - Création + listing de slots sur une team persistente.
//! - fill_slot : validation skill_id prérequis + auto-join team_members.
//! - fill_slot : refus si slot déjà pris.
//! - fill_slot : refus si user ne match pas le level requis.
//! - leave_slot : libère le slot mais garde la membership team.
//! - delete_slot : refuse si slot rempli, marche si vide.
//! - UNIQUE partial : un user ne peut pas prendre 2 slots dans la même team.
//! - Marketplace : find_open_slots_by_role.

use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

use skilluv_backend::services::{CreateSlotParams, TeamRolesService};

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p10_2_test_{}",
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

async fn insert_persistent_team(db: &PgPool, founder: Uuid) -> Uuid {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO challenge_teams
            (challenge_id, name, created_by, max_members, is_persistent, status)
         VALUES (NULL, $1, $2, 6, TRUE, 'open')
         RETURNING id",
    )
    .bind(format!("team-{}", &founder.to_string()[..8]))
    .bind(founder)
    .fetch_one(db)
    .await
    .expect("insert team");
    sqlx::query("INSERT INTO team_members (team_id, user_id) VALUES ($1, $2)")
        .bind(id)
        .bind(founder)
        .execute(db)
        .await
        .expect("add founder");
    id
}

async fn seed_user_skill(db: &PgPool, user_id: Uuid, skill_slug: &str, level: i16) {
    let skill_id: Uuid = sqlx::query_scalar("SELECT id FROM skill_nodes WHERE slug = $1")
        .bind(skill_slug)
        .fetch_one(db)
        .await
        .expect("skill_id");
    sqlx::query(
        "INSERT INTO user_skills
            (user_id, skill_id, proven_count, weighted_proven_count, proficiency_level,
             first_proven_at, last_proven_at)
         VALUES ($1, $2, 1, 10, $3, NOW(), NOW())",
    )
    .bind(user_id)
    .bind(skill_id)
    .bind(level)
    .execute(db)
    .await
    .expect("insert user_skills");
}

// ═══════════════════════════════════════════════════════════════════
// Création + listing
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn create_and_list_slots_on_a_team() {
    let (db, name) = setup_test_db().await;
    let founder = insert_user(&db).await;
    let team_id = insert_persistent_team(&db, founder).await;

    TeamRolesService::create_slot(
        &db,
        CreateSlotParams {
            team_id,
            role_slug: "musician",
            role_display_name: Some("Musicien"),
            required_skill_slug: None,
            min_proficiency_level: 1,
        },
    )
    .await
    .expect("create musician slot");

    TeamRolesService::create_slot(
        &db,
        CreateSlotParams {
            team_id,
            role_slug: "coder",
            role_display_name: Some("Coder Godot"),
            required_skill_slug: Some("rust"),
            min_proficiency_level: 2,
        },
    )
    .await
    .expect("create coder slot");

    let slots = TeamRolesService::list_slots(&db, team_id).await.expect("list");
    assert_eq!(slots.len(), 2);
    assert!(slots.iter().any(|s| s.role_slug == "musician"));
    assert!(slots.iter().any(|s| s.role_slug == "coder" && s.required_skill_id.is_some()));

    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn create_slot_with_unknown_skill_slug_fails() {
    let (db, name) = setup_test_db().await;
    let founder = insert_user(&db).await;
    let team_id = insert_persistent_team(&db, founder).await;

    let res = TeamRolesService::create_slot(
        &db,
        CreateSlotParams {
            team_id,
            role_slug: "wizard",
            role_display_name: None,
            required_skill_slug: Some("necromancy-blueprint-9000"),
            min_proficiency_level: 3,
        },
    )
    .await;
    assert!(res.is_err(), "unknown skill_slug doit être rejeté");

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// fill_slot : succès + validation skill
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn fill_slot_succeeds_when_user_meets_skill_requirement() {
    let (db, name) = setup_test_db().await;
    let founder = insert_user(&db).await;
    let team_id = insert_persistent_team(&db, founder).await;
    let joiner = insert_user(&db).await;
    seed_user_skill(&db, joiner, "rust", 3).await;

    let slot = TeamRolesService::create_slot(
        &db,
        CreateSlotParams {
            team_id,
            role_slug: "coder",
            role_display_name: None,
            required_skill_slug: Some("rust"),
            min_proficiency_level: 2,
        },
    )
    .await
    .expect("slot");

    let filled = TeamRolesService::fill_slot(&db, slot.id, joiner)
        .await
        .expect("fill");
    assert_eq!(filled.filled_by_user_id, Some(joiner));
    assert!(filled.filled_at.is_some());

    // Auto-join : joiner est maintenant dans team_members
    let is_member: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM team_members WHERE team_id = $1 AND user_id = $2)",
    )
    .bind(team_id)
    .bind(joiner)
    .fetch_one(&db)
    .await
    .expect("check member");
    assert!(is_member, "joiner doit être auto-ajouté à team_members");

    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn fill_slot_rejects_user_below_required_level() {
    let (db, name) = setup_test_db().await;
    let founder = insert_user(&db).await;
    let team_id = insert_persistent_team(&db, founder).await;
    let joiner = insert_user(&db).await;
    seed_user_skill(&db, joiner, "rust", 1).await;

    let slot = TeamRolesService::create_slot(
        &db,
        CreateSlotParams {
            team_id,
            role_slug: "coder",
            role_display_name: None,
            required_skill_slug: Some("rust"),
            min_proficiency_level: 3,
        },
    )
    .await
    .expect("slot");

    let res = TeamRolesService::fill_slot(&db, slot.id, joiner).await;
    assert!(res.is_err(), "user niveau 1 doit être refusé sur slot niveau 3");

    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn fill_slot_rejects_user_with_no_skill_record() {
    let (db, name) = setup_test_db().await;
    let founder = insert_user(&db).await;
    let team_id = insert_persistent_team(&db, founder).await;
    let joiner = insert_user(&db).await;
    // Pas de seed_user_skill → user n'a pas la skill du tout

    let slot = TeamRolesService::create_slot(
        &db,
        CreateSlotParams {
            team_id,
            role_slug: "coder",
            role_display_name: None,
            required_skill_slug: Some("python"),
            min_proficiency_level: 1,
        },
    )
    .await
    .expect("slot");

    let res = TeamRolesService::fill_slot(&db, slot.id, joiner).await;
    assert!(res.is_err(), "user sans skill doit être refusé");

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// fill_slot : slot déjà pris + double-slot par user
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn fill_slot_rejects_when_already_filled() {
    let (db, name) = setup_test_db().await;
    let founder = insert_user(&db).await;
    let team_id = insert_persistent_team(&db, founder).await;
    let a = insert_user(&db).await;
    let b = insert_user(&db).await;

    let slot = TeamRolesService::create_slot(
        &db,
        CreateSlotParams {
            team_id,
            role_slug: "musician",
            role_display_name: None,
            required_skill_slug: None,
            min_proficiency_level: 1,
        },
    )
    .await
    .expect("slot");

    TeamRolesService::fill_slot(&db, slot.id, a).await.expect("a fills");
    let res = TeamRolesService::fill_slot(&db, slot.id, b).await;
    assert!(res.is_err(), "b ne peut pas prendre un slot déjà rempli par a");

    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn user_cannot_hold_two_slots_in_same_team() {
    let (db, name) = setup_test_db().await;
    let founder = insert_user(&db).await;
    let team_id = insert_persistent_team(&db, founder).await;
    let joiner = insert_user(&db).await;

    let slot_a = TeamRolesService::create_slot(
        &db,
        CreateSlotParams {
            team_id,
            role_slug: "musician",
            role_display_name: None,
            required_skill_slug: None,
            min_proficiency_level: 1,
        },
    )
    .await
    .expect("a");
    let slot_b = TeamRolesService::create_slot(
        &db,
        CreateSlotParams {
            team_id,
            role_slug: "designer",
            role_display_name: None,
            required_skill_slug: None,
            min_proficiency_level: 1,
        },
    )
    .await
    .expect("b");

    TeamRolesService::fill_slot(&db, slot_a.id, joiner).await.expect("first");
    let res = TeamRolesService::fill_slot(&db, slot_b.id, joiner).await;
    assert!(res.is_err(), "UNIQUE partial doit empêcher double-slot par user dans la même team");

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// leave_slot + delete_slot
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn leave_slot_releases_but_keeps_team_membership() {
    let (db, name) = setup_test_db().await;
    let founder = insert_user(&db).await;
    let team_id = insert_persistent_team(&db, founder).await;
    let joiner = insert_user(&db).await;

    let slot = TeamRolesService::create_slot(
        &db,
        CreateSlotParams {
            team_id,
            role_slug: "musician",
            role_display_name: None,
            required_skill_slug: None,
            min_proficiency_level: 1,
        },
    )
    .await
    .expect("slot");
    TeamRolesService::fill_slot(&db, slot.id, joiner).await.expect("fill");
    let after = TeamRolesService::leave_slot(&db, slot.id, joiner)
        .await
        .expect("leave");
    assert!(after.filled_by_user_id.is_none());

    // Toujours membre de la team (leave_slot ≠ leave_team)
    let is_member: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM team_members WHERE team_id = $1 AND user_id = $2)",
    )
    .bind(team_id)
    .bind(joiner)
    .fetch_one(&db)
    .await
    .expect("check");
    assert!(is_member);

    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn delete_slot_refuses_when_filled() {
    let (db, name) = setup_test_db().await;
    let founder = insert_user(&db).await;
    let team_id = insert_persistent_team(&db, founder).await;
    let joiner = insert_user(&db).await;

    let slot = TeamRolesService::create_slot(
        &db,
        CreateSlotParams {
            team_id,
            role_slug: "musician",
            role_display_name: None,
            required_skill_slug: None,
            min_proficiency_level: 1,
        },
    )
    .await
    .expect("slot");
    TeamRolesService::fill_slot(&db, slot.id, joiner).await.expect("fill");
    let res = TeamRolesService::delete_slot(&db, slot.id).await;
    assert!(res.is_err(), "delete refusé si slot rempli");

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Marketplace : find_open_slots_by_role
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn marketplace_finds_open_slots_by_role() {
    let (db, name) = setup_test_db().await;
    let founder_a = insert_user(&db).await;
    let team_a = insert_persistent_team(&db, founder_a).await;
    let founder_b = insert_user(&db).await;
    let team_b = insert_persistent_team(&db, founder_b).await;

    // Team A cherche musicien + coder (rempli)
    TeamRolesService::create_slot(&db, CreateSlotParams { team_id: team_a, role_slug: "musician", role_display_name: None, required_skill_slug: None, min_proficiency_level: 1 }).await.expect("a1");
    let a_coder = TeamRolesService::create_slot(&db, CreateSlotParams { team_id: team_a, role_slug: "coder", role_display_name: None, required_skill_slug: None, min_proficiency_level: 1 }).await.expect("a2");
    TeamRolesService::fill_slot(&db, a_coder.id, founder_a).await.expect("fill a_coder");

    // Team B cherche musicien
    TeamRolesService::create_slot(&db, CreateSlotParams { team_id: team_b, role_slug: "musician", role_display_name: None, required_skill_slug: None, min_proficiency_level: 1 }).await.expect("b1");

    let musicians = TeamRolesService::find_open_slots_by_role(&db, "musician", 10).await.expect("musicians");
    assert_eq!(musicians.len(), 2, "les 2 teams cherchent musicien");
    assert!(musicians.iter().all(|s| s.filled_by_user_id.is_none()));

    let coders = TeamRolesService::find_open_slots_by_role(&db, "coder", 10).await.expect("coders");
    assert_eq!(coders.len(), 0, "team A a rempli son coder");

    db.close().await;
    cleanup_test_db(&name).await;
}
