//! Tests P19.2 : intégration hook depuis review verdict Approve →
//! recompute automatique badges + rank + capabilities.
//!
//! Le hook est async via tokio::spawn — on lui laisse un court moment pour
//! s'exécuter avant d'observer les side-effects.

use sqlx::postgres::{PgPool, PgPoolOptions};
use std::time::Duration;
use uuid::Uuid;

use skilluv_backend::services::reviews::{ReviewsService, SubmitParams, Verdict};

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p19_2_test_{}",
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
    sqlx::query(
        "INSERT INTO user_ranks (user_id, rank) VALUES ($1, 'apprenti') ON CONFLICT DO NOTHING",
    )
    .bind(uid)
    .execute(db)
    .await
    .unwrap();
    uid
}

async fn create_pending_deliverable(db: &PgPool, user_id: Uuid) -> Uuid {
    let cid: Uuid = sqlx::query_scalar(
        "INSERT INTO challenge_templates (title, description, instructions, skill_domain,
             difficulty, is_training, status)
         VALUES ('T', 'D', 'I', 'code', 2, TRUE, 'published') RETURNING id",
    )
    .fetch_one(db)
    .await
    .unwrap();
    sqlx::query_scalar(
        "INSERT INTO deliverables (challenge_id, user_id, artifact_type, artifact_url,
             verifiable_by, verification_status)
         VALUES ($1, $2, 'other', 'x', 'human_review', 'pending_manual_review') RETURNING id",
    )
    .bind(cid)
    .bind(user_id)
    .fetch_one(db)
    .await
    .unwrap()
}

#[tokio::test]
async fn approve_verdict_triggers_recompute_capabilities_and_rank() {
    let (db, name) = setup_test_db().await;
    let author = create_user(&db).await;
    let reviewer = create_user(&db).await;

    // 4 deliverables pending (seuils ranger : 4 verified)
    let mut deliverables = Vec::new();
    for _ in 0..4 {
        deliverables.push(create_pending_deliverable(&db, author).await);
    }

    // Approve les 4
    for d_id in &deliverables {
        ReviewsService::submit_verdict(
            &db,
            SubmitParams {
                deliverable_id: *d_id,
                reviewer_user_id: reviewer,
                verdict: Verdict::Approve,
                body: "OK".into(),
                time_spent_seconds: Some(10),
            },
        )
        .await
        .expect("submit");
    }

    // Le hook est async → laisse 800ms pour qu'il s'exécute.
    tokio::time::sleep(Duration::from_millis(800)).await;

    // Vérifie que challenger a été accordée (auto via capabilities_engine).
    let has_challenger: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM user_capabilities
                        WHERE user_id = $1 AND capability = 'challenger' AND revoked_at IS NULL)",
    )
    .bind(author)
    .fetch_one(&db)
    .await
    .unwrap();
    assert!(has_challenger, "challenger auto-accordé via hook");

    // Vérifie que le rank a été promu à ranger (4 verified).
    let rank: String = sqlx::query_scalar("SELECT rank FROM user_ranks WHERE user_id = $1")
        .bind(author)
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(rank, "ranger", "rank auto-promu via hook après 4 verified");

    db.close().await;
    cleanup_test_db(&name).await;
}
