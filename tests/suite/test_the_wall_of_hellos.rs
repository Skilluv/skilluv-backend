//! What the wall shows, and what it refuses to.
//!
//! The code rite is visible because GitHub publishes it, not because we do.
//! Take GitHub away and the same gesture in any other trade produced an
//! artefact its own author could not find: `hello_wall_entries` cannot hold a
//! designer, and the design profile excludes rite deliverables twice over.
//!
//! The wall is the answer, and it is a read rather than a table. These tests
//! spend most of their length on the refusals, because publishing somebody's
//! file is the part that cannot be taken back.

use crate::common::TestApp;
use uuid::Uuid;

/// The three conditions the wall reads together, and a HELLO that meets them.
async fn a_published_hello(app: &TestApp, username: &str, said: &str) -> Uuid {
    app.register_user(username).await;
    let user: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(username)
        .fetch_one(&app.db)
        .await
        .unwrap();

    // What an approved verdict does, reproduced: `reviews::submit` sets
    // `profile_active` when a rite settles, and the design profile is a 404
    // without it. Writing the deliverable by hand and skipping this would be
    // testing a state the platform never reaches.
    sqlx::query("UPDATE users SET profile_active = TRUE WHERE id = $1")
        .bind(user)
        .execute(&app.db)
        .await
        .unwrap();

    sqlx::query_scalar(
        "INSERT INTO deliverables
             (challenge_id, user_id, artifact_type, artifact_url, artifact_metadata,
              verifiable_by, verification_status, verified_at, public, submitted_at)
         SELECT ct.id, $1, 'other', 'skilluv:submission:' || gen_random_uuid(),
                jsonb_build_object('code_content', $2::text),
                'human_review', 'verified', NOW(), TRUE, NOW()
           FROM challenge_templates ct
          WHERE ct.is_domain_rite AND ct.skill_domain = 'design'
            AND ct.status = 'published'
         RETURNING id",
    )
    .bind(user)
    .bind(said)
    .fetch_one(&app.db)
    .await
    .unwrap_or_else(|e| panic!("could not write a HELLO for {username}: {e}"))
}

async fn wall(app: &TestApp, domain: &str) -> Vec<serde_json::Value> {
    let resp = app.get(&format!("/api/hello-wall?domain={domain}")).await;
    let status = resp.status();
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(status.is_success(), "the wall must answer: {status} {body}");
    body["data"]["entries"]
        .as_array()
        .unwrap_or_else(|| panic!("entries is an array: {body}"))
        .clone()
}

/// A read HELLO is on the wall, under its author's name.
///
/// Signed out, deliberately. The first screen somebody sees of Skilluv should
/// be the people already on it, not a login form.
#[tokio::test]
async fn a_hello_that_was_read_is_shown_to_anybody() {
    let app = TestApp::spawn().await;
    a_published_hello(&app, "wallhand", "I am Ama. I draw.").await;

    let entries = wall(&app, "design").await;
    let mine = entries
        .iter()
        .find(|e| e["username"] == "wallhand")
        .unwrap_or_else(|| panic!("wallhand is not on the wall: {entries:?}"));

    assert_eq!(mine["said"], "I am Ama. I draw.");
    assert!(
        mine["shown_at"].is_string(),
        "the wall says when it was read"
    );
}

/// A HELLO nobody has read yet is not on it.
///
/// The wall is not a queue. Somebody whose verdict has not come has not been
/// vouched for, and putting them beside people who have would make the wall
/// mean nothing.
#[tokio::test]
async fn a_hello_awaiting_a_verdict_is_not_shown() {
    let app = TestApp::spawn().await;
    let id = a_published_hello(&app, "pendinghand", "Not read yet.").await;
    sqlx::query(
        "UPDATE deliverables SET verification_status = 'pending', verified_at = NULL WHERE id = $1",
    )
    .bind(id)
    .execute(&app.db)
    .await
    .unwrap();

    let entries = wall(&app, "design").await;
    assert!(
        !entries.iter().any(|e| e["username"] == "pendinghand"),
        "an unread HELLO reached the wall: {entries:?}"
    );
}

/// A hidden profile is hidden here too.
///
/// Somebody who hides their profile has said they do not want to be looked
/// at. A wall that kept showing their introduction would be the one place the
/// setting did not reach, and it is the most visible place there is.
#[tokio::test]
async fn somebody_who_hid_their_profile_is_not_on_the_wall() {
    let app = TestApp::spawn().await;
    a_published_hello(&app, "shyhand", "I am here, quietly.").await;
    sqlx::query("UPDATE users SET profile_hidden = TRUE WHERE username = 'shyhand'")
        .execute(&app.db)
        .await
        .unwrap();

    let entries = wall(&app, "design").await;
    assert!(
        !entries.iter().any(|e| e["username"] == "shyhand"),
        "a hidden profile was shown on the wall: {entries:?}"
    );
}

