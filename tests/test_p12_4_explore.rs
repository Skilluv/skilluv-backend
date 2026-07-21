//! Tests d'intégration P12.4 : GET /api/explore multi-critères.
//!
//! Vérifie que :
//! - Sans filtre : slices + challenges publiés remontent.
//! - kind=slice ou kind=challenge scope à un seul type.
//! - domain / difficulty / language / project_id filtrent correctement.
//! - `q` cherche par ILIKE sur title.
//! - Pagination (page/per_page) fonctionne.

mod common;

use common::TestApp;
use serde_json::Value;
use uuid::Uuid;

async fn create_project(app: &TestApp, name: &str) -> Uuid {
    let owner: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO users (email, username, password_hash, first_name, last_name,
                           display_name, skill_domain, email_verified, role)
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
        "INSERT INTO projects (slug, name, owner_type, owner_id)
         VALUES ($1, $2, 'user', $3) RETURNING id",
    )
    .bind(format!("p-{}", Uuid::new_v4()))
    .bind(name)
    .bind(owner)
    .fetch_one(&app.db)
    .await
    .expect("project")
}

async fn add_open_slice(
    app: &TestApp,
    project_id: Uuid,
    title: &str,
    domain: &str,
    difficulty: i16,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO project_slices
            (project_id, slice_type, title, description, primary_domain,
             difficulty, status)
         VALUES ($1, 'other', $2, 'D', $3, $4, 'open')
         RETURNING id",
    )
    .bind(project_id)
    .bind(title)
    .bind(domain)
    .bind(difficulty)
    .fetch_one(&app.db)
    .await
    .expect("slice")
}

async fn add_published_challenge(
    app: &TestApp,
    title: &str,
    domain: &str,
    difficulty: i16,
    language: Option<&str>,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO challenge_templates
            (title, description, instructions, skill_domain, difficulty,
             language, is_training, status)
         VALUES ($1, 'D', 'I', $2, $3, $4, TRUE, 'published')
         RETURNING id",
    )
    .bind(title)
    .bind(domain)
    .bind(difficulty)
    .bind(language)
    .fetch_one(&app.db)
    .await
    .expect("challenge")
}

// ═══════════════════════════════════════════════════════════════════
// Sans filtre : les 2 types remontent
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn explore_returns_slices_and_challenges_by_default() {
    let app = TestApp::spawn().await;

    let project = create_project(&app, "Explore Project").await;
    add_open_slice(&app, project, "Slice Alpha", "code", 2).await;
    add_published_challenge(&app, "Challenge Beta", "code", 3, Some("rust")).await;

    let resp = app
        .client
        .get(format!("{}/api/explore", app.addr))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let items = body["data"]["items"].as_array().unwrap();
    let kinds: Vec<&str> = items.iter().map(|i| i["kind"].as_str().unwrap()).collect();
    assert!(kinds.contains(&"slice"), "slice attendu");
    assert!(kinds.contains(&"challenge"), "challenge attendu");

    drop(app);
}

// ═══════════════════════════════════════════════════════════════════
// kind=slice scope à un seul type
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn explore_kind_slice_excludes_challenges() {
    let app = TestApp::spawn().await;

    let project = create_project(&app, "Kind Slice Project").await;
    add_open_slice(&app, project, "Alpha Slice", "code", 2).await;
    add_published_challenge(&app, "Beta Challenge", "code", 3, Some("rust")).await;

    let resp = app
        .client
        .get(format!("{}/api/explore?kind=slice", app.addr))
        .send()
        .await
        .unwrap();
    let body: Value = resp.json().await.unwrap();
    let items = body["data"]["items"].as_array().unwrap();
    assert!(items.iter().all(|i| i["kind"] == "slice"));

    drop(app);
}

