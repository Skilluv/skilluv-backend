//! Integration tests for SKI-36 (bookmarks) and SKI-37 (private notes).
//!
//! Both features share the polymorphic `saved_items` plumbing, so they are
//! tested together — the interesting cases are the ones that exercise the
//! shared invariants: target-type validation, visibility enforcement, and
//! the dangling-target policy.

mod common;

use common::TestApp;
use reqwest::StatusCode;
use serde_json::{Value, json};
use uuid::Uuid;

/// Insert a published challenge template and return its id. Challenge
/// templates are the cheapest bookmarkable target — no owner, no
/// visibility rules beyond "not archived".
///
/// `is_training = TRUE` satisfies `challenge_templates_project_or_training`
/// (migration 0061): a published challenge must either be training material
/// or be attached to a real project.
async fn seed_challenge(app: &TestApp, title: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO challenge_templates
            (id, title, description, instructions, skill_domain, difficulty,
             status, is_training)
         VALUES ($1, $2, 'desc', 'instr', 'code', 2, 'published', TRUE)",
    )
    .bind(id)
    .bind(title)
    .execute(&app.db)
    .await
    .expect("seed challenge");
    id
}

async fn seed_project(app: &TestApp, owner: Uuid, slug: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO projects (id, slug, name, owner_type, owner_id)
         VALUES ($1, $2, 'Test project', 'user', $3)",
    )
    .bind(id)
    .bind(slug)
    .bind(owner)
    .execute(&app.db)
    .await
    .expect("seed project");
    id
}

fn user_id_of(register_body: &Value) -> Uuid {
    register_body["data"]["user"]["id"]
        .as_str()
        .expect("register response carries a user id")
        .parse()
        .expect("user id is a uuid")
}

