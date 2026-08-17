//! The AI craft score: derived on every read, and worth nothing once the work
//! behind it is revoked.

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
             ai_subtype, ai_external_hosting_url, difficulty, orientation_id)
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

async fn profile(app: &TestApp, username: &str) -> serde_json::Value {
    let resp = app.get(&format!("/api/users/{username}/ai-profile")).await;
    assert_eq!(resp.status().as_u16(), 200, "{:?}", resp.text().await);
    let body: serde_json::Value = resp.json().await.unwrap();
    body["data"].clone()
}

#[tokio::test]
async fn an_empty_profile_is_an_apprentice_at_zero() {
    let app = TestApp::spawn().await;
    a_user(&app, "prof_empty").await;

    let p = profile(&app, "prof_empty").await;
    assert_eq!(p["craft_score"], 0);
    assert_eq!(p["tier"], "apprentice");
    assert_eq!(p["counts"]["verified_artifacts"], 0);
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
async fn a_shipped_model_moves_the_score() {
    let app = TestApp::spawn().await;
    let user = a_user(&app, "prof_model").await;
    a_verified_model(&app, user, "ml-engineer").await;

    skilluv_backend::services::proof_hooks::recompute_all_for_user(&app.db, user)
        .await
        .unwrap();

    let p = profile(&app, "prof_model").await;
    assert_eq!(p["counts"]["verified_artifacts"], 1);
    assert_eq!(p["counts"]["models_shipped"], 1);
    // Five for the artefact, sixty for the model.
    assert_eq!(p["craft_score"], 65);
    assert_eq!(p["tier"], "apprentice");
}

#[tokio::test]
async fn revoked_work_scores_nothing() {
    let app = TestApp::spawn().await;
    let user = a_user(&app, "prof_revoked").await;
    let slice = a_verified_model(&app, user, "ml-engineer").await;

    skilluv_backend::services::proof_hooks::recompute_all_for_user(&app.db, user)
        .await
        .unwrap();
    assert!(profile(&app, "prof_revoked").await["craft_score"]
        .as_i64()
        .unwrap()
        > 0);

    sqlx::query(
        "UPDATE deliverables SET revoked_at = NOW(), revocation_reason = 'plagiat'
          WHERE slice_id = $1",
    )
    .bind(slice)
    .execute(&app.db)
    .await
    .unwrap();

    // A score that survives the revocation of what it rests on is the exact
    // failure this platform sells against. Nothing is recomputed in between
    // on purpose: the number is derived on read, so it cannot go stale.
    let p = profile(&app, "prof_revoked").await;
    assert_eq!(p["craft_score"], 0);
    assert_eq!(p["counts"]["verified_artifacts"], 0);
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
}

#[tokio::test]
async fn reach_is_counted_but_bounded() {
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
    assert_eq!(p["hub_downloads_recent"], 50_000_000i64);
    assert_eq!(p["hub_likes"], 1200);

    // Five for the artefact plus the capped four hundred. One model going
    // around the world must not outweigh a career.
    assert_eq!(p["craft_score"], 405);
}
