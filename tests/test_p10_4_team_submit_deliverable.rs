//! Tests d'intégration P10.4 : team submit → deliverable partagé.
//!
//! Vérifie que `DeliverablesService::create_from_team_submission` :
//! - Crée un deliverable verified avec artifact_metadata.contributors
//! - Est idempotent (même code + même team → même deliverable)
//! - Fragments_awarded = somme des parts des contributeurs
//! - Le hash intègre le team_id (2 teams avec même code → 2 deliverables)

use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

use skilluv_backend::services::{DeliverablesService, TeamContributor};

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p10_4_test_{}",
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

async fn insert_challenge(db: &PgPool) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO challenge_templates
            (title, description, instructions, skill_domain, difficulty,
             reward_fragments, is_training, mode, status)
         VALUES ('Game jam', 'Jeu 2D', 'Instr', 'game', 3, 100, TRUE, 'team', 'published')
         RETURNING id",
    )
    .fetch_one(db)
    .await
    .expect("challenge")
}

// ═══════════════════════════════════════════════════════════════════
// deliverable créé avec contributors matérialisés
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn team_submission_produces_deliverable_with_contributors() {
    let (db, name) = setup_test_db().await;
    let leader = insert_user(&db).await;
    let musician = insert_user(&db).await;
    let coder = insert_user(&db).await;
    let challenge = insert_challenge(&db).await;
    let team_id = Uuid::new_v4();
    let submission_id = Uuid::new_v4();

    let contributors = vec![
        TeamContributor { user_id: leader, role_slug: Some("designer".into()), fragments_awarded: 40 },
        TeamContributor { user_id: musician, role_slug: Some("musician".into()), fragments_awarded: 30 },
        TeamContributor { user_id: coder, role_slug: Some("coder".into()), fragments_awarded: 30 },
    ];

    let deliverable_id = DeliverablesService::create_from_team_submission(
        &db,
        team_id,
        leader,
        challenge,
        submission_id,
        "print('team code')",
        &contributors,
        Some("python"),
        Some("team code\n"),
        None,
    )
    .await
    .expect("create");

    let (fragments, meta, stored_user_id): (i32, serde_json::Value, Uuid) = sqlx::query_as(
        "SELECT fragments_awarded, artifact_metadata, user_id
         FROM deliverables WHERE id = $1",
    )
    .bind(deliverable_id)
    .fetch_one(&db)
    .await
    .expect("fetch");

    assert_eq!(fragments, 100, "total = 40+30+30");
    assert_eq!(stored_user_id, leader, "deliverable rattaché au team leader");
    assert_eq!(meta["contributors"].as_array().unwrap().len(), 3);
    assert_eq!(meta["team_id"], team_id.to_string());
    assert_eq!(meta["code_content"], "print('team code')");
    assert_eq!(meta["language"], "python");

    let roles: Vec<String> = meta["contributors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["role_slug"].as_str().unwrap().to_string())
        .collect();
    assert!(roles.contains(&"designer".to_string()));
    assert!(roles.contains(&"musician".to_string()));
    assert!(roles.contains(&"coder".to_string()));

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Idempotence : même team + même code = même deliverable
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn same_team_same_code_is_idempotent() {
    let (db, name) = setup_test_db().await;
    let leader = insert_user(&db).await;
    let challenge = insert_challenge(&db).await;
    let team_id = Uuid::new_v4();
    let sub_id = Uuid::new_v4();
    let code = "same code";

    let contribs = vec![TeamContributor {
        user_id: leader,
        role_slug: None,
        fragments_awarded: 50,
    }];

    let d1 = DeliverablesService::create_from_team_submission(
        &db,
        team_id,
        leader,
        challenge,
        sub_id,
        code,
        &contribs,
        None,
        None,
        None,
    )
    .await
    .expect("d1");

    let d2 = DeliverablesService::create_from_team_submission(
        &db,
        team_id,
        leader,
        challenge,
        sub_id,
        code,
        &contribs,
        None,
        None,
        None,
    )
    .await
    .expect("d2");

    assert_eq!(d1, d2, "même team + code → même deliverable_id");

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM deliverables WHERE user_id = $1")
            .bind(leader)
            .fetch_one(&db)
            .await
            .expect("count");
    assert_eq!(count, 1);

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Le hash intègre le team_id : 2 teams avec même code → 2 deliverables
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn different_teams_same_code_produce_distinct_deliverables() {
    let (db, name) = setup_test_db().await;
    let leader_a = insert_user(&db).await;
    let leader_b = insert_user(&db).await;
    let challenge = insert_challenge(&db).await;
    let code = "shared code";

    let d_a = DeliverablesService::create_from_team_submission(
        &db,
        Uuid::new_v4(),
        leader_a,
        challenge,
        Uuid::new_v4(),
        code,
        &[TeamContributor {
            user_id: leader_a,
            role_slug: None,
            fragments_awarded: 10,
        }],
        None,
        None,
        None,
    )
    .await
    .expect("a");

    let d_b = DeliverablesService::create_from_team_submission(
        &db,
        Uuid::new_v4(),
        leader_b,
        challenge,
        Uuid::new_v4(),
        code,
        &[TeamContributor {
            user_id: leader_b,
            role_slug: None,
            fragments_awarded: 10,
        }],
        None,
        None,
        None,
    )
    .await
    .expect("b");

    assert_ne!(
        d_a, d_b,
        "team_id différent → hash différent → deliverables distincts"
    );

    let (hash_a, hash_b): (String, String) = {
        let a: String = sqlx::query_scalar("SELECT artifact_hash FROM deliverables WHERE id = $1")
            .bind(d_a)
            .fetch_one(&db)
            .await
            .expect("hash a");
        let b: String = sqlx::query_scalar("SELECT artifact_hash FROM deliverables WHERE id = $1")
            .bind(d_b)
            .fetch_one(&db)
            .await
            .expect("hash b");
        (a, b)
    };
    assert_ne!(hash_a, hash_b, "hashes distincts (team_id dans le hash)");

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// artifact_url convention
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn artifact_url_uses_team_submission_convention() {
    let (db, name) = setup_test_db().await;
    let leader = insert_user(&db).await;
    let challenge = insert_challenge(&db).await;
    let team_id = Uuid::new_v4();
    let sub_id = Uuid::new_v4();

    let deliverable_id = DeliverablesService::create_from_team_submission(
        &db,
        team_id,
        leader,
        challenge,
        sub_id,
        "code",
        &[TeamContributor {
            user_id: leader,
            role_slug: None,
            fragments_awarded: 1,
        }],
        None,
        None,
        None,
    )
    .await
    .expect("create");

    let url: String = sqlx::query_scalar("SELECT artifact_url FROM deliverables WHERE id = $1")
        .bind(deliverable_id)
        .fetch_one(&db)
        .await
        .expect("fetch");

    assert_eq!(url, format!("skilluv:team_submission:{team_id}:{sub_id}"));

    db.close().await;
    cleanup_test_db(&name).await;
}
