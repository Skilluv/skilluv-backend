//! Code artefacts: what they are, what languages they touch, and what a
//! performance claim has to carry to be disputable.

mod common;
use common::TestApp;
use uuid::Uuid;

async fn a_project(app: &TestApp) -> Uuid {
    app.register_user("proj_owner").await;
    let owner: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'proj_owner'")
        .fetch_one(&app.db)
        .await
        .unwrap();

    sqlx::query_scalar(
        "INSERT INTO projects (slug, name, owner_type, owner_id)
         VALUES ($1, 'Projet test', 'user', $2)
         RETURNING id",
    )
    .bind(format!("proj-{}", Uuid::new_v4().simple()))
    .bind(owner)
    .fetch_one(&app.db)
    .await
    .expect("project")
}

async fn a_code_slice(app: &TestApp, project: Uuid, languages: &[&str]) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO project_slices
            (project_id, title, description, primary_domain, slice_type, code_subtype,
             code_languages, difficulty)
         VALUES ($1, 'Tranche', 'x', 'code', 'code_artifact', 'pr_upstream', $2, 3)
         RETURNING id",
    )
    .bind(project)
    .bind(languages)
    .fetch_one(&app.db)
    .await
    .expect("slice")
}

#[tokio::test]
async fn a_code_artifact_must_say_what_it_produced() {
    let app = TestApp::spawn().await;
    let project = a_project(&app).await;

    // A code artefact with no subtype is a slice nothing can be attested
    // against.
    let refused = sqlx::query(
        "INSERT INTO project_slices
            (project_id, title, description, primary_domain, slice_type, difficulty)
         VALUES ($1, 'Sans sous-type', 'x', 'code', 'code_artifact', 3)",
    )
    .bind(project)
    .execute(&app.db)
    .await;
    assert!(refused.is_err());
}

#[tokio::test]
async fn a_subtype_on_another_kind_of_slice_is_refused() {
    let app = TestApp::spawn().await;
    let project = a_project(&app).await;

    // "library_published" on a Figma frame is a claim nothing reads.
    let refused = sqlx::query(
        "INSERT INTO project_slices
            (project_id, title, description, primary_domain, slice_type, code_subtype, difficulty)
         VALUES ($1, 'Maquette', 'x', 'design', 'figma_frame', 'library_published', 3)",
    )
    .bind(project)
    .execute(&app.db)
    .await;
    assert!(refused.is_err());
}

#[tokio::test]
async fn a_published_library_says_where_it_was_published() {
    let app = TestApp::spawn().await;
    let project = a_project(&app).await;

    // Without the registry URL the claim cannot be checked, which is the
    // only thing that makes it more than a sentence.
    let refused = sqlx::query(
        "INSERT INTO project_slices
            (project_id, title, description, primary_domain, slice_type, code_subtype, difficulty)
         VALUES ($1, 'Biblio', 'x', 'code', 'code_artifact', 'library_published', 3)",
    )
    .bind(project)
    .execute(&app.db)
    .await;
    assert!(refused.is_err());

    let accepted = sqlx::query(
        "INSERT INTO project_slices
            (project_id, title, description, primary_domain, slice_type, code_subtype,
             code_package_registry_url, difficulty)
         VALUES ($1, 'Biblio', 'x', 'code', 'code_artifact', 'library_published',
                 'https://crates.io/crates/exemple', 3)",
    )
    .bind(project)
    .execute(&app.db)
    .await;
    assert!(accepted.is_ok(), "{accepted:?}");
}

