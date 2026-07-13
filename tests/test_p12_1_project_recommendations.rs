//! Tests d'intégration P12.1 : recommend_for_user.
//!
//! Vérifie que :
//! - User sans skills prouvés → vec vide.
//! - Le matching se fait sur `projects.skill_domains && user_top_domains`.
//! - Les projets où le user a déjà un deliverable verified sont exclus.
//! - Le score intègre health_score + looking_for_contributors boost.
//! - Le tri est par match_score DESC.

use bigdecimal::BigDecimal;
use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

use skilluv_backend::services::projects;

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p12_1_test_{}",
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

async fn insert_user(db: &PgPool) -> Uuid {
    let user_id = Uuid::new_v4();
    let short = &user_id.to_string()[..8];
    sqlx::query(
        "INSERT INTO users
            (id, email, username, first_name, last_name, display_name, password_hash,
             profile_active, total_fragments)
         VALUES ($1, $2, $3, 'T', 'U', 'Test', 'dummy', TRUE, 0)",
    )
    .bind(user_id)
    .bind(format!("test-{user_id}@example.com"))
    .bind(format!("t{short}"))
    .execute(db)
    .await
    .expect("insert user");
    user_id
}

async fn add_user_skill(db: &PgPool, user_id: Uuid, slug: &str, wpc: i32) {
    let skill_id: Uuid = sqlx::query_scalar("SELECT id FROM skill_nodes WHERE slug = $1")
        .bind(slug)
        .fetch_one(db)
        .await
        .expect("skill_id");
    sqlx::query(
        "INSERT INTO user_skills
            (user_id, skill_id, proven_count, weighted_proven_count, proficiency_level,
             first_proven_at, last_proven_at)
         VALUES ($1, $2, 1, $3, 1, NOW(), NOW())
         ON CONFLICT (user_id, skill_id) DO UPDATE SET weighted_proven_count = $3",
    )
    .bind(user_id)
    .bind(skill_id)
    .bind(wpc)
    .execute(db)
    .await
    .expect("insert user_skills");
}

async fn insert_project(
    db: &PgPool,
    owner: Uuid,
    name: &str,
    skill_domains: &[&str],
    health: Option<f64>,
    looking: bool,
) -> Uuid {
    let health_bd: Option<BigDecimal> = health
        .and_then(|h| BigDecimal::try_from(h).ok());
    sqlx::query_scalar(
        r#"
        INSERT INTO projects
            (slug, name, owner_type, owner_id, skill_domains,
             health_score, looking_for_contributors)
        VALUES ($1, $2, 'user', $3, $4, $5, $6)
        RETURNING id
        "#,
    )
    .bind(format!("p-{}", Uuid::new_v4()))
    .bind(name)
    .bind(owner)
    .bind(skill_domains)
    .bind(health_bd)
    .bind(looking)
    .fetch_one(db)
    .await
    .expect("insert project")
}

