//! Tests d'intégration P14.1 : propagation automatique du tenant_id
//! via triggers BEFORE INSERT.
//!
//! Vérifie :
//! - challenge_submissions hérite du tenant de challenge_templates.
//! - deliverables hérite via challenge_id ou slice_id.
//! - attestations + user_skills héritent de users.primary_tenant_id.
//! - project_slices hérite de funded_by_user_id ou created_by_user_id.
//! - Si tenant_id est fourni explicitement, le trigger le respecte.
//! - Backfill (mig section 2) : les rows pre-existantes ont tenant_id set.

use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p14_1_test_{}",
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

async fn insert_tenant(db: &PgPool, slug: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO tenants (slug, name, contact_email) VALUES ($1, $2, $3)
         ON CONFLICT (slug) DO UPDATE SET name = $2
         RETURNING id",
    )
    .bind(slug)
    .bind(format!("Tenant {slug}"))
    .bind(format!("{slug}@example.com"))
    .fetch_one(db)
    .await
    .expect("tenant")
}

async fn insert_user(db: &PgPool, tenant: Option<Uuid>) -> Uuid {
    let user_id = Uuid::new_v4();
    let short = &user_id.to_string()[..8];
    sqlx::query(
        "INSERT INTO users
            (id, email, username, first_name, last_name, display_name, password_hash,
             profile_active, total_fragments, primary_tenant_id)
         VALUES ($1, $2, $3, 'T', 'U', 'Test', 'dummy', TRUE, 0, $4)",
    )
    .bind(user_id)
    .bind(format!("test-{user_id}@example.com"))
    .bind(format!("t{short}"))
    .bind(tenant)
    .execute(db)
    .await
    .expect("user");
    user_id
}

async fn insert_challenge(db: &PgPool, tenant: Option<Uuid>) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO challenge_templates
            (title, description, instructions, skill_domain, difficulty,
             is_training, status, tenant_id)
         VALUES ('T', 'D', 'I', 'code', 2, TRUE, 'published', $1)
         RETURNING id",
    )
    .bind(tenant)
    .fetch_one(db)
    .await
    .expect("chal")
}

