//! Integration tests for SKI-42 — external reputation signals.
//!
//! The load-bearing test here is [`external_signals_never_touch_proofs`]:
//! the whole feature is only acceptable because importing off-platform
//! history cannot buy rank, badges or proven-skill counts. URL validation
//! is unit-tested in `services::external_signals`.

mod common;

use common::TestApp;
use reqwest::StatusCode;
use serde_json::{Value, json};
use uuid::Uuid;

use skilluv_backend::services::proof_hooks;

fn user_id_of(register_body: &Value) -> Uuid {
    register_body["data"]["user"]["id"]
        .as_str()
        .expect("register response carries a user id")
        .parse()
        .expect("user id is a uuid")
}

/// Give a user a GitHub OAuth link, which is the only self-verifying path.
async fn link_github(app: &TestApp, user_id: Uuid, login: &str) {
    sqlx::query(
        "INSERT INTO github_connections
            (user_id, github_user_id, github_login, access_token_encrypted, access_token_nonce)
         VALUES ($1, $2, $3, '\\x00'::BYTEA, '\\x00'::BYTEA)",
    )
    .bind(user_id)
    .bind(rand_github_id())
    .bind(login)
    .execute(&app.db)
    .await
    .expect("link github");
}

/// github_user_id is UNIQUE, so each test user needs a distinct one.
fn rand_github_id() -> i64 {
    // Derived from a fresh uuid rather than a counter: test binaries run in
    // parallel against separate databases, but this keeps collisions out
    // even within one.
    let bytes = Uuid::new_v4().into_bytes();
    i64::from(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

#[tokio::test]
async fn github_signal_self_verifies_when_the_login_matches() {
    let app = TestApp::spawn().await;
    let me = app.register_user("exgithub").await;
    let my_id = user_id_of(&me);
    link_github(&app, my_id, "alice").await;
    app.login("exgithub").await;

    let resp = app
        .post(
            "/api/users/me/external-signals",
            &json!({
                "provider": "github",
                "url": "https://github.com/alice/my-repo",
                "title": "My open-source repo",
            }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(
        body["data"]["auto_verified"], true,
        "an existing OAuth link proves ownership with no outbound request"
    );
    assert_eq!(
        body["data"]["signal"]["verification_method"],
        "oauth_github"
    );
    assert_eq!(body["data"]["signal"]["meta"]["github_login"], "alice");
}

#[tokio::test]
async fn github_signal_for_another_login_stays_unverified() {
    let app = TestApp::spawn().await;
    let me = app.register_user("eximpostor").await;
    let my_id = user_id_of(&me);
    link_github(&app, my_id, "alice").await;
    app.login("eximpostor").await;

    let body: Value = app
        .post(
            "/api/users/me/external-signals",
            &json!({
                "provider": "github",
                "url": "https://github.com/torvalds/linux",
                "title": "Definitely mine",
            }),
        )
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(
        body["data"]["auto_verified"], false,
        "claiming someone else's repo must not self-verify"
    );
    assert!(body["data"]["signal"]["verified_at"].is_null());
}

#[tokio::test]
async fn blog_signals_are_declared_then_moderator_verified() {
    let app = TestApp::spawn().await;
    app.register_user("exblogger").await;
    app.login("exblogger").await;

    let created: Value = app
        .post(
            "/api/users/me/external-signals",
            &json!({
                "provider": "dev_to",
                "url": "https://dev.to/exblogger/my-post",
                "title": "How I learned Rust",
            }),
        )
        .await
        .json()
        .await
        .unwrap();
    let signal_id = created["data"]["signal"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(created["data"]["auto_verified"], false);

    // Buckets keep declared and verified apart.
    let body: Value = app
        .get("/api/users/me/external-signals")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["declared"].as_array().unwrap().len(), 1);
    assert!(body["data"]["verified"].as_array().unwrap().is_empty());
    assert!(
        body["data"]["disclaimer"].is_string(),
        "the payload states these are not proofs"
    );

    // A plain user cannot reach the moderation queue.
    let resp = app.get("/api/moderation/external-signals").await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    let moderator = app.register_user("exmoderator").await;
    let moderator_id = user_id_of(&moderator);
    sqlx::query(
        "INSERT INTO user_capabilities (user_id, capability, granted_reason)
         VALUES ($1, 'community_moderator', 'test')",
    )
    .bind(moderator_id)
    .execute(&app.db)
    .await
    .unwrap();
    app.login("exmoderator").await;

    let body: Value = app
        .get("/api/moderation/external-signals")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["pending"].as_array().unwrap().len(), 1);

    let resp = app
        .post(
            &format!("/api/moderation/external-signals/{signal_id}/verify"),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(
        body["data"]["signal"]["verification_method"],
        "manual_review"
    );

    // Verifying twice is a 404 — it is no longer pending.
    let resp = app
        .post(
            &format!("/api/moderation/external-signals/{signal_id}/verify"),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    app.login("exblogger").await;
    let body: Value = app
        .get("/api/users/me/external-signals")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["verified"].as_array().unwrap().len(), 1);
    assert!(body["data"]["declared"].as_array().unwrap().is_empty());
}

/// The invariant the whole feature rests on.
#[tokio::test]
async fn external_signals_never_touch_proofs() {
    let app = TestApp::spawn().await;
    let me = app.register_user("exproof").await;
    let my_id = user_id_of(&me);
    link_github(&app, my_id, "exproof").await;
    app.login("exproof").await;

    // Baseline after a proof recompute with no signals.
    proof_hooks::recompute_all_for_user(&app.db, my_id)
        .await
        .expect("baseline recompute");
    let rank_before: Option<String> =
        sqlx::query_scalar("SELECT rank FROM user_ranks WHERE user_id = $1")
            .bind(my_id)
            .fetch_optional(&app.db)
            .await
            .unwrap();
    let wpc_before: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(weighted_proven_count), 0)::BIGINT FROM user_skills WHERE user_id = $1",
    )
    .bind(my_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    let badges_before: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM user_badges WHERE user_id = $1")
            .bind(my_id)
            .fetch_one(&app.db)
            .await
            .unwrap();

    // Import the maximum allowed, all verified where possible.
    for i in 0..5 {
        let resp = app
            .post(
                "/api/users/me/external-signals",
                &json!({
                    "provider": "github",
                    "url": format!("https://github.com/exproof/repo-{i}"),
                    "title": format!("Repo {i}"),
                }),
            )
            .await;
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    // Recompute again: nothing about the proof engine may have moved.
    let report = proof_hooks::recompute_all_for_user(&app.db, my_id)
        .await
        .expect("recompute after signals");
    assert!(
        !report.rank_promoted,
        "importing external reputation must never promote"
    );

    let rank_after: Option<String> =
        sqlx::query_scalar("SELECT rank FROM user_ranks WHERE user_id = $1")
            .bind(my_id)
            .fetch_optional(&app.db)
            .await
            .unwrap();
    let wpc_after: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(weighted_proven_count), 0)::BIGINT FROM user_skills WHERE user_id = $1",
    )
    .bind(my_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    let badges_after: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM user_badges WHERE user_id = $1")
            .bind(my_id)
            .fetch_one(&app.db)
            .await
            .unwrap();

    assert_eq!(rank_before, rank_after, "rank is unchanged");
    assert_eq!(
        wpc_before, wpc_after,
        "weighted_proven_count is unchanged — 'proven on Skilluv' stays literal"
    );
    assert_eq!(badges_before, badges_after, "no badge is earned");
}

#[tokio::test]
async fn signals_are_capped_deduplicated_and_owner_scoped() {
    let app = TestApp::spawn().await;
    let me = app.register_user("excap").await;
    let my_id = user_id_of(&me);
    app.login("excap").await;

    for i in 0..20 {
        let resp = app
            .post(
                "/api/users/me/external-signals",
                &json!({
                    "provider": "dev_to",
                    "url": format!("https://dev.to/excap/post-{i}"),
                    "title": format!("Post {i}"),
                }),
            )
            .await;
        assert_eq!(resp.status(), StatusCode::CREATED, "signal {i}");
    }

    let resp = app
        .post(
            "/api/users/me/external-signals",
            &json!({
                "provider": "dev_to",
                "url": "https://dev.to/excap/one-too-many",
                "title": "One too many",
            }),
        )
        .await;
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "external context is a sidebar, not a second portfolio"
    );

    // Free a slot, so the duplicate check below is reached: at the cap the
    // quota check fires first (it is the cheaper of the two), and a
    // duplicate would be refused as "too many" rather than "already there".
    let last_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM external_signals WHERE user_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(my_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    let resp = app
        .delete(&format!("/api/users/me/external-signals/{last_id}"))
        .await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // The same URL twice is a duplicate.
    let resp = app
        .post(
            "/api/users/me/external-signals",
            &json!({
                "provider": "dev_to",
                "url": "https://dev.to/excap/post-0",
                "title": "Same link again",
            }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::CONFLICT);

    let created: Value = sqlx::query_scalar::<_, Value>(
        "SELECT jsonb_build_object('id', id) FROM external_signals LIMIT 1",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    let signal_id = created["id"].as_str().unwrap().to_string();

    app.register_user("exother").await;
    app.login("exother").await;
    let resp = app
        .delete(&format!("/api/users/me/external-signals/{signal_id}"))
        .await;
    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "another user's signal is not deletable"
    );
}

#[tokio::test]
async fn public_profile_shows_both_buckets_and_respects_hiding() {
    let app = TestApp::spawn().await;
    let me = app.register_user("expublic").await;
    let my_id = user_id_of(&me);
    app.login("expublic").await;
    app.post(
        "/api/users/me/external-signals",
        &json!({
            "provider": "conf_ref",
            "url": "https://rustconf.test/talks/2026/expublic",
            "title": "My conference talk",
        }),
    )
    .await;

    let resp = app
        .get(&format!("/api/users/{my_id}/external-signals"))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(
        body["data"]["declared"].as_array().unwrap().len(),
        1,
        "unverified signals stay visible — hiding them would erase the distinction \
         exactly where a recruiter needs it"
    );

    sqlx::query("UPDATE users SET profile_hidden = TRUE WHERE id = $1")
        .bind(my_id)
        .execute(&app.db)
        .await
        .unwrap();

    app.register_user("exviewer").await;
    app.login("exviewer").await;
    let resp = app
        .get(&format!("/api/users/{my_id}/external-signals"))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn invalid_urls_are_refused_at_the_endpoint() {
    let app = TestApp::spawn().await;
    app.register_user("exurl").await;
    app.login("exurl").await;

    let cases = [
        // Provider / host mismatch.
        json!({ "provider": "github", "url": "https://evil.test/alice", "title": "Fake" }),
        // Allowlist bypass via userinfo.
        json!({ "provider": "github", "url": "https://github.com@evil.test/a", "title": "Fake" }),
        // Internal network shapes.
        json!({ "provider": "conf_ref", "url": "https://127.0.0.1/talk", "title": "Local" }),
        json!({ "provider": "conf_ref", "url": "https://localhost/talk", "title": "Local" }),
        // Plaintext.
        json!({ "provider": "dev_to", "url": "http://dev.to/a/b", "title": "Insecure" }),
        // Unknown provider.
        json!({ "provider": "linkedin", "url": "https://linkedin.test/in/a", "title": "X" }),
        // Title bounds.
        json!({ "provider": "dev_to", "url": "https://dev.to/a/b", "title": "ab" }),
    ];

    for case in cases {
        let resp = app.post("/api/users/me/external-signals", &case).await;
        assert_eq!(
            resp.status(),
            StatusCode::BAD_REQUEST,
            "payload {case} should be rejected"
        );
    }
}
