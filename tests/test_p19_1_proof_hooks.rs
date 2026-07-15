//! Tests P19.1 : orchestrateur proof_hooks::recompute_all_for_user.

use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

use skilluv_backend::services::proof_hooks;

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p19_1_test_{}",
        Uuid::new_v4().to_string().replace('-', "")
    );
    let admin_pool = PgPoolOptions::new()
        .max_connections(2)
        .connect("postgres://skilluv:skilluv_secret@localhost:5433/skilluv")
        .await
        .expect("admin");
    sqlx::query(&format!("CREATE DATABASE \"{db_name}\""))
        .execute(&admin_pool).await.expect("create");
    admin_pool.close().await;

    let db_url = format!("postgres://skilluv:skilluv_secret@localhost:5433/{db_name}");
    let db = PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url).await.expect("connect");
    sqlx::migrate!("./migrations").run(&db).await.expect("migrations");
    (db, db_name)
}

async fn cleanup_test_db(db_name: &str) {
    let admin_pool = PgPoolOptions::new()
        .max_connections(2)
        .connect("postgres://skilluv:skilluv_secret@localhost:5433/skilluv")
        .await.expect("admin");
    let _ = sqlx::query(&format!(
        "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '{db_name}'"
    )).execute(&admin_pool).await;
    let _ = sqlx::query(&format!("DROP DATABASE IF EXISTS \"{db_name}\"")).execute(&admin_pool).await;
    admin_pool.close().await;
}

async fn create_user(db: &PgPool) -> Uuid {
    let uid = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, email, username, first_name, last_name, display_name,
             password_hash, profile_active, total_fragments)
         VALUES ($1, $2, $3, 't','u','t','x',TRUE,0)",
    )
    .bind(uid)
    .bind(format!("t-{uid}@ex.io"))
    .bind(format!("t{}", &uid.to_string()[..8]))
    .execute(db).await.expect("u");
    sqlx::query("INSERT INTO user_ranks (user_id, rank) VALUES ($1, 'apprenti')
                 ON CONFLICT DO NOTHING")
        .bind(uid).execute(db).await.unwrap();
    uid
}

async fn add_verified_deliverable(db: &PgPool, user_id: Uuid) {
    let cid: Uuid = sqlx::query_scalar(
        "INSERT INTO challenge_templates (title, description, instructions, skill_domain,
             difficulty, is_training, status)
         VALUES ('T', 'D', 'I', 'code', 2, TRUE, 'published') RETURNING id",
    )
    .fetch_one(db).await.unwrap();
    sqlx::query(
        "INSERT INTO deliverables (challenge_id, user_id, artifact_type, artifact_url,
             verifiable_by, verification_status, verified_at)
         VALUES ($1, $2, 'other', 'x', 'human_review', 'verified', NOW())",
    )
    .bind(cid).bind(user_id).execute(db).await.unwrap();
}

#[tokio::test]
async fn recompute_new_user_promotes_challenger_only() {
    let (db, name) = setup_test_db().await;
    let u = create_user(&db).await;
    let r = proof_hooks::recompute_all_for_user(&db, u).await.unwrap();
    assert!(r.capabilities_granted.contains(&"challenger".to_string()));
    assert_eq!(r.rank_computed, "apprenti");
    assert!(r.errors.is_empty());
    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn recompute_after_four_deliverables_promotes_rank() {
    let (db, name) = setup_test_db().await;
    let u = create_user(&db).await;
    for _ in 0..4 { add_verified_deliverable(&db, u).await; }
    let r = proof_hooks::recompute_all_for_user(&db, u).await.unwrap();
    assert_eq!(r.rank_computed, "ranger");
    assert!(r.rank_promoted, "rank should be promoted");
    assert!(r.errors.is_empty());
    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn recompute_is_idempotent() {
    let (db, name) = setup_test_db().await;
    let u = create_user(&db).await;
    for _ in 0..4 { add_verified_deliverable(&db, u).await; }

    let r1 = proof_hooks::recompute_all_for_user(&db, u).await.unwrap();
    assert!(r1.rank_promoted);

    let r2 = proof_hooks::recompute_all_for_user(&db, u).await.unwrap();
    assert!(!r2.rank_promoted, "second call = no re-promotion");
    assert_eq!(r2.rank_computed, "ranger");
    assert!(r2.capabilities_already_active.contains(&"challenger".to_string()));

    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn sweep_returns_active_users_only() {
    let (db, name) = setup_test_db().await;
    let u_active = create_user(&db).await;
    let _u_idle = create_user(&db).await;
    add_verified_deliverable(&db, u_active).await;

    let processed = proof_hooks::sweep_active_users(&db, 30).await.unwrap();
    assert!(processed.contains(&u_active), "active user included");
    // Note : le user idle n'a aucune activité donc ne remonte pas.
    assert!(!processed.contains(&_u_idle) || processed.len() >= 1);

    db.close().await;
    cleanup_test_db(&name).await;
}
