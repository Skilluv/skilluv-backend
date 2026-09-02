//! Integration tests for SKI-286 — mention inbox.
//!
//! The load-bearing tests here are the confidentiality ones: a mention
//! inside a DM or a private diary entry must never surface the content to
//! someone who cannot already read it.

mod common;

use common::TestApp;
use reqwest::StatusCode;
use serde_json::{Value, json};
use uuid::Uuid;

use skilluv_backend::services::mentions;

fn user_id_of(register_body: &Value) -> Uuid {
    register_body["data"]["user"]["id"]
        .as_str()
        .expect("register response carries a user id")
        .parse()
        .expect("user id is a uuid")
}

async fn seed_forum_category(app: &TestApp) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO forum_categories (id, slug, name, description, position)
         VALUES ($1, $2, 'Test category', 'desc', 1)",
    )
    .bind(id)
    .bind(format!("cat-{}", &id.to_string()[..8]))
    .execute(&app.db)
    .await
    .expect("seed category");
    id
}

/// Insert a forum post directly through the service, so the mention hook
/// runs exactly as it does in production.
async fn post_with_body(app: &TestApp, category_id: Uuid, author: Uuid, body: &str) -> Uuid {
    let post = skilluv_backend::services::forum::create_post(
        &app.db,
        skilluv_backend::services::forum::CreatePostInput {
            category_id,
            author_id: author,
            kind: "discussion".to_string(),
            title: "A post that mentions someone".to_string(),
            body: body.to_string(),
            bounty_fragments: 0,
            challenge_id: None,
        },
        "user",
    )
    .await
    .expect("create post");
    post.id
}

