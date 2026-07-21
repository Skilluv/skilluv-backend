//! Tests P16.1 : catalogue orientations + mapping vers skills.
//!
//! Vérifie :
//!   - Le seed initial insère bien ~30 orientations curated valides.
//!   - Slugs contraints (regex, longueur).
//!   - Domain primary est bien dans l'enum autorisé.
//!   - `orientation_skill_map` FK-vérifie skill_id et orientation_id.
//!   - Un track archivé reste queryable mais est filtrable.

use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p16_1_test_{}",
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

#[tokio::test]
async fn seed_produces_at_least_thirty_curated_orientations() {
    let (db, name) = setup_test_db().await;
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM orientations WHERE is_curated = TRUE")
            .fetch_one(&db)
            .await
            .expect("count");
    assert!(
        count >= 30,
        "expected 30+ curated orientations, got {count}"
    );
    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn all_seed_orientations_have_valid_primary_domain() {
    let (db, name) = setup_test_db().await;
    let bad: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM orientations WHERE primary_domain NOT IN
         ('code','design','game','security','soft_skills','ai','ops')",
    )
    .fetch_one(&db)
    .await
    .expect("bad");
    assert_eq!(bad, 0, "seed contains a track with invalid domain");
    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn slug_regex_rejects_invalid_input() {
    let (db, name) = setup_test_db().await;
    let res = sqlx::query(
        "INSERT INTO orientations (slug, name, primary_domain) VALUES ('Bad Slug!', 'x', 'code')",
    )
    .execute(&db)
    .await;
    assert!(res.is_err(), "invalid slug must be rejected");
    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn slug_uniqueness_enforced() {
    let (db, name) = setup_test_db().await;
    let dup = sqlx::query(
        "INSERT INTO orientations (slug, name, primary_domain) VALUES ('dev-frontend', 'Dup', 'code')",
    )
    .execute(&db)
    .await;
    assert!(
        dup.is_err(),
        "duplicate slug must be rejected (seed already has dev-frontend)"
    );
    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn orientation_skill_map_upserts_and_joins() {
    let (db, name) = setup_test_db().await;

    let orientation_id: Uuid =
        sqlx::query_scalar("SELECT id FROM orientations WHERE slug = 'dev-frontend'")
            .fetch_one(&db)
            .await
            .expect("orientation");

    // Ajoute deux skills existants du seed
    let skill_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM skill_nodes WHERE slug IN ('code-review', 'component-composition') LIMIT 2",
    )
    .fetch_all(&db)
    .await
    .expect("skills");

    for (i, sid) in skill_ids.iter().enumerate() {
        sqlx::query(
            "INSERT INTO orientation_skill_map (orientation_id, skill_id, is_core, weight)
             VALUES ($1, $2, $3, $4)",
        )
        .bind(orientation_id)
        .bind(sid)
        .bind(i == 0)
        .bind(2.0f32 - (i as f32) * 0.5)
        .execute(&db)
        .await
        .expect("insert");
    }

    let joined: Vec<(String, bool, f32)> = sqlx::query_as(
        "SELECT sn.slug, tsm.is_core, tsm.weight
         FROM orientation_skill_map tsm
         JOIN skill_nodes sn ON sn.id = tsm.skill_id
         WHERE tsm.orientation_id = $1
         ORDER BY tsm.weight DESC",
    )
    .bind(orientation_id)
    .fetch_all(&db)
    .await
    .expect("join");
    assert_eq!(joined.len(), 2);
    assert!(joined[0].1, "highest weight should be core");

    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn archived_orientations_stay_visible_via_flag() {
    let (db, name) = setup_test_db().await;

    sqlx::query("UPDATE orientations SET is_archived = TRUE WHERE slug = 'smart-contract-dev'")
        .execute(&db)
        .await
        .expect("archive");

    let active: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM orientations WHERE is_curated = TRUE AND is_archived = FALSE",
    )
    .fetch_one(&db)
    .await
    .expect("active");
    let archived: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM orientations WHERE is_archived = TRUE")
            .fetch_one(&db)
            .await
            .expect("arch");

    assert_eq!(archived, 1);
    assert!(active >= 29);

    db.close().await;
    cleanup_test_db(&name).await;
}
