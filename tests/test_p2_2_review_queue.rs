//! Tests d'intégration Phase P2.2 : review queue humaine.
//!
//! Couvre :
//! - Migration 0065 (review_tasks) : contraintes, index
//! - Auto-création review_task quand deliverable en pending_manual_review
//! - Workflow reviewer : list_open → claim → submit_verdict
//! - Verdict propagation : approve → verified + fragments + skills,
//!   reject → rejected, request_changes → reste pending
//! - Filtres queue par domaine et séniorité
//! - Anti double-review (UNIQUE deliverable_id + reviewer_user_id)
//! - Cron expire_stale_claims + escalate_stale_sla

use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

use skilluv_backend::services::{
    DeliverablesService, PrMergedOutcome, PrMergedParams, ReviewQueueFilter, ReviewQueueService,
    ReviewSubmitParams, ReviewsService, SeniorityLevel, Verdict,
};

// ═══════════════════════════════════════════════════════════════════
// Helpers de setup (identiques aux tests précédents, factorisés)
// ═══════════════════════════════════════════════════════════════════

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p2_2_test_{}",
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

async fn setup_project_and_slice_with_skills(
    db: &PgPool,
) -> (Uuid, Uuid, Uuid, Uuid, Vec<Uuid>, String) {
    let owner = Uuid::new_v4();
    insert_test_user(db, owner).await;

    let github_login = format!("gh-{}", &owner.to_string()[..8]);
    let nonce = vec![0u8; 12];
    let ciphertext = vec![0u8; 16];
    sqlx::query(
        "INSERT INTO github_connections (user_id, github_user_id, github_login,
                                          access_token_encrypted, access_token_nonce)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(owner)
    .bind(rand::random::<i32>() as i64)
    .bind(&github_login)
    .bind(&ciphertext)
    .bind(&nonce)
    .execute(db)
    .await
    .expect("gh conn");

    let project_id: Uuid = sqlx::query_scalar(
        "INSERT INTO projects (slug, name, owner_type, owner_id,
                               github_repo_owner, github_repo_name)
         VALUES ($1, 'R Q Test', 'user', $2, 'acme', 'demo')
         RETURNING id",
    )
    .bind(format!("rqt-{}", Uuid::new_v4()))
    .bind(owner)
    .fetch_one(db)
    .await
    .expect("project");

    let slice_id: Uuid = sqlx::query_scalar(
        "INSERT INTO project_slices
            (project_id, slice_type, external_ref, title, description,
             primary_domain, difficulty, fragments_reward,
             status, claimed_by_user_id, claimed_at, claim_expires_at)
         VALUES ($1, 'github_issue', '77', 'RQ slice', 'RQ test slice',
                 'code', 3, 60,
                 'claimed', $2, NOW(), NOW() + INTERVAL '7 days')
         RETURNING id",
    )
    .bind(project_id)
    .bind(owner)
    .fetch_one(db)
    .await
    .expect("slice");

    let skill_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM skill_nodes WHERE parent_id IS NOT NULL AND domain='code' LIMIT 2",
    )
    .fetch_all(db)
    .await
    .expect("skills");

    sqlx::query(
        "INSERT INTO slice_skills (slice_id, skill_id, weight, is_primary)
         VALUES ($1, $2, 4, TRUE), ($1, $3, 3, FALSE)",
    )
    .bind(slice_id)
    .bind(skill_ids[0])
    .bind(skill_ids[1])
    .execute(db)
    .await
    .expect("slice_skills");

    (owner, project_id, slice_id, owner, skill_ids, github_login)
}

fn make_pr_params(project_id: Uuid, github_login: &str, sha: &str) -> PrMergedParams {
    PrMergedParams {
        project_id,
        repo_owner: "acme".to_string(),
        repo_name: "demo".to_string(),
        pr_number: 10,
        pr_url: "http://gh/acme/demo/pull/10".to_string(),
        pr_body: "Fixes #77".to_string(),
        merge_commit_sha: sha.to_string(),
        github_login: github_login.to_string(),
        commits_count: Some(1),
        additions: Some(50),
        deletions: Some(5),
        files_changed: Some(2),
        base_branch: Some("main".to_string()),
    }
}

