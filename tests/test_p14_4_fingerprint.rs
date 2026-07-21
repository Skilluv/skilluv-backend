//! Tests d'intégration P14.4 : fingerprinting + détection multi-account.

use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

use skilluv_backend::services::fingerprint;

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p14_4_test_{}",
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

async fn insert_user(db: &PgPool) -> Uuid {
    let uid = Uuid::new_v4();
    let short = &uid.to_string()[..8];
    sqlx::query(
        "INSERT INTO users
            (id, email, username, first_name, last_name, display_name, password_hash,
             profile_active, total_fragments)
         VALUES ($1, $2, $3, 'T', 'U', 'Test', 'dummy', TRUE, 0)",
    )
    .bind(uid)
    .bind(format!("t-{uid}@ex.io"))
    .bind(format!("t{short}"))
    .execute(db)
    .await
    .expect("u");
    uid
}

// ═══════════════════════════════════════════════════════════════════
// hash_str deterministe
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn hash_str_is_deterministic() {
    assert_eq!(fingerprint::hash_str("abc"), fingerprint::hash_str("abc"));
    assert_ne!(fingerprint::hash_str("abc"), fingerprint::hash_str("abd"));
    assert_eq!(fingerprint::hash_str("").len(), 64, "SHA-256 hex = 64");
}

// ═══════════════════════════════════════════════════════════════════
// record_fingerprint insere la ligne
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn record_fingerprint_inserts_row() {
    let (db, name) = setup_test_db().await;
    let user = insert_user(&db).await;

    fingerprint::record_fingerprint(
        &db,
        user,
        "1.2.3.4",
        "Mozilla/5.0 Firefox",
        Some("canvas-abc-123"),
    )
    .await
    .expect("rec");

    let (ip, ua, canvas): (String, String, Option<String>) = sqlx::query_as(
        "SELECT ip_hash, ua_hash, canvas_hash
         FROM user_fingerprints WHERE user_id = $1",
    )
    .bind(user)
    .fetch_one(&db)
    .await
    .expect("row");
    assert_eq!(ip, fingerprint::hash_str("1.2.3.4"));
    assert_eq!(ua, fingerprint::hash_str("Mozilla/5.0 Firefox"));
    assert_eq!(canvas, Some(fingerprint::hash_str("canvas-abc-123")));

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// detect_multi_accounts flag les groupes > seuil
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn detect_flags_groups_sharing_ip_and_ua() {
    let (db, name) = setup_test_db().await;

    // 4 users partagent la même IP + UA → suspect.
    let mut ids = Vec::new();
    for _ in 0..4 {
        let u = insert_user(&db).await;
        fingerprint::record_fingerprint(&db, u, "10.0.0.1", "SharedUA/1.0", Some("cv-same"))
            .await
            .expect("r");
        ids.push(u);
    }

    // 1 user isolé (autre IP + UA).
    let lonely = insert_user(&db).await;
    fingerprint::record_fingerprint(&db, lonely, "192.168.1.1", "SoloUA", None)
        .await
        .expect("r");

    let groups = fingerprint::detect_multi_accounts(&db, 24, 3)
        .await
        .expect("d");
    assert_eq!(groups.len(), 1, "1 groupe detecte");
    assert_eq!(groups[0].user_ids.len(), 4);

    // Les 4 users du groupe sont flaggés.
    let flagged: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM users WHERE suspected_multi_account = TRUE AND id = ANY($1)",
    )
    .bind(&ids)
    .fetch_one(&db)
    .await
    .expect("c");
    assert_eq!(flagged, 4);

    // Le solitaire n'est pas flaggé.
    let solo_flagged: bool =
        sqlx::query_scalar("SELECT suspected_multi_account FROM users WHERE id = $1")
            .bind(lonely)
            .fetch_one(&db)
            .await
            .expect("s");
    assert!(!solo_flagged);

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// min_group_size respecte
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn detect_does_not_flag_below_min_group_size() {
    let (db, name) = setup_test_db().await;

    // 2 users only sharing → sous le seuil 3.
    for _ in 0..2 {
        let u = insert_user(&db).await;
        fingerprint::record_fingerprint(&db, u, "5.5.5.5", "PairUA", None)
            .await
            .expect("r");
    }

    let groups = fingerprint::detect_multi_accounts(&db, 24, 3)
        .await
        .expect("d");
    assert!(groups.is_empty(), "2 users < seuil 3");

    let flagged: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE suspected_multi_account = TRUE")
            .fetch_one(&db)
            .await
            .expect("c");
    assert_eq!(flagged, 0);

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// purge_old_fingerprints supprime > keep_days
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn purge_removes_old_rows_only() {
    let (db, name) = setup_test_db().await;
    let u = insert_user(&db).await;

    // Row récente : garder
    fingerprint::record_fingerprint(&db, u, "1.1.1.1", "UA1", None)
        .await
        .expect("r1");
    // Row artificiellement vieille
    sqlx::query(
        "INSERT INTO user_fingerprints
            (user_id, ip_hash, ua_hash, canvas_hash, created_at)
         VALUES ($1, $2, $3, NULL, NOW() - INTERVAL '100 days')",
    )
    .bind(u)
    .bind(fingerprint::hash_str("2.2.2.2"))
    .bind(fingerprint::hash_str("UA2"))
    .execute(&db)
    .await
    .expect("old");

    let deleted = fingerprint::purge_old_fingerprints(&db, 90)
        .await
        .expect("p");
    assert_eq!(deleted, 1, "1 row supprimee");

    let remaining: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM user_fingerprints WHERE user_id = $1")
            .bind(u)
            .fetch_one(&db)
            .await
            .expect("c");
    assert_eq!(remaining, 1);

    db.close().await;
    cleanup_test_db(&name).await;
}
