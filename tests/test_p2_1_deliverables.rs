//! Tests d'intégration Phase P2.1 : service `DeliverablesService` + workflow G.1
//! "PR mergée → deliverable auto-vérifié".
//!
//! Couvre :
//! - Migration 0064 (reviews + review_metrics)
//! - Résolution slice via marker body + Closes #N + best-effort claimed-by
//! - Vérification légitimité (author match / mismatch → pending_manual_review)
//! - Insertion transactionnelle deliverable + slice merged + fragments + skill propagation
//! - Idempotence (même artifact_hash)
//! - Propagation skills : proven_count, weighted_proven_count, proficiency_level

use bigdecimal::BigDecimal;
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::str::FromStr;
use uuid::Uuid;

use skilluv_backend::services::{DeliverablesService, PrMergedOutcome, PrMergedParams};

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p2_test_{}",
        Uuid::new_v4().to_string().replace('-', "")
    );

    let admin_pool = PgPoolOptions::new()
        .max_connections(2)
        .connect("postgres://skilluv:skilluv_secret@localhost:5433/skilluv")
        .await
        .expect("admin connect");

    sqlx::query(sqlx::AssertSqlSafe(format!(
        "CREATE DATABASE \"{db_name}\""
    )))
    .execute(&admin_pool)
    .await
    .expect("create db");

    admin_pool.close().await;

    let db_url = format!("postgres://skilluv:skilluv_secret@localhost:5433/{db_name}");
    let db = PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await
        .expect("test connect");

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
        .expect("admin connect");
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

async fn insert_test_user(db: &PgPool, user_id: Uuid) {
    let short = &user_id.to_string()[..8];
    sqlx::query(
        "INSERT INTO users
            (id, email, username, first_name, last_name, display_name, password_hash,
             profile_active, total_fragments)
         VALUES ($1, $2, $3, $4, $5, $6, $7, FALSE, 0)",
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

/// Setup : user + project (with GitHub repo) + claimed slice with skills.
/// Returns (user_id, github_login, project_id, slice_id, skill_ids)
async fn setup_claimed_slice_with_skills(
    db: &PgPool,
    slice_type: &str,
    external_ref: Option<&str>,
) -> (Uuid, String, Uuid, Uuid, Vec<Uuid>) {
    let user_id = Uuid::new_v4();
    insert_test_user(db, user_id).await;

    let github_login = format!("gh-{}", &user_id.to_string()[..8]);

    // Register the GitHub connection so the webhook can resolve the login
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
    .expect("insert github_connection");

    let project_id: Uuid = sqlx::query_scalar(
        "INSERT INTO projects (slug, name, owner_type, owner_id,
                               github_repo_owner, github_repo_name)
         VALUES ($1, 'Deliverables Test', 'user', $2, 'acme', 'demo')
         RETURNING id",
    )
    .bind(format!("dtest-{}", Uuid::new_v4()))
    .bind(user_id)
    .fetch_one(db)
    .await
    .expect("insert project");

    let slice_id: Uuid = sqlx::query_scalar(
        "INSERT INTO project_slices
            (project_id, slice_type, external_ref, title, description,
             primary_domain, difficulty, fragments_reward,
             status, claimed_by_user_id, claimed_at, claim_expires_at)
         VALUES ($1, $2, $3, 'Test slice', 'Test description',
                 'code', 3, 50,
                 'claimed', $4, NOW(), NOW() + INTERVAL '7 days')
         RETURNING id",
    )
    .bind(project_id)
    .bind(slice_type)
    .bind(external_ref)
    .bind(user_id)
    .fetch_one(db)
    .await
    .expect("insert slice");

    // Attach 2 skills to the slice
    let skill_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM skill_nodes WHERE parent_id IS NOT NULL AND domain='code' LIMIT 2",
    )
    .fetch_all(db)
    .await
    .expect("fetch skills");
    assert_eq!(skill_ids.len(), 2, "Need 2 seeded code skills");

    // First skill with weight 3 (primary), second with weight 2
    sqlx::query(
        "INSERT INTO slice_skills (slice_id, skill_id, weight, is_primary)
         VALUES ($1, $2, 3, TRUE), ($1, $3, 2, FALSE)",
    )
    .bind(slice_id)
    .bind(skill_ids[0])
    .bind(skill_ids[1])
    .execute(db)
    .await
    .expect("insert slice_skills");

    (user_id, github_login, project_id, slice_id, skill_ids)
}

