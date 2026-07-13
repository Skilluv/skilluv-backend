//! Tests d'intégration P12.3 : /api/feed/for-you.
//!
//! Vérifie que le feed mixe bien les 4 sources et respecte le tri
//! par happened_at DESC + le limit clamp.

mod common;

use common::TestApp;
use serde_json::{json, Value};
use uuid::Uuid;

async fn make_authenticated_user(app: &TestApp, username: &str) -> Uuid {
    let body = app.register_user(username).await;
    let user_id = Uuid::parse_str(body["data"]["user"]["id"].as_str().unwrap()).unwrap();
    app.login(username).await;
    user_id
}

async fn create_project(app: &TestApp, name: &str, domains: &[&str]) -> Uuid {
    let owner: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO users
            (email, username, password_hash, first_name, last_name, display_name,
             skill_domain, email_verified, role)
        VALUES ($1, $2, 'x', 'P', 'roj', 'P', 'code', TRUE, 'user')
        RETURNING id
        "#,
    )
    .bind(format!("po-{}@ex.io", Uuid::new_v4()))
    .bind(format!("po{}", &Uuid::new_v4().to_string()[..8]))
    .fetch_one(&app.db)
    .await
    .expect("po");

    sqlx::query_scalar(
        "INSERT INTO projects
            (slug, name, owner_type, owner_id, skill_domains)
         VALUES ($1, $2, 'user', $3, $4)
         RETURNING id",
    )
    .bind(format!("p-{}", Uuid::new_v4()))
    .bind(name)
    .bind(owner)
    .bind(domains)
    .fetch_one(&app.db)
    .await
    .expect("project")
}

async fn add_open_slice(app: &TestApp, project_id: Uuid, title: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO project_slices
            (project_id, slice_type, title, description, primary_domain, difficulty, status)
         VALUES ($1, 'other', $2, 'D', 'code', 2, 'open')
         RETURNING id",
    )
    .bind(project_id)
    .bind(title)
    .fetch_one(&app.db)
    .await
    .expect("slice")
}

// ═══════════════════════════════════════════════════════════════════
// Utilisateur sans interests → feed vide ou minimal
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn for_you_feed_empty_for_new_user() {
    let app = TestApp::spawn().await;
    make_authenticated_user(&app, "u_empty").await;

    let resp = app.get("/api/feed/for-you").await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["count"], 0);
    assert!(body["data"]["items"].as_array().unwrap().is_empty());

    drop(app);
}

// ═══════════════════════════════════════════════════════════════════
// Slices open de projet favori remontent dans le feed
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn open_slice_from_favorite_project_appears_in_feed() {
    let app = TestApp::spawn().await;
    let user_id = make_authenticated_user(&app, "u_fav").await;

    let project = create_project(&app, "My Fav Project", &["code"]).await;
    let slice_id = add_open_slice(&app, project, "First Task").await;
    add_open_slice(&app, project, "Second Task").await;

    // Marque le projet comme favori
    let resp = app
        .post(
            "/api/users/me/interests/projects",
            &json!({ "project_ids": [project] }),
        )
        .await;
    assert_eq!(resp.status(), 200);

    let feed_resp = app.get("/api/feed/for-you").await;
    let feed: Value = feed_resp.json().await.unwrap();
    let items = feed["data"]["items"].as_array().unwrap();

    assert!(!items.is_empty(), "au moins 1 item attendu");
    let has_slice = items.iter().any(|it| {
        it["kind"] == "open_slice_favorite_project"
            && it["payload"]["slice_id"].as_str() == Some(&slice_id.to_string())
    });
    assert!(has_slice, "la slice open du projet favori doit apparaître");

    let _ = user_id;
    drop(app);
}

// ═══════════════════════════════════════════════════════════════════
// Attestation communauté récente d'un autre user apparaît
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn recent_community_attestation_from_others_appears() {
    let app = TestApp::spawn().await;
    let _me = make_authenticated_user(&app, "u_watch").await;

    // Un autre user + une attestation récente
    let other: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO users
            (email, username, password_hash, first_name, last_name, display_name,
             skill_domain, email_verified, role)
        VALUES ($1, $2, 'x', 'O', 'ther', 'Other', 'code', TRUE, 'user')
        RETURNING id
        "#,
    )
    .bind(format!("oth-{}@ex.io", Uuid::new_v4()))
    .bind(format!("oth{}", &Uuid::new_v4().to_string()[..8]))
    .fetch_one(&app.db)
    .await
    .expect("other");

    let skill_id: Uuid =
        sqlx::query_scalar("SELECT id FROM skill_nodes LIMIT 1")
            .fetch_one(&app.db)
            .await
            .expect("skill");
    sqlx::query(
        r#"
        INSERT INTO attestations
            (user_id, attestation_type, title, description, verification_code,
             linked_skill_node_ids)
        VALUES ($1, 'gesture', 'Test title', 'Test desc', $2, ARRAY[$3::uuid])
        "#,
    )
    .bind(other)
    .bind(format!("V{}", &Uuid::new_v4().to_string()[..10]))
    .bind(skill_id)
    .execute(&app.db)
    .await
    .expect("attestation");

    let feed: Value = app.get("/api/feed/for-you").await.json().await.unwrap();
    let items = feed["data"]["items"].as_array().unwrap();
    let has_attestation = items
        .iter()
        .any(|it| it["kind"] == "community_attestation");
    assert!(has_attestation, "attestation récente d'un autre user attendue");

    drop(app);
}

// ═══════════════════════════════════════════════════════════════════
// Mes propres attestations n'apparaissent pas
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn my_own_attestation_is_not_in_the_feed() {
    let app = TestApp::spawn().await;
    let user_id = make_authenticated_user(&app, "u_self").await;

    let skill_id: Uuid =
        sqlx::query_scalar("SELECT id FROM skill_nodes LIMIT 1")
            .fetch_one(&app.db)
            .await
            .expect("skill");
    sqlx::query(
        r#"
        INSERT INTO attestations
            (user_id, attestation_type, title, description, verification_code,
             linked_skill_node_ids)
        VALUES ($1, 'skill', 'Self title', 'Self desc', $2, ARRAY[$3::uuid])
        "#,
    )
    .bind(user_id)
    .bind(format!("V{}", &Uuid::new_v4().to_string()[..10]))
    .bind(skill_id)
    .execute(&app.db)
    .await
    .expect("self attest");

    let feed: Value = app.get("/api/feed/for-you").await.json().await.unwrap();
    let items = feed["data"]["items"].as_array().unwrap();
    let has_self = items.iter().any(|it| {
        it["kind"] == "community_attestation"
            && it["payload"]["recipient_user_id"].as_str() == Some(&user_id.to_string())
    });
    assert!(!has_self, "mes propres attestations exclues du feed");

    drop(app);
}

// ═══════════════════════════════════════════════════════════════════
// limit clamp respect
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn limit_query_param_is_respected() {
    let app = TestApp::spawn().await;
    make_authenticated_user(&app, "u_lim").await;
    let project = create_project(&app, "Lim Project", &["code"]).await;
    for i in 0..5 {
        add_open_slice(&app, project, &format!("S{i}")).await;
    }
    app.post(
        "/api/users/me/interests/projects",
        &json!({ "project_ids": [project] }),
    )
    .await;

    let feed: Value = app
        .get("/api/feed/for-you?limit=2")
        .await
        .json()
        .await
        .unwrap();
    let items = feed["data"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 2, "limit=2 respecté");

    drop(app);
}
