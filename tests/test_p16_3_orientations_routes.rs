//! Tests P16.3 : routes API orientations (catalogue public + user_orientations).

mod common;
use common::TestApp;
use serde_json::json;

#[tokio::test]
async fn get_catalog_lists_curated_orientations() {
    let app = TestApp::spawn().await;
    let resp = app.get("/api/orientations?limit=100").await;
    assert!(resp.status().is_success());
    let body: serde_json::Value = resp.json().await.unwrap();
    let list = body["data"]["orientations"].as_array().unwrap();
    assert!(list.len() >= 30, "seed should expose 30+ curated");
    // Vérifie qu'un slug attendu est présent
    assert!(list.iter().any(|o| o["slug"] == "dev-frontend"));
}

#[tokio::test]
async fn get_catalog_filters_by_domain() {
    let app = TestApp::spawn().await;
    let resp = app.get("/api/orientations?domain=security&limit=100").await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let list = body["data"]["orientations"].as_array().unwrap();
    assert!(!list.is_empty());
    for o in list {
        assert_eq!(o["primary_domain"], "security");
    }
}

#[tokio::test]
async fn get_orientation_detail_includes_skills() {
    let app = TestApp::spawn().await;

    // Attache 1 skill au track dev-frontend pour vérifier le join.
    let track_id: uuid::Uuid = sqlx::query_scalar(
        "SELECT id FROM orientations WHERE slug = 'dev-frontend'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    let skill_id: uuid::Uuid = sqlx::query_scalar(
        "SELECT id FROM skill_nodes WHERE slug = 'component-composition' LIMIT 1",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO orientation_skill_map (orientation_id, skill_id, is_core, weight)
         VALUES ($1, $2, TRUE, 2.5) ON CONFLICT DO NOTHING",
    )
    .bind(track_id)
    .bind(skill_id)
    .execute(&app.db)
    .await
    .unwrap();

    let resp = app.get("/api/orientations/dev-frontend").await;
    assert!(resp.status().is_success());
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["orientation"]["slug"], "dev-frontend");
    let skills = body["data"]["skills"].as_array().unwrap();
    assert!(!skills.is_empty());
    assert!(skills.iter().any(|s| s["slug"] == "component-composition"));
}

#[tokio::test]
async fn get_orientation_detail_404_on_unknown() {
    let app = TestApp::spawn().await;
    let resp = app.get("/api/orientations/does-not-exist").await;
    assert_eq!(resp.status().as_u16(), 404);
}

#[tokio::test]
async fn register_orientation_auto_promotes_first_to_primary() {
    let app = TestApp::spawn().await;
    app.register_user("kim16r3").await;
    app.login("kim16r3").await;

    let resp = app
        .post(
            "/api/users/me/orientations",
            &json!({ "slug": "dev-frontend", "mode": "learning" }),
        )
        .await;
    assert_eq!(resp.status().as_u16(), 201);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["is_primary"], true, "1st orientation auto-primary");
}

#[tokio::test]
async fn cannot_register_more_than_three_active_orientations() {
    let app = TestApp::spawn().await;
    app.register_user("kim16r4").await;
    app.login("kim16r4").await;

    for slug in ["dev-frontend", "dev-backend", "web-designer"] {
        let r = app.post("/api/users/me/orientations", &json!({ "slug": slug })).await;
        assert_eq!(r.status().as_u16(), 201, "slug {slug} should succeed");
    }
    let over = app.post("/api/users/me/orientations", &json!({ "slug": "pentester-web" })).await;
    assert_eq!(over.status().as_u16(), 400, "cap 3 enforced");
}

#[tokio::test]
async fn delete_orientation_historises_but_keeps_row() {
    let app = TestApp::spawn().await;
    app.register_user("kim16r5").await;
    app.login("kim16r5").await;

    app.post("/api/users/me/orientations", &json!({ "slug": "dev-backend" }))
        .await;
    let del = app.delete("/api/users/me/orientations/dev-backend").await;
    assert_eq!(del.status().as_u16(), 200);

    // La ligne existe encore avec ended_at, invisible dans le "actives" mais
    // présente en base — historisation.
    let cnt: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_orientations uo
         JOIN orientations o ON o.id = uo.orientation_id
         WHERE o.slug = 'dev-backend' AND uo.ended_at IS NOT NULL",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(cnt, 1);

    // Peut ré-inscrire après end
    let re = app.post("/api/users/me/orientations", &json!({ "slug": "dev-backend" })).await;
    assert_eq!(re.status().as_u16(), 201, "re-registering after end works");
}

#[tokio::test]
async fn patch_switches_primary_flag_atomically() {
    let app = TestApp::spawn().await;
    app.register_user("kim16r6").await;
    app.login("kim16r6").await;

    app.post("/api/users/me/orientations", &json!({ "slug": "dev-frontend" }))
        .await; // auto-primary
    app.post("/api/users/me/orientations", &json!({ "slug": "web-designer" }))
        .await;

    let patch = app
        .put(
            "/api/users/me/orientations/web-designer",
            &json!({ "is_primary": true }),
        )
        .await;
    // Note: TestApp::put uses PUT, but our route is PATCH. Use the raw client.
    // We'll skip this test path if PATCH isn't in TestApp — swap with client.
    let _ = patch;
    let resp = app
        .client
        .patch(format!("{}/api/users/me/orientations/web-designer", app.addr))
        .json(&json!({ "is_primary": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 200);

    // La seule primary active est maintenant web-designer.
    let (front_primary, design_primary): (bool, bool) = sqlx::query_as(
        "SELECT
            COALESCE(BOOL_OR(o.slug='dev-frontend' AND uo.is_primary), FALSE),
            COALESCE(BOOL_OR(o.slug='web-designer' AND uo.is_primary), FALSE)
         FROM user_orientations uo JOIN orientations o ON o.id = uo.orientation_id
         WHERE uo.ended_at IS NULL",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(!front_primary);
    assert!(design_primary);
}
