//! Tests d'intégration P10.1 : teams persistentes + slice_claims collectifs.
//!
//! Vérifie :
//! - `challenge_teams.challenge_id` nullable (team persistente sans challenge)
//! - `challenge_teams.is_persistent` flag
//! - CHECK constraint `challenge_teams_persistence_coherent`
//! - `project_slices.claimed_by_team_id` alternative au claim solo
//! - CHECK XOR (jamais user + team en même temps)
//! - `SlicesService::claim_as_team` + `unclaim_by_team` + `list_claimed_by_team`
//! - `expire_stale_claims` nettoie aussi `claimed_by_team_id`

use chrono::{Duration, Utc};
use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

use skilluv_backend::services::SlicesService;

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p10_1_test_{}",
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

async fn insert_project(db: &PgPool, owner_id: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO projects (slug, name, owner_type, owner_id)
         VALUES ($1, 'Test Project', 'user', $2) RETURNING id",
    )
    .bind(format!("p-{}", Uuid::new_v4()))
    .bind(owner_id)
    .fetch_one(db)
    .await
    .expect("insert project")
}

async fn insert_open_slice(db: &PgPool, project_id: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO project_slices
            (project_id, slice_type, title, description, primary_domain, difficulty, status)
         VALUES ($1, 'other', 'T', 'D', 'code', 2, 'open') RETURNING id",
    )
    .bind(project_id)
    .fetch_one(db)
    .await
    .expect("insert slice")
}

async fn insert_persistent_team(db: &PgPool, founder: Uuid, name: &str) -> Uuid {
    let team_id: Uuid = sqlx::query_scalar(
        "INSERT INTO challenge_teams
            (challenge_id, name, created_by, max_members, is_persistent, status)
         VALUES (NULL, $1, $2, 4, TRUE, 'open')
         RETURNING id",
    )
    .bind(name)
    .bind(founder)
    .fetch_one(db)
    .await
    .expect("insert team");
    sqlx::query("INSERT INTO team_members (team_id, user_id) VALUES ($1, $2)")
        .bind(team_id)
        .bind(founder)
        .execute(db)
        .await
        .expect("add founder");
    team_id
}