#[tokio::test]
async fn a_slice_spanning_two_languages_counts_for_both() {
    let app = TestApp::spawn().await;
    let project = a_project(&app).await;
    let slice = a_code_slice(&app, project, &["rust", "typescript"]).await;

    app.register_user("lang_user").await;
    let user: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'lang_user'")
        .fetch_one(&app.db)
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO deliverables
            (user_id, slice_id, artifact_type, artifact_url, verifiable_by,
             verification_status, verified_at)
         VALUES ($1, $2, 'pr_merged', 'https://example.test/pr', 'github_webhook',
                 'verified', NOW())",
    )
    .bind(user)
    .bind(slice)
    .execute(&app.db)
    .await
    .unwrap();

    let resp = app
        .client
        .get(format!("{}/api/users/lang_user/code-languages", app.addr))
        .send()
        .await
        .expect("GET languages");
    assert_eq!(resp.status().as_u16(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    let langs: Vec<&str> = body["data"]
        .as_array()
        .expect("array")
        .iter()
        .map(|l| l["language"].as_str().unwrap_or_default())
        .collect();

    // One slice, two true statements about it.
    assert!(langs.contains(&"rust"), "{langs:?}");
    assert!(langs.contains(&"typescript"), "{langs:?}");
}

#[tokio::test]
async fn unverified_work_does_not_appear_in_the_breakdown() {
    let app = TestApp::spawn().await;
    let project = a_project(&app).await;
    let slice = a_code_slice(&app, project, &["cobol"]).await;

    app.register_user("lang_pending").await;
    let user: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'lang_pending'")
        .fetch_one(&app.db)
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO deliverables
            (user_id, slice_id, artifact_type, artifact_url, verifiable_by, verification_status)
         VALUES ($1, $2, 'pr_open', 'https://example.test/pr', 'github_webhook', 'pending')",
    )
    .bind(user)
    .bind(slice)
    .execute(&app.db)
    .await
    .unwrap();

    let body: serde_json::Value = app
        .client
        .get(format!(
            "{}/api/users/lang_pending/code-languages",
            app.addr
        ))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(
        body["data"].as_array().map(|a| a.len()),
        Some(0),
        "a claimed language with no verified work is a declaration"
    );
}

#[tokio::test]
async fn asking_about_an_unknown_person_says_so() {
    let app = TestApp::spawn().await;

    let resp = app
        .client
        .get(format!(
            "{}/api/users/personne-inexistante/code-languages",
            app.addr
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status().as_u16(), 404);
}

#[tokio::test]
async fn a_benchmark_without_a_baseline_is_not_a_comparison() {
    let app = TestApp::spawn().await;
    let project = a_project(&app).await;
    let slice = a_code_slice(&app, project, &["rust"]).await;

    let refused = sqlx::query(
        "INSERT INTO code_benchmark_results
            (slice_id, benchmark_name, metric_name, metric_unit, metric_value,
             lower_is_better, comparison_baselines, methodology_md, code_url)
         VALUES ($1, 'parse', 'latence', 'ms', 1.2, TRUE, '[]'::jsonb,
                 'Machine dédiée, 100 itérations, chauffe de 10 secondes, entrée de 5 Mo.',
                 'https://example.test/bench')",
    )
    .bind(slice)
    .execute(&app.db)
    .await;

    assert!(refused.is_err(), "\"twice as fast\" needs a second term");
}

#[tokio::test]
async fn a_benchmark_without_a_method_cannot_be_judged_fair() {
    let app = TestApp::spawn().await;
    let project = a_project(&app).await;
    let slice = a_code_slice(&app, project, &["rust"]).await;

    let refused = sqlx::query(
        "INSERT INTO code_benchmark_results
            (slice_id, benchmark_name, metric_name, metric_unit, metric_value,
             lower_is_better, comparison_baselines, methodology_md, code_url)
         VALUES ($1, 'parse', 'latence', 'ms', 1.2, TRUE,
                 '[{\"name\": \"serde\", \"value\": 2.4}]'::jsonb, 'vite',
                 'https://example.test/bench')",
    )
    .bind(slice)
    .execute(&app.db)
    .await;

    assert!(refused.is_err(), "a one-word method hides the comparison");
}

#[tokio::test]
async fn a_complete_benchmark_is_accepted_and_reproduction_is_all_or_nothing() {
    let app = TestApp::spawn().await;
    let project = a_project(&app).await;
    let slice = a_code_slice(&app, project, &["rust"]).await;

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO code_benchmark_results
            (slice_id, benchmark_name, metric_name, metric_unit, metric_value,
             lower_is_better, comparison_baselines, methodology_md, harness, code_url)
         VALUES ($1, 'parse', 'latence', 'ms', 1.2, TRUE,
                 '[{\"name\": \"serde_json\", \"value\": 2.4}]'::jsonb,
                 'Machine dédiée sans autre charge, 100 itérations après 10 s de chauffe, entrée de 5 Mo.',
                 'criterion', 'https://example.test/bench')
         RETURNING id",
    )
    .bind(slice)
    .fetch_one(&app.db)
    .await
    .expect("complete benchmark accepted");

    // A reproduction with no author says nothing about who checked.
    let half = sqlx::query("UPDATE code_benchmark_results SET reproduced_at = NOW() WHERE id = $1")
        .bind(id)
        .execute(&app.db)
        .await;
    assert!(half.is_err());
}
