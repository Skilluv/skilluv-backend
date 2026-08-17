//! Attestations that generate themselves from AI artefacts, and the four
//! things that must stay true about them.

mod common;
use common::TestApp;
use skilluv_backend::services::ai_attestations;
use uuid::Uuid;

async fn a_user(app: &TestApp, username: &str) -> Uuid {
    app.register_user(username).await;
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT id FROM users WHERE username = '{username}'"
    )))
    .fetch_one(&app.db)
    .await
    .unwrap()
}

async fn a_project(app: &TestApp, owner: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO projects (slug, name, owner_type, owner_id)
         VALUES ($1, 'Projet IA', 'user', $2)
         RETURNING id",
    )
    .bind(format!("proj-{}", Uuid::new_v4().simple()))
    .bind(owner)
    .fetch_one(&app.db)
    .await
    .unwrap()
}

/// An artefact of the given subtype, delivered and verified.
async fn a_verified_artifact(
    app: &TestApp,
    project: Uuid,
    user: Uuid,
    subtype: &str,
    orientation: &str,
) -> Uuid {
    let slice: Uuid = sqlx::query_scalar(
        "INSERT INTO project_slices
            (project_id, title, description, primary_domain, slice_type,
             ai_subtype, ai_external_hosting_url, difficulty, orientation_id)
         VALUES ($1, 'Artefact', 'x', 'ai', 'ai_artifact', $2,
                 'https://huggingface.co/skilluv/demo', 3,
                 (SELECT id FROM orientations WHERE slug = $3))
         RETURNING id",
    )
    .bind(project)
    .bind(subtype)
    .bind(orientation)
    .fetch_one(&app.db)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO deliverables
            (user_id, slice_id, artifact_type, artifact_url, verifiable_by,
             verification_status, verified_at)
         VALUES ($1, $2, 'other', 'https://example.test/a', 'human_review',
                 'verified', NOW())",
    )
    .bind(user)
    .bind(slice)
    .execute(&app.db)
    .await
    .unwrap();

    slice
}

async fn bases_held_by(app: &TestApp, user: Uuid) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT basis FROM attestations
          WHERE user_id = $1 AND revoked_at IS NULL AND basis IS NOT NULL
          ORDER BY basis",
    )
    .bind(user)
    .fetch_all(&app.db)
    .await
    .unwrap()
}

#[tokio::test]
async fn a_shipped_model_attests_itself() {
    let app = TestApp::spawn().await;
    let user = a_user(&app, "att_model").await;
    let project = a_project(&app, user).await;
    let slice = a_verified_artifact(&app, project, user, "ml_model", "ml-engineer").await;

    let issued = ai_attestations::issue_for_slice(&app.db, slice)
        .await
        .unwrap();
    assert_eq!(issued, vec!["ai_model_shipped"]);
    assert_eq!(bases_held_by(&app, user).await, vec!["ai_model_shipped"]);
}

#[tokio::test]
async fn running_the_generator_twice_issues_nothing_new() {
    let app = TestApp::spawn().await;
    let user = a_user(&app, "att_twice").await;
    let project = a_project(&app, user).await;
    let slice = a_verified_artifact(&app, project, user, "dataset", "data-engineer").await;

    ai_attestations::issue_for_slice(&app.db, slice)
        .await
        .unwrap();
    let second = ai_attestations::issue_for_slice(&app.db, slice)
        .await
        .unwrap();

    // What makes it safe to run from a hook that does not remember.
    assert!(second.is_empty(), "the second pass must issue nothing");
    assert_eq!(bases_held_by(&app, user).await.len(), 1);
}

#[tokio::test]
async fn two_models_in_one_project_both_count() {
    let app = TestApp::spawn().await;
    let user = a_user(&app, "att_two").await;
    let project = a_project(&app, user).await;

    let first = a_verified_artifact(&app, project, user, "ml_model", "ml-engineer").await;
    let second = a_verified_artifact(&app, project, user, "ml_model", "ml-engineer").await;

    ai_attestations::issue_for_slice(&app.db, first)
        .await
        .unwrap();
    let issued = ai_attestations::issue_for_slice(&app.db, second)
        .await
        .unwrap();

    // The index from 0068 allowed one skill attestation per skill node, so
    // the second one used to vanish. Two models is two pieces of work.
    assert_eq!(issued, vec!["ai_model_shipped"]);
    assert_eq!(bases_held_by(&app, user).await.len(), 2);
}

