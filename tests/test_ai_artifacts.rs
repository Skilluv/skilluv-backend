//! AI artefacts: what they must say about themselves, and what a measured
//! claim has to carry before anybody can dispute it.

mod common;
use common::TestApp;
use serde_json::json;
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
    .expect("project")
}

/// An `ml_model` slice belonging to a trade, so review rights can be checked.
async fn a_model_slice(app: &TestApp, project: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO project_slices
            (project_id, title, description, primary_domain, slice_type,
             ai_subtype, ai_frameworks, ai_external_hosting_url, difficulty,
             orientation_id)
         VALUES ($1, 'Modèle', 'x', 'ai', 'ai_artifact', 'ml_model',
                 ARRAY['pytorch'], 'https://huggingface.co/skilluv/demo', 3,
                 (SELECT id FROM orientations WHERE slug = 'ml-engineer'))
         RETURNING id",
    )
    .bind(project)
    .fetch_one(&app.db)
    .await
    .expect("slice")
}

async fn a_verified_deliverable(app: &TestApp, user: Uuid, slice: Uuid) {
    sqlx::query(
        "INSERT INTO deliverables
            (user_id, slice_id, artifact_type, artifact_url, verifiable_by,
             verification_status, verified_at)
         VALUES ($1, $2, 'other', 'https://example.test/model', 'human_review',
                 'verified', NOW())",
    )
    .bind(user)
    .bind(slice)
    .execute(&app.db)
    .await
    .unwrap();
}

async fn grant(app: &TestApp, user: Uuid, capability: &str) {
    sqlx::query(
        "INSERT INTO user_capabilities (user_id, capability, granted_reason)
         VALUES ($1, $2, 'test')",
    )
    .bind(user)
    .bind(capability)
    .execute(&app.db)
    .await
    .unwrap();
}

// ═══════════════════════════════════════════════════════════════════
// The slice itself
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn an_ai_artifact_must_say_what_it_produced() {
    let app = TestApp::spawn().await;
    let owner = a_user(&app, "ai_art_owner").await;
    let project = a_project(&app, owner).await;

    let refused = sqlx::query(
        "INSERT INTO project_slices
            (project_id, title, description, primary_domain, slice_type, difficulty)
         VALUES ($1, 'Sans sous-type', 'x', 'ai', 'ai_artifact', 3)",
    )
    .bind(project)
    .execute(&app.db)
    .await;
    assert!(refused.is_err());
}

#[tokio::test]
async fn a_model_says_where_it_lives() {
    let app = TestApp::spawn().await;
    let owner = a_user(&app, "ai_art_host").await;
    let project = a_project(&app, owner).await;

    // Skilluv hosts no weights. Without the address the claim cannot be
    // opened by a stranger, which is the whole premise.
    let refused = sqlx::query(
        "INSERT INTO project_slices
            (project_id, title, description, primary_domain, slice_type, ai_subtype, difficulty)
         VALUES ($1, 'Modèle', 'x', 'ai', 'ai_artifact', 'ml_model', 3)",
    )
    .bind(project)
    .execute(&app.db)
    .await;
    assert!(refused.is_err());

    let accepted = sqlx::query(
        "INSERT INTO project_slices
            (project_id, title, description, primary_domain, slice_type, ai_subtype,
             ai_external_hosting_url, difficulty)
         VALUES ($1, 'Modèle', 'x', 'ai', 'ai_artifact', 'ml_model',
                 'https://huggingface.co/skilluv/demo', 3)",
    )
    .bind(project)
    .execute(&app.db)
    .await;
    assert!(accepted.is_ok(), "{accepted:?}");
}