/// A withdrawn HELLO leaves the wall.
#[tokio::test]
async fn a_revoked_hello_stops_being_shown() {
    let app = TestApp::spawn().await;
    let id = a_published_hello(&app, "gonehand", "Taken back.").await;
    sqlx::query("UPDATE deliverables SET revoked_at = NOW() WHERE id = $1")
        .bind(id)
        .execute(&app.db)
        .await
        .unwrap();

    let entries = wall(&app, "design").await;
    assert!(
        !entries.iter().any(|e| e["username"] == "gonehand"),
        "a revoked HELLO stayed on the wall: {entries:?}"
    );
}

/// Ordinary work is not a HELLO.
///
/// The wall reads `is_domain_rite`, not `public`. Every challenge submission
/// is written with `public = TRUE` by `services::deliverables`, so trusting
/// that flag alone would have put somebody's exercise on the front page of
/// their trade without anybody deciding to.
#[tokio::test]
async fn an_ordinary_deliverable_is_not_mistaken_for_a_hello() {
    let app = TestApp::spawn().await;
    app.register_user("workhand").await;

    let brief: Uuid = sqlx::query_scalar(
        "INSERT INTO challenge_templates
             (title, description, instructions, skill_domain, difficulty, status, is_training)
         VALUES ('Ordinary design work', 'Not the entrance.', 'Do the work.',
                 'design', 3, 'published', TRUE)
         RETURNING id",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO deliverables
             (challenge_id, user_id, artifact_type, artifact_url, artifact_metadata,
              verifiable_by, verification_status, verified_at, public, submitted_at)
         SELECT $1, id, 'other', 'skilluv:submission:' || gen_random_uuid(),
                jsonb_build_object('code_content', 'a piece of work'),
                'human_review', 'verified', NOW(), TRUE, NOW()
           FROM users WHERE username = 'workhand'",
    )
    .bind(brief)
    .execute(&app.db)
    .await
    .unwrap();

    let entries = wall(&app, "design").await;
    assert!(
        !entries.iter().any(|e| e["username"] == "workhand"),
        "ordinary work reached the wall of introductions: {entries:?}"
    );
}

/// One wall per trade.
#[tokio::test]
async fn a_wall_shows_one_domain_and_refuses_a_domain_that_does_not_exist() {
    let app = TestApp::spawn().await;
    a_published_hello(&app, "designonly", "Design.").await;

    let code = wall(&app, "code").await;
    assert!(
        !code.iter().any(|e| e["username"] == "designonly"),
        "a design HELLO appeared on the code wall"
    );

    let resp = app.get("/api/hello-wall?domain=not-a-trade").await;
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::BAD_REQUEST,
        "an unknown domain has to be refused rather than answered with nothing"
    );
}

/// A HELLO with no upload is shown without one.
///
/// Somebody whose HELLO is a pasted link has no `design_upload`, and the wall
/// says so with a null rather than inventing a placeholder. Showing a stock
/// tile under somebody's name would be showing something they did not make.
#[tokio::test]
async fn a_hello_with_nothing_uploaded_is_shown_without_a_picture() {
    let app = TestApp::spawn().await;
    a_published_hello(&app, "linkhand", "It lives at figma.com/file/xyz.").await;

    let entries = wall(&app, "design").await;
    let mine = entries
        .iter()
        .find(|e| e["username"] == "linkhand")
        .expect("linkhand is on the wall");

    assert!(mine["preview_url"].is_null(), "a picture was invented");
    assert_eq!(mine["is_preview"], false);
    assert!(
        mine["said"].as_str().unwrap().contains("figma.com"),
        "the text is what there is to show, so it has to be there"
    );
}

/// A HELLO is on its author's own profile.
///
/// This is the half that matters more than the wall. The design profile joins
/// `project_slices`, which a rite deliverable does not have, and filters
/// `artifact_type` to `design_artifact`, which a challenge submission is not.
/// So the first thing somebody made here was missing from the one page whose
/// job is to show what they have done, and the rite read as an exercise
/// thrown away rather than a first portfolio piece.
#[tokio::test]
async fn a_hello_is_on_its_authors_own_profile() {
    let app = TestApp::spawn().await;
    a_published_hello(&app, "profilehand", "I am Ama. I draw.").await;

    let body: serde_json::Value = app
        .get("/api/users/profilehand/design-profile")
        .await
        .json()
        .await
        .unwrap();

    let hello = &body["data"]["hello"];
    assert!(
        !hello.is_null(),
        "the HELLO is missing from its author's profile: {body}"
    );
    assert_eq!(hello["said"], "I am Ama. I draw.");
    assert_eq!(hello["username"], "profilehand");
}