// ═══════════════════════════════════════════════════════════════════
// User sans skill → vec vide
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn user_without_skills_gets_empty_recommendations() {
    let (db, name) = setup_test_db().await;
    let user = insert_user(&db).await;

    let recos = projects::recommend_for_user(&db, user, 10).await.expect("r");
    assert!(recos.is_empty());

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Matching sur skill_domains && user_top_domains
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn recommends_projects_matching_user_domains() {
    let (db, name) = setup_test_db().await;
    let user = insert_user(&db).await;
    add_user_skill(&db, user, "rust", 20).await; // domain=code

    let owner = insert_user(&db).await;
    let code_project = insert_project(&db, owner, "Code Proj", &["code"], Some(0.8), false).await;
    let _design_project = insert_project(&db, owner, "Design", &["design"], Some(0.9), true).await;

    let recos = projects::recommend_for_user(&db, user, 10).await.expect("r");
    let ids: Vec<Uuid> = recos.iter().map(|r| r.project.id).collect();
    assert!(ids.contains(&code_project), "code project doit être recommandé");
    assert_eq!(ids.len(), 1, "seul le code project match les domaines du user");

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Exclusion des projets où user a déjà un deliverable verified
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn excludes_projects_with_existing_verified_deliverable() {
    let (db, name) = setup_test_db().await;
    let user = insert_user(&db).await;
    add_user_skill(&db, user, "rust", 15).await;

    let owner = insert_user(&db).await;
    let project_a = insert_project(&db, owner, "Project Alpha", &["code"], Some(0.9), true).await;
    let project_b = insert_project(&db, owner, "Project Beta", &["code"], Some(0.7), false).await;

    // Crée slice + deliverable verified sur project_a
    let slice_a: Uuid = sqlx::query_scalar(
        "INSERT INTO project_slices
            (project_id, slice_type, title, description, primary_domain, difficulty, status)
         VALUES ($1, 'other', 'Done', 'D', 'code', 2, 'merged')
         RETURNING id",
    )
    .bind(project_a)
    .fetch_one(&db)
    .await
    .expect("slice");
    sqlx::query(
        "INSERT INTO deliverables
            (slice_id, user_id, artifact_type, artifact_url,
             verifiable_by, verification_status, verified_at)
         VALUES ($1, $2, 'other', 'skilluv:proof:1',
                 'human_review', 'verified', NOW())",
    )
    .bind(slice_a)
    .bind(user)
    .execute(&db)
    .await
    .expect("deliverable");

    let recos = projects::recommend_for_user(&db, user, 10).await.expect("r");
    let ids: Vec<Uuid> = recos.iter().map(|r| r.project.id).collect();
    assert!(!ids.contains(&project_a), "project A exclu (deliverable verified)");
    assert!(ids.contains(&project_b), "project B recommandé");

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Score : health + contributor boost
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn score_reflects_health_and_contributor_boost() {
    let (db, name) = setup_test_db().await;
    let user = insert_user(&db).await;
    add_user_skill(&db, user, "rust", 100).await; // WPC=100 sur code

    let owner = insert_user(&db).await;
    // A : health 0.5, pas de boost
    let a = insert_project(&db, owner, "Project Alpha", &["code"], Some(0.5), false).await;
    // B : health 0.5, avec boost x1.5
    let b = insert_project(&db, owner, "Project Bravo", &["code"], Some(0.5), true).await;
    // C : health 1.0, pas de boost → même score que B ?
    let c = insert_project(&db, owner, "Project Charlie", &["code"], Some(1.0), false).await;

    let recos = projects::recommend_for_user(&db, user, 10).await.expect("r");
    let map: std::collections::HashMap<Uuid, f64> =
        recos.iter().map(|r| (r.project.id, r.match_score)).collect();

    // A : 100 × 0.5 × 1.0 = 50
    // B : 100 × 0.5 × 1.5 = 75
    // C : 100 × 1.0 × 1.0 = 100
    assert!((map[&a] - 50.0).abs() < 0.1, "A ~50, got {}", map[&a]);
    assert!((map[&b] - 75.0).abs() < 0.1, "B ~75, got {}", map[&b]);
    assert!((map[&c] - 100.0).abs() < 0.1, "C ~100, got {}", map[&c]);

    // Tri : C > B > A
    let ids: Vec<Uuid> = recos.iter().map(|r| r.project.id).collect();
    assert_eq!(ids[0], c, "premier = plus haut score");
    assert_eq!(ids[1], b);
    assert_eq!(ids[2], a);

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Projet archivé exclu
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn archived_projects_excluded() {
    let (db, name) = setup_test_db().await;
    let user = insert_user(&db).await;
    add_user_skill(&db, user, "rust", 5).await;
    let owner = insert_user(&db).await;
    let project = insert_project(&db, owner, "Old", &["code"], Some(0.9), true).await;

    sqlx::query("UPDATE projects SET archived_at = NOW() WHERE id = $1")
        .bind(project)
        .execute(&db)
        .await
        .expect("archive");

    let recos = projects::recommend_for_user(&db, user, 10).await.expect("r");
    assert!(recos.is_empty(), "projets archivés ne remontent pas");

    db.close().await;
    cleanup_test_db(&name).await;
}
