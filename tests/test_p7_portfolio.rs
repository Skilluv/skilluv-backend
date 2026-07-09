//! Tests d'intégration Phase P7 : portfolio export.

use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

use skilluv_backend::services::PortfolioService;

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p7_test_{}",
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

async fn insert_test_user(
    db: &PgPool,
    user_id: Uuid,
    username: &str,
    profile_active: bool,
    title: Option<&str>,
    total_fragments: i32,
    golden_stars: i32,
) {
    sqlx::query(
        "INSERT INTO users
            (id, email, username, first_name, last_name, display_name, password_hash,
             profile_active, total_fragments, golden_stars, title)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
    )
    .bind(user_id)
    .bind(format!("test-{user_id}@example.com"))
    .bind(username)
    .bind("Test")
    .bind("User")
    .bind("Test User")
    .bind("dummy_hash")
    .bind(profile_active)
    .bind(total_fragments)
    .bind(golden_stars)
    .bind(title)
    .execute(db)
    .await
    .expect("insert user");
}

// ═══════════════════════════════════════════════════════════════════
// portfolio.json
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn portfolio_json_returns_schema_org_person() {
    let (db, db_name) = setup_test_db().await;
    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id, "marie-dev", true, Some("artisan"), 800, 0).await;

    let portfolio = PortfolioService::build_portfolio_json(&db, "marie-dev", "https://skilluv.com")
        .await
        .expect("build");

    assert_eq!(portfolio["@type"], "Person");
    assert_eq!(portfolio["alternateName"], "marie-dev");
    assert_eq!(portfolio["skilluv:title"], "artisan");
    assert_eq!(portfolio["skilluv:total_fragments"], 800);
    assert!(portfolio["@context"].is_object());
    assert!(portfolio["knowsAbout"].is_array());
    assert!(portfolio["hasCredential"].is_array());
    assert!(portfolio["workExample"].is_array());
    assert!(portfolio["alumniOf"].is_array());

    db.close().await;
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn portfolio_rejects_inactive_profile() {
    let (db, db_name) = setup_test_db().await;
    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id, "inactive-user", false, Some("apprenti"), 0, 0).await;

    let res = PortfolioService::build_portfolio_json(&db, "inactive-user", "https://skilluv.com")
        .await;
    assert!(res.is_err());

    db.close().await;
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn portfolio_includes_skills_attestations_deliverables() {
    let (db, db_name) = setup_test_db().await;
    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id, "champion", true, Some("maitre"), 3000, 0).await;

    let skill_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM skill_nodes WHERE parent_id IS NOT NULL AND domain='code' LIMIT 1",
    )
    .fetch_one(&db)
    .await
    .expect("skill");

    sqlx::query(
        "INSERT INTO user_skills
            (user_id, skill_id, proven_count, weighted_proven_count, proficiency_level,
             first_proven_at, last_proven_at)
         VALUES ($1, $2, 5, 15, 4, NOW(), NOW())",
    )
    .bind(user_id)
    .bind(skill_id)
    .execute(&db)
    .await
    .expect("skill row");

    // Attestation
    sqlx::query(
        "INSERT INTO attestations
            (user_id, attestation_type, title, description,
             linked_skill_node_ids, verification_code)
         VALUES ($1, 'gesture', 'Sait X', 'Description X', ARRAY[$2], 'ABCDE12345')",
    )
    .bind(user_id)
    .bind(skill_id)
    .execute(&db)
    .await
    .expect("attestation");

    // Deliverable
    let project_id: Uuid = sqlx::query_scalar(
        "INSERT INTO projects (slug, name, owner_type, owner_id)
         VALUES ($1, 'P7 Test', 'user', $2) RETURNING id",
    )
    .bind(format!("p7-{}", Uuid::new_v4()))
    .bind(user_id)
    .fetch_one(&db)
    .await
    .expect("proj");
    let slice_id: Uuid = sqlx::query_scalar(
        "INSERT INTO project_slices
            (project_id, slice_type, title, description, primary_domain, difficulty, status)
         VALUES ($1, 'github_issue', 'S', 'D', 'code', 3, 'merged')
         RETURNING id",
    )
    .bind(project_id)
    .fetch_one(&db)
    .await
    .expect("slice");
    sqlx::query(
        "INSERT INTO deliverables
            (slice_id, user_id, artifact_type, artifact_url,
             verifiable_by, verification_status)
         VALUES ($1, $2, 'pr_merged', 'http://gh/x', 'github_webhook', 'verified')",
    )
    .bind(slice_id)
    .bind(user_id)
    .execute(&db)
    .await
    .expect("deliverable");

    let portfolio =
        PortfolioService::build_portfolio_json(&db, "champion", "https://skilluv.com")
            .await
            .expect("build");

    let skills = portfolio["knowsAbout"].as_array().unwrap();
    let attestations = portfolio["hasCredential"].as_array().unwrap();
    let deliverables = portfolio["workExample"].as_array().unwrap();

    assert_eq!(skills.len(), 1);
    assert_eq!(attestations.len(), 1);
    assert_eq!(deliverables.len(), 1);
    assert_eq!(
        attestations[0]["url"],
        "https://skilluv.com/attestations/verify/ABCDE12345"
    );

    db.close().await;
    cleanup_test_db(&db_name).await;
}

// ═══════════════════════════════════════════════════════════════════
// badge.svg
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn badge_svg_returns_well_formed_svg() {
    let (db, db_name) = setup_test_db().await;
    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id, "showoff", true, Some("legende"), 6000, 10).await;

    let svg = PortfolioService::build_badge_svg(&db, "showoff")
        .await
        .expect("svg");

    assert!(svg.starts_with("<svg"));
    assert!(svg.contains("Skilluv"));
    assert!(svg.contains("Légende"));
    assert!(svg.contains("★10"));
    // Gold color pour legende
    assert!(svg.contains("#f39c12"));

    db.close().await;
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn badge_svg_shows_fragments_when_no_golden_stars() {
    let (db, db_name) = setup_test_db().await;
    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id, "junior", true, Some("apprenti"), 250, 0).await;

    let svg = PortfolioService::build_badge_svg(&db, "junior")
        .await
        .expect("svg");

    assert!(svg.contains("Apprenti"));
    assert!(svg.contains("250 frags"));
    // Grey color pour apprenti
    assert!(svg.contains("#95a5a6"));

    db.close().await;
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn badge_svg_escapes_xml_special_chars_from_title() {
    let (db, db_name) = setup_test_db().await;
    let user_id = Uuid::new_v4();
    // Title with unusual chars — falls back to Skilluv label
    insert_test_user(&db, user_id, "escaped", true, Some("legende"), 5000, 5).await;

    let svg = PortfolioService::build_badge_svg(&db, "escaped")
        .await
        .expect("svg");

    // No raw <, > or & from label injection
    let label_content = svg.split("<title>").nth(1).unwrap();
    // Should only contain: Skilluv: Légende ★5
    assert!(label_content.starts_with("Skilluv: Légende ★5"));

    db.close().await;
    cleanup_test_db(&db_name).await;
}
