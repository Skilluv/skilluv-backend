//! Handing in a file too large to send through the API.
//!
//! These tests exercise the parts that do not need a live object store: the
//! ceilings, the ownership rule, the session lifecycle and the refusals. The
//! presigning itself needs MinIO, and the suite says so rather than mocking a
//! signature — a mocked presign proves nothing about whether the real one is
//! accepted.

mod common;
use common::TestApp;
use serde_json::{Value, json};
use uuid::Uuid;

async fn user_id(app: &TestApp, username: &str) -> Uuid {
    sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(username)
        .fetch_one(&app.db)
        .await
        .unwrap()
}

/// Whether MinIO answered. Without it, `init` cannot open a multipart upload,
/// and the tests that need one say they were skipped rather than failing —
/// a red suite that means "no object store on this machine" teaches people to
/// ignore red suites.
async fn storage_is_up(app: &TestApp) -> bool {
    app.register_user("upload_probe").await;
    app.login("upload_probe").await;
    let resp = app
        .post(
            "/api/design/uploads",
            &json!({
                "design_subtype": "icon_set",
                "filename": "probe.svg",
                "content_type": "image/svg+xml",
                "declared_bytes": 1024,
            }),
        )
        .await;
    resp.status().as_u16() == 201
}

const MB: i64 = 1024 * 1024;

#[tokio::test]
async fn a_file_larger_than_its_subtype_allows_is_refused_before_a_byte_moves() {
    let app = TestApp::spawn().await;
    app.register_user("upload_too_big").await;
    app.login("upload_too_big").await;

    // A copy deck is words. Telling somebody after five gigabytes have moved
    // is telling them too late, so the refusal is on the declared size.
    let resp = app
        .post(
            "/api/design/uploads",
            &json!({
                "design_subtype": "copy_deck",
                "filename": "textes.docx",
                "content_type": "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
                "declared_bytes": 500 * MB,
            }),
        )
        .await;
    assert_eq!(resp.status().as_u16(), 400);
    let body = resp.text().await.unwrap();
    assert!(
        body.contains("100"),
        "the message names the ceiling: {body}"
    );
}

#[tokio::test]
async fn an_unknown_subtype_is_refused() {
    let app = TestApp::spawn().await;
    app.register_user("upload_bad_subtype").await;
    app.login("upload_bad_subtype").await;

    let resp = app
        .post(
            "/api/design/uploads",
            &json!({
                "design_subtype": "tapisserie",
                "filename": "x.png",
                "content_type": "image/png",
                "declared_bytes": 1024,
            }),
        )
        .await;
    assert_eq!(resp.status().as_u16(), 400);
}

#[tokio::test]
async fn an_upload_cannot_be_parked_against_somebody_elses_challenge() {
    let app = TestApp::spawn().await;
    app.register_user("upload_owner").await;
    app.register_user("upload_stranger").await;
    let owner = user_id(&app, "upload_owner").await;

    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO projects (slug, name, owner_type, owner_id)
         VALUES ($1, 'Projet', 'user', $2) RETURNING id",
    )
    .bind(format!("upload-p-{}", Uuid::new_v4()))
    .bind(owner)
    .fetch_one(&app.db)
    .await
    .unwrap();

    let slice: Uuid = sqlx::query_scalar(
        "INSERT INTO project_slices
            (project_id, slice_type, title, description, primary_domain, difficulty,
             status, claimed_by_user_id, claimed_at, design_subtype, orientation_id)
         VALUES ($1, 'design_artifact', 'Identité', 'Un brief.', 'design', 2, 'claimed',
                 $2, NOW(), 'brand_kit',
                 (SELECT id FROM orientations WHERE slug = 'design-brand-identity'))
         RETURNING id",
    )
    .bind(project)
    .bind(owner)
    .fetch_one(&app.db)
    .await
    .unwrap();

    // Otherwise an upload could be parked against work that is not yours, and
    // the reviewer would read it as the claimant's.
    app.login("upload_stranger").await;
    let resp = app
        .post(
            "/api/design/uploads",
            &json!({
                "design_subtype": "brand_kit",
                "filename": "identite.svg",
                "content_type": "image/svg+xml",
                "declared_bytes": 1024,
                "slice_id": slice,
            }),
        )
        .await;
    assert_eq!(resp.status().as_u16(), 403);
}

