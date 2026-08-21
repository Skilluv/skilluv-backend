//! The AI craft score: a formula read from rows, and worth nothing once the
//! work behind it is revoked.

mod common;
use common::TestApp;
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

async fn a_verified_model(app: &TestApp, user: Uuid, orientation: &str) -> Uuid {
    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO projects (slug, name, owner_type, owner_id)
         VALUES ($1, 'Projet IA', 'user', $2)
         RETURNING id",
    )
    .bind(format!("proj-{}", Uuid::new_v4().simple()))
    .bind(user)
    .fetch_one(&app.db)
    .await
    .unwrap();

    let slice: Uuid = sqlx::query_scalar(
        "INSERT INTO project_slices
            (project_id, title, description, primary_domain, slice_type,
             ai_subtype, published_artifact_url, difficulty, orientation_id)
         VALUES ($1, 'Modèle', 'x', 'ai', 'ai_artifact', 'ml_model',
                 'https://huggingface.co/skilluv/demo', 3,
                 (SELECT id FROM orientations WHERE slug = $2))
         RETURNING id",
    )
    .bind(project)
    .bind(orientation)
    .fetch_one(&app.db)
    .await
    .unwrap();

    // `ai_assistance_level` set so the deliverable is countable: the
    // disclosure view drops anything past its deadline with nothing declared.
    sqlx::query(
        "INSERT INTO deliverables
            (user_id, slice_id, artifact_type, artifact_url, verifiable_by,
             verification_status, verified_at, ai_assistance_level)
         VALUES ($1, $2, 'other', 'https://example.test/a', 'human_review',
                 'verified', NOW(), 'none')",
    )
    .bind(user)
    .bind(slice)
    .execute(&app.db)
    .await
    .unwrap();

    slice
}

async fn profile(app: &TestApp, username: &str) -> serde_json::Value {
    let resp = app.get(&format!("/api/users/{username}/ai-profile")).await;
    assert_eq!(resp.status().as_u16(), 200, "{:?}", resp.text().await);
    let body: serde_json::Value = resp.json().await.unwrap();
    body["data"].clone()
}

fn term(profile: &serde_json::Value, name: &str) -> Option<serde_json::Value> {
    profile["breakdown"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["term"] == name)
        .cloned()
}

