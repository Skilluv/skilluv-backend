//! Tests P16.5 : playlist onboarding pour une orientation.

use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

use skilluv_backend::services::orientations_playlist;

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p16_5_test_{}",
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

async fn create_user(db: &PgPool) -> Uuid {
    let uid = Uuid::new_v4();
    let short = &uid.to_string()[..8];
    sqlx::query(
        "INSERT INTO users
            (id, email, username, first_name, last_name, display_name, password_hash,
             profile_active, total_fragments)
         VALUES ($1, $2, $3, 'T', 'U', 'T', 'x', TRUE, 0)",
    )
    .bind(uid)
    .bind(format!("t-{uid}@ex.io"))
    .bind(format!("t{short}"))
    .execute(db)
    .await
    .expect("u");
    uid
}

async fn insert_training_challenge(db: &PgPool, title: &str, domain: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO challenge_templates
            (title, description, instructions, skill_domain, difficulty,
             is_training, status)
         VALUES ($1, 'd', 'i', $2, 2, TRUE, 'published') RETURNING id",
    )
    .bind(title)
    .bind(domain)
    .fetch_one(db)
    .await
    .expect("ch")
}

#[tokio::test]
async fn playlist_unknown_orientation_returns_not_found() {
    let (db, name) = setup_test_db().await;
    let u = create_user(&db).await;
    let res = orientations_playlist::playlist_for(&db, u, "not-a-real-slug").await;
    assert!(res.is_err(), "unknown orientation must fail");
    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn playlist_returns_up_to_three_training_challenges_in_domain() {
    let (db, name) = setup_test_db().await;
    let u = create_user(&db).await;

    // dev-frontend a primary=code, secondary=[design]
    for i in 0..4 {
        insert_training_challenge(&db, &format!("Front #{i}"), "code").await;
    }
    // Un design doit aussi être capté (secondary_domain)
    insert_training_challenge(&db, "Design One", "design").await;
    // Un game NE doit PAS remonter (hors domaine)
    insert_training_challenge(&db, "Game X", "game").await;

    let pl = orientations_playlist::playlist_for(&db, u, "dev-frontend")
        .await
        .expect("playlist");
    assert_eq!(pl.training_challenges.len(), 3, "cap 3");
    for c in &pl.training_challenges {
        assert!(
            matches!(c.skill_domain.as_str(), "code" | "design"),
            "domain hors orientation: {}",
            c.skill_domain
        );
    }

    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn playlist_excludes_challenges_already_verified() {
    let (db, name) = setup_test_db().await;
    let u = create_user(&db).await;
    let done = insert_training_challenge(&db, "Done", "code").await;
    let a = insert_training_challenge(&db, "A", "code").await;

    sqlx::query(
        "INSERT INTO deliverables (challenge_id, user_id, artifact_type, artifact_url,
                                    verifiable_by, verification_status)
         VALUES ($1, $2, 'other', 'x', 'human_review', 'verified')",
    )
    .bind(done)
    .bind(u)
    .execute(&db)
    .await
    .expect("d");

    let pl = orientations_playlist::playlist_for(&db, u, "dev-frontend")
        .await
        .expect("pl");
    let ids: Vec<Uuid> = pl.training_challenges.iter().map(|c| c.id).collect();
    assert!(!ids.contains(&done), "verified challenge excluded");
    assert!(ids.contains(&a), "unverified challenge included");
    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn playlist_returns_open_slots_matching_core_skill() {
    let (db, name) = setup_test_db().await;
    let creator = create_user(&db).await;
    let seeker = create_user(&db).await;

    // Setup : dev-frontend + skill core react-hooks (existant dans le seed).
    let ori_id: Uuid =
        sqlx::query_scalar("SELECT id FROM orientations WHERE slug = 'dev-frontend'")
            .fetch_one(&db)
            .await
            .unwrap();
    let skill_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM skill_nodes WHERE slug = 'component-composition' LIMIT 1",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO orientation_skill_map (orientation_id, skill_id, is_core)
         VALUES ($1, $2, TRUE) ON CONFLICT DO NOTHING",
    )
    .bind(ori_id)
    .bind(skill_id)
    .execute(&db)
    .await
    .unwrap();

    // Team d'un autre user + slot exigeant ce skill.
    let cid: Uuid = sqlx::query_scalar(
        "INSERT INTO challenge_templates (title, description, instructions, skill_domain,
             difficulty, is_training, status)
         VALUES ('T', 'D', 'I', 'code', 3, TRUE, 'published') RETURNING id",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    let tid: Uuid = sqlx::query_scalar(
        "INSERT INTO challenge_teams (challenge_id, name, created_by, max_members)
         VALUES ($1, 'Alpha', $2, 4) RETURNING id",
    )
    .bind(cid)
    .bind(creator)
    .fetch_one(&db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO team_role_slots (team_id, role_slug, role_display_name,
                                       required_skill_id, min_proficiency_level)
         VALUES ($1, 'coder', 'Frontend Coder', $2, 2)",
    )
    .bind(tid)
    .bind(skill_id)
    .execute(&db)
    .await
    .unwrap();

    let pl = orientations_playlist::playlist_for(&db, seeker, "dev-frontend")
        .await
        .expect("pl");
    assert_eq!(pl.open_team_slots.len(), 1, "1 slot matches");
    assert_eq!(pl.open_team_slots[0].role_slug, "coder");
    assert_eq!(pl.open_team_slots[0].team_name, "Alpha");
    assert_eq!(pl.open_team_slots[0].skill_slug, "component-composition");

    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn playlist_excludes_slots_from_own_team() {
    let (db, name) = setup_test_db().await;
    let me = create_user(&db).await;

    let ori_id: Uuid =
        sqlx::query_scalar("SELECT id FROM orientations WHERE slug = 'dev-frontend'")
            .fetch_one(&db)
            .await
            .unwrap();
    let skill_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM skill_nodes WHERE slug = 'component-composition' LIMIT 1",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO orientation_skill_map (orientation_id, skill_id, is_core)
         VALUES ($1, $2, TRUE)",
    )
    .bind(ori_id)
    .bind(skill_id)
    .execute(&db)
    .await
    .unwrap();

    let cid: Uuid = sqlx::query_scalar(
        "INSERT INTO challenge_templates (title, description, instructions, skill_domain,
             difficulty, is_training, status)
         VALUES ('T', 'D', 'I', 'code', 3, TRUE, 'published') RETURNING id",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    let tid: Uuid = sqlx::query_scalar(
        "INSERT INTO challenge_teams (challenge_id, name, created_by, max_members)
         VALUES ($1, 'Mine', $2, 4) RETURNING id",
    )
    .bind(cid)
    .bind(me)
    .fetch_one(&db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO team_role_slots (team_id, role_slug, required_skill_id, min_proficiency_level)
         VALUES ($1, 'coder', $2, 1)",
    )
    .bind(tid)
    .bind(skill_id)
    .execute(&db)
    .await
    .unwrap();

    let pl = orientations_playlist::playlist_for(&db, me, "dev-frontend")
        .await
        .expect("pl");
    assert_eq!(pl.open_team_slots.len(), 0, "own team slot excluded");
    db.close().await;
    cleanup_test_db(&name).await;
}
