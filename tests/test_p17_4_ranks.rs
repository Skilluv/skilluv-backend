//! Tests P17.4 : rank system Apprenti → Doyen.

use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

use skilluv_backend::services::ranks;

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p17_4_test_{}",
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

async fn create_user(db: &PgPool, role: &str) -> Uuid {
    let uid = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, email, username, first_name, last_name, display_name,
                             password_hash, profile_active, total_fragments, role)
         VALUES ($1, $2, $3, 't','u','t','x',TRUE,0,$4)",
    )
    .bind(uid)
    .bind(format!("t-{uid}@ex.io"))
    .bind(format!("t{}", &uid.to_string()[..8]))
    .bind(role)
    .execute(db)
    .await
    .expect("u");
    sqlx::query(
        "INSERT INTO user_ranks (user_id, rank) VALUES ($1, 'apprenti')
                 ON CONFLICT DO NOTHING",
    )
    .bind(uid)
    .execute(db)
    .await
    .unwrap();
    uid
}

async fn add_verified_deliverable(db: &PgPool, user_id: Uuid) {
    let cid: Uuid = sqlx::query_scalar(
        "INSERT INTO challenge_templates
            (title, description, instructions, skill_domain, difficulty, is_training, status)
         VALUES ('T', 'D', 'I', 'code', 2, TRUE, 'published') RETURNING id",
    )
    .fetch_one(db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO deliverables
            (challenge_id, user_id, artifact_type, artifact_url,
             verifiable_by, verification_status)
         VALUES ($1, $2, 'other', 'x', 'human_review', 'verified')",
    )
    .bind(cid)
    .bind(user_id)
    .execute(db)
    .await
    .unwrap();
}

async fn add_attestations(db: &PgPool, user_id: Uuid, n: usize) {
    // Fetch n distinct skill ids to satisfy the (user_id, type, skill_ids) UNIQUE.
    let skill_ids: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM skill_nodes ORDER BY slug LIMIT $1")
            .bind(n as i64)
            .fetch_all(db)
            .await
            .unwrap();
    for (i, sid) in skill_ids.iter().enumerate() {
        sqlx::query(
            "INSERT INTO attestations (user_id, attestation_type, title, description,
                                        linked_skill_node_ids,
                                        verification_code, issued_at)
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
async fn new_user_starts_apprenti() {
    let (db, name) = setup_test_db().await;
    let u = create_user(&db, "user").await;
    let (current, computed, promoted) = ranks::recompute_rank_for_user(&db, u).await.unwrap();
    assert_eq!(current, "apprenti");
    assert_eq!(computed, "apprenti");
    assert!(!promoted);
    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn four_deliverables_promote_to_ranger() {
    let (db, name) = setup_test_db().await;
    let u = create_user(&db, "user").await;
    for _ in 0..4 {
        add_verified_deliverable(&db, u).await;
    }
    let (_prev, computed, promoted) = ranks::recompute_rank_for_user(&db, u).await.unwrap();
    assert_eq!(computed, "ranger");
    assert!(promoted);

    // Historique enregistré
    let hist: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_rank_history WHERE user_id = $1 AND to_rank = 'ranger'",
    )
    .bind(u)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(hist, 1);

    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn artisan_requires_deliverables_and_attestation() {
    let (db, name) = setup_test_db().await;
    let u = create_user(&db, "user").await;

    for _ in 0..11 {
        add_verified_deliverable(&db, u).await;
    }
    let (_, c, _) = ranks::recompute_rank_for_user(&db, u).await.unwrap();
    assert_eq!(
        c, "ranger",
        "11 deliverables sans attestation restent ranger"
    );

    add_attestations(&db, u, 1).await;
    let (_, c, promoted) = ranks::recompute_rank_for_user(&db, u).await.unwrap();
    assert_eq!(c, "artisan");
    assert!(promoted);

    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn doyen_via_mentor_capability_not_role() {
    let (db, name) = setup_test_db().await;
    // User avec role='user' mais capability='mentor' explicite → doit passer.
    let u = create_user(&db, "user").await;
    sqlx::query("INSERT INTO user_capabilities (user_id, capability) VALUES ($1, 'mentor')")
        .bind(u)
        .execute(&db)
        .await
        .unwrap();
    for _ in 0..50 {
        add_verified_deliverable(&db, u).await;
    }
    add_attestations(&db, u, 5).await;
    let (_, c, _) = ranks::recompute_rank_for_user(&db, u).await.unwrap();
    assert_eq!(
        c, "doyen",
        "capability mentor accordée doit débloquer doyen"
    );
    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn doyen_requires_mentor_role() {
    let (db, name) = setup_test_db().await;
    let u_normal = create_user(&db, "user").await;
    let u_mentor = create_user(&db, "mentor").await;

    for uid in [u_normal, u_mentor] {
        for _ in 0..50 {
            add_verified_deliverable(&db, uid).await;
        }
        add_attestations(&db, uid, 5).await;
    }

    let (_, c_normal, _) = ranks::recompute_rank_for_user(&db, u_normal).await.unwrap();
    let (_, c_mentor, _) = ranks::recompute_rank_for_user(&db, u_mentor).await.unwrap();
    assert_eq!(c_normal, "maitre", "sans mentor, plafond maitre");
    assert_eq!(c_mentor, "doyen", "mentor + seuils → doyen");

    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn recompute_is_unidirectional_no_demotion() {
    let (db, name) = setup_test_db().await;
    let u = create_user(&db, "user").await;
    for _ in 0..4 {
        add_verified_deliverable(&db, u).await;
    }
    ranks::recompute_rank_for_user(&db, u).await.unwrap();

    // Simule révocation : verified → revoked
    sqlx::query("UPDATE deliverables SET verification_status = 'revoked' WHERE user_id = $1")
        .bind(u)
        .execute(&db)
        .await
        .unwrap();

    let (current, computed, promoted) = ranks::recompute_rank_for_user(&db, u).await.unwrap();
    // Le calcul dit apprenti mais current reste ranger (no demotion)
    assert_eq!(computed, "apprenti");
    assert_eq!(current, "ranger", "acquis rang conservé");
    assert!(!promoted);

    db.close().await;
    cleanup_test_db(&name).await;
}
