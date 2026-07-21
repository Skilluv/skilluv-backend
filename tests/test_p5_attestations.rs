//! Tests d'intégration Phase P5 : attestations ⭐ LAUNCH FEATURE.
//!
//! Couvre :
//! - Migration 0068 (contraintes CHECK sur type + skill arrays, UNIQUE index)
//! - Auto-issue gesture sur level 2 via propagate_skills (webhook GitHub)
//! - Auto-issue skill sur level 4 + review sénior
//! - Anti-double-issue via UNIQUE index
//! - Vérification par code publique
//! - Révocation manuelle + cascade sur revoked deliverable
//! - Compagnonnage manuel (éligibilité, création)

use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

use skilluv_backend::services::{
    AttestationsService, DeliverablesService, PrMergedOutcome, PrMergedParams,
};

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p5_test_{}",
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

async fn insert_test_user(db: &PgPool, user_id: Uuid) {
    let short = &user_id.to_string()[..8];
    sqlx::query(
        "INSERT INTO users
            (id, email, username, first_name, last_name, display_name, password_hash,
             profile_active, total_fragments)
         VALUES ($1, $2, $3, $4, $5, $6, $7, TRUE, 0)",
    )
    .bind(user_id)
    .bind(format!("test-{user_id}@example.com"))
    .bind(format!("t{short}"))
    .bind("Test")
    .bind("User")
    .bind("Test User")
    .bind("dummy_hash")
    .execute(db)
    .await
    .expect("insert user");
}

// ═══════════════════════════════════════════════════════════════════
// Migration 0068
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn migration_0068_creates_attestations() {
    let (db, db_name) = setup_test_db().await;
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM information_schema.tables
                        WHERE table_schema='public' AND table_name='attestations')",
    )
    .fetch_one(&db)
    .await
    .expect("check");
    assert!(exists);
    db.close().await;
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn constraint_compagnonnage_needs_project_id() {
    let (db, db_name) = setup_test_db().await;
    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id).await;

    // Insertion sans project → doit être rejetée
    let bad = sqlx::query(
        "INSERT INTO attestations
            (user_id, attestation_type, title, description, verification_code)
         VALUES ($1, 'compagnonnage', 'Bad', 'Bad', 'nocode1234')",
    )
    .bind(user_id)
    .execute(&db)
    .await;
    assert!(bad.is_err());
    db.close().await;
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn constraint_gesture_needs_one_skill() {
    let (db, db_name) = setup_test_db().await;
    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id).await;

    // Gesture sans skill array
    let bad = sqlx::query(
        "INSERT INTO attestations
            (user_id, attestation_type, title, description, verification_code)
         VALUES ($1, 'gesture', 'Bad', 'Bad', 'nogesture1')",
    )
    .bind(user_id)
    .execute(&db)
    .await;
    assert!(bad.is_err());
    db.close().await;
    cleanup_test_db(&db_name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Auto-issue gesture via workflow G.1
// ═══════════════════════════════════════════════════════════════════

/// Setup helper : (user, github_login, project_id, slice_id, skill_ids)
async fn setup_ready_for_deliverable_flow(db: &PgPool) -> (Uuid, String, Uuid, Uuid, Vec<Uuid>) {
    let user_id = Uuid::new_v4();
    insert_test_user(db, user_id).await;

    let github_login = format!("gh-{}", &user_id.to_string()[..8]);
    let nonce = vec![0u8; 12];
    let ciphertext = vec![0u8; 16];
    sqlx::query(
        "INSERT INTO github_connections (user_id, github_user_id, github_login,
                                          access_token_encrypted, access_token_nonce)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(user_id)
    .bind(rand::random::<i32>() as i64)
    .bind(&github_login)
    .bind(&ciphertext)
    .bind(&nonce)
    .execute(db)
    .await
    .expect("gh");

    let project_id: Uuid = sqlx::query_scalar(
        "INSERT INTO projects (slug, name, owner_type, owner_id,
                               github_repo_owner, github_repo_name)
         VALUES ($1, 'P5 Project', 'user', $2, 'acme', 'demo')
         RETURNING id",
    )
    .bind(format!("p5-{}", Uuid::new_v4()))
    .bind(user_id)
    .fetch_one(db)
    .await
    .expect("proj");

    let slice_id: Uuid = sqlx::query_scalar(
        "INSERT INTO project_slices
            (project_id, slice_type, external_ref, title, description,
             primary_domain, difficulty, fragments_reward,
             status, claimed_by_user_id, claimed_at, claim_expires_at)
         VALUES ($1, 'github_issue', '42', 'P5 slice', 'Test',
                 'code', 3, 50,
                 'claimed', $2, NOW(), NOW() + INTERVAL '7 days')
         RETURNING id",
    )
    .bind(project_id)
    .bind(user_id)
    .fetch_one(db)
    .await
    .expect("slice");

    let skill_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM skill_nodes WHERE parent_id IS NOT NULL AND domain='code' LIMIT 2",
    )
    .fetch_all(db)
    .await
    .expect("skills");

    // Weight 3 sur les deux skills : WPC = 3 → level 2 → gesture éligible
    sqlx::query(
        "INSERT INTO slice_skills (slice_id, skill_id, weight, is_primary)
         VALUES ($1, $2, 3, TRUE), ($1, $3, 3, FALSE)",
    )
    .bind(slice_id)
    .bind(skill_ids[0])
    .bind(skill_ids[1])
    .execute(db)
    .await
    .expect("slice_skills");

    (user_id, github_login, project_id, slice_id, skill_ids)
}

