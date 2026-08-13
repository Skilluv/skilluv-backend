//! SKI-293 (duplicate routes, untyped decide, unsubscribe) and SKI-294
//! (boot without GeoNames).
//!
//! The duplicates mattered because the documented route was not the one
//! anyone used: a reader of the OpenAPI document would have coded against
//! `/api/social/mentions/me` or `/api/auth/me/email-preferences` and got a
//! different shape from the one the front consumes.

mod common;
use common::TestApp;
use serde_json::json;

// ─── SKI-293.1/2 — the duplicates are gone ────────────────────────

#[tokio::test]
async fn the_legacy_mentions_alias_is_gone() {
    let app = TestApp::spawn().await;
    app.register_user("mention_reader").await;
    app.login("mention_reader").await;

    let resp = app.get("/api/social/mentions/me").await;
    assert_eq!(
        resp.status().as_u16(),
        404,
        "the alias was removed in favour of /api/users/me/mentions"
    );
}

#[tokio::test]
async fn the_surviving_mentions_route_still_answers() {
    let app = TestApp::spawn().await;
    app.register_user("mention_reader2").await;
    app.login("mention_reader2").await;

    let resp = app.get("/api/users/me/mentions").await;
    assert_eq!(resp.status().as_u16(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["data"].is_array(), "data is the mention array");
    assert!(body["pagination"].is_object());
}

#[tokio::test]
async fn the_legacy_email_preferences_route_is_gone() {
    let app = TestApp::spawn().await;
    app.register_user("prefs_reader").await;
    app.login("prefs_reader").await;

    let resp = app.get("/api/auth/me/email-preferences").await;
    assert_eq!(
        resp.status().as_u16(),
        404,
        "two routes answering the same question with different shapes is the \
         bug being fixed"
    );
}

#[tokio::test]
async fn the_surviving_email_preferences_route_keeps_its_flat_shape() {
    let app = TestApp::spawn().await;
    app.register_user("prefs_reader2").await;
    app.login("prefs_reader2").await;

    let resp = app.get("/api/users/me/email-preferences").await;
    assert_eq!(resp.status().as_u16(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(
        body["data"]["digest_weekly"].is_boolean(),
        "flat data, not data.preferences: {body}"
    );
    assert!(
        body["data"]["preferences"].is_null(),
        "the nested legacy shape must not come back"
    );
}

// ─── SKI-293.3 — decide is typed ──────────────────────────────────

#[tokio::test]
async fn decide_rejects_a_payload_that_is_not_the_contract() {
    let app = TestApp::spawn().await;
    app.register_user("decider").await;
    app.login("decider").await;

    // `{"decision": "accept"}` is the shape someone would guess from an
    // untyped requestBody. It must be refused loudly, not silently ignored.
    let resp = app
        .post(
            &format!("/api/guild-applications/{}/decide", uuid::Uuid::new_v4()),
            &json!({ "decision": "accept" }),
        )
        .await;
    assert!(
        resp.status().is_client_error(),
        "expected a 4xx for a wrong body, got {}",
        resp.status()
    );
}

// ─── SKI-293.5 — one-click unsubscribe is reachable ───────────────

#[tokio::test]
async fn one_click_unsubscribe_is_served() {
    let app = TestApp::spawn().await;

    // A bogus token must not 404 the route itself: RFC 8058 requires the
    // endpoint to exist and answer. Signature rejection is a different
    // status from "no such route".
    let resp = app.get("/api/email/unsubscribe/not-a-real-token").await;
    assert_ne!(
        resp.status().as_u16(),
        404,
        "the route must exist — the token being invalid is a separate matter"
    );
}

// ─── SKI-294 — booting without GeoNames ───────────────────────────

#[tokio::test]
async fn geo_service_falls_back_to_empty_instead_of_panicking() {
    // The published image has no `data/` directory. Loading must degrade,
    // not abort the process: countries feed profile autocompletion, nothing
    // that justifies refusing every request.
    let missing = std::path::Path::new("definitely/not/a/real/geonames/dir");
    let geo = skilluv_backend::services::GeoService::load_or_empty(missing);
    assert_eq!(geo.countries().len(), 0);
    assert_eq!(geo.total_cities(), 0);
}

#[tokio::test]
async fn geonames_dir_is_configurable() {
    // SAFETY: set and read within this test before any concurrent reader.
    unsafe { std::env::set_var("GEONAMES_DIR", "/somewhere/else") };
    let dir = skilluv_backend::services::GeoService::data_dir_from_env();
    unsafe { std::env::remove_var("GEONAMES_DIR") };
    assert_eq!(dir, std::path::PathBuf::from("/somewhere/else"));

    let default = skilluv_backend::services::GeoService::data_dir_from_env();
    assert_eq!(
        default,
        std::path::PathBuf::from("data"),
        "the default must stay `data` so a checkout keeps working unchanged"
    );
}

#[tokio::test]
async fn the_country_endpoint_answers_even_with_no_data() {
    let app = TestApp::spawn().await;
    let resp = app.get("/api/geo/countries").await;
    assert!(
        resp.status().is_success(),
        "an empty catalogue is a valid answer; a 500 is not"
    );
}