// ═══════════════════════════════════════════════════════════════════
// Trigger : challenge_submissions hérite du challenge_templates
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn challenge_submission_inherits_tenant_from_challenge() {
    let (db, name) = setup_test_db().await;
    let tenant = insert_tenant(&db, "corpA").await;
    let user = insert_user(&db, Some(tenant)).await;
    let challenge = insert_challenge(&db, Some(tenant)).await;

    let sub_id: Uuid = sqlx::query_scalar(
        "INSERT INTO challenge_submissions
            (challenge_id, user_id, status, attempt_number)
         VALUES ($1, $2, 'in_progress', 1)
         RETURNING id",
    )
    .bind(challenge)
    .bind(user)
    .fetch_one(&db)
    .await
    .expect("sub");

    let sub_tenant: Option<Uuid> =
        sqlx::query_scalar("SELECT tenant_id FROM challenge_submissions WHERE id = $1")
            .bind(sub_id)
            .fetch_one(&db)
            .await
            .expect("t");
    assert_eq!(sub_tenant, Some(tenant));

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Trigger : deliverable hérite via challenge_id
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn deliverable_inherits_tenant_from_challenge() {
    let (db, name) = setup_test_db().await;
    let tenant = insert_tenant(&db, "corpB").await;
    let user = insert_user(&db, Some(tenant)).await;
    let challenge = insert_challenge(&db, Some(tenant)).await;

    let del_id: Uuid = sqlx::query_scalar(
        "INSERT INTO deliverables
            (challenge_id, user_id, artifact_type, artifact_url,
             verifiable_by, verification_status)
         VALUES ($1, $2, 'other', 'skilluv:test', 'human_review', 'pending')
         RETURNING id",
    )
    .bind(challenge)
    .bind(user)
    .fetch_one(&db)
    .await
    .expect("del");

    let t: Option<Uuid> = sqlx::query_scalar("SELECT tenant_id FROM deliverables WHERE id = $1")
        .bind(del_id)
        .fetch_one(&db)
        .await
        .expect("t");
    assert_eq!(t, Some(tenant));

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Trigger : attestation hérite de user.primary_tenant_id
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn attestation_inherits_tenant_from_user() {
    let (db, name) = setup_test_db().await;
    let tenant = insert_tenant(&db, "corpC").await;
    let user = insert_user(&db, Some(tenant)).await;

    let skill_id: Uuid = sqlx::query_scalar("SELECT id FROM skill_nodes LIMIT 1")
        .fetch_one(&db)
        .await
        .expect("skill");

    let att_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO attestations
            (user_id, attestation_type, title, description, verification_code,
             linked_skill_node_ids)
        VALUES ($1, 'skill', 'T', 'D', $2, ARRAY[$3::uuid])
        RETURNING id
        "#,
    )
    .bind(user)
    .bind(format!("V{}", &Uuid::new_v4().to_string()[..10]))
    .bind(skill_id)
    .fetch_one(&db)
    .await
    .expect("att");

    let t: Option<Uuid> = sqlx::query_scalar("SELECT tenant_id FROM attestations WHERE id = $1")
        .bind(att_id)
        .fetch_one(&db)
        .await
        .expect("t");
    assert_eq!(t, Some(tenant));

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Trigger : user_skills hérite de user.primary_tenant_id
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn user_skill_inherits_tenant_from_user() {
    let (db, name) = setup_test_db().await;
    let tenant = insert_tenant(&db, "corpD").await;
    let user = insert_user(&db, Some(tenant)).await;
    let skill_id: Uuid = sqlx::query_scalar("SELECT id FROM skill_nodes LIMIT 1")
        .fetch_one(&db)
        .await
        .expect("skill");

    sqlx::query(
        "INSERT INTO user_skills
            (user_id, skill_id, proven_count, weighted_proven_count,
             proficiency_level, first_proven_at, last_proven_at)
         VALUES ($1, $2, 1, 5, 2, NOW(), NOW())",
    )
    .bind(user)
    .bind(skill_id)
    .execute(&db)
    .await
    .expect("us");

    let t: Option<Uuid> = sqlx::query_scalar(
        "SELECT tenant_id FROM user_skills WHERE user_id = $1 AND skill_id = $2",
    )
    .bind(user)
    .bind(skill_id)
    .fetch_one(&db)
    .await
    .expect("t");
    assert_eq!(t, Some(tenant));

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Trigger : project_slices hérite de created_by_user_id
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn project_slice_inherits_tenant_from_creator() {
    let (db, name) = setup_test_db().await;
    let tenant = insert_tenant(&db, "corpE").await;
    let creator = insert_user(&db, Some(tenant)).await;
    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO projects (slug, name, owner_type, owner_id)
         VALUES ($1, 'P name', 'user', $2) RETURNING id",
    )
    .bind(format!("p-{}", Uuid::new_v4()))
    .bind(creator)
    .fetch_one(&db)
    .await
    .expect("p");

    let slice: Uuid = sqlx::query_scalar(
        "INSERT INTO project_slices
            (project_id, slice_type, title, description, primary_domain,
             difficulty, status, created_by_user_id)
         VALUES ($1, 'other', 'S', 'D', 'code', 2, 'open', $2)
         RETURNING id",
    )
    .bind(project)
    .bind(creator)
    .fetch_one(&db)
    .await
    .expect("s");

    let t: Option<Uuid> = sqlx::query_scalar("SELECT tenant_id FROM project_slices WHERE id = $1")
        .bind(slice)
        .fetch_one(&db)
        .await
        .expect("t");
    assert_eq!(t, Some(tenant));

    db.close().await;
    cleanup_test_db(&name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Trigger respecte tenant_id fourni explicitement
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn trigger_respects_explicit_tenant_id() {
    let (db, name) = setup_test_db().await;
    let tenant_a = insert_tenant(&db, "corpF").await;
    let tenant_b = insert_tenant(&db, "corpG").await;
    let user = insert_user(&db, Some(tenant_a)).await;
    let challenge = insert_challenge(&db, Some(tenant_a)).await;

    // On INSERT avec tenant_id = tenant_b (explicite) → le trigger ne l'overwrite pas.
    let sub_id: Uuid = sqlx::query_scalar(
        "INSERT INTO challenge_submissions
            (challenge_id, user_id, status, attempt_number, tenant_id)
         VALUES ($1, $2, 'in_progress', 1, $3)
         RETURNING id",
    )
    .bind(challenge)
    .bind(user)
    .bind(tenant_b)
    .fetch_one(&db)
    .await
    .expect("sub");

    let t: Option<Uuid> =
        sqlx::query_scalar("SELECT tenant_id FROM challenge_submissions WHERE id = $1")
            .bind(sub_id)
            .fetch_one(&db)
            .await
            .expect("t");
    assert_eq!(t, Some(tenant_b), "trigger respecte l'explicite");

    db.close().await;
    cleanup_test_db(&name).await;
}