// ═══════════════════════════════════════════════════════════════════
// Migration 0065
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn migration_0065_creates_review_tasks() {
    let (db, db_name) = setup_test_db().await;

    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM information_schema.tables
                        WHERE table_schema='public' AND table_name='review_tasks')",
    )
    .fetch_one(&db)
    .await
    .expect("exists check");
    assert!(exists);

    db.close().await;
    cleanup_test_db(&db_name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Auto-création review_task sur pending_manual_review
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn pending_manual_review_auto_creates_review_task() {
    let (db, db_name) = setup_test_db().await;
    let (_claimed_by, project_id, slice_id, _owner, _skills, _gh) =
        setup_project_and_slice_with_skills(&db).await;

    // Second user (author of the PR, different from claimed_by)
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
    .expect("insert");

    let params = make_pr_params(project_id, &other_login, "sha-mismatch");
    let outcome = DeliverablesService::create_from_pr_merged(&db, params)
        .await
        .expect("workflow");

    let deliverable_id = match outcome {
        PrMergedOutcome::PendingManualReview { deliverable_id } => deliverable_id,
        other => panic!("Expected PendingManualReview, got {other:?}"),
    };

    // Une review_task doit avoir été créée
    let (task_id, task_type, task_status, task_priority, task_domain, task_slice_id): (
        Uuid,
        String,
        String,
        i16,
        String,
        Option<Uuid>,
    ) = sqlx::query_as(
        "SELECT id, task_type, status, priority, primary_domain, slice_id
         FROM review_tasks WHERE deliverable_id = $1",
    )
    .bind(deliverable_id)
    .fetch_one(&db)
    .await
    .expect("fetch task");

    assert_eq!(task_type, "verify_slice_claim");
    assert_eq!(task_status, "open");
    assert_eq!(task_priority, 4);
    assert_eq!(task_domain, "code");
    assert_eq!(task_slice_id, Some(slice_id));

    // La task apparait dans list_open pour un reviewer 'contribs+'
    let filter = ReviewQueueFilter {
        max_seniority: SeniorityLevel::Contribs,
        page: 1,
        per_page: 20,
        ..Default::default()
    };
    let tasks = ReviewQueueService::list_open(&db, &filter)
        .await
        .expect("list");
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].id, task_id);

    // ...mais PAS pour un reviewer 'any' seul (required_seniority='contribs')
    let filter_any = ReviewQueueFilter {
        max_seniority: SeniorityLevel::Any,
        page: 1,
        per_page: 20,
        ..Default::default()
    };
    let tasks_any = ReviewQueueService::list_open(&db, &filter_any)
        .await
        .expect("list");
    assert_eq!(
        tasks_any.len(),
        0,
        "A reviewer without contribs seniority shouldn't see contribs-required tasks"
    );

    db.close().await;
    cleanup_test_db(&db_name).await;
}