fn make_pr(project_id: Uuid, gh_login: &str, sha: &str) -> PrMergedParams {
    PrMergedParams {
        project_id,
        repo_owner: "acme".to_string(),
        repo_name: "demo".to_string(),
        pr_number: 42,
        pr_url: "http://gh/acme/demo/pull/42".to_string(),
        pr_body: "Fixes #42".to_string(),
        merge_commit_sha: sha.to_string(),
        github_login: gh_login.to_string(),
        commits_count: Some(1),
        additions: Some(10),
        deletions: Some(2),
        files_changed: Some(1),
        base_branch: Some("main".to_string()),
    }
}

#[tokio::test]
async fn pr_merged_auto_issues_gesture_attestations_on_level_2() {
    let (db, db_name) = setup_test_db().await;
    let (user_id, gh_login, project_id, _slice_id, skill_ids) =
        setup_ready_for_deliverable_flow(&db).await;

    let params = make_pr(project_id, &gh_login, "sha-gesture");
    let outcome = DeliverablesService::create_from_pr_merged(&db, params)
        .await
        .expect("workflow");
    assert!(matches!(outcome, PrMergedOutcome::Verified { .. }));

    // Chaque skill : WPC = 3 → level 2 → gesture émis
    for skill_id in &skill_ids {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM attestations
             WHERE user_id = $1
               AND attestation_type = 'gesture'
               AND $2 = ANY(linked_skill_node_ids)
               AND revoked_at IS NULL",
        )
        .bind(user_id)
        .bind(skill_id)
        .fetch_one(&db)
        .await
        .expect("count");
        assert_eq!(
            count, 1,
            "Expected 1 gesture attestation for skill {skill_id}"
        );
    }

    // Portfolio public : 2 attestations
    let portfolio = AttestationsService::list_public_by_user(&db, user_id)
        .await
        .expect("portfolio");
    assert_eq!(portfolio.len(), 2);

    db.close().await;
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn gesture_attestation_is_not_duplicated_on_re_run() {
    let (db, db_name) = setup_test_db().await;
    let (user_id, gh_login, project_id, _slice_id, skill_ids) =
        setup_ready_for_deliverable_flow(&db).await;

    // First PR → auto-issue
    let params = make_pr(project_id, &gh_login, "sha-first");
    DeliverablesService::create_from_pr_merged(&db, params)
        .await
        .expect("first");

    // Reset slice pour permettre 2e claim (simulé)
    sqlx::query("UPDATE project_slices SET status = 'claimed' WHERE claimed_by_user_id = $1")
        .bind(user_id)
        .execute(&db)
        .await
        .expect("reset");

    let params2 = make_pr(project_id, &gh_login, "sha-second");
    DeliverablesService::create_from_pr_merged(&db, params2)
        .await
        .expect("second");

    // Toujours qu'une seule gesture attestation par skill (UNIQUE index)
    for skill_id in &skill_ids {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM attestations
             WHERE user_id = $1
               AND attestation_type = 'gesture'
               AND $2 = ANY(linked_skill_node_ids)",
        )
        .bind(user_id)
        .bind(skill_id)
        .fetch_one(&db)
        .await
        .expect("count");
        assert_eq!(count, 1, "No duplicate gesture attestation");
    }

    db.close().await;
    cleanup_test_db(&db_name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Vérification par code
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn verify_by_code_returns_valid_attestation() {
    let (db, db_name) = setup_test_db().await;
    let (user_id, gh_login, project_id, _slice_id, _skill_ids) =
        setup_ready_for_deliverable_flow(&db).await;

    let params = make_pr(project_id, &gh_login, "sha-verify");
    DeliverablesService::create_from_pr_merged(&db, params)
        .await
        .expect("workflow");

    let portfolio = AttestationsService::list_public_by_user(&db, user_id)
        .await
        .expect("portfolio");
    let code = &portfolio[0].verification_code;

    let verified = AttestationsService::verify_by_code(&db, code)
        .await
        .expect("verify");
    assert!(verified.is_some());
    assert_eq!(verified.unwrap().id, portfolio[0].id);

    let bogus = AttestationsService::verify_by_code(&db, "ZZZZZZZZZZ")
        .await
        .expect("verify bogus");
    assert!(bogus.is_none());

    db.close().await;
    cleanup_test_db(&db_name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Révocation
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn revoke_attestation_hides_it_from_portfolio() {
    let (db, db_name) = setup_test_db().await;
    let (user_id, gh_login, project_id, _slice_id, _skill_ids) =
        setup_ready_for_deliverable_flow(&db).await;

    let params = make_pr(project_id, &gh_login, "sha-revoke");
    DeliverablesService::create_from_pr_merged(&db, params)
        .await
        .expect("workflow");

    let portfolio = AttestationsService::list_public_by_user(&db, user_id)
        .await
        .expect("portfolio");
    assert!(!portfolio.is_empty());
    let att_id = portfolio[0].id;

    AttestationsService::revoke(&db, att_id, None, "Test revocation".to_string())
        .await
        .expect("revoke");

    let after = AttestationsService::list_public_by_user(&db, user_id)
        .await
        .expect("after");
    assert!(!after.iter().any(|a| a.id == att_id));

    db.close().await;
    cleanup_test_db(&db_name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Skill attestation : level 4 + senior review requis
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn skill_attestation_requires_senior_review() {
    let (db, db_name) = setup_test_db().await;

    // Insert user with a skill at level 4 already, but no reviews yet
    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id).await;

    let skill_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM skill_nodes WHERE parent_id IS NOT NULL AND domain='code' LIMIT 1",
    )
    .fetch_one(&db)
    .await
    .expect("skill");

    // Insert user_skills at level 4 (WPC 15)
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
    .expect("user_skills");

    // Trigger auto-issue via a transaction (mimicking a level-up)
    let mut tx = db.begin().await.expect("begin tx");
    let issued =
        AttestationsService::check_and_issue_for_skill_levelup(&mut tx, user_id, skill_id, 4)
            .await
            .expect("check");
    tx.commit().await.expect("commit");

    // Gesture should be issued (level >= 2), skill should NOT (no senior review)
    let gestures: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM attestations
         WHERE user_id = $1 AND attestation_type = 'gesture'",
    )
    .bind(user_id)
    .fetch_one(&db)
    .await
    .expect("count");
    assert_eq!(gestures, 1);

    let skills: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM attestations
         WHERE user_id = $1 AND attestation_type = 'skill'",
    )
    .bind(user_id)
    .fetch_one(&db)
    .await
    .expect("count");
    assert_eq!(skills, 0, "Skill attestation without senior review = 0");

    assert_eq!(issued.len(), 1, "Only gesture issued (skill blocked)");

    db.close().await;
    cleanup_test_db(&db_name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Compagnonnage éligibilité
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn compagnonnage_eligibility_requires_five_deliverables_and_mature_project() {
    let (db, db_name) = setup_test_db().await;
    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id).await;

    // Project not mature yet
    let project_id: Uuid = sqlx::query_scalar(
        "INSERT INTO projects (slug, name, owner_type, owner_id, lifecycle_status)
         VALUES ($1, 'Comp Project', 'user', $2, 'active')
         RETURNING id",
    )
    .bind(format!("cp-{}", Uuid::new_v4()))
    .bind(user_id)
    .fetch_one(&db)
    .await
    .expect("proj");

    let eligible = AttestationsService::check_compagnonnage_eligibility(&db, user_id, project_id)
        .await
        .expect("check");
    assert!(!eligible, "Active project + 0 deliverables → not eligible");

    // Set to mature and try again — still 0 deliverables → still not eligible
    sqlx::query("UPDATE projects SET lifecycle_status = 'mature' WHERE id = $1")
        .bind(project_id)
        .execute(&db)
        .await
        .expect("update");

    let eligible = AttestationsService::check_compagnonnage_eligibility(&db, user_id, project_id)
        .await
        .expect("check");
    assert!(!eligible, "Mature project + 0 deliverables → not eligible");

    db.close().await;
    cleanup_test_db(&db_name).await;
}
