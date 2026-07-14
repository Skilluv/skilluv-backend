//! Tests P17.6 : events + participation.

mod common;
use common::TestApp;

async fn insert_event(app: &TestApp, slug: &str, active: bool) -> uuid::Uuid {
    sqlx::query_scalar(
        "INSERT INTO events (slug, name, description, starts_at, is_active)
         VALUES ($1, $2, 'd', NOW(), $3) RETURNING id",
    )
    .bind(slug)
    .bind(format!("Event {slug}"))
    .bind(active)
    .fetch_one(&app.db).await.unwrap()
}

#[tokio::test]
async fn list_events_returns_active_only() {
    let app = TestApp::spawn().await;
    insert_event(&app, "hacktoberfest-2026", true).await;
    insert_event(&app, "archived-2020", false).await;

    let resp = app.get("/api/badge-events").await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let slugs: Vec<String> = body["data"]["events"]
        .as_array().unwrap().iter()
        .map(|e| e["slug"].as_str().unwrap().into())
        .collect();
    assert!(slugs.contains(&"hacktoberfest-2026".into()));
    assert!(!slugs.contains(&"archived-2020".into()));
}

#[tokio::test]
async fn join_event_idempotent() {
    let app = TestApp::spawn().await;
    insert_event(&app, "skilluv-fest-2026", true).await;
    app.register_user("kim176a").await;
    app.login("kim176a").await;

    let r1 = app.post("/api/badge-events/skilluv-fest-2026/join", &serde_json::json!({})).await;
    assert_eq!(r1.status().as_u16(), 200);
    let r2 = app.post("/api/badge-events/skilluv-fest-2026/join", &serde_json::json!({})).await;
    assert_eq!(r2.status().as_u16(), 200, "second join is idempotent");

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_event_participation
         WHERE event_id = (SELECT id FROM events WHERE slug = 'skilluv-fest-2026')",
    )
    .fetch_one(&app.db).await.unwrap();
    assert_eq!(count, 1, "one row per (user, event)");
}

#[tokio::test]
async fn cannot_join_inactive_event() {
    let app = TestApp::spawn().await;
    insert_event(&app, "closed-event", false).await;
    app.register_user("kim176b").await;
    app.login("kim176b").await;
    let r = app.post("/api/badge-events/closed-event/join", &serde_json::json!({})).await;
    assert_eq!(r.status().as_u16(), 400);
}

#[tokio::test]
async fn my_events_lists_joined_only() {
    let app = TestApp::spawn().await;
    insert_event(&app, "ev-a", true).await;
    insert_event(&app, "ev-b", true).await;
    app.register_user("kim176c").await;
    app.login("kim176c").await;
    app.post("/api/badge-events/ev-a/join", &serde_json::json!({})).await;

    let resp = app.get("/api/users/me/badge-events").await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let slugs: Vec<String> = body["data"]["events"]
        .as_array().unwrap().iter()
        .map(|e| e["event_slug"].as_str().unwrap().into()).collect();
    assert_eq!(slugs, vec!["ev-a"], "only joined events are returned");
}

#[tokio::test]
async fn join_404_on_unknown_slug() {
    let app = TestApp::spawn().await;
    app.register_user("kim176d").await;
    app.login("kim176d").await;
    let r = app.post("/api/badge-events/nope-nope/join", &serde_json::json!({})).await;
    assert_eq!(r.status().as_u16(), 404);
}