#[tokio::test]
async fn unverified_work_attests_nothing() {
    let app = TestApp::spawn().await;
    let user = a_user(&app, "att_pending").await;
    let project = a_project(&app, user).await;

    let slice: Uuid = sqlx::query_scalar(
        "INSERT INTO project_slices
            (project_id, title, description, primary_domain, slice_type,
             ai_subtype, ai_external_hosting_url, difficulty, orientation_id)
         VALUES ($1, 'En attente', 'x', 'ai', 'ai_artifact', 'ml_model',
                 'https://huggingface.co/skilluv/demo', 3,
                 (SELECT id FROM orientations WHERE slug = 'ml-engineer'))
         RETURNING id",
    )
    .bind(project)
    .fetch_one(&app.db)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO deliverables
            (user_id, slice_id, artifact_type, artifact_url, verifiable_by, verification_status)
         VALUES ($1, $2, 'other', 'https://example.test/a', 'human_review', 'pending')",
    )
    .bind(user)
    .bind(slice)
    .execute(&app.db)
    .await
    .unwrap();

    let issued = ai_attestations::issue_for_slice(&app.db, slice)
        .await
        .unwrap();
    assert!(issued.is_empty());
}

#[tokio::test]
async fn a_pipeline_earns_a_verified_artifact_and_no_attestation() {
    let app = TestApp::spawn().await;
    let user = a_user(&app, "att_pipeline").await;
    let project = a_project(&app, user).await;

    let slice: Uuid = sqlx::query_scalar(
        "INSERT INTO project_slices
            (project_id, title, description, primary_domain, slice_type,
             ai_subtype, difficulty, orientation_id)
         VALUES ($1, 'Pipeline', 'x', 'ai', 'ai_artifact', 'data_pipeline', 3,
                 (SELECT id FROM orientations WHERE slug = 'data-engineer'))
         RETURNING id",
    )
    .bind(project)
    .fetch_one(&app.db)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO deliverables
            (user_id, slice_id, artifact_type, artifact_url, verifiable_by,
             verification_status, verified_at)
         VALUES ($1, $2, 'other', 'https://example.test/a', 'human_review',
                 'verified', NOW())",
    )
    .bind(user)
    .bind(slice)
    .execute(&app.db)
    .await
    .unwrap();

    // None of the seven bases describes a pipeline, and inventing one would
    // attest a repository twice.
    let issued = ai_attestations::issue_for_slice(&app.db, slice)
        .await
        .unwrap();
    assert!(issued.is_empty());
}

#[tokio::test]
async fn a_benchmark_attests_only_once_somebody_else_has_run_it() {
    let app = TestApp::spawn().await;
    let user = a_user(&app, "att_bench").await;
    let reviewer = a_user(&app, "att_bench_rev").await;
    let project = a_project(&app, user).await;
    let slice = a_verified_artifact(&app, project, user, "ml_model", "ml-engineer").await;

    let bench: Uuid = sqlx::query_scalar(
        "INSERT INTO benchmark_results
            (slice_id, benchmark_name, metric_name, metric_unit, metric_value,
             lower_is_better, comparison_baselines, methodology_md, code_url)
         VALUES ($1, 'MMLU', 'accuracy', '%', 68.4, FALSE,
                 '[{\"name\": \"base\", \"value\": 66.6}]',
                 'lm-evaluation-harness v0.4, 5-shot, graine fixée, jeu complet.',
                 'https://github.com/skilluv/demo-eval')
         RETURNING id",
    )
    .bind(slice)
    .fetch_one(&app.db)
    .await
    .unwrap();

    let before = ai_attestations::issue_for_slice(&app.db, slice)
        .await
        .unwrap();
    assert!(
        !before.contains(&"ai_benchmark_result".to_string()),
        "the claim is not the result, the reproduction is"
    );

    sqlx::query(
        "UPDATE benchmark_results
            SET reproduced_at = NOW(), reproduced_by_user_id = $2
          WHERE id = $1",
    )
    .bind(bench)
    .bind(reviewer)
    .execute(&app.db)
    .await
    .unwrap();

    let after = ai_attestations::issue_for_slice(&app.db, slice)
        .await
        .unwrap();
    assert!(after.contains(&"ai_benchmark_result".to_string()));
}

