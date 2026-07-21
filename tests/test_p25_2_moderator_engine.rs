//! Tests P25.2 : auto-promotion des community moderator caps.

use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

use skilluv_backend::services::capabilities_engine;

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p25_2_test_{}",
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

async fn ensure_forum_category(db: &PgPool) -> Uuid {
    let existing: Option<Uuid> = sqlx::query_scalar("SELECT id FROM forum_categories LIMIT 1")
        .fetch_optional(db)
        .await
        .unwrap();
    if let Some(id) = existing {
        return id;
    }
    sqlx::query_scalar(
        "INSERT INTO forum_categories (slug, name) VALUES ('general', 'General') RETURNING id",
    )
    .fetch_one(db)
    .await
    .unwrap()
}

async fn add_posts(db: &PgPool, user_id: Uuid, cat_id: Uuid, n: usize) {
    for i in 0..n {
        sqlx::query(
            "INSERT INTO posts (category_id, author_id, title, body, kind)
             VALUES ($1, $2, $3, 'body content', 'discussion')",
        )
        .bind(cat_id)
        .bind(user_id)
        .bind(format!("Post #{i}"))
        .execute(db)
        .await
        .unwrap();
    }
}

// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn community_curator_granted_at_three_published_proposals() {
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
    assert!(r.granted.contains(&"community_curator".to_string()));
    // Umbrella meta-cap auto-granted en cascade
    assert!(
        r.granted.contains(&"community_moderator".to_string()),
        "community_moderator umbrella auto-granted when curator active"
    );
    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn forum_moderator_granted_at_twenty_posts() {
    let (db, name) = setup_test_db().await;
    let u = create_user(&db).await;
    let cat = ensure_forum_category(&db).await;

    add_posts(&db, u, cat, 19).await;
    let r = capabilities_engine::recompute_capabilities_for_user(&db, u)
        .await
        .unwrap();
    assert!(
        !r.granted.contains(&"forum_moderator".to_string()),
        "19 posts insuffisant"
    );

    add_posts(&db, u, cat, 1).await;
    let r = capabilities_engine::recompute_capabilities_for_user(&db, u)
        .await
        .unwrap();
    assert!(
        r.granted.contains(&"forum_moderator".to_string()),
        "20 posts déclenchent la promotion"
    );

    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn community_moderator_umbrella_requires_any_sub_cap() {
    let (db, name) = setup_test_db().await;
    let u = create_user(&db).await;

    let r = capabilities_engine::recompute_capabilities_for_user(&db, u)
        .await
        .unwrap();
    assert!(
        !r.granted.contains(&"community_moderator".to_string()),
        "sans sub-cap → pas umbrella"
    );

    // Grant manuellement un plagiarism_reviewer
    sqlx::query(
        "INSERT INTO user_capabilities (user_id, capability, granted_reason)
         VALUES ($1, 'plagiarism_reviewer', 'test_manual')",
    )
    .bind(u)
    .execute(&db)
    .await
    .unwrap();

    let r = capabilities_engine::recompute_capabilities_for_user(&db, u)
        .await
        .unwrap();
    assert!(
        r.granted.contains(&"community_moderator".to_string()),
        "umbrella auto-granted quand sub-cap active"
    );

    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn plagiarism_and_kyc_reviewer_never_auto_granted() {
    let (db, name) = setup_test_db().await;
    let u = create_user(&db).await;
    // Setup : simule le max d'activité mesurable
    let cat = ensure_forum_category(&db).await;
    add_posts(&db, u, cat, 50).await;
    for i in 0..10 {
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
    assert!(
        !r.granted.contains(&"plagiarism_reviewer".to_string()),
        "plagiarism_reviewer manual-only"
    );
    assert!(
        !r.granted.contains(&"kyc_reviewer".to_string()),
        "kyc_reviewer manual-only"
    );

    db.close().await;
    cleanup_test_db(&name).await;
}