fn make_params(
    project_id: Uuid,
    github_login: &str,
    pr_number: i32,
    pr_body: &str,
    merge_commit_sha: &str,
) -> PrMergedParams {
    PrMergedParams {
        project_id,
        repo_owner: "acme".to_string(),
        repo_name: "demo".to_string(),
        pr_number,
        pr_url: format!("https://github.com/acme/demo/pull/{pr_number}"),
        pr_body: pr_body.to_string(),
        merge_commit_sha: merge_commit_sha.to_string(),
        github_login: github_login.to_string(),
        commits_count: Some(3),
        additions: Some(100),
        deletions: Some(20),
        files_changed: Some(5),
        base_branch: Some("main".to_string()),
    }
}

// ═══════════════════════════════════════════════════════════════════
// Migration 0064
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn migration_0064_creates_reviews_and_metrics() {
    let (db, db_name) = setup_test_db().await;

    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT table_name FROM information_schema.tables
         WHERE table_schema='public' AND table_name IN ('reviews','review_metrics')",
    )
    .fetch_all(&db)
    .await
    .expect("query");
    assert_eq!(tables.len(), 2);

    // Default reputation_score = 0.5 (bootstrap)
    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id).await;

    let default_reputation: BigDecimal = sqlx::query_scalar(
        "INSERT INTO review_metrics (reviewer_user_id) VALUES ($1) RETURNING reputation_score",
    )
    .bind(user_id)
    .fetch_one(&db)
    .await
    .expect("insert");
    assert_eq!(
        default_reputation,
        BigDecimal::from_str("0.5").unwrap(),
        "reputation_score should default to 0.5"
    );

    db.close().await;
    cleanup_test_db(&db_name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Workflow G.1 : PR merged → verified
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn pr_merged_with_closes_marker_creates_verified_deliverable() {
    let (db, db_name) = setup_test_db().await;
    let (user_id, github_login, project_id, slice_id, skill_ids) =
        setup_claimed_slice_with_skills(&db, "github_issue", Some("42")).await;

    let params = make_params(
        project_id,
        &github_login,
        7,
        "Fixes #42 — the description here.",
        "abc123def456sha",
    );

    let outcome = DeliverablesService::create_from_pr_merged(&db, params)
        .await
        .expect("workflow");

    let deliverable_id = match outcome {
        PrMergedOutcome::Verified { deliverable_id } => deliverable_id,
        other => panic!("Expected Verified, got {other:?}"),
    };

    // Deliverable is verified with the right fields
    let (verification_status, artifact_hash, fragments_awarded): (String, Option<String>, i32) =
        sqlx::query_as(
            "SELECT verification_status, artifact_hash, fragments_awarded
             FROM deliverables WHERE id = $1",
        )
        .bind(deliverable_id)
        .fetch_one(&db)
        .await
        .expect("fetch deliverable");
    assert_eq!(verification_status, "verified");
    assert_eq!(artifact_hash.as_deref(), Some("abc123def456sha"));
    assert_eq!(fragments_awarded, 50);

    // Slice is now merged
    let slice_status: String =
        sqlx::query_scalar("SELECT status FROM project_slices WHERE id = $1")
            .bind(slice_id)
            .fetch_one(&db)
            .await
            .expect("fetch slice");
    assert_eq!(slice_status, "merged");

    // User fragments incremented
    let total_fragments: i32 =
        sqlx::query_scalar("SELECT total_fragments FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_one(&db)
            .await
            .expect("fetch user");
    assert_eq!(total_fragments, 50);

    // Skills propagated. Formule proficiency = min(5, ceil(log2(WPC + 1))) :
    //   WPC=2 → ceil(log2(3)) = 2 → level 2
    //   WPC=3 → ceil(log2(4)) = 2 → level 2
    for (i, skill_id) in skill_ids.iter().enumerate() {
        let expected_weight = if i == 0 { 3 } else { 2 };
        let (proven_count, wpc, level): (i32, i32, i16) = sqlx::query_as(
            "SELECT proven_count, weighted_proven_count, proficiency_level
             FROM user_skills WHERE user_id = $1 AND skill_id = $2",
        )
        .bind(user_id)
        .bind(skill_id)
        .fetch_one(&db)
        .await
        .expect("fetch user_skills");
        assert_eq!(proven_count, 1);
        assert_eq!(wpc, expected_weight);
        assert_eq!(level, 2, "WPC {expected_weight} → level 2 (formula)");
    }

    db.close().await;
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn pr_merged_with_skilluv_slice_marker_resolves_by_uuid() {
    let (db, db_name) = setup_test_db().await;
    let (_user, github_login, project_id, slice_id, _skills) =
        setup_claimed_slice_with_skills(&db, "cli_task", None).await;

    let body = format!("Some body text.\n\nSkilluv-Slice: {slice_id}\n\nMore text.");
    let params = make_params(project_id, &github_login, 11, &body, "sha-marker-test");

    let outcome = DeliverablesService::create_from_pr_merged(&db, params)
        .await
        .expect("workflow");

    assert!(matches!(outcome, PrMergedOutcome::Verified { .. }));

    db.close().await;
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn pr_merged_with_author_mismatch_creates_pending_manual_review() {
    let (db, db_name) = setup_test_db().await;
    let (_claimed_by, _github_login, project_id, slice_id, _skills) =
        setup_claimed_slice_with_skills(&db, "github_issue", Some("100")).await;

    // Add a second user with their own github_connection
    let other_user = Uuid::new_v4();
    insert_test_user(&db, other_user).await;
    let other_login = format!("gh-{}", &other_user.to_string()[..8]);
    let nonce = vec![0u8; 12];
    let ciphertext = vec![0u8; 16];
    sqlx::query(
        "INSERT INTO github_connections (user_id, github_user_id, github_login,
                                          access_token_encrypted, access_token_nonce)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(other_user)
    .bind(rand::random::<i32>() as i64)
    .bind(&other_login)
    .bind(&ciphertext)
    .bind(&nonce)
    .execute(&db)
    .await
    .expect("insert other github_connection");

    let params = make_params(
        project_id,
        &other_login, // ← author ≠ claimed_by
        99,
        "Closes #100",
        "sha-mismatch-test",
    );

    let outcome = DeliverablesService::create_from_pr_merged(&db, params)
        .await
        .expect("workflow");

    let deliverable_id = match outcome {
        PrMergedOutcome::PendingManualReview { deliverable_id } => deliverable_id,
        other => panic!("Expected PendingManualReview, got {other:?}"),
    };

    let verification_status: String =
        sqlx::query_scalar("SELECT verification_status FROM deliverables WHERE id = $1")
            .bind(deliverable_id)
            .fetch_one(&db)
            .await
            .expect("fetch");
    assert_eq!(verification_status, "pending_manual_review");

    // Slice moved to in_review (not merged)
    let slice_status: String =
        sqlx::query_scalar("SELECT status FROM project_slices WHERE id = $1")
            .bind(slice_id)
            .fetch_one(&db)
            .await
            .expect("fetch slice");
    assert_eq!(slice_status, "in_review");

    db.close().await;
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn pr_merged_without_matching_slice_returns_no_matching_slice() {
    let (db, db_name) = setup_test_db().await;

    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id).await;

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
    .execute(&db)
    .await
    .expect("insert");

    let project_id: Uuid = sqlx::query_scalar(
        "INSERT INTO projects (slug, name, owner_type, owner_id, github_repo_owner, github_repo_name)
         VALUES ($1, 'Empty Project', 'user', $2, 'acme', 'demo')
         RETURNING id",
    )
    .bind(format!("ep-{}", Uuid::new_v4()))
    .bind(user_id)
    .fetch_one(&db)
    .await
    .expect("insert project");

    let params = make_params(
        project_id,
        &github_login,
        99,
        "No marker, no closes anywhere",
        "sha-no-slice",
    );

    let outcome = DeliverablesService::create_from_pr_merged(&db, params)
        .await
        .expect("workflow");

    assert!(matches!(outcome, PrMergedOutcome::NoMatchingSlice));

    db.close().await;
    cleanup_test_db(&db_name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Idempotence
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn pr_merged_same_sha_is_idempotent() {
    let (db, db_name) = setup_test_db().await;
    let (_user, github_login, project_id, _slice, _skills) =
        setup_claimed_slice_with_skills(&db, "github_issue", Some("7")).await;

    let params = make_params(project_id, &github_login, 1, "Closes #7", "same-sha-please");

    let first = DeliverablesService::create_from_pr_merged(&db, params.clone())
        .await
        .expect("first");
    let first_id = match first {
        PrMergedOutcome::Verified { deliverable_id } => deliverable_id,
        other => panic!("Expected first Verified, got {other:?}"),
    };

    // Note : after the first run, slice.status = 'merged', so the second call
    // will resolve the same slice but find it in a non-actionable state.
    // We can simulate a webhook redelivery by resetting the slice status
    // to 'claimed' first (this is not idempotent as far as slice state, only
    // the deliverable insert should be idempotent).
    sqlx::query(
        "UPDATE project_slices SET status = 'claimed' WHERE id IN
         (SELECT slice_id FROM deliverables WHERE id = $1)",
    )
    .bind(first_id)
    .execute(&db)
    .await
    .expect("reset slice for idempotence test");

    let second = DeliverablesService::create_from_pr_merged(&db, params)
        .await
        .expect("second");

    match second {
        PrMergedOutcome::AlreadyProcessed { deliverable_id } => {
            assert_eq!(
                deliverable_id, first_id,
                "Should return same deliverable_id"
            );
        }
        other => panic!("Expected AlreadyProcessed, got {other:?}"),
    }

    // Count : still exactly one deliverable
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM deliverables")
        .fetch_one(&db)
        .await
        .expect("count");
    assert_eq!(count, 1);

    db.close().await;
    cleanup_test_db(&db_name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Lecture publique (portfolio)
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn list_public_by_user_returns_only_verified_public_non_revoked() {
    let (db, db_name) = setup_test_db().await;
    let (user_id, github_login, project_id, _slice, _skills) =
        setup_claimed_slice_with_skills(&db, "github_issue", Some("55")).await;

    let params = make_params(project_id, &github_login, 1, "Fixes #55", "verified-sha");
    let outcome = DeliverablesService::create_from_pr_merged(&db, params)
        .await
        .expect("workflow");
    let verified_id = match outcome {
        PrMergedOutcome::Verified { deliverable_id } => deliverable_id,
        _ => panic!("expected Verified"),
    };

    // Add a pending deliverable manually (should not appear in public list)
    sqlx::query(
        "INSERT INTO deliverables
            (slice_id, user_id, artifact_type, artifact_url, verifiable_by,
             verification_status)
         VALUES (
            (SELECT slice_id FROM deliverables WHERE id = $1),
            $2, 'other', 'http://x.com/pending', 'human_review', 'pending'
         )",
    )
    .bind(verified_id)
    .bind(user_id)
    .execute(&db)
    .await
    .expect("insert pending");

    let public_deliverables = DeliverablesService::list_public_by_user(&db, user_id, 10)
        .await
        .expect("list");
    assert_eq!(public_deliverables.len(), 1);
    assert_eq!(public_deliverables[0].id, verified_id);

    db.close().await;
    cleanup_test_db(&db_name).await;
}