#[tokio::test]
async fn a_forum_post_mention_lands_in_the_inbox() {
    let app = TestApp::spawn().await;
    let author = app.register_user("mentionauthor").await;
    let author_id = user_id_of(&author);
    app.register_user("kofi").await;
    let category = seed_forum_category(&app).await;

    post_with_body(
        &app,
        category,
        author_id,
        "on devrait demander a @kofi de relire ce patch",
    )
    .await;

    app.login("kofi").await;
    let resp = app.get("/api/users/me/mentions").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = resp.json().await.unwrap();

    let items = body["data"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["source_type"], "forum_post");
    assert!(
        items[0]["source_url"]
            .as_str()
            .unwrap()
            .starts_with("/forum/"),
        "the back end builds the front-end path"
    );
    assert!(
        items[0]["excerpt"].as_str().unwrap().contains("@kofi"),
        "the excerpt is centred on the mention"
    );
    assert_eq!(items[0]["author"]["username"], "mentionauthor");
    assert!(items[0]["read_at"].is_null());
    assert_eq!(body["pagination"]["total"], 1);

    // The notification was raised too.
    let kinds: Vec<String> = sqlx::query_scalar(
        "SELECT notification_type FROM notifications WHERE user_id =
             (SELECT id FROM users WHERE username = 'kofi')",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();
    assert!(kinds.contains(&"mention_received".to_string()));
}

#[tokio::test]
async fn self_mentions_are_ignored() {
    let app = TestApp::spawn().await;
    let author = app.register_user("selfmention").await;
    let author_id = user_id_of(&author);
    let category = seed_forum_category(&app).await;

    post_with_body(
        &app,
        category,
        author_id,
        "note pour @selfmention plus tard",
    )
    .await;

    app.login("selfmention").await;
    let body: Value = app
        .get("/api/users/me/mentions")
        .await
        .json()
        .await
        .unwrap();
    assert!(body["data"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn editing_only_mentions_the_newly_named_user() {
    let app = TestApp::spawn().await;
    let author = app.register_user("editauthor").await;
    let author_id = user_id_of(&author);
    app.register_user("firstpeer").await;
    app.register_user("secondpeer").await;
    let category = seed_forum_category(&app).await;

    let post_id = post_with_body(&app, category, author_id, "avis de @firstpeer ?").await;

    // Re-saving with an extra handle must not duplicate the first mention.
    skilluv_backend::services::forum::edit_post(
        &app.db,
        post_id,
        author_id,
        "user",
        "A post that mentions someone",
        "avis de @firstpeer et @secondpeer ?",
    )
    .await
    .expect("edit post");

    let first_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM mentions WHERE mentioned_user_id =
             (SELECT id FROM users WHERE username = 'firstpeer')",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(first_count, 1, "the existing mention is not duplicated");

    let second_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM mentions WHERE mentioned_user_id =
             (SELECT id FROM users WHERE username = 'secondpeer')",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(second_count, 1, "the newly named user is mentioned");

    // And only the new user got a notification from the edit.
    let notif_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM notifications
          WHERE notification_type = 'mention_received'
            AND user_id = (SELECT id FROM users WHERE username = 'firstpeer')",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(notif_count, 1, "no second ping for someone already told");
}

#[tokio::test]
async fn a_deleted_source_removes_the_mention_from_the_list() {
    let app = TestApp::spawn().await;
    let author = app.register_user("delauthor").await;
    let author_id = user_id_of(&author);
    app.register_user("deltarget").await;
    let category = seed_forum_category(&app).await;

    let post_id = post_with_body(&app, category, author_id, "coucou @deltarget").await;

    app.login("deltarget").await;
    let body: Value = app
        .get("/api/users/me/mentions")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"].as_array().unwrap().len(), 1);

    sqlx::query("UPDATE posts SET deleted_at = NOW() WHERE id = $1")
        .bind(post_id)
        .execute(&app.db)
        .await
        .unwrap();

    let body: Value = app
        .get("/api/users/me/mentions")
        .await
        .json()
        .await
        .unwrap();
    assert!(
        body["data"].as_array().unwrap().is_empty(),
        "a mention whose source was removed by moderation disappears"
    );
    assert_eq!(body["pagination"]["total"], 0);
}

/// The confidentiality guarantee, on direct messages.
#[tokio::test]
async fn a_mention_in_a_dm_never_leaks_to_a_third_party() {
    let app = TestApp::spawn().await;
    let alice = app.register_user("dmalice").await;
    let alice_id = user_id_of(&alice);
    let bob = app.register_user("dmbob").await;
    let bob_id = user_id_of(&bob);
    app.register_user("dmcarol").await;

    // Alice DMs Bob and names Carol, who is not in the conversation.
    let conversation =
        skilluv_backend::services::dm::open_or_get_conversation(&app.db, alice_id, bob_id)
            .await
            .expect("open conversation");
    skilluv_backend::services::dm::send_message(
        &app.db,
        alice_id,
        conversation.id,
        "il faudrait demander a @dmcarol et @dmbob",
    )
    .await
    .expect("send message");

    // Carol is named, but cannot read the conversation.
    app.login("dmcarol").await;
    let body: Value = app
        .get("/api/users/me/mentions")
        .await
        .json()
        .await
        .unwrap();
    assert!(
        body["data"].as_array().unwrap().is_empty(),
        "naming a third party in a private conversation must not hand them its contents"
    );

    // Bob is a participant, so his mention surfaces.
    app.login("dmbob").await;
    let body: Value = app
        .get("/api/users/me/mentions")
        .await
        .json()
        .await
        .unwrap();
    let items = body["data"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["source_type"], "message");
}

/// The confidentiality guarantee, on private slice diary entries.
#[tokio::test]
async fn a_mention_in_a_private_diary_entry_stays_private() {
    let app = TestApp::spawn().await;
    let author = app.register_user("diaryauthor").await;
    let author_id = user_id_of(&author);
    app.register_user("diarypeer").await;

    // A slice claimed by the author, so the diary endpoint accepts a post.
    let project_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO projects (id, slug, name, owner_type, owner_id)
         VALUES ($1, $2, 'Diary project', 'user', $3)",
    )
    .bind(project_id)
    .bind(format!("diary-proj-{}", &project_id.to_string()[..8]))
    .bind(author_id)
    .execute(&app.db)
    .await
    .unwrap();
    let slice_id = Uuid::new_v4();
    // `claimed_at` must accompany `claimed_by_user_id`
    // (project_slices_claim_coherent, migration 0058).
    sqlx::query(
        "INSERT INTO project_slices
            (id, project_id, slice_type, title, description, primary_domain,
             difficulty, status, claimed_by_user_id, claimed_at)
         VALUES ($1, $2, 'github_issue', 'Slice', 'desc', 'code', 2, 'claimed', $3, NOW())",
    )
    .bind(slice_id)
    .bind(project_id)
    .bind(author_id)
    .execute(&app.db)
    .await
    .unwrap();

    app.login("diaryauthor").await;
    let resp = app
        .post(
            &format!("/api/slices/{slice_id}/diary"),
            &json!({ "body_markdown": "bloque ici, @diarypeer aurait une idee",
                     "is_public": false }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    app.login("diarypeer").await;
    let body: Value = app
        .get("/api/users/me/mentions")
        .await
        .json()
        .await
        .unwrap();
    assert!(
        body["data"].as_array().unwrap().is_empty(),
        "a private diary entry must not reach the person it names"
    );

    // Flipping the entry public makes the mention visible — visibility is
    // evaluated on read, against the content's current state.
    sqlx::query("UPDATE slice_diary_entries SET is_public = TRUE WHERE slice_id = $1")
        .bind(slice_id)
        .execute(&app.db)
        .await
        .unwrap();

    let body: Value = app
        .get("/api/users/me/mentions")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(
        body["data"].as_array().unwrap().len(),
        1,
        "publishing the entry surfaces the mention it always contained"
    );
}

#[tokio::test]
async fn read_and_read_all_are_idempotent() {
    let app = TestApp::spawn().await;
    let author = app.register_user("readauthor").await;
    let author_id = user_id_of(&author);
    app.register_user("readtarget").await;
    let category = seed_forum_category(&app).await;

    post_with_body(&app, category, author_id, "hello @readtarget one").await;
    post_with_body(&app, category, author_id, "hello @readtarget two").await;

    app.login("readtarget").await;
    let body: Value = app
        .get("/api/users/me/mentions")
        .await
        .json()
        .await
        .unwrap();
    let items = body["data"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    let first_id = items[0]["id"].as_str().unwrap().to_string();

    let resp = app
        .post(
            &format!("/api/users/me/mentions/{first_id}/read"),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = resp.json().await.unwrap();
    let first_read_at = body["data"]["read_at"].as_str().unwrap().to_string();

    // Reading twice keeps the original timestamp.
    let body: Value = app
        .post(
            &format!("/api/users/me/mentions/{first_id}/read"),
            &json!({}),
        )
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["read_at"].as_str().unwrap(), first_read_at);

    // unread_only now returns just the other one.
    let body: Value = app
        .get("/api/users/me/mentions?unread_only=true")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"].as_array().unwrap().len(), 1);

    let body: Value = app
        .post("/api/users/me/mentions/read-all", &json!({}))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["marked"], 1, "only the still-unread one");

    let body: Value = app
        .post("/api/users/me/mentions/read-all", &json!({}))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["marked"], 0);
}

#[tokio::test]
async fn another_users_mention_cannot_be_marked_read() {
    let app = TestApp::spawn().await;
    let author = app.register_user("scopeauthor").await;
    let author_id = user_id_of(&author);
    app.register_user("scopetarget").await;
    let category = seed_forum_category(&app).await;
    post_with_body(&app, category, author_id, "hey @scopetarget").await;

    let mention_id: Uuid = sqlx::query_scalar("SELECT id FROM mentions LIMIT 1")
        .fetch_one(&app.db)
        .await
        .unwrap();

    app.register_user("scopeother").await;
    app.login("scopeother").await;
    let resp = app
        .post(
            &format!("/api/users/me/mentions/{mention_id}/read"),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn pagination_is_reported() {
    let app = TestApp::spawn().await;
    let author = app.register_user("pageauthor").await;
    let author_id = user_id_of(&author);
    app.register_user("pagetarget").await;
    let category = seed_forum_category(&app).await;

    for i in 0..5 {
        post_with_body(&app, category, author_id, &format!("ping {i} @pagetarget")).await;
    }

    app.login("pagetarget").await;
    let body: Value = app
        .get("/api/users/me/mentions?per_page=2&page=1")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"].as_array().unwrap().len(), 2);
    assert_eq!(body["pagination"]["total"], 5);
    assert_eq!(body["pagination"]["total_pages"], 3);

    let body: Value = app
        .get("/api/users/me/mentions?per_page=2&page=3")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"].as_array().unwrap().len(), 1, "last page");
}

#[tokio::test]
async fn mentions_require_authentication() {
    let app = TestApp::spawn().await;
    let resp = app.get("/api/users/me/mentions").await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn unknown_and_banned_handles_are_not_mentioned() {
    let app = TestApp::spawn().await;
    let author = app.register_user("banauthor").await;
    let author_id = user_id_of(&author);
    let banned = app.register_user("bannedguy").await;
    let banned_id = user_id_of(&banned);
    sqlx::query("UPDATE users SET is_banned = TRUE WHERE id = $1")
        .bind(banned_id)
        .execute(&app.db)
        .await
        .unwrap();
    let category = seed_forum_category(&app).await;

    post_with_body(
        &app,
        category,
        author_id,
        "cc @bannedguy et @doesnotexist123",
    )
    .await;

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM mentions")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn excerpt_is_plain_text() {
    let app = TestApp::spawn().await;
    let author = app.register_user("mdauthor").await;
    let author_id = user_id_of(&author);
    app.register_user("mdtarget").await;
    let category = seed_forum_category(&app).await;

    post_with_body(
        &app,
        category,
        author_id,
        "**important** : `cargo test` puis demander a @mdtarget",
    )
    .await;

    app.login("mdtarget").await;
    let body: Value = app
        .get("/api/users/me/mentions")
        .await
        .json()
        .await
        .unwrap();
    let excerpt = body["data"][0]["excerpt"].as_str().unwrap();
    assert!(
        !excerpt.contains('*') && !excerpt.contains('`'),
        "markdown markers are stripped: {excerpt}"
    );
    assert!(excerpt.contains("@mdtarget"));
}

#[tokio::test]
async fn source_type_allowlist_is_enforced_at_the_service_boundary() {
    // Guards against a caller inventing a source type that the inbox query
    // has no branch for — such a mention would be invisible forever.
    assert!(mentions::validate_source_type("forum_post").is_ok());
    assert!(mentions::validate_source_type("tweet").is_err());
}