/// And somebody who has not passed it has no HELLO rather than an empty one.
#[tokio::test]
async fn a_profile_without_a_hello_says_null_rather_than_pretending() {
    let app = TestApp::spawn().await;
    app.register_user("nohello").await;
    sqlx::query("UPDATE users SET profile_active = TRUE WHERE username = 'nohello'")
        .execute(&app.db)
        .await
        .unwrap();

    let body: serde_json::Value = app
        .get("/api/users/nohello/design-profile")
        .await
        .json()
        .await
        .unwrap();

    assert!(
        body["data"]["hello"].is_null(),
        "a profile invented a HELLO: {body}"
    );
}

/// Writes a completed upload and attaches it to a HELLO.
async fn with_an_upload(
    app: &TestApp,
    username: &str,
    subtype: &str,
    stored_bytes: i64,
    preview: bool,
    cover: bool,
) -> Uuid {
    let deliverable = a_published_hello(app, username, "Here it is.").await;
    let session: Uuid = sqlx::query_scalar(
        "INSERT INTO design_upload_sessions
             (user_id, design_subtype, filename, content_type, declared_bytes,
              stored_bytes, part_size, part_count, storage_key, s3_upload_id,
              preview_key, cover_key, status, completed_at, expires_at)
         SELECT id, $2, 'thing.png', 'image/png', $3, $3,
                5 * 1024 * 1024, 1, 'design/x/source', 'test-multipart-id',
                CASE WHEN $4 THEN 'design/x/preview' END,
                CASE WHEN $5 THEN 'design/x/cover' END,
                'completed', NOW(), NOW() + INTERVAL '1 day'
           FROM users WHERE username = $1
         RETURNING id",
    )
    .bind(username)
    .bind(subtype)
    .bind(stored_bytes)
    .bind(preview)
    .bind(cover)
    .fetch_one(&app.db)
    .await
    .unwrap_or_else(|e| panic!("could not write an upload for {username}: {e}"));

    sqlx::query(
        "UPDATE deliverables
            SET artifact_metadata = artifact_metadata
                || jsonb_build_object('attachments',
                     jsonb_build_array('design_upload:' || $2::text))
          WHERE id = $1",
    )
    .bind(deliverable)
    .bind(session)
    .execute(&app.db)
    .await
    .unwrap();

    session
}

async fn entry_of(app: &TestApp, username: &str) -> serde_json::Value {
    wall(app, "design")
        .await
        .into_iter()
        .find(|e| e["username"] == username)
        .unwrap_or_else(|| panic!("{username} is not on the wall"))
}

/// The cover wins, because it is the one picture chosen to be a tile.
#[tokio::test]
async fn the_cover_is_what_a_grid_shows() {
    let app = TestApp::spawn().await;
    with_an_upload(&app, "coverhand", "motion", 400 * 1024 * 1024, true, true).await;

    let mine = entry_of(&app, "coverhand").await;
    assert!(mine["preview_url"].is_string(), "nothing was shown");
    assert_eq!(
        mine["is_preview"], false,
        "a cover is not a preview: the front decides what element to render from this"
    );
}

/// Without a cover, the preview: it is what a reviewer opens for the four
/// subtypes a browser cannot.
#[tokio::test]
async fn the_preview_is_the_fallback_and_says_so() {
    let app = TestApp::spawn().await;
    with_an_upload(
        &app,
        "previewhand",
        "motion",
        400 * 1024 * 1024,
        true,
        false,
    )
    .await;

    let mine = entry_of(&app, "previewhand").await;
    assert!(mine["preview_url"].is_string());
    assert_eq!(mine["is_preview"], true);
}

/// A small file is its own tile.
#[tokio::test]
async fn a_small_file_needs_no_cover() {
    let app = TestApp::spawn().await;
    with_an_upload(&app, "smallhand", "icon_set", 64 * 1024, false, false).await;

    let mine = entry_of(&app, "smallhand").await;
    assert!(
        mine["preview_url"].is_string(),
        "an icon set of 64 kB is a perfectly good tile"
    );
    assert_eq!(mine["is_preview"], false);
}

/// And a large one is not, cover or nothing.
///
/// This is the refusal the ceiling exists for. Serving a 200 MB brand kit as
/// a thumbnail is not a worse tile, it is a page nobody on a phone can load.
/// The entry stays, with its text and no picture.
#[tokio::test]
async fn a_source_too_large_to_be_a_tile_is_not_served_as_one() {
    let app = TestApp::spawn().await;
    with_an_upload(
        &app,
        "heavyhand",
        "brand_kit",
        200 * 1024 * 1024,
        false,
        false,
    )
    .await;

    let mine = entry_of(&app, "heavyhand").await;
    assert!(
        mine["preview_url"].is_null(),
        "a 200 MB source was handed to a grid"
    );
    assert_eq!(
        mine["subtype"], "brand_kit",
        "the entry still says what it is, so the front can show a placeholder of its own"
    );
    assert!(mine["said"].is_string(), "and it still carries its text");
}