#[tokio::test]
async fn a_pipeline_needs_no_hosting_address() {
    let app = TestApp::spawn().await;
    let owner = a_user(&app, "ai_art_pipe").await;
    let project = a_project(&app, owner).await;

    // A pipeline is a repository, and `fork_repo_url` already carries that.
    let accepted = sqlx::query(
        "INSERT INTO project_slices
            (project_id, title, description, primary_domain, slice_type, ai_subtype, difficulty)
         VALUES ($1, 'Pipeline', 'x', 'ai', 'ai_artifact', 'data_pipeline', 3)",
    )
    .bind(project)
    .execute(&app.db)
    .await;
    assert!(accepted.is_ok(), "{accepted:?}");
}

#[tokio::test]
async fn an_ai_subtype_on_another_kind_of_slice_is_refused() {
    let app = TestApp::spawn().await;
    let owner = a_user(&app, "ai_art_wrong").await;
    let project = a_project(&app, owner).await;

    let refused = sqlx::query(
        "INSERT INTO project_slices
            (project_id, title, description, primary_domain, slice_type, ai_subtype, difficulty)
         VALUES ($1, 'Issue', 'x', 'code', 'github_issue', 'ml_model', 3)",
    )
    .bind(project)
    .execute(&app.db)
    .await;
    assert!(refused.is_err());
}

// ═══════════════════════════════════════════════════════════════════
// Benchmarks
// ═══════════════════════════════════════════════════════════════════

fn a_benchmark_body() -> serde_json::Value {
    json!({
        "benchmark_name": "MMLU",
        "metric_name": "accuracy",
        "metric_unit": "%",
        "metric_value": 68.4,
        "lower_is_better": false,
        "comparison_baselines": [{"name": "Llama-3-8B", "value": 66.6}],
        "methodology_md": "lm-evaluation-harness v0.4, 5-shot, une seule carte, \
                           graine fixée à 42, jeu de test complet.",
        "harness": "lm-evaluation-harness",
        "code_url": "https://github.com/skilluv/demo-eval",
        "dataset_url": "https://huggingface.co/datasets/cais/mmlu",
        "dataset_split": "test"
    })
}

#[tokio::test]
async fn only_the_person_who_did_the_work_records_a_measurement() {
    let app = TestApp::spawn().await;
    let author = a_user(&app, "bench_author").await;
    let project = a_project(&app, author).await;
    let slice = a_model_slice(&app, project).await;
    a_verified_deliverable(&app, author, slice).await;

    // A stranger, logged in, with no connection to this slice.
    a_user(&app, "bench_stranger").await;
    app.login("bench_stranger").await;

    let resp = app
        .post(
            &format!("/api/slices/{slice}/benchmarks"),
            &a_benchmark_body(),
        )
        .await;
    assert_eq!(resp.status().as_u16(), 403, "a stranger must be refused");
}

#[tokio::test]
async fn a_measurement_without_a_method_is_refused() {
    let app = TestApp::spawn().await;
    let author = a_user(&app, "bench_nomethod").await;
    let project = a_project(&app, author).await;
    let slice = a_model_slice(&app, project).await;
    a_verified_deliverable(&app, author, slice).await;
    app.login("bench_nomethod").await;

    let mut body = a_benchmark_body();
    body["methodology_md"] = json!("vite");

    let resp = app
        .post(&format!("/api/slices/{slice}/benchmarks"), &body)
        .await;
    assert!(
        !resp.status().is_success(),
        "a benchmark nobody can situate is a screenshot"
    );
}

#[tokio::test]
async fn a_measurement_without_a_baseline_is_refused() {
    let app = TestApp::spawn().await;
    let author = a_user(&app, "bench_nobase").await;
    let project = a_project(&app, author).await;
    let slice = a_model_slice(&app, project).await;
    a_verified_deliverable(&app, author, slice).await;
    app.login("bench_nobase").await;

    let mut body = a_benchmark_body();
    body["comparison_baselines"] = json!([]);

    let resp = app
        .post(&format!("/api/slices/{slice}/benchmarks"), &body)
        .await;
    assert!(
        !resp.status().is_success(),
        "\"twice as fast\" needs a second term"
    );
}