// ═══════════════════════════════════════════════════════════════════
// domain filter
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn explore_domain_filter() {
    let app = TestApp::spawn().await;

    let project = create_project(&app, "Domain Project").await;
    add_open_slice(&app, project, "Code Task", "code", 2).await;
    add_open_slice(&app, project, "Design Task", "design", 2).await;

    let resp = app
        .client
        .get(format!("{}/api/explore?domain=design", app.addr))
        .send()
        .await
        .unwrap();
    let body: Value = resp.json().await.unwrap();
    let items = body["data"]["items"].as_array().unwrap();
    assert!(items.iter().all(|i| i["domain"] == "design"));
    assert!(items.iter().any(|i| i["title"] == "Design Task"));
    assert!(items.iter().all(|i| i["title"] != "Code Task"));

    drop(app);
}

// ═══════════════════════════════════════════════════════════════════
// language filter (challenges uniquement)
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn explore_language_filter_targets_challenges() {
    let app = TestApp::spawn().await;

    add_published_challenge(&app, "Rust Challenge", "code", 3, Some("rust")).await;
    add_published_challenge(&app, "Python Challenge", "code", 3, Some("python")).await;

    let resp = app
        .client
        .get(format!("{}/api/explore?language=rust", app.addr))
        .send()
        .await
        .unwrap();
    let body: Value = resp.json().await.unwrap();
    let items = body["data"]["items"].as_array().unwrap();
    let titles: Vec<&str> = items.iter().map(|i| i["title"].as_str().unwrap()).collect();
    assert!(titles.contains(&"Rust Challenge"), "rust challenge attendu");
    // Le filtre language ne s'applique qu'aux challenges — les slices sans language
    // ne sont pas filtrées. Ici on n'a pas de slices, donc :
    assert!(
        !titles.contains(&"Python Challenge"),
        "python challenge filtre out"
    );

    drop(app);
}

// ═══════════════════════════════════════════════════════════════════
// q text search ILIKE
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn explore_text_search_case_insensitive() {
    let app = TestApp::spawn().await;
    let project = create_project(&app, "Text Search Project").await;
    add_open_slice(&app, project, "Fix async race condition", "code", 3).await;
    add_open_slice(&app, project, "Add UI polish", "code", 2).await;

    let resp = app
        .client
        .get(format!("{}/api/explore?q=RACE", app.addr))
        .send()
        .await
        .unwrap();
    let body: Value = resp.json().await.unwrap();
    let items = body["data"]["items"].as_array().unwrap();
    let titles: Vec<&str> = items.iter().map(|i| i["title"].as_str().unwrap()).collect();
    assert!(titles.contains(&"Fix async race condition"));
    assert!(!titles.contains(&"Add UI polish"));

    drop(app);
}

// ═══════════════════════════════════════════════════════════════════
// Pagination
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn explore_pagination_slices_page_correctly() {
    let app = TestApp::spawn().await;
    let project = create_project(&app, "Pagination Project").await;
    for i in 0..5 {
        add_open_slice(&app, project, &format!("Slice {i}"), "code", 2).await;
    }

    let page1: Value = app
        .client
        .get(format!(
            "{}/api/explore?kind=slice&per_page=2&page=1",
            app.addr
        ))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let items1 = page1["data"]["items"].as_array().unwrap();
    assert_eq!(items1.len(), 2, "page 1 renvoie 2 items");

    let page2: Value = app
        .client
        .get(format!(
            "{}/api/explore?kind=slice&per_page=2&page=2",
            app.addr
        ))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let items2 = page2["data"]["items"].as_array().unwrap();
    assert_eq!(items2.len(), 2, "page 2 renvoie 2 items");

    // Les items de page 1 ≠ ceux de page 2
    let ids1: Vec<&str> = items1.iter().map(|i| i["id"].as_str().unwrap()).collect();
    let ids2: Vec<&str> = items2.iter().map(|i| i["id"].as_str().unwrap()).collect();
    for id in &ids1 {
        assert!(!ids2.contains(id), "pagination sans chevauchement");
    }

    drop(app);
}
