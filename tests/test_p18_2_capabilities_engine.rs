//! Tests P18.2 : capabilities_engine auto-promotion.

use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

use skilluv_backend::services::capabilities_engine;

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p18_2_test_{}",
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
    sqlx::query(
        "INSERT INTO users (id, email, username, first_name, last_name, display_name,
                             password_hash, profile_active, total_fragments)
         VALUES ($1, $2, $3, 't','u','t','x',TRUE,0)",
    )
    .bind(uid)
    .bind(format!("t-{uid}@ex.io"))
    .bind(format!("t{}", &uid.to_string()[..8]))
    .execute(db)
    .await
    .expect("u");
    uid
}

async fn add_attestations(db: &PgPool, user_id: Uuid, n: usize) {
    let already: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM attestations WHERE user_id = $1")
        .bind(user_id)
        .fetch_one(db)
        .await
        .unwrap_or(0);
    let skill_ids: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM skill_nodes ORDER BY slug OFFSET $1 LIMIT $2")
            .bind(already)
            .bind(n as i64)
            .fetch_all(db)
            .await
            .unwrap();
    for (i, sid) in skill_ids.iter().enumerate() {
        sqlx::query(
            "INSERT INTO attestations (user_id, attestation_type, title, description,
                                        linked_skill_node_ids, verification_code, issued_at)
             VALUES ($1, 'gesture', 'T', 'D', ARRAY[$2::UUID], $3, NOW())",
        )
        .bind(user_id)
        .bind(sid)
        .bind(format!(
            "{}-{}",
            &Uuid::new_v4().to_string().replace('-', "")[..7],
            i
        ))
        .execute(db)
        .await
        .unwrap();
    }
}

// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn new_user_gets_challenger_auto() {
    let (db, name) = setup_test_db().await;
    let u = create_user(&db).await;
    let r = capabilities_engine::recompute_capabilities_for_user(&db, u)
        .await
        .unwrap();
    assert!(r.granted.contains(&"challenger".to_string()));
    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn mentor_promoted_at_five_attestations() {
    let (db, name) = setup_test_db().await;
    let u = create_user(&db).await;

    add_attestations(&db, u, 4).await;
    let r = capabilities_engine::recompute_capabilities_for_user(&db, u)
        .await
        .unwrap();
    assert!(
        !r.granted.contains(&"mentor".to_string()),
        "4 attestations insuffisantes"
    );

    add_attestations(&db, u, 1).await; // total 5
    let r = capabilities_engine::recompute_capabilities_for_user(&db, u)
        .await
        .unwrap();
    assert!(
        r.granted.contains(&"mentor".to_string())
            || r.already_active.contains(&"mentor".to_string())
    );

    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn issue_proposer_at_three_published_community_challenges() {
    let (db, name) = setup_test_db().await;
    let u = create_user(&db).await;

    for i in 0..3 {
        sqlx::query(
            "INSERT INTO challenge_templates
                (title, description, instructions, skill_domain, difficulty,
                 is_training, status, is_community, created_by)
             VALUES ($1, 'D', 'I', 'code', 2, TRUE, 'published', TRUE, $2)",
        )
        .bind(format!("Prop {i}"))
        .bind(u)
        .execute(&db)
        .await
        .unwrap();
    }
    let r = capabilities_engine::recompute_capabilities_for_user(&db, u)
        .await
        .unwrap();
    assert!(r.granted.contains(&"issue_proposer".to_string()));

    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn project_steward_at_one_owned_project() {
    let (db, name) = setup_test_db().await;
    let u = create_user(&db).await;

    sqlx::query(
        "INSERT INTO projects (slug, name, description, owner_type, owner_id)
         VALUES ($1, 'MyProj', 'D', 'user', $2)",
    )
    .bind(format!("p-{}", &Uuid::new_v4().to_string()[..8]))
    .bind(u)
    .execute(&db)
    .await
    .unwrap();

    let r = capabilities_engine::recompute_capabilities_for_user(&db, u)
        .await
        .unwrap();
    assert!(r.granted.contains(&"project_steward".to_string()));

    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn recompute_is_idempotent() {
    let (db, name) = setup_test_db().await;
    let u = create_user(&db).await;
    add_attestations(&db, u, 5).await;

    let r1 = capabilities_engine::recompute_capabilities_for_user(&db, u)
        .await
        .unwrap();
    assert!(!r1.granted.is_empty());
    let r2 = capabilities_engine::recompute_capabilities_for_user(&db, u)
        .await
        .unwrap();
    assert!(r2.granted.is_empty(), "no re-grant on idempotent call");
    assert!(r2.already_active.contains(&"mentor".to_string()));

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_capabilities
         WHERE user_id = $1 AND capability = 'mentor' AND revoked_at IS NULL",
    )
    .bind(u)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(count, 1);

    db.close().await;
    cleanup_test_db(&name).await;
}