#[tokio::test]
async fn an_upload_needs_an_account_that_finished_signing_up() {
    let app = TestApp::spawn().await;

    // An upload reserves storage somebody pays for, and an unverified address
    // is the cheapest thing in the world to make.
    let resp = reqwest::Client::new()
        .post(format!("{}/api/design/uploads", app.addr))
        .json(&json!({
            "design_subtype": "icon_set",
            "filename": "x.svg",
            "content_type": "image/svg+xml",
            "declared_bytes": 1024,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 401);
}

#[tokio::test]
async fn a_large_file_is_split_into_parts_and_the_last_one_is_short() {
    let app = TestApp::spawn().await;
    if !storage_is_up(&app).await {
        eprintln!("skipped: no object store on this machine");
        return;
    }

    app.register_user("upload_parts").await;
    app.login("upload_parts").await;

    // 40 MB at a 16 MB part size: three parts, the last one 8 MB.
    let declared = 40 * MB;
    let body: Value = app
        .post(
            "/api/design/uploads",
            &json!({
                "design_subtype": "illustration_set",
                "filename": "planches.zip",
                "content_type": "application/zip",
                "declared_bytes": declared,
            }),
        )
        .await
        .json()
        .await
        .unwrap();

    let parts = body["data"]["upload"]["parts"].as_array().unwrap();
    assert_eq!(parts.len(), 3, "{body}");
    assert_eq!(parts[0]["bytes"].as_i64().unwrap(), 16 * MB);
    assert_eq!(parts[2]["bytes"].as_i64().unwrap(), 8 * MB);
    assert!(
        parts[0]["url"].as_str().unwrap().contains("partNumber=1"),
        "the part number is signed into the URL: {}",
        parts[0]["url"]
    );
}

#[tokio::test]
async fn the_subtypes_a_browser_cannot_open_are_told_so_before_the_upload() {
    let app = TestApp::spawn().await;
    if !storage_is_up(&app).await {
        eprintln!("skipped: no object store on this machine");
        return;
    }

    app.register_user("upload_preview").await;
    app.login("upload_preview").await;

    for (subtype, expected) in [("three_d_scene", true), ("icon_set", false)] {
        let body: Value = app
            .post(
                "/api/design/uploads",
                &json!({
                    "design_subtype": subtype,
                    "filename": "fichier.bin",
                    "content_type": "application/octet-stream",
                    "declared_bytes": 1024,
                }),
            )
            .await
            .json()
            .await
            .unwrap();
        // Learned before the upload rather than at completion: five gigabytes
        // is a long way to travel to be told you also needed a still.
        assert_eq!(
            body["data"]["upload"]["preview_required"],
            json!(expected),
            "{subtype}"
        );
    }
}

#[tokio::test]
async fn completing_with_the_wrong_number_of_parts_is_refused() {
    let app = TestApp::spawn().await;
    if !storage_is_up(&app).await {
        eprintln!("skipped: no object store on this machine");
        return;
    }

    app.register_user("upload_wrong_parts").await;
    app.login("upload_wrong_parts").await;

    let body: Value = app
        .post(
            "/api/design/uploads",
            &json!({
                "design_subtype": "illustration_set",
                "filename": "planches.zip",
                "content_type": "application/zip",
                "declared_bytes": 40 * MB,
            }),
        )
        .await
        .json()
        .await
        .unwrap();
    let session = body["data"]["upload"]["session_id"].as_str().unwrap();

    let resp = app
        .post(
            &format!("/api/design/uploads/{session}/complete"),
            &json!({ "parts": [{ "part_number": 1, "etag": "\"deadbeef\"" }] }),
        )
        .await;
    assert_eq!(resp.status().as_u16(), 400);
}

#[tokio::test]
async fn somebody_elses_upload_is_not_yours_to_finish() {
    let app = TestApp::spawn().await;
    if !storage_is_up(&app).await {
        eprintln!("skipped: no object store on this machine");
        return;
    }

    app.register_user("upload_mine").await;
    app.register_user("upload_theirs").await;

    app.login("upload_mine").await;
    let body: Value = app
        .post(
            "/api/design/uploads",
            &json!({
                "design_subtype": "icon_set",
                "filename": "icones.svg",
                "content_type": "image/svg+xml",
                "declared_bytes": 2048,
            }),
        )
        .await
        .json()
        .await
        .unwrap();
    let session = body["data"]["upload"]["session_id"].as_str().unwrap();

    app.login("upload_theirs").await;
    let resp = app
        .get(&format!("/api/design/uploads/{session}/download-url"))
        .await;
    // Not 403: a stranger should not learn that this session exists.
    assert_eq!(resp.status().as_u16(), 404);
}

#[tokio::test]
async fn an_unfinished_upload_has_nothing_to_download() {
    let app = TestApp::spawn().await;
    if !storage_is_up(&app).await {
        eprintln!("skipped: no object store on this machine");
        return;
    }

    app.register_user("upload_unfinished").await;
    app.login("upload_unfinished").await;

    let body: Value = app
        .post(
            "/api/design/uploads",
            &json!({
                "design_subtype": "icon_set",
                "filename": "icones.svg",
                "content_type": "image/svg+xml",
                "declared_bytes": 2048,
            }),
        )
        .await
        .json()
        .await
        .unwrap();
    let session = body["data"]["upload"]["session_id"].as_str().unwrap();

    let resp = app
        .get(&format!("/api/design/uploads/{session}/download-url"))
        .await;
    assert_eq!(resp.status().as_u16(), 409);
}

#[tokio::test]
async fn an_expired_session_is_not_resumable() {
    let app = TestApp::spawn().await;
    if !storage_is_up(&app).await {
        eprintln!("skipped: no object store on this machine");
        return;
    }

    app.register_user("upload_expired").await;
    app.login("upload_expired").await;

    let body: Value = app
        .post(
            "/api/design/uploads",
            &json!({
                "design_subtype": "icon_set",
                "filename": "icones.svg",
                "content_type": "image/svg+xml",
                "declared_bytes": 2048,
            }),
        )
        .await
        .json()
        .await
        .unwrap();
    let session = body["data"]["upload"]["session_id"].as_str().unwrap();

    sqlx::query("UPDATE design_upload_sessions SET expires_at = NOW() - INTERVAL '1 hour' WHERE id = $1::uuid")
        .bind(session)
        .execute(&app.db)
        .await
        .unwrap();

    let resp = app
        .get(&format!("/api/design/uploads/{session}/parts?from=1&to=1"))
        .await;
    assert_eq!(resp.status().as_u16(), 409);
}