#[tokio::test]
async fn nobody_reproduces_their_own_measurement() {
    let app = TestApp::spawn().await;
    let author = a_user(&app, "bench_self").await;
    let project = a_project(&app, author).await;
    let slice = a_model_slice(&app, project).await;
    a_verified_deliverable(&app, author, slice).await;

    // Reviewer rights on their own trade, and it changes nothing: confirming
    // your own numbers is what a reproduction exists to rule out.
    grant(&app, author, "ai_reviewer:all").await;
    app.login("bench_self").await;

    let created = app
        .post(
            &format!("/api/slices/{slice}/benchmarks"),
            &a_benchmark_body(),
        )
        .await;
    assert_eq!(created.status().as_u16(), 200, "recording must work");
    let body: serde_json::Value = created.json().await.unwrap();
    let bench_id = body["data"]["id"].as_str().unwrap().to_string();

    let refused = app
        .post(&format!("/api/benchmarks/{bench_id}/reproduce"), &json!({}))
        .await;
    assert!(
        !refused.status().is_success(),
        "the author must not confirm their own measurement"
    );
}

#[tokio::test]
async fn a_reviewer_of_the_trade_reproduces_it() {
    let app = TestApp::spawn().await;
    let author = a_user(&app, "bench_other_a").await;
    let project = a_project(&app, author).await;
    let slice = a_model_slice(&app, project).await;
    a_verified_deliverable(&app, author, slice).await;

    let bench_id: Uuid = sqlx::query_scalar(
        "INSERT INTO benchmark_results
            (slice_id, benchmark_name, metric_name, metric_unit, metric_value,
             lower_is_better, comparison_baselines, methodology_md, code_url)
         VALUES ($1, 'MMLU', 'accuracy', '%', 68.4, FALSE,
                 '[{\"name\": \"Llama-3-8B\", \"value\": 66.6}]',
                 'lm-evaluation-harness v0.4, 5-shot, graine fixée, jeu complet.',
                 'https://github.com/skilluv/demo-eval')
         RETURNING id",
    )
    .bind(slice)
    .fetch_one(&app.db)
    .await
    .unwrap();

    let reviewer = a_user(&app, "bench_other_r").await;
    grant(&app, reviewer, "ai_reviewer:ml").await;
    app.login("bench_other_r").await;

    let resp = app
        .post(
            &format!("/api/benchmarks/{bench_id}/reproduce"),
            &json!({"notes": "mêmes chiffres à 0,2 point près"}),
        )
        .await;
    assert_eq!(resp.status().as_u16(), 200, "{:?}", resp.text().await);

    let reproduced: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT reproduced_at FROM benchmark_results WHERE id = $1")
            .bind(bench_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(reproduced.is_some());
}

#[tokio::test]
async fn a_reviewer_of_another_family_cannot_reproduce_it() {
    let app = TestApp::spawn().await;
    let author = a_user(&app, "bench_fam_a").await;
    let project = a_project(&app, author).await;
    let slice = a_model_slice(&app, project).await;
    a_verified_deliverable(&app, author, slice).await;

    let bench_id: Uuid = sqlx::query_scalar(
        "INSERT INTO benchmark_results
            (slice_id, benchmark_name, metric_name, metric_unit, metric_value,
             lower_is_better, comparison_baselines, methodology_md, code_url)
         VALUES ($1, 'MMLU', 'accuracy', '%', 68.4, FALSE,
                 '[{\"name\": \"base\", \"value\": 1.0}]',
                 'lm-evaluation-harness v0.4, 5-shot, graine fixée, jeu complet.',
                 'https://github.com/skilluv/demo-eval')
         RETURNING id",
    )
    .bind(slice)
    .fetch_one(&app.db)
    .await
    .unwrap();

    let reviewer = a_user(&app, "bench_fam_r").await;
    grant(&app, reviewer, "ai_reviewer:data").await;
    app.login("bench_fam_r").await;

    let refused = app
        .post(&format!("/api/benchmarks/{bench_id}/reproduce"), &json!({}))
        .await;
    assert_eq!(refused.status().as_u16(), 403);
}
