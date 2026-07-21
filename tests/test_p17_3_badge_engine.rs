//! Tests P17.3 : rules engine + recalc badges.

use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

use skilluv_backend::services::badge_engine;

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p17_3_test_{}",
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

async fn insert_rule(db: &PgPool, slug: &str, conditions: serde_json::Value, rarity: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO badge_rules (slug, output_type, display_name, conditions, rarity)
         VALUES ($1, 'skill_patch', $1, $2, $3)
         RETURNING id",
    )
    .bind(slug)
    .bind(&conditions)
    .bind(rarity)
    .fetch_one(db)
    .await
    .expect("rule")
}

async fn insert_verified_deliverable(db: &PgPool, user_id: Uuid, skill_slug: Option<&str>) -> Uuid {
    let cid: Uuid = sqlx::query_scalar(
        "INSERT INTO challenge_templates
            (title, description, instructions, skill_domain, difficulty, is_training, status)
         VALUES ('T', 'D', 'I', 'code', 2, TRUE, 'published') RETURNING id",
    )
    .fetch_one(db)
    .await
    .unwrap();
    let uniq = Uuid::new_v4().to_string();
    let pid: Uuid = sqlx::query_scalar(
        "INSERT INTO projects (slug, name, description, owner_type, owner_id)
         VALUES ($1, 'Proj', 'D', 'user', $2) RETURNING id",
    )
    .bind(format!("proj-{}", &uniq[..8]))
    .bind(user_id)
    .fetch_one(db)
    .await
    .unwrap();
    let sid: Uuid = sqlx::query_scalar(
        "INSERT INTO project_slices
            (project_id, slice_type, external_ref, title, description,
             acceptance_criteria, primary_domain, difficulty, status,
             created_by_user_id, ingested_from)
         VALUES ($1, 'other', $2, 'S', 'D', 'AC', 'code', 2, 'open', $3, 'manual')
         RETURNING id",
    )
    .bind(pid)
    .bind(format!("ref-{}", &uniq[..8]))
    .bind(user_id)
    .fetch_one(db)
    .await
    .unwrap();

    if let Some(slug) = skill_slug {
        let skill_id: Uuid =
            sqlx::query_scalar("SELECT id FROM skill_nodes WHERE slug = $1 LIMIT 1")
                .bind(slug)
                .fetch_one(db)
                .await
                .unwrap();
        sqlx::query(
            "INSERT INTO slice_skills (slice_id, skill_id, weight) VALUES ($1, $2, 1.0)
             ON CONFLICT DO NOTHING",
        )
        .bind(sid)
        .bind(skill_id)
        .execute(db)
        .await
        .unwrap();
    }

    sqlx::query_scalar(
        "INSERT INTO deliverables
            (challenge_id, user_id, slice_id, artifact_type, artifact_url,
             verifiable_by, verification_status)
         VALUES ($1, $2, $3, 'other', 'x', 'human_review', 'verified') RETURNING id",
    )
    .bind(cid)
    .bind(user_id)
    .bind(sid)
    .fetch_one(db)
    .await
    .unwrap()
}

// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn rule_awarded_when_min_count_met() {
    let (db, name) = setup_test_db().await;
    let u = create_user(&db).await;

    let rule_slug = "test-any-2";
    let rid = insert_rule(
        &db,
        rule_slug,
        serde_json::json!({
            "proof_types": ["deliverable_verified"], "min_count": 2
        }),
        "common",
    )
    .await;

    // 1 deliverable → pas encore
    insert_verified_deliverable(&db, u, None).await;
    let r = badge_engine::recompute_badges_for_user(&db, u)
        .await
        .unwrap();
    assert!(!r.awarded.contains(&rule_slug.to_string()));

    // 2ᵉ deliverable → award
    insert_verified_deliverable(&db, u, None).await;
    let r = badge_engine::recompute_badges_for_user(&db, u)
        .await
        .unwrap();
    assert!(r.awarded.contains(&rule_slug.to_string()));

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_badges WHERE user_id = $1 AND rule_id = $2 AND revoked_at IS NULL",
    )
    .bind(u).bind(rid).fetch_one(&db).await.unwrap();
    assert_eq!(count, 1);

    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn deprecated_rules_do_not_award() {
    let (db, name) = setup_test_db().await;
    let u = create_user(&db).await;
    for _ in 0..3 {
        insert_verified_deliverable(&db, u, None).await;
    }
    let r = badge_engine::recompute_badges_for_user(&db, u)
        .await
        .unwrap();
    // Aucun award pour les 9 legacy_* deprecated
    assert!(!r.awarded.iter().any(|s| s.starts_with("legacy_")));
    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn skill_tag_filter_isolates_matches() {
    let (db, name) = setup_test_db().await;
    let u = create_user(&db).await;

    insert_rule(
        &db,
        "test-react-1",
        serde_json::json!({
            "proof_types": ["deliverable_verified"],
            "min_count": 1,
            "skill_tag": "component-composition"
        }),
        "auto",
    )
    .await;

    // Deliverable sans lien slice_skill à ce slug → doit pas matcher
    insert_verified_deliverable(&db, u, Some("code-review")).await;
    let r = badge_engine::recompute_badges_for_user(&db, u)
        .await
        .unwrap();
    assert!(!r.awarded.contains(&"test-react-1".to_string()));

    // Deliverable avec le bon skill → match
    insert_verified_deliverable(&db, u, Some("component-composition")).await;
    let r = badge_engine::recompute_badges_for_user(&db, u)
        .await
        .unwrap();
    assert!(r.awarded.contains(&"test-react-1".to_string()));

    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn rarity_auto_derives_from_count() {
    let (db, name) = setup_test_db().await;
    let u = create_user(&db).await;

    let rid = insert_rule(
        &db,
        "test-auto-rare",
        serde_json::json!({
            "proof_types": ["deliverable_verified"], "min_count": 5
        }),
        "auto",
    )
    .await;
    for _ in 0..5 {
        insert_verified_deliverable(&db, u, None).await;
    }

    badge_engine::recompute_badges_for_user(&db, u)
        .await
        .unwrap();
    let rarity: String = sqlx::query_scalar("SELECT rarity FROM user_badges WHERE rule_id = $1")
        .bind(rid)
        .fetch_one(&db)
        .await
        .unwrap();
    // 5 matched → "rare" per resolve_rarity thresholds
    assert_eq!(rarity, "rare");

    db.close().await;
    cleanup_test_db(&name).await;
}

#[tokio::test]
async fn recompute_is_idempotent() {
    let (db, name) = setup_test_db().await;
    let u = create_user(&db).await;
    insert_rule(
        &db,
        "test-idem",
        serde_json::json!({
            "proof_types": ["deliverable_verified"], "min_count": 1
        }),
        "common",
    )
    .await;
    insert_verified_deliverable(&db, u, None).await;

    let r1 = badge_engine::recompute_badges_for_user(&db, u)
        .await
        .unwrap();
    assert_eq!(r1.awarded.len(), 1);
    let r2 = badge_engine::recompute_badges_for_user(&db, u)
        .await
        .unwrap();
    assert_eq!(r2.awarded.len(), 0, "no re-award on idempotent recompute");
    assert!(r2.unchanged >= 1);

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM user_badges WHERE user_id = $1")
        .bind(u)
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(count, 1, "single user_badge row");

    db.close().await;
    cleanup_test_db(&name).await;
}
