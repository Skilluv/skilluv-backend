//! Tests FE-M9 — routes modération inline (require_any_capability).

mod common;
use common::TestApp;
use serde_json::json;

async fn setup_curator(app: &TestApp, username: &str, cap: &str) -> uuid::Uuid {
    app.register_user(username).await;
    let uid: uuid::Uuid = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT id FROM users WHERE username = '{username}'"
    )))
    .fetch_one(&app.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO user_capabilities (user_id, capability, granted_reason)
         VALUES ($1, $2, 'test') ON CONFLICT DO NOTHING",
    )
    .bind(uid)
    .bind(cap)
    .execute(&app.db)
    .await
    .unwrap();
    app.login(username).await;
    uid
}

#[tokio::test]
async fn user_mutes_table_exists() {
    let app = TestApp::spawn().await;
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM information_schema.tables WHERE table_name='user_mutes')",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(exists);
}

#[tokio::test]
async fn community_review_queue_accessible_to_curator() {
    let app = TestApp::spawn().await;
    setup_curator(&app, "curator_a", "community_curator").await;

    let resp = app.get("/api/community/challenges/review").await;
    assert_eq!(resp.status().as_u16(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["data"].is_array());
    assert!(body["pagination"].is_object());
}

#[tokio::test]
async fn community_review_queue_forbidden_for_regular_user() {
    let app = TestApp::spawn().await;
    app.register_user("regular_a").await;
    app.login("regular_a").await;

    let resp = app.get("/api/community/challenges/review").await;
    assert_eq!(resp.status().as_u16(), 403);
}

#[tokio::test]
async fn fraud_flagged_list_accessible_to_reviewer() {
    let app = TestApp::spawn().await;
    setup_curator(&app, "rev_a", "plagiarism_reviewer").await;

    let resp = app.get("/api/fraud/deliverables/flagged").await;
    assert_eq!(resp.status().as_u16(), 200);
}

#[tokio::test]
async fn forum_moderate_post_hides() {
    let app = TestApp::spawn().await;
    setup_curator(&app, "mod_a", "forum_moderator").await;

    // Seed un post + auteur (direct DB pour ne pas casser session moderator).
    let author_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, username, password_hash, first_name, last_name, display_name, role, skill_domain)
         VALUES ('author_moda@t.com', 'author_moda', 'x', 'A', 'B', 'AB', 'user', 'code')
         RETURNING id",
    ).fetch_one(&app.db).await.unwrap();
    let cat_id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM forum_categories LIMIT 1")
        .fetch_one(&app.db)
        .await
        .unwrap();
    let post_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO posts (category_id, author_id, kind, title, body)
         VALUES ($1, $2, 'discussion', 'test post', 'body content') RETURNING id",
    )
    .bind(cat_id)
    .bind(author_id)
    .fetch_one(&app.db)
    .await
    .unwrap();

    let resp = app
        .post(
            &format!("/api/forum/posts/{post_id}/moderate"),
            &json!({
                "action": "hide",
                "reason": "test violation of rules",
            }),
        )
        .await;
    assert_eq!(resp.status().as_u16(), 200);
    let deleted: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT deleted_at FROM posts WHERE id = $1")
            .bind(post_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(deleted.is_some());
}

#[tokio::test]
async fn forum_mute_user_creates_row_with_expiry() {
    let app = TestApp::spawn().await;
    let mod_uid = setup_curator(&app, "mod_b", "forum_moderator").await;

    let target_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, username, password_hash, first_name, last_name, display_name, role, skill_domain)
         VALUES ('target_mute@t.com', 'target_mute', 'x', 'A', 'B', 'AB', 'user', 'code')
         RETURNING id",
    ).fetch_one(&app.db).await.unwrap();

    let resp = app
        .post(
            &format!("/api/forum/users/{target_id}/mute"),
            &json!({
                "reason": "spamming discussions",
                "duration_hours": 2,
            }),
        )
        .await;
    assert_eq!(resp.status().as_u16(), 200);
    let (muted_by, expires): (uuid::Uuid, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
        "SELECT muted_by, expires_at FROM user_mutes WHERE user_id = $1 ORDER BY created_at DESC LIMIT 1",
    ).bind(target_id).fetch_one(&app.db).await.unwrap();
    assert_eq!(muted_by, mod_uid);
    assert!(expires > chrono::Utc::now());
}