// ═══════════════════════════════════════════════════════════════════
// SKI-36 — bookmarks
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn bookmark_create_list_and_delete() {
    let app = TestApp::spawn().await;
    app.register_user("bmuser").await;
    app.login("bmuser").await;

    let challenge = seed_challenge(&app, "Bookmarkable challenge").await;

    let resp = app
        .post(
            "/api/bookmarks",
            &json!({
                "target_type": "challenge_template",
                "target_id": challenge,
                "folder_slug": "game-dev-projects",
                "notes": "revisit this one",
            }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body: Value = resp.json().await.unwrap();
    let bookmark_id = body["data"]["bookmark"]["id"].as_str().unwrap().to_string();
    assert_eq!(
        body["data"]["bookmark"]["folder_slug"], "game-dev-projects",
        "folder is stored as sent"
    );

    // Listing resolves the target label from challenge_templates.
    let resp = app.get("/api/users/me/bookmarks").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = resp.json().await.unwrap();
    let items = body["data"]["bookmarks"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["target"]["title"], "Bookmarkable challenge");

    let resp = app.delete(&format!("/api/bookmarks/{bookmark_id}")).await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let resp = app.get("/api/users/me/bookmarks").await;
    let body: Value = resp.json().await.unwrap();
    assert!(body["data"]["bookmarks"].as_array().unwrap().is_empty());

    // Deleting twice is a 404, not a silent success — the client should be
    // able to tell "gone now" from "was never there".
    let resp = app.delete(&format!("/api/bookmarks/{bookmark_id}")).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn bookmark_post_is_an_idempotent_upsert() {
    let app = TestApp::spawn().await;
    app.register_user("bmupsert").await;
    app.login("bmupsert").await;
    let challenge = seed_challenge(&app, "Upsert target").await;

    let first: Value = app
        .post(
            "/api/bookmarks",
            &json!({ "target_type": "challenge_template", "target_id": challenge }),
        )
        .await
        .json()
        .await
        .unwrap();
    let first_id = first["data"]["bookmark"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Same target, now filed into a folder: must update in place, not
    // create a second row (the front-end save button is idempotent).
    let second: Value = app
        .post(
            "/api/bookmarks",
            &json!({
                "target_type": "challenge_template",
                "target_id": challenge,
                "folder_slug": "later",
            }),
        )
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(
        second["data"]["bookmark"]["id"].as_str().unwrap(),
        first_id,
        "re-bookmarking must reuse the same row"
    );
    assert_eq!(second["data"]["bookmark"]["folder_slug"], "later");

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM bookmarks")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn bookmark_filters_by_type_and_folder() {
    let app = TestApp::spawn().await;
    let me = app.register_user("bmfilter").await;
    app.login("bmfilter").await;
    let my_id = user_id_of(&me);

    let challenge = seed_challenge(&app, "Filter challenge").await;
    let project = seed_project(&app, my_id, "filter-project").await;

    app.post(
        "/api/bookmarks",
        &json!({
            "target_type": "challenge_template",
            "target_id": challenge,
            "folder_slug": "alpha",
        }),
    )
    .await;
    app.post(
        "/api/bookmarks",
        &json!({ "target_type": "project", "target_id": project }),
    )
    .await;

    let body: Value = app
        .get("/api/users/me/bookmarks?target_type=project")
        .await
        .json()
        .await
        .unwrap();
    let items = body["data"]["bookmarks"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["target_type"], "project");
    assert_eq!(items[0]["target"]["slug"], "filter-project");

    let body: Value = app
        .get("/api/users/me/bookmarks?folder_slug=alpha")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["bookmarks"].as_array().unwrap().len(), 1);

    // `unfiled` is the sentinel for "no folder".
    let body: Value = app
        .get("/api/users/me/bookmarks?folder_slug=unfiled")
        .await
        .json()
        .await
        .unwrap();
    let items = body["data"]["bookmarks"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["target_type"], "project");

    // Folder facets report both buckets.
    let body: Value = app
        .get("/api/users/me/bookmarks/folders")
        .await
        .json()
        .await
        .unwrap();
    let folders = body["data"]["folders"].as_array().unwrap();
    assert_eq!(folders.len(), 2);
    let names: Vec<&str> = folders
        .iter()
        .map(|f| f["folder_slug"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"alpha"));
    assert!(names.contains(&"unfiled"));
}

#[tokio::test]
async fn bookmark_rejects_unknown_type_and_missing_target() {
    let app = TestApp::spawn().await;
    app.register_user("bmreject").await;
    app.login("bmreject").await;

    let resp = app
        .post(
            "/api/bookmarks",
            &json!({ "target_type": "enterprise", "target_id": Uuid::new_v4() }),
        )
        .await;
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "unknown target_type is a validation error"
    );

    // Well-formed type, target that does not exist.
    let resp = app
        .post(
            "/api/bookmarks",
            &json!({ "target_type": "challenge_template", "target_id": Uuid::new_v4() }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    // Folder slug must match the DB CHECK shape, and fail as a 400 rather
    // than surfacing a database error.
    let challenge = seed_challenge(&app, "Slug shape").await;
    let resp = app
        .post(
            "/api/bookmarks",
            &json!({
                "target_type": "challenge_template",
                "target_id": challenge,
                "folder_slug": "Not A Slug",
            }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn bookmark_refuses_a_private_deliverable_of_another_user() {
    let app = TestApp::spawn().await;
    let author = app.register_user("bmauthor").await;
    let author_id = user_id_of(&author);

    // A private deliverable belonging to someone else.
    let deliverable_id = Uuid::new_v4();
    let project = seed_project(&app, author_id, "private-deliv-project").await;
    let slice_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO project_slices
            (id, project_id, slice_type, title, description, primary_domain, difficulty, status)
         VALUES ($1, $2, 'github_issue', 'Slice', 'desc', 'code', 2, 'open')",
    )
    .bind(slice_id)
    .bind(project)
    .execute(&app.db)
    .await
    .expect("seed slice");
    sqlx::query(
        "INSERT INTO deliverables
            (id, slice_id, user_id, artifact_type, artifact_url, verifiable_by,
             verification_status, public)
         VALUES ($1, $2, $3, 'pr_merged', 'https://example.test/pr/1',
                 'human_review', 'verified', FALSE)",
    )
    .bind(deliverable_id)
    .bind(slice_id)
    .bind(author_id)
    .execute(&app.db)
    .await
    .expect("seed deliverable");

    app.register_user("bmsnooper").await;
    app.login("bmsnooper").await;

    let resp = app
        .post(
            "/api/bookmarks",
            &json!({ "target_type": "deliverable", "target_id": deliverable_id }),
        )
        .await;
    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "a private deliverable must not be bookmarkable by a stranger, \
         and must answer 404 rather than 403 so it is not an existence oracle"
    );

    // The author themselves can bookmark it.
    app.login("bmauthor").await;
    let resp = app
        .post(
            "/api/bookmarks",
            &json!({ "target_type": "deliverable", "target_id": deliverable_id }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn bookmark_of_a_deleted_target_drops_out_of_the_listing() {
    let app = TestApp::spawn().await;
    app.register_user("bmdangling").await;
    app.login("bmdangling").await;
    let challenge = seed_challenge(&app, "Doomed challenge").await;

    app.post(
        "/api/bookmarks",
        &json!({ "target_type": "challenge_template", "target_id": challenge }),
    )
    .await;

    sqlx::query("DELETE FROM challenge_templates WHERE id = $1")
        .bind(challenge)
        .execute(&app.db)
        .await
        .expect("delete target");

    let resp = app.get("/api/users/me/bookmarks").await;
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "a dangling row must not break the listing"
    );
    let body: Value = resp.json().await.unwrap();
    assert!(
        body["data"]["bookmarks"].as_array().unwrap().is_empty(),
        "the orphaned bookmark is filtered out of the response"
    );
}

#[tokio::test]
async fn bookmarks_are_scoped_to_their_owner() {
    let app = TestApp::spawn().await;
    app.register_user("bmowner").await;
    app.login("bmowner").await;
    let challenge = seed_challenge(&app, "Owned").await;
    let created: Value = app
        .post(
            "/api/bookmarks",
            &json!({ "target_type": "challenge_template", "target_id": challenge }),
        )
        .await
        .json()
        .await
        .unwrap();
    let id = created["data"]["bookmark"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    app.register_user("bmother").await;
    app.login("bmother").await;

    let body: Value = app
        .get("/api/users/me/bookmarks")
        .await
        .json()
        .await
        .unwrap();
    assert!(
        body["data"]["bookmarks"].as_array().unwrap().is_empty(),
        "another user's bookmarks are invisible"
    );

    let resp = app.delete(&format!("/api/bookmarks/{id}")).await;
    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "deleting someone else's bookmark must not succeed"
    );
}

#[tokio::test]
async fn bookmarks_require_authentication() {
    let app = TestApp::spawn().await;
    let resp = app.get("/api/users/me/bookmarks").await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ═══════════════════════════════════════════════════════════════════
// SKI-37 — private notes
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn note_upsert_fetch_and_delete() {
    let app = TestApp::spawn().await;
    app.register_user("noteuser").await;
    app.login("noteuser").await;
    let challenge = seed_challenge(&app, "Annotated challenge").await;
    let path = format!("/api/users/me/notes/challenge_template/{challenge}");

    // Absent note reads as 200 + null, so the editor can open empty.
    let resp = app.get(&path).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = resp.json().await.unwrap();
    assert!(body["data"]["note"].is_null());

    let resp = app
        .put(&path, &json!({ "body": "j'ai aimé cette approche" }))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["note"]["body"], "j'ai aimé cette approche");
    let created_at = body["data"]["note"]["created_at"]
        .as_str()
        .unwrap()
        .to_string();

    // Second PUT updates in place and keeps created_at.
    let body: Value = app
        .put(&path, &json!({ "body": "revoir plus tard" }))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["note"]["body"], "revoir plus tard");
    assert_eq!(
        body["data"]["note"]["created_at"].as_str().unwrap(),
        created_at,
        "upsert preserves the original creation timestamp"
    );

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM user_notes")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(count, 1, "upsert must not stack rows");

    let resp = app.delete(&path).await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    let resp = app.delete(&path).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn note_enforces_length_bounds() {
    let app = TestApp::spawn().await;
    app.register_user("notelen").await;
    app.login("notelen").await;
    let challenge = seed_challenge(&app, "Length bounds").await;
    let path = format!("/api/users/me/notes/challenge_template/{challenge}");

    // Whitespace-only is a 400, not a silent delete.
    let resp = app.put(&path, &json!({ "body": "   " })).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let resp = app.put(&path, &json!({ "body": "x".repeat(1001) })).await;
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "the 1000-char cap is enforced before the DB CHECK"
    );

    // Exactly at the cap is accepted.
    let resp = app.put(&path, &json!({ "body": "x".repeat(1000) })).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn notes_are_private_to_their_author() {
    let app = TestApp::spawn().await;
    let challenge = seed_challenge(&app, "Private note target").await;
    let path = format!("/api/users/me/notes/challenge_template/{challenge}");

    app.register_user("noteauthor").await;
    app.login("noteauthor").await;
    app.put(&path, &json!({ "body": "my private thought" }))
        .await;

    app.register_user("notereader").await;
    app.login("notereader").await;

    let body: Value = app.get(&path).await.json().await.unwrap();
    assert!(
        body["data"]["note"].is_null(),
        "another user's note on the same target is invisible"
    );

    let body: Value = app.get("/api/users/me/notes").await.json().await.unwrap();
    assert!(body["data"]["notes"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn note_rejects_unknown_target_type() {
    let app = TestApp::spawn().await;
    app.register_user("notetype").await;
    app.login("notetype").await;

    let resp = app
        .put(
            &format!("/api/users/me/notes/enterprise/{}", Uuid::new_v4()),
            &json!({ "body": "nope" }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let resp = app
        .get(&format!(
            "/api/users/me/notes/enterprise/{}",
            Uuid::new_v4()
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn note_listing_filters_by_type_and_resolves_labels() {
    let app = TestApp::spawn().await;
    let me = app.register_user("notelist").await;
    app.login("notelist").await;
    let my_id = user_id_of(&me);

    let challenge = seed_challenge(&app, "Noted challenge").await;
    let project = seed_project(&app, my_id, "noted-project").await;

    app.put(
        &format!("/api/users/me/notes/challenge_template/{challenge}"),
        &json!({ "body": "note on challenge" }),
    )
    .await;
    app.put(
        &format!("/api/users/me/notes/project/{project}"),
        &json!({ "body": "note on project" }),
    )
    .await;

    let body: Value = app.get("/api/users/me/notes").await.json().await.unwrap();
    assert_eq!(body["data"]["notes"].as_array().unwrap().len(), 2);

    let body: Value = app
        .get("/api/users/me/notes?target_type=project")
        .await
        .json()
        .await
        .unwrap();
    let items = body["data"]["notes"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["target"]["title"], "Test project");
}
