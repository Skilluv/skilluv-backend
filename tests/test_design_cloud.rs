//! Design tools Skilluv does not own.
//!
//! Two halves. Reading a pasted link works today and needs no credential —
//! and it is the half that pays off, because knowing a Figma link is private
//! *before* a deliverable is submitted is the difference between a review
//! queue that moves and one that does not.
//!
//! Connecting an account is complete up to the wall: Skilluv has no developer
//! account on Figma, Miro or Webflow, so the call that needs a client secret
//! says which variable is missing.

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

async fn inspect(app: &TestApp, url: &str) -> Value {
    let encoded: String = url
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect();
    let resp = app
        .get(&format!("/api/design/cloud/inspect?url={encoded}"))
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    resp.json().await.unwrap()
}

#[tokio::test]
async fn a_figma_link_is_read_without_anybody_logging_in() {
    let app = TestApp::spawn().await;
    // Public and unauthenticated on purpose: it parses a string and touches
    // nothing, and the front needs it on a form nobody has submitted yet.
    let body = inspect(
        &app,
        "https://www.figma.com/file/ABC123/identite?node-id=1%3A2",
    )
    .await;
    let source = &body["data"]["source"];

    assert_eq!(source["provider"], "figma");
    assert_eq!(source["key"], "ABC123");
    assert_eq!(source["node_id"], "1:2");
}

#[tokio::test]
async fn a_private_link_is_warned_about_before_it_is_submitted() {
    let app = TestApp::spawn().await;
    let body = inspect(&app, "https://www.figma.com/design/XYZ/identite").await;

    // The person pasting is the only one who can fix the sharing, and the
    // moment they paste is the only moment they will.
    assert_eq!(body["data"]["source"]["opens_without_account"], false);
    // A code the client renders in the reader's language, not a French
    // sentence, and the provider so it can name it (SKI-311).
    assert_eq!(
        body["data"]["warning_code"], "needs_public_sharing",
        "{body}"
    );
    assert_eq!(body["data"]["warning_provider"], "figma", "{body}");
}

#[tokio::test]
async fn a_published_site_carries_no_warning() {
    let app = TestApp::spawn().await;
    let body = inspect(&app, "https://exemple.webflow.io/accueil").await;

    // A published Webflow site is a website. That is the whole point of it.
    assert_eq!(body["data"]["source"]["opens_without_account"], true);
    assert!(body["data"]["warning_code"].is_null(), "{body}");
}

#[tokio::test]
async fn a_link_that_is_not_a_design_tool_says_so() {
    let app = TestApp::spawn().await;
    let body = inspect(&app, "https://github.com/org/repo").await;

    // A GitHub URL in a design deliverable is a mistake worth surfacing.
    assert!(body["data"]["source"].is_null(), "{body}");
    assert_eq!(body["data"]["warning_code"], "unrecognised_link", "{body}");
}

#[tokio::test]
async fn connecting_a_tool_with_no_oauth_is_refused_with_the_alternative() {
    let app = TestApp::spawn().await;
    app.register_user("dc_framer").await;
    app.login("dc_framer").await;

    // Framer has no public OAuth. A "connection" to it would be a row that
    // means nothing, and the message says what to do instead.
    let resp = app.get("/api/design/cloud/framer/start").await;
    assert_eq!(resp.status(), 400);
    let body: Value = resp.json().await.unwrap();
    assert!(
        body["error"]["message"].as_str().unwrap().contains("URL"),
        "{body}"
    );
}

#[tokio::test]
async fn an_unconfigured_provider_names_the_variable_it_is_missing() {
    let app = TestApp::spawn().await;
    app.register_user("dc_unconfigured").await;
    app.login("dc_unconfigured").await;

    // Skilluv has no developer account on any of the three, so this is the
    // state of every deployment today. An operator reading it needs the
    // variable name, not a stack trace — and a button that silently does
    // nothing is worse than one that says why.
    let resp = app.get("/api/design/cloud/figma/start").await;
    assert_eq!(resp.status(), 503);
    let body: Value = resp.json().await.unwrap();
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("FIGMA_CLIENT_ID"),
        "{body}"
    );
}

