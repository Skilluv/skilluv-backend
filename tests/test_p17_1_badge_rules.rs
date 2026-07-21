//! Tests P17.1 : badge_rules + refactor user_badges.

use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p17_1_test_{}",
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

#[tokio::test]
async fn seed_migrates_nine_legacy_badges_all_deprecated() {
    let (db, name) = setup_test_db().await;
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM badge_rules WHERE slug LIKE 'legacy_%'")
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(count, 9, "9 legacy badges migrated to rules");

    let non_deprecated: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM badge_rules
         WHERE slug LIKE 'legacy_%' AND deprecated_at IS NULL",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(non_deprecated, 0, "all legacy rules must be deprecated");

    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn output_type_check_rejects_invalid_family() {
    let (db, name) = setup_test_db().await;
    let bad = sqlx::query(
        "INSERT INTO badge_rules (slug, output_type, display_name)
         VALUES ('x-invalid-fam', 'trophy', 'X')",
    )
    .execute(&db)
    .await;
    assert!(bad.is_err());
    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn slug_regex_rejects_special_chars() {
    let (db, name) = setup_test_db().await;
    let bad = sqlx::query(
        "INSERT INTO badge_rules (slug, output_type, display_name)
         VALUES ('Bad Slug!', 'medal', 'X')",
    )
    .execute(&db)
    .await;
    assert!(bad.is_err());
    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn slug_uniqueness_enforced() {
    let (db, name) = setup_test_db().await;
    let dup = sqlx::query(
        "INSERT INTO badge_rules (slug, output_type, display_name)
         VALUES ('legacy_first_challenge', 'medal', 'Dup')",
    )
    .execute(&db)
    .await;
    assert!(dup.is_err(), "duplicate legacy slug rejected");
    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn user_badge_stores_source_proofs_and_rarity() {
    let (db, name) = setup_test_db().await;
    let u = create_user(&db).await;

    // Prends une rule existante (une legacy)
    let rule_id: Uuid =
        sqlx::query_scalar("SELECT id FROM badge_rules WHERE slug = 'legacy_first_challenge'")
            .fetch_one(&db)
            .await
            .unwrap();

    // Fait référence à un badge legacy (via badge_id, table historique).
    let badge_id: Uuid = sqlx::query_scalar("SELECT id FROM badges WHERE slug = 'first_challenge'")
        .fetch_one(&db)
        .await
        .unwrap();

    let proof_a = Uuid::new_v4();
    let proof_b = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO user_badges (user_id, badge_id, rule_id, source_proofs, rarity)
         VALUES ($1, $2, $3, $4, 'rare')",
    )
    .bind(u)
    .bind(badge_id)
    .bind(rule_id)
    .bind(&vec![proof_a, proof_b])
    .execute(&db)
    .await
    .expect("insert");

    let (stored_rarity, stored_proofs): (String, Vec<Uuid>) = sqlx::query_as(
        "SELECT rarity, source_proofs FROM user_badges WHERE user_id = $1 AND badge_id = $2",
    )
    .bind(u)
    .bind(badge_id)
    .fetch_one(&db)
    .await
    .expect("select");
    assert_eq!(stored_rarity, "rare");
    assert_eq!(stored_proofs, vec![proof_a, proof_b]);

    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn user_badge_rarity_check_rejects_invalid() {
    let (db, name) = setup_test_db().await;
    let u = create_user(&db).await;
    let badge_id: Uuid = sqlx::query_scalar("SELECT id FROM badges WHERE slug = 'first_challenge'")
        .fetch_one(&db)
        .await
        .unwrap();
    let bad = sqlx::query(
        "INSERT INTO user_badges (user_id, badge_id, rarity) VALUES ($1, $2, 'mythical')",
    )
    .bind(u)
    .bind(badge_id)
    .execute(&db)
    .await;
    assert!(bad.is_err(), "invalid rarity rejected");
    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn revoked_at_cascades_soft_delete() {
    let (db, name) = setup_test_db().await;
    let u = create_user(&db).await;
    let badge_id: Uuid = sqlx::query_scalar("SELECT id FROM badges WHERE slug = 'first_challenge'")
        .fetch_one(&db)
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO user_badges (user_id, badge_id, rarity)
         VALUES ($1, $2, 'common')",
    )
    .bind(u)
    .bind(badge_id)
    .execute(&db)
    .await
    .unwrap();

    sqlx::query(
        "UPDATE user_badges SET revoked_at = NOW(), revoked_reason = 'source_proof_removed'
         WHERE user_id = $1 AND badge_id = $2",
    )
    .bind(u)
    .bind(badge_id)
    .execute(&db)
    .await
    .unwrap();

    let active: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM user_badges WHERE revoked_at IS NULL")
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(active, 0);
    let historic: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM user_badges WHERE revoked_at IS NOT NULL")
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(historic, 1, "row stays for audit trail");

    db.close().await;
    cleanup_test_db(&name).await;
}