#[tokio::test]
async fn a_safety_finding_attests_only_once_it_has_been_disclosed() {
    let app = TestApp::spawn().await;
    let user = a_user(&app, "att_safety").await;
    let reviewer = a_user(&app, "att_safety_rev").await;
    let project = a_project(&app, user).await;
    let slice = a_verified_artifact(
        &app,
        project,
        user,
        "ai_research_paper",
        "ai-safety-researcher",
    )
    .await;

    let report: Uuid = sqlx::query_scalar(
        "INSERT INTO ai_safety_reports
            (slice_id, target_model, target_version, attack_type, reproduction_md,
             observed_output, attempts, successes, severity_tier,
             severity_rationale_md, mitigation_proposed_md, reproduced_at,
             reproduced_by_user_id)
         VALUES ($1, 'Mistral-7B-Instruct', 'v0.3', 'jailbreak',
                 'Conversation en trois tours, consigne réintroduite en citation.',
                 'Le modèle produit la consigne interdite.', 50, 31, 'high',
                 'Contournement reproductible sans accès privilégié.',
                 'Séparer instruction et donnée dans le gabarit.', NOW(), $2)
         RETURNING id",
    )
    .bind(slice)
    .bind(reviewer)
    .fetch_one(&app.db)
    .await
    .unwrap();

    // Reproduced, but still private: nobody who could fix it has been told,
    // and attesting that would reward sitting on a vulnerability.
    let before = ai_attestations::issue_for_slice(&app.db, slice)
        .await
        .unwrap();
    assert!(!before.contains(&"ai_safety_finding_validated".to_string()));

    sqlx::query(
        "UPDATE ai_safety_reports
            SET disclosure_status = 'vendor_notified', vendor_notified_at = NOW()
          WHERE id = $1",
    )
    .bind(report)
    .execute(&app.db)
    .await
    .unwrap();

    let after = ai_attestations::issue_for_slice(&app.db, slice)
        .await
        .unwrap();
    assert!(after.contains(&"ai_safety_finding_validated".to_string()));
}

#[tokio::test]
async fn revoking_the_work_revokes_the_attestation() {
    let app = TestApp::spawn().await;
    let user = a_user(&app, "att_revoked").await;
    let project = a_project(&app, user).await;
    let slice = a_verified_artifact(&app, project, user, "ml_model", "ml-engineer").await;

    ai_attestations::issue_for_slice(&app.db, slice)
        .await
        .unwrap();
    assert_eq!(bases_held_by(&app, user).await.len(), 1);

    sqlx::query(
        "UPDATE deliverables SET revoked_at = NOW(), revocation_reason = 'plagiat'
          WHERE slice_id = $1",
    )
    .bind(slice)
    .execute(&app.db)
    .await
    .unwrap();

    // The cascade existed as a function nobody called until 0207 made it a
    // trigger. Without it, the record kept saying a stranger could go and
    // check something that had been withdrawn.
    assert!(
        bases_held_by(&app, user).await.is_empty(),
        "an attestation standing on nothing must not stand"
    );
}

#[tokio::test]
async fn moderation_revocation_reaches_the_attestation_too() {
    let app = TestApp::spawn().await;
    let user = a_user(&app, "att_moderated").await;
    let project = a_project(&app, user).await;
    let slice = a_verified_artifact(&app, project, user, "dataset", "data-engineer").await;

    ai_attestations::issue_for_slice(&app.db, slice)
        .await
        .unwrap();

    // The moderation path sets the status and never touches `revoked_at`.
    // Watching only one of the two columns would miss half the revocations.
    sqlx::query("UPDATE deliverables SET verification_status = 'revoked' WHERE slice_id = $1")
        .bind(slice)
        .execute(&app.db)
        .await
        .unwrap();

    assert!(bases_held_by(&app, user).await.is_empty());
}

#[tokio::test]
async fn the_proof_orchestrator_issues_them() {
    let app = TestApp::spawn().await;
    let user = a_user(&app, "att_hook").await;
    let project = a_project(&app, user).await;
    a_verified_artifact(&app, project, user, "llm_agent", "llm-engineer").await;

    // The engine must not be dormant: nothing else calls the generator on the
    // normal path, and a schema whose generator never runs is worse than no
    // schema.
    skilluv_backend::services::proof_hooks::recompute_all_for_user(&app.db, user)
        .await
        .unwrap();

    assert_eq!(
        bases_held_by(&app, user).await,
        vec!["ai_agent_system_deployed"]
    );
}

#[tokio::test]
async fn the_badge_follows_the_attestation_in_the_same_pass() {
    let app = TestApp::spawn().await;
    let user = a_user(&app, "att_badge").await;
    let project = a_project(&app, user).await;
    a_verified_artifact(&app, project, user, "ml_model", "ml-engineer").await;

    skilluv_backend::services::proof_hooks::recompute_all_for_user(&app.db, user)
        .await
        .unwrap();

    let awarded: Vec<String> = sqlx::query_scalar(
        "SELECT r.slug FROM user_badges b
           JOIN badge_rules r ON r.id = b.rule_id
          WHERE b.user_id = $1 AND b.revoked_at IS NULL",
    )
    .bind(user)
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert!(
        awarded.contains(&"ai-model-shipped".to_string()),
        "the attestation and its badge must land together, got {awarded:?}"
    );
}