#[tokio::test]
async fn connections_are_listed_without_their_tokens() {
    let app = TestApp::spawn().await;
    app.register_user("dc_listed").await;
    let user = user_id(&app, "dc_listed").await;

    sqlx::query(
        "INSERT INTO design_cloud_connections
             (user_id, provider, access_token_ciphertext, access_token_nonce,
              scopes, remote_handle)
         VALUES ($1, 'figma', '\\x01'::BYTEA, '\\x02'::BYTEA,
                 ARRAY['file_read'], 'une-designer')",
    )
    .bind(user)
    .execute(&app.db)
    .await
    .unwrap();

    app.login("dc_listed").await;
    let body: Value = app
        .get("/api/design/cloud/connections")
        .await
        .json()
        .await
        .unwrap();

    let row = &body["data"][0];
    assert_eq!(row["provider"], "figma");
    assert_eq!(row["remote_handle"], "une-designer");
    // A field that is never returned cannot be returned by accident.
    assert!(row.get("access_token_ciphertext").is_none(), "{body}");
    assert!(row.get("access_token_nonce").is_none(), "{body}");
}

#[tokio::test]
async fn disconnecting_wipes_the_tokens_and_keeps_the_row() {
    let app = TestApp::spawn().await;
    app.register_user("dc_revoked").await;
    let user = user_id(&app, "dc_revoked").await;

    sqlx::query(
        "INSERT INTO design_cloud_connections
             (user_id, provider, access_token_ciphertext, access_token_nonce, scopes)
         VALUES ($1, 'miro', '\\xdeadbeef'::BYTEA, '\\x02'::BYTEA, ARRAY['boards:read'])",
    )
    .bind(user)
    .execute(&app.db)
    .await
    .unwrap();

    app.login("dc_revoked").await;
    assert_eq!(
        app.post("/api/design/cloud/miro/disconnect", &json!({}))
            .await
            .status(),
        204
    );

    let (revoked, cipher): (Option<chrono::DateTime<chrono::Utc>>, Vec<u8>) = sqlx::query_as(
        "SELECT revoked_at, access_token_ciphertext FROM design_cloud_connections
          WHERE user_id = $1 AND provider = 'miro'",
    )
    .bind(user)
    .fetch_one(&app.db)
    .await
    .unwrap();

    // The row survives: a later question about what was fetched and when
    // needs an answer. What must not survive is the ability to fetch more.
    assert!(revoked.is_some());
    assert!(cipher.is_empty(), "the token is still there");

    // And it drops out of the listing.
    let body: Value = app
        .get("/api/design/cloud/connections")
        .await
        .json()
        .await
        .unwrap();
    assert!(body["data"].as_array().unwrap().is_empty(), "{body}");
}

#[tokio::test]
async fn disconnecting_something_that_was_not_connected_is_not_an_error() {
    let app = TestApp::spawn().await;
    app.register_user("dc_absent").await;
    app.login("dc_absent").await;

    // Answering 404 would tell a caller whether an account was connected.
    assert_eq!(
        app.post("/api/design/cloud/webflow/disconnect", &json!({}))
            .await
            .status(),
        204
    );
}

#[tokio::test]
async fn one_live_connection_per_provider() {
    let app = TestApp::spawn().await;
    app.register_user("dc_double").await;
    let user = user_id(&app, "dc_double").await;

    let insert = |n: u8| {
        sqlx::query(
            "INSERT INTO design_cloud_connections
                 (user_id, provider, access_token_ciphertext, access_token_nonce, scopes)
             VALUES ($1, 'figma', $2, '\\x02'::BYTEA, ARRAY['file_read'])",
        )
        .bind(user)
        .bind(vec![n])
        .execute(&app.db)
    };

    insert(1).await.unwrap();
    // Two live tokens for one provider would leave no rule saying which a
    // fetch should use.
    assert!(insert(2).await.is_err());
}