// ═══════════════════════════════════════════════════════════════════
// Schéma : persistent team avec challenge_id NULL
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn persistent_team_can_be_created_without_challenge() {
    let (db, name) = setup_test_db().await;
    let user = insert_user(&db).await;
    let team_id = insert_persistent_team(&db, user, "The Legends").await;

    let (challenge_id_opt, is_persistent): (Option<Uuid>, bool) = sqlx::query_as(
        "SELECT challenge_id, is_persistent FROM challenge_teams WHERE id = $1",
    )
    .bind(team_id)
    .fetch_one(&db)
    .await
    .expect("fetch");
    assert!(challenge_id_opt.is_none());
    assert!(is_persistent);

    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn non_persistent_team_requires_challenge_id() {
    let (db, name) = setup_test_db().await;
    let user = insert_user(&db).await;

    let res = sqlx::query(
        "INSERT INTO challenge_teams
            (challenge_id, name, created_by, max_members, is_persistent, status)
         VALUES (NULL, 'bad', $1, 4, FALSE, 'open')",
    )
    .bind(user)
    .execute(&db)
    .await;
    assert!(res.is_err(), "CHECK constraint should reject non-persistent team without challenge");

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// XOR : user vs team claim sur project_slices
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn slice_can_be_claimed_by_a_team() {
    let (db, name) = setup_test_db().await;
    let founder = insert_user(&db).await;
    let project_id = insert_project(&db, founder).await;
    let slice_id = insert_open_slice(&db, project_id).await;
    let team_id = insert_persistent_team(&db, founder, "Alpha").await;

    let slice = SlicesService::claim_as_team(&db, slice_id, team_id)
        .await
        .expect("claim as team");

    assert_eq!(slice.status, "claimed");
    assert_eq!(slice.claimed_by_team_id, Some(team_id));
    assert!(slice.claimed_by_user_id.is_none(), "user claim doit être NULL");
    assert!(slice.claim_expires_at.is_some());

    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn slice_xor_constraint_rejects_dual_claim() {
    let (db, name) = setup_test_db().await;
    let user = insert_user(&db).await;
    let project_id = insert_project(&db, user).await;
    let team_id = insert_persistent_team(&db, user, "Bravo").await;

    // Tentative de forcer les 2 en même temps → CHECK constraint refuse.
    let res = sqlx::query(
        "INSERT INTO project_slices
            (project_id, slice_type, title, description, primary_domain, difficulty,
             status, claimed_by_user_id, claimed_by_team_id, claimed_at, claim_expires_at)
         VALUES ($1, 'other', 'X', 'X', 'code', 1, 'claimed', $2, $3, NOW(), NOW() + INTERVAL '7 days')",
    )
    .bind(project_id)
    .bind(user)
    .bind(team_id)
    .execute(&db)
    .await;
    assert!(res.is_err(), "XOR constraint doit refuser user + team simultanés");

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// unclaim_by_team + list_claimed_by_team
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn team_can_unclaim_its_slice() {
    let (db, name) = setup_test_db().await;
    let user = insert_user(&db).await;
    let project_id = insert_project(&db, user).await;
    let slice_id = insert_open_slice(&db, project_id).await;
    let team_id = insert_persistent_team(&db, user, "Charlie").await;

    SlicesService::claim_as_team(&db, slice_id, team_id)
        .await
        .expect("claim");
    let slice = SlicesService::unclaim_by_team(&db, slice_id, team_id)
        .await
        .expect("unclaim");
    assert_eq!(slice.status, "open");
    assert!(slice.claimed_by_team_id.is_none());
    assert!(slice.claimed_at.is_none());

    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn list_claimed_by_team_returns_only_active_claims() {
    let (db, name) = setup_test_db().await;
    let user = insert_user(&db).await;
    let project_id = insert_project(&db, user).await;
    let team_id = insert_persistent_team(&db, user, "Delta").await;

    let slice_a = insert_open_slice(&db, project_id).await;
    let slice_b = insert_open_slice(&db, project_id).await;
    SlicesService::claim_as_team(&db, slice_a, team_id).await.expect("a");
    SlicesService::claim_as_team(&db, slice_b, team_id).await.expect("b");

    let mine = SlicesService::list_claimed_by_team(&db, team_id)
        .await
        .expect("list");
    assert_eq!(mine.len(), 2);
    assert!(mine.iter().all(|s| s.status == "claimed"));

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// expire_stale_claims nettoie aussi les team-claims
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn expire_stale_claims_resets_team_claims_too() {
    let (db, name) = setup_test_db().await;
    let user = insert_user(&db).await;
    let project_id = insert_project(&db, user).await;
    let team_id = insert_persistent_team(&db, user, "Echo").await;

    // Slice claim par team avec expiration dans le passé
    let past = Utc::now() - Duration::days(1);
    let slice_id: Uuid = sqlx::query_scalar(
        "INSERT INTO project_slices
            (project_id, slice_type, title, description, primary_domain, difficulty,
             status, claimed_by_team_id, claimed_at, claim_expires_at)
         VALUES ($1, 'other', 'Expiring', 'X', 'code', 1,
                 'claimed', $2, NOW() - INTERVAL '10 days', $3)
         RETURNING id",
    )
    .bind(project_id)
    .bind(team_id)
    .bind(past)
    .fetch_one(&db)
    .await
    .expect("insert stale");

    let cleaned = SlicesService::expire_stale_claims(&db)
        .await
        .expect("expire");
    assert!(cleaned >= 1);

    let (status, team, at): (String, Option<Uuid>, Option<chrono::DateTime<Utc>>) =
        sqlx::query_as(
            "SELECT status, claimed_by_team_id, claimed_at FROM project_slices WHERE id = $1",
        )
        .bind(slice_id)
        .fetch_one(&db)
        .await
        .expect("fetch");
    assert_eq!(status, "open");
    assert!(team.is_none());
    assert!(at.is_none());

    db.close().await;
    cleanup_test_db(&name).await;
}