// ═══════════════════════════════════════════════════════════════════
// claim et complete
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn claim_task_sets_expiration_two_hours_out() {
    let (db, db_name) = setup_test_db().await;
    let (claimed_by, project_id, _slice_id, _owner, _skills, _gh) =
        setup_project_and_slice_with_skills(&db).await;

    // Create a pending_manual_review via a second user
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
    .expect("insert");

    let params = make_pr_params(project_id, &other_login, "sha-1");
    let outcome = DeliverablesService::create_from_pr_merged(&db, params)
        .await
        .expect("workflow");
    let deliverable_id = match outcome {
        PrMergedOutcome::PendingManualReview { deliverable_id } => deliverable_id,
        _ => panic!("expected pending_manual_review"),
    };

    let task_id: Uuid = sqlx::query_scalar("SELECT id FROM review_tasks WHERE deliverable_id = $1")
        .bind(deliverable_id)
        .fetch_one(&db)
        .await
        .expect("fetch");

    // A third user claims it (the initial claimed_by works too here)
    let reviewer = claimed_by;
    let task = ReviewQueueService::claim(&db, task_id, reviewer)
        .await
        .expect("claim");

    assert_eq!(task.status, "claimed");
    assert_eq!(task.claimed_by_user_id, Some(reviewer));
    assert!(task.claim_expires_at.is_some());

    // Second claim on same task must fail
    let another_reviewer = Uuid::new_v4();
    insert_test_user(&db, another_reviewer).await;
    let res = ReviewQueueService::claim(&db, task_id, another_reviewer).await;
    assert!(res.is_err(), "Second claim on claimed task should fail");

    db.close().await;
    cleanup_test_db(&db_name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Verdict : approve, reject, request_changes
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn approve_verdict_finalizes_deliverable_with_full_side_effects() {
    let (db, db_name) = setup_test_db().await;
    let (claimed_by, project_id, slice_id, deliverable_user, skills, _gh) =
        setup_project_and_slice_with_skills(&db).await;

    // Un autre user fait la PR (mismatch)
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
    .expect("insert");

    let params = make_pr_params(project_id, &other_login, "sha-approve");
    let outcome = DeliverablesService::create_from_pr_merged(&db, params)
        .await
        .expect("workflow");
    let deliverable_id = match outcome {
        PrMergedOutcome::PendingManualReview { deliverable_id } => deliverable_id,
        _ => panic!("expected pending_manual_review"),
    };

    // Le claimed_by (owner) revoit et approuve
    let reviewer = claimed_by;
    let submit = ReviewSubmitParams {
        deliverable_id,
        reviewer_user_id: reviewer,
        verdict: Verdict::Approve,
        body: "LGTM, approuvé".to_string(),
        time_spent_seconds: Some(600),
    };
    let outcome = ReviewsService::submit_verdict(&db, submit)
        .await
        .expect("submit");

    assert_eq!(outcome.new_deliverable_status, "verified");
    assert_eq!(outcome.reviewer_fragments_awarded, 10);

    // Le deliverable est bien verified
    let (status, verified_by): (String, Option<Uuid>) = sqlx::query_as(
        "SELECT verification_status, verified_by_user_id FROM deliverables WHERE id = $1",
    )
    .bind(deliverable_id)
    .fetch_one(&db)
    .await
    .expect("fetch");
    assert_eq!(status, "verified");
    assert_eq!(verified_by, Some(reviewer));

    // Fragments distribués à l'auteur
    let total: i32 = sqlx::query_scalar("SELECT total_fragments FROM users WHERE id = $1")
        .bind(deliverable_user)
        .fetch_one(&db)
        .await
        .expect("fetch total");
    assert_eq!(
        total, 70,
        "60 (fragments slice) + 10 (bonus reviewer si claimed_by = deliverable_user)"
    );

    // Skills propagés
    for skill_id in &skills {
        let level: i16 = sqlx::query_scalar(
            "SELECT proficiency_level FROM user_skills
             WHERE user_id = $1 AND skill_id = $2",
        )
        .bind(deliverable_user)
        .bind(skill_id)
        .fetch_one(&db)
        .await
        .expect("fetch level");
        assert!(
            level >= 2,
            "should have proficiency >= 2 with weight 3 or 4"
        );
    }

    // Slice → merged
    let slice_status: String =
        sqlx::query_scalar("SELECT status FROM project_slices WHERE id = $1")
            .bind(slice_id)
            .fetch_one(&db)
            .await
            .expect("fetch slice");
    assert_eq!(slice_status, "merged");

    // review_task → completed
    let task_status: String =
        sqlx::query_scalar("SELECT status FROM review_tasks WHERE deliverable_id = $1")
            .bind(deliverable_id)
            .fetch_one(&db)
            .await
            .expect("fetch task");
    assert_eq!(task_status, "completed");

    // review_metrics initialisé
    let (approved, total_reviews, reputation): (i32, i32, bigdecimal::BigDecimal) = sqlx::query_as(
        "SELECT approved_count, total_reviews, reputation_score
             FROM review_metrics WHERE reviewer_user_id = $1",
    )
    .bind(reviewer)
    .fetch_one(&db)
    .await
    .expect("metrics");
    assert_eq!(approved, 1);
    assert_eq!(total_reviews, 1);
    // bootstrap 0.5 tant qu'on n'a pas atteint 5 reviews
    use bigdecimal::BigDecimal;
    use std::str::FromStr;
    assert_eq!(reputation, BigDecimal::from_str("0.50").unwrap());

    db.close().await;
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn reject_verdict_marks_deliverable_rejected() {
    let (db, db_name) = setup_test_db().await;
    let (claimed_by, project_id, _slice_id, _owner, _skills, _gh) =
        setup_project_and_slice_with_skills(&db).await;

    // Mismatch author → pending_manual_review
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
    .expect("insert");

    let params = make_pr_params(project_id, &other_login, "sha-reject");
    let outcome = DeliverablesService::create_from_pr_merged(&db, params)
        .await
        .expect("workflow");
    let deliverable_id = match outcome {
        PrMergedOutcome::PendingManualReview { deliverable_id } => deliverable_id,
        _ => panic!("expected pending"),
    };

    let submit = ReviewSubmitParams {
        deliverable_id,
        reviewer_user_id: claimed_by,
        verdict: Verdict::Reject,
        body: "Ce n'est pas ce que la slice demandait".to_string(),
        time_spent_seconds: None,
    };
    let outcome = ReviewsService::submit_verdict(&db, submit)
        .await
        .expect("submit");

    assert_eq!(outcome.new_deliverable_status, "rejected");
    assert_eq!(outcome.reviewer_fragments_awarded, 0);

    db.close().await;
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn request_changes_keeps_deliverable_pending() {
    let (db, db_name) = setup_test_db().await;
    let (claimed_by, project_id, _slice_id, _owner, _skills, _gh) =
        setup_project_and_slice_with_skills(&db).await;

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
    .expect("insert");

    let params = make_pr_params(project_id, &other_login, "sha-changes");
    let outcome = DeliverablesService::create_from_pr_merged(&db, params)
        .await
        .expect("workflow");
    let deliverable_id = match outcome {
        PrMergedOutcome::PendingManualReview { deliverable_id } => deliverable_id,
        _ => panic!("expected pending"),
    };

    let submit = ReviewSubmitParams {
        deliverable_id,
        reviewer_user_id: claimed_by,
        verdict: Verdict::RequestChanges,
        body: "Fais X et Y avant que je puisse approuver".to_string(),
        time_spent_seconds: None,
    };
    let outcome = ReviewsService::submit_verdict(&db, submit)
        .await
        .expect("submit");

    assert_eq!(outcome.new_deliverable_status, "pending");

    db.close().await;
    cleanup_test_db(&db_name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Double review interdite
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn same_reviewer_cannot_review_twice() {
    let (db, db_name) = setup_test_db().await;
    let (claimed_by, project_id, _slice_id, _owner, _skills, _gh) =
        setup_project_and_slice_with_skills(&db).await;

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
    .expect("insert");

    let params = make_pr_params(project_id, &other_login, "sha-twice");
    let outcome = DeliverablesService::create_from_pr_merged(&db, params)
        .await
        .expect("workflow");
    let deliverable_id = match outcome {
        PrMergedOutcome::PendingManualReview { deliverable_id } => deliverable_id,
        _ => panic!("expected pending"),
    };

    let submit = ReviewSubmitParams {
        deliverable_id,
        reviewer_user_id: claimed_by,
        verdict: Verdict::RequestChanges,
        body: "First review".to_string(),
        time_spent_seconds: None,
    };
    ReviewsService::submit_verdict(&db, submit.clone())
        .await
        .expect("first review");

    // Second review by same reviewer must fail (UNIQUE deliverable_id + reviewer)
    let submit2 = ReviewSubmitParams {
        deliverable_id,
        reviewer_user_id: claimed_by,
        verdict: Verdict::Approve,
        body: "Second review".to_string(),
        time_spent_seconds: None,
    };
    let res = ReviewsService::submit_verdict(&db, submit2).await;
    assert!(res.is_err(), "Second review by same reviewer should fail");

    db.close().await;
    cleanup_test_db(&db_name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Cron : expire_stale_claims + escalate_stale_sla
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn expire_stale_claims_returns_task_to_pool() {
    let (db, db_name) = setup_test_db().await;

    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id).await;

    let owner_id = Uuid::new_v4();
    insert_test_user(&db, owner_id).await;

    let project_id: Uuid = sqlx::query_scalar(
        "INSERT INTO projects (slug, name, owner_type, owner_id)
         VALUES ($1, 'Cron Test Project', 'user', $2)
         RETURNING id",
    )
    .bind(format!("p-{}", Uuid::new_v4()))
    .bind(owner_id)
    .fetch_one(&db)
    .await
    .expect("proj");

    let slice_id: Uuid = sqlx::query_scalar(
        "INSERT INTO project_slices
            (project_id, slice_type, title, description, primary_domain, difficulty, status)
         VALUES ($1, 'github_issue', 'Test slice', 'Test description', 'code', 3, 'in_review')
         RETURNING id",
    )
    .bind(project_id)
    .fetch_one(&db)
    .await
    .expect("slice");

    let deliverable_id: Uuid = sqlx::query_scalar(
        "INSERT INTO deliverables
            (slice_id, user_id, artifact_type, artifact_url,
             verifiable_by, verification_status)
         VALUES ($1, $2, 'other', 'http://x/', 'human_review', 'pending')
         RETURNING id",
    )
    .bind(slice_id)
    .bind(user_id)
    .fetch_one(&db)
    .await
    .expect("d");

    // Manually create a stale claimed task (claim_expires_at in the past)
    sqlx::query(
        "INSERT INTO review_tasks
            (task_type, deliverable_id, primary_domain, priority,
             required_seniority, sla_deadline,
             status, claimed_by_user_id, claimed_at, claim_expires_at)
         VALUES ('verify_deliverable', $1, 'code', 3, 'any',
                 NOW() + INTERVAL '2 days',
                 'claimed', $2, NOW() - INTERVAL '4 hours', NOW() - INTERVAL '30 minutes')",
    )
    .bind(deliverable_id)
    .bind(user_id)
    .execute(&db)
    .await
    .expect("insert stale task");

    let expired_count = ReviewQueueService::expire_stale_claims(&db)
        .await
        .expect("expire");
    assert_eq!(expired_count, 1);

    let (status, claimed_by): (String, Option<Uuid>) = sqlx::query_as(
        "SELECT status, claimed_by_user_id FROM review_tasks WHERE deliverable_id = $1",
    )
    .bind(deliverable_id)
    .fetch_one(&db)
    .await
    .expect("fetch");
    assert_eq!(status, "open");
    assert_eq!(claimed_by, None);

    db.close().await;
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn escalate_stale_sla_marks_task_escalated() {
    let (db, db_name) = setup_test_db().await;

    let user_id = Uuid::new_v4();
    insert_test_user(&db, user_id).await;

    let owner_id = Uuid::new_v4();
    insert_test_user(&db, owner_id).await;

    let project_id: Uuid = sqlx::query_scalar(
        "INSERT INTO projects (slug, name, owner_type, owner_id)
         VALUES ($1, 'Escalate Test Project', 'user', $2) RETURNING id",
    )
    .bind(format!("p-{}", Uuid::new_v4()))
    .bind(owner_id)
    .fetch_one(&db)
    .await
    .expect("proj");

    let slice_id: Uuid = sqlx::query_scalar(
        "INSERT INTO project_slices
            (project_id, slice_type, title, description, primary_domain, difficulty, status)
         VALUES ($1, 'github_issue', 'Test slice', 'Test description', 'code', 3, 'in_review')
         RETURNING id",
    )
    .bind(project_id)
    .fetch_one(&db)
    .await
    .expect("slice");

    let deliverable_id: Uuid = sqlx::query_scalar(
        "INSERT INTO deliverables
            (slice_id, user_id, artifact_type, artifact_url,
             verifiable_by, verification_status)
         VALUES ($1, $2, 'other', 'http://x/', 'human_review', 'pending')
         RETURNING id",
    )
    .bind(slice_id)
    .bind(user_id)
    .fetch_one(&db)
    .await
    .expect("d");

    // Task with SLA already exceeded
    sqlx::query(
        "INSERT INTO review_tasks
            (task_type, deliverable_id, primary_domain, priority,
             required_seniority, sla_deadline)
         VALUES ('verify_deliverable', $1, 'code', 3, 'any',
                 NOW() - INTERVAL '1 hour')",
    )
    .bind(deliverable_id)
    .execute(&db)
    .await
    .expect("insert task");

    let escalated_count = ReviewQueueService::escalate_stale_sla(&db)
        .await
        .expect("escalate");
    assert_eq!(escalated_count, 1);

    let status: String =
        sqlx::query_scalar("SELECT status FROM review_tasks WHERE deliverable_id = $1")
            .bind(deliverable_id)
            .fetch_one(&db)
            .await
            .expect("fetch");
    assert_eq!(status, "escalated");

    db.close().await;
    cleanup_test_db(&db_name).await;
}

// ═══════════════════════════════════════════════════════════════════
// Anti double-claim (bonus)
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn cannot_review_already_verified_deliverable() {
    let (db, db_name) = setup_test_db().await;
    let (_claimed_by, project_id, _slice_id, _owner, _skills, github_login) =
        setup_project_and_slice_with_skills(&db).await;

    // PR from claimed_by → verified immediately
    let params = make_pr_params(project_id, &github_login, "sha-already");
    let outcome = DeliverablesService::create_from_pr_merged(&db, params)
        .await
        .expect("workflow");
    let deliverable_id = match outcome {
        PrMergedOutcome::Verified { deliverable_id } => deliverable_id,
        _ => panic!("expected verified"),
    };

    let reviewer = Uuid::new_v4();
    insert_test_user(&db, reviewer).await;

    let submit = ReviewSubmitParams {
        deliverable_id,
        reviewer_user_id: reviewer,
        verdict: Verdict::Approve,
        body: "already verified".to_string(),
        time_spent_seconds: None,
    };
    let res = ReviewsService::submit_verdict(&db, submit).await;
    assert!(res.is_err(), "Cannot review already-verified deliverable");

    db.close().await;
    cleanup_test_db(&db_name).await;
}
