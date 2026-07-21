//! Tests P16.4 : search recruteur v3 (orientation + skills + mode).

mod common;
use common::TestApp;
use serde_json::json;

/// Setup : 1 recruteur + 3 talents avec profils différents.
async fn seed_talents(app: &TestApp) -> (uuid::Uuid, uuid::Uuid, uuid::Uuid) {
    // Talent A : dev-frontend active + a prouvé react
    app.register_user("alice_p164").await;
    app.login("alice_p164").await;
    let alice_id: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM users WHERE username = 'alice_p164'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    let react_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO skill_nodes (slug, display_name, domain) VALUES ('p164-react', 'React', 'code')
         ON CONFLICT (slug) DO UPDATE SET display_name = EXCLUDED.display_name
         RETURNING id",
    )
    .fetch_one(&app.db).await.unwrap();
    sqlx::query(
        "INSERT INTO user_skills (user_id, skill_id, proficiency_level, weighted_proven_count)
                 VALUES ($1, $2, 4, 15)",
    )
    .bind(alice_id)
    .bind(react_id)
    .execute(&app.db)
    .await
    .unwrap();
    app.post(
        "/api/users/me/orientations",
        &json!({ "slug": "dev-frontend", "mode": "active", "is_primary": true }),
    )
    .await;

    // Talent B : dev-frontend learning (aspirationnel, pas de prouvé)
    app.register_user("bob_p164").await;
    app.login("bob_p164").await;
    let bob_id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'bob_p164'")
        .fetch_one(&app.db)
        .await
        .unwrap();
    app.post(
        "/api/users/me/orientations",
        &json!({ "slug": "dev-frontend", "mode": "learning" }),
    )
    .await;

    // Talent C : pentester-web active (autre métier)
    app.register_user("carol_p164").await;
    app.login("carol_p164").await;
    let carol_id: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM users WHERE username = 'carol_p164'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    app.post(
        "/api/users/me/orientations",
        &json!({ "slug": "pentester-web", "mode": "active", "is_primary": true }),
    )
    .await;

    (alice_id, bob_id, carol_id)
}

#[tokio::test]
async fn search_by_orientation_returns_active_only_by_default() {
    let app = TestApp::spawn().await;
    let (alice, bob, _carol) = seed_talents(&app).await;

    let resp = app
        .get("/api/talents/search/v3?orientation=dev-frontend")
        .await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let talents = body["data"]["talents"].as_array().unwrap();
    // Alice (active) doit être présente, Bob (learning) non.
    let ids: Vec<String> = talents
        .iter()
        .map(|t| t["user_id"].as_str().unwrap().into())
        .collect();
    assert!(ids.iter().any(|id| id == &alice.to_string()));
    assert!(
        !ids.iter().any(|id| id == &bob.to_string()),
        "learning excluded by default"
    );
}

#[tokio::test]
async fn search_mode_both_includes_learners() {
    let app = TestApp::spawn().await;
    let (_alice, bob, _c) = seed_talents(&app).await;

    let resp = app
        .get("/api/talents/search/v3?orientation=dev-frontend&mode=both")
        .await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let talents = body["data"]["talents"].as_array().unwrap();
    let ids: Vec<String> = talents
        .iter()
        .map(|t| t["user_id"].as_str().unwrap().into())
        .collect();
    assert!(
        ids.contains(&bob.to_string()),
        "learner surfaces in mode=both"
    );
}

#[tokio::test]
async fn search_filters_by_skill_requires_proven() {
    let app = TestApp::spawn().await;
    let (alice, _bob, _c) = seed_talents(&app).await;

    let resp = app
        .get("/api/talents/search/v3?orientation=dev-frontend&skills=p164-react&min_proficiency=3")
        .await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let talents = body["data"]["talents"].as_array().unwrap();
    // Seule alice a prouvé react à niveau ≥ 3
    assert_eq!(talents.len(), 1);
    assert_eq!(talents[0]["user_id"], alice.to_string());
    // Le count matched + wpc doivent être remontés
    assert_eq!(talents[0]["matched_skills_count"], 1);
    let wpc = talents[0]["matched_wpc_total"].as_i64().unwrap();
    assert!(wpc >= 15, "wpc >= 15");
}

#[tokio::test]
async fn search_excludes_ended_orientations() {
    let app = TestApp::spawn().await;
    let (alice, _b, _c) = seed_talents(&app).await;

    // Alice ferme son orientation → doit disparaître du search.
    app.login("alice_p164").await;
    app.delete("/api/talents/../users/me/orientations/dev-frontend")
        .await;
    // Fallback via SQL direct au cas où le path relatif ne matche pas :
    sqlx::query(
        "UPDATE user_orientations SET ended_at = NOW()
                 WHERE user_id = $1",
    )
    .bind(alice)
    .execute(&app.db)
    .await
    .unwrap();

    let resp = app
        .get("/api/talents/search/v3?orientation=dev-frontend")
        .await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let talents = body["data"]["talents"].as_array().unwrap();
    let ids: Vec<String> = talents
        .iter()
        .map(|t| t["user_id"].as_str().unwrap().into())
        .collect();
    assert!(
        !ids.contains(&alice.to_string()),
        "ended orientation excluded"
    );
}

#[tokio::test]
async fn search_rejects_invalid_mode() {
    let app = TestApp::spawn().await;
    let resp = app
        .get("/api/talents/search/v3?orientation=dev-frontend&mode=expert")
        .await;
    assert_eq!(resp.status().as_u16(), 400);
}

#[tokio::test]
async fn search_returns_empty_when_orientation_unknown() {
    let app = TestApp::spawn().await;
    seed_talents(&app).await;
    let resp = app
        .get("/api/talents/search/v3?orientation=nope-nope")
        .await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let talents = body["data"]["talents"].as_array().unwrap();
    assert_eq!(talents.len(), 0);
}