#[tokio::test]
async fn an_empty_profile_is_an_apprentice_at_zero() {
    let app = TestApp::spawn().await;
    a_user(&app, "prof_empty").await;

    let p = profile(&app, "prof_empty").await;
    assert_eq!(p["craft_score"], 0);
    assert_eq!(p["tier"], "apprentice");
    assert!(p["breakdown"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn an_unknown_user_is_a_404_not_an_empty_profile() {
    let app = TestApp::spawn().await;

    // Answering zeros for a name that does not exist would let anybody
    // fabricate a profile page by choosing a URL.
    let resp = app.get("/api/users/personne-du-tout/ai-profile").await;
    assert_eq!(resp.status().as_u16(), 404);
}

#[tokio::test]
async fn a_shipped_model_moves_the_score_and_says_why() {
    let app = TestApp::spawn().await;
    let user = a_user(&app, "prof_model").await;
    a_verified_model(&app, user, "ml-engineer").await;

    skilluv_backend::services::proof_hooks::recompute_all_for_user(&app.db, user)
        .await
        .unwrap();

    let p = profile(&app, "prof_model").await;

    // Five for the attestation, sixty for the model, twenty for the trade.
    assert_eq!(p["craft_score"], 85);
    assert_eq!(p["tier"], "apprentice");

    // The breakdown is the point: a score with no explanation is a number
    // somebody has to trust.
    let models = term(&p, "models_shipped").expect("the model term");
    assert_eq!(models["points"], 60);
    assert!(
        !models["explanation"].as_str().unwrap().is_empty(),
        "every term explains itself to the person it describes"
    );
}

#[tokio::test]
async fn the_formula_lives_in_rows_and_can_be_argued_with() {
    let app = TestApp::spawn().await;
    let user = a_user(&app, "prof_weights").await;
    a_verified_model(&app, user, "ml-engineer").await;
    skilluv_backend::services::proof_hooks::recompute_all_for_user(&app.db, user)
        .await
        .unwrap();

    let before = profile(&app, "prof_weights").await["craft_score"]
        .as_i64()
        .unwrap();

    // An operator changes what a shipped model is worth. No deployment.
    sqlx::query(
        "UPDATE craft_score_weights SET weight = 600
          WHERE skill_domain = 'ai' AND term = 'models_shipped'",
    )
    .execute(&app.db)
    .await
    .unwrap();

    let after = profile(&app, "prof_weights").await["craft_score"]
        .as_i64()
        .unwrap();
    assert_eq!(after, before + 540);
}

#[tokio::test]
async fn revoked_work_scores_nothing() {
    let app = TestApp::spawn().await;
    let user = a_user(&app, "prof_revoked").await;
    let slice = a_verified_model(&app, user, "ml-engineer").await;

    skilluv_backend::services::proof_hooks::recompute_all_for_user(&app.db, user)
        .await
        .unwrap();
    assert!(
        profile(&app, "prof_revoked").await["craft_score"]
            .as_i64()
            .unwrap()
            > 0
    );

    sqlx::query(
        "UPDATE deliverables SET revoked_at = NOW(), revocation_reason = 'plagiat'
          WHERE slice_id = $1",
    )
    .bind(slice)
    .execute(&app.db)
    .await
    .unwrap();

    // A score that survives the revocation of what it rests on is the exact
    // failure this platform sells against. The endpoint recomputes rather
    // than reading the stored figure, so the answer cannot be an hour stale.
    let p = profile(&app, "prof_revoked").await;
    assert_eq!(p["craft_score"], 0);
    assert!(p["orientations"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn the_score_is_stored_so_a_listing_can_sort_on_it() {
    let app = TestApp::spawn().await;
    let user = a_user(&app, "prof_stored").await;
    a_verified_model(&app, user, "ml-engineer").await;
    skilluv_backend::services::proof_hooks::recompute_all_for_user(&app.db, user)
        .await
        .unwrap();

    profile(&app, "prof_stored").await;

    let row: Option<(i32, Option<String>)> = sqlx::query_as(
        "SELECT score, tier_slug FROM craft_scores
          WHERE user_id = $1 AND skill_domain = 'ai'",
    )
    .bind(user)
    .fetch_optional(&app.db)
    .await
    .unwrap();

    let (score, tier) = row.expect("a row a recruiter search can sort on");
    assert_eq!(score, 85);
    assert_eq!(tier.as_deref(), Some("apprentice"));
}

#[tokio::test]
async fn an_ai_attestation_does_not_pay_into_the_code_score() {
    let app = TestApp::spawn().await;
    let user = a_user(&app, "prof_nocross").await;
    a_verified_model(&app, user, "ml-engineer").await;
    skilluv_backend::services::proof_hooks::recompute_all_for_user(&app.db, user)
        .await
        .unwrap();

    // The code formula counted every attestation carrying a basis, whichever
    // domain issued it. A published model was paying into a code score.
    let code = skilluv_backend::services::craft_score::compute(&app.db, user)
        .await
        .unwrap();
    assert_eq!(code.score, 0, "AI work must not score in the code domain");
}

#[tokio::test]
async fn the_trades_someone_has_worked_in_are_listed() {
    let app = TestApp::spawn().await;
    let user = a_user(&app, "prof_trades").await;
    a_verified_model(&app, user, "ml-engineer").await;
    a_verified_model(&app, user, "nlp-engineer").await;

    let p = profile(&app, "prof_trades").await;
    let trades: Vec<&str> = p["orientations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o.as_str().unwrap())
        .collect();

    assert_eq!(trades, vec!["ml-engineer", "nlp-engineer"]);
    assert_eq!(term(&p, "orientations_distinct").unwrap()["points"], 40);
}

#[tokio::test]
async fn reach_is_counted_on_a_logarithmic_scale() {
    let app = TestApp::spawn().await;
    let user = a_user(&app, "prof_reach").await;
    let slice = a_verified_model(&app, user, "ml-engineer").await;

    sqlx::query(
        "INSERT INTO published_artifact_stats
            (slice_id, registry, package_name, downloads_recent, likes_count)
         VALUES ($1, 'huggingface_models', 'skilluv/demo', 50000000, 1200)",
    )
    .bind(slice)
    .execute(&app.db)
    .await
    .unwrap();

    let p = profile(&app, "prof_reach").await;
    let downloads = term(&p, "hub_downloads").expect("the downloads term");

    // 40 × log10(1 + 50 000 000) ≈ 308. A linear term would have made
    // download count the only thing the score measured.
    assert_eq!(downloads["points"], 308);
}

#[tokio::test]
async fn a_benchmark_nobody_re_ran_scores_nothing() {
    let app = TestApp::spawn().await;
    let user = a_user(&app, "prof_bench").await;
    let slice = a_verified_model(&app, user, "ml-engineer").await;

    sqlx::query(
        "INSERT INTO benchmark_results
            (slice_id, benchmark_name, metric_name, metric_unit, metric_value,
             lower_is_better, comparison_baselines, methodology_md, code_url)
         VALUES ($1, 'MMLU', 'accuracy', '%', 68.4, FALSE,
                 '[{\"name\": \"base\", \"value\": 66.6}]',
                 'lm-evaluation-harness v0.4, 5-shot, graine fixée, jeu complet.',
                 'https://github.com/skilluv/demo-eval')",
    )
    .bind(slice)
    .fetch_optional(&app.db)
    .await
    .unwrap();

    // An unverified record is the easiest thing to overstate in this domain.
    let p = profile(&app, "prof_bench").await;
    assert!(term(&p, "benchmarks_reproduced").is_none());
}
