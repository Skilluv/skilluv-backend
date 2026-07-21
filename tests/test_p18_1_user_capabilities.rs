//! Tests P18.1 : user_capabilities + backfill + partial UNIQUE + révocation.

use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p18_1_test_{}",
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

async fn create_user_with_role(db: &PgPool, role: &str) -> Uuid {
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
    uid
}

#[tokio::test]
async fn capability_check_rejects_invalid_value() {
    let (db, name) = setup_test_db().await;
    let u = create_user_with_role(&db, "user").await;
    let bad =
        sqlx::query("INSERT INTO user_capabilities (user_id, capability) VALUES ($1, 'god_mode')")
            .bind(u)
            .execute(&db)
            .await;
    assert!(bad.is_err());
    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn one_active_row_per_user_capability() {
    let (db, name) = setup_test_db().await;
    let u = create_user_with_role(&db, "user").await;

    // 1re insertion mentor OK (backfill l'a peut-être déjà mise en réalité —
    // ici pour user 'user' non-mentor rien n'a été backfillé sauf challenger).
    sqlx::query("INSERT INTO user_capabilities (user_id, capability) VALUES ($1, 'mentor')")
        .bind(u)
        .execute(&db)
        .await
        .expect("mentor");

    // 2ᵉ mentor active → doit être rejetée par la partial UNIQUE.
    let dup =
        sqlx::query("INSERT INTO user_capabilities (user_id, capability) VALUES ($1, 'mentor')")
            .bind(u)
            .execute(&db)
            .await;
    assert!(dup.is_err(), "duplicate active capability rejected");

    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn revoked_frees_slot_for_new_grant() {
    let (db, name) = setup_test_db().await;
    let u = create_user_with_role(&db, "user").await;

    sqlx::query(
        "INSERT INTO user_capabilities (user_id, capability, revoked_at, revoked_reason)
         VALUES ($1, 'pr_reviewer', NOW(), 'inactivité 6 mois')",
    )
    .bind(u)
    .execute(&db)
    .await
    .expect("historical");

    // Peut ré-attribuer maintenant (l'ancienne est revoked donc hors partial UNIQUE).
    let re = sqlx::query(
        "INSERT INTO user_capabilities (user_id, capability, granted_reason)
         VALUES ($1, 'pr_reviewer', 'renomination')",
    )
    .bind(u)
    .execute(&db)
    .await;
    assert!(re.is_ok());

    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn user_can_stack_multiple_active_capabilities() {
    let (db, name) = setup_test_db().await;
    let u = create_user_with_role(&db, "user").await;
    for cap in ["mentor", "pr_reviewer", "issue_proposer", "bounty_funder"] {
        sqlx::query("INSERT INTO user_capabilities (user_id, capability) VALUES ($1, $2)")
            .bind(u)
            .bind(cap)
            .execute(&db)
            .await
            .expect("cap");
    }
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_capabilities WHERE user_id = $1 AND revoked_at IS NULL",
    )
    .bind(u)
    .fetch_one(&db)
    .await
    .unwrap();
    // 4 stackées. Le challenger backfill ne joue que pour les users existants
    // à l'exécution de la migration ; les users créés après (comme ici) ne
    // reçoivent pas challenger jusqu'à un futur trigger auto-grant.
    assert_eq!(n, 4);
    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn backfill_grants_challenger_to_every_user_and_derives_from_role() {
    let (db, name) = setup_test_db().await;

    let u_user = create_user_with_role(&db, "user").await;
    let u_mentor = create_user_with_role(&db, "mentor").await;
    let u_admin = create_user_with_role(&db, "admin").await;
    let u_enter = create_user_with_role(&db, "enterprise").await;
    let u_recr = create_user_with_role(&db, "recruiter").await;

    // Ré-applique le backfill (idempotent : ON CONFLICT DO NOTHING).
    sqlx::query(
        "INSERT INTO user_capabilities (user_id, capability, granted_reason)
         SELECT id, 'challenger', 'backfill:default_all_users' FROM users
         ON CONFLICT DO NOTHING",
    )
    .execute(&db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO user_capabilities (user_id, capability, granted_reason)
         SELECT id, 'mentor', 'backfill:from_users_role' FROM users WHERE role='mentor'
         ON CONFLICT DO NOTHING",
    )
    .execute(&db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO user_capabilities (user_id, capability, granted_reason)
         SELECT id, 'admin', 'backfill:from_users_role' FROM users WHERE role='admin'
         ON CONFLICT DO NOTHING",
    )
    .execute(&db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO user_capabilities (user_id, capability, granted_reason)
         SELECT id, 'enterprise_recruiter', 'backfill:from_users_role'
         FROM users WHERE role IN ('enterprise','recruiter')
         ON CONFLICT DO NOTHING",
    )
    .execute(&db)
    .await
    .unwrap();

    // Everyone got challenger
    for uid in [u_user, u_mentor, u_admin, u_enter, u_recr] {
        let has: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM user_capabilities WHERE user_id = $1 AND capability = 'challenger'",
        )
        .bind(uid).fetch_one(&db).await.unwrap();
        assert_eq!(has, 1, "challenger expected for user {uid}");
    }

    // Mentor got mentor
    let m: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_capabilities WHERE user_id = $1 AND capability = 'mentor'",
    )
    .bind(u_mentor)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(m, 1);

    // Admin got admin
    let a: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_capabilities WHERE user_id = $1 AND capability = 'admin'",
    )
    .bind(u_admin)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(a, 1);

    // Enterprise + recruiter → enterprise_recruiter
    for uid in [u_enter, u_recr] {
        let r: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM user_capabilities
             WHERE user_id = $1 AND capability = 'enterprise_recruiter'",
        )
        .bind(uid)
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(r, 1, "enterprise_recruiter expected");
    }

    // User simple n'a QUE challenger
    let n_user: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_capabilities WHERE user_id = $1 AND revoked_at IS NULL",
    )
    .bind(u_user)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(n_user, 1);

    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn expires_at_supports_temporal_grants() {
    let (db, name) = setup_test_db().await;
    let u = create_user_with_role(&db, "user").await;
    sqlx::query(
        "INSERT INTO user_capabilities (user_id, capability, expires_at)
         VALUES ($1, 'jury_tournament', NOW() + INTERVAL '30 days')",
    )
    .bind(u)
    .execute(&db)
    .await
    .expect("temporal");

    let exp: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT expires_at FROM user_capabilities WHERE user_id = $1 AND capability = 'jury_tournament'",
    )
    .bind(u).fetch_one(&db).await.unwrap();
    assert!(exp.is_some());
    db.close().await;
    cleanup_test_db(&name).await;
}
