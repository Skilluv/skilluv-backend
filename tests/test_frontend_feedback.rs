//! Front-end feedback batch — the backend halves of SKI-309…330.
//!
//! These hold the public contracts the front raised: a warning that comes back
//! as a code rather than a French sentence, portfolio labels that carry a
//! language-neutral key, award categories that expose their family, and the
//! challenge list accepting a `security_kind` filter. All four are public GETs,
//! so no fixture beyond a running app is needed.

mod common;
use common::TestApp;
use serde_json::Value;

// SKI-311 — no natural-language string leaves /design/cloud/inspect without a
// code the client can translate.
#[tokio::test]
async fn inspect_warns_with_a_code_not_a_french_sentence() {
    let app = TestApp::spawn().await;

    // An unrecognised link.
    let resp = app
        .get("/api/design/cloud/inspect?url=https://example.com/whatever")
        .await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(
        body["data"]["warning_code"], "unrecognised_link",
        "an unknown link warns with a code"
    );
    // The old French free-text field is gone.
    assert!(
        body["data"].get("warning").is_none(),
        "no natural-language warning field"
    );

    // A private Figma link — needs public sharing, and names the provider.
    let resp = app
        .get("/api/design/cloud/inspect?url=https://www.figma.com/file/abc/Design")
        .await;
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["warning_code"], "needs_public_sharing");
    assert_eq!(body["data"]["warning_provider"], "figma");
}

// SKI-311 — portfolio labels carry a language-neutral key, and it is ascii.
#[tokio::test]
async fn portfolio_platforms_expose_ascii_label_keys() {
    let app = TestApp::spawn().await;
    let resp = app.get("/api/portfolio-platforms").await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let platforms = body["data"].as_array().expect("an array of platforms");
    assert!(!platforms.is_empty(), "some platforms are seeded");

    let mut saw_a_key = false;
    for p in platforms {
        if let Some(key) = p["items_label_key"].as_str() {
            saw_a_key = true;
            assert!(
                key.is_ascii() && !key.is_empty(),
                "an items label key is a plain code, got {key:?}"
            );
        }
        if let Some(key) = p["reach_label_key"].as_str() {
            assert!(
                key.is_ascii(),
                "a reach label key is a plain code, got {key:?}"
            );
        }
    }
    assert!(
        saw_a_key,
        "at least one platform carries an items_label_key"
    );
}

// SKI-314 — award categories expose skill_domain, and the domain filter is
// accepted.
#[tokio::test]
async fn award_categories_carry_skill_domain() {
    let app = TestApp::spawn().await;

    let resp = app.get("/api/awards/categories").await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let categories = body["data"]["categories"]
        .as_array()
        .expect("categories array");
    for c in categories {
        assert!(
            c.as_object().unwrap().contains_key("skill_domain"),
            "every category exposes skill_domain (null when cross-cutting)"
        );
    }

    // The domain filter is wired and validated (a real domain is accepted).
    let resp = app.get("/api/awards/categories?domain=design").await;
    assert_eq!(resp.status(), 200, "a real domain filters rather than 400s");
    // An unknown domain is refused, not silently ignored.
    let resp = app.get("/api/awards/categories?domain=notadomain").await;
    assert_eq!(resp.status(), 400, "an unknown domain is refused");
}

// SKI-320 — the challenge list accepts a security_kind filter.
#[tokio::test]
async fn challenges_accept_a_security_kind_filter() {
    let app = TestApp::spawn().await;

    let resp = app.get("/api/challenges?security_kind=ctf_flag").await;
    assert_eq!(
        resp.status(),
        200,
        "a valid security_kind filters rather than errors"
    );
    let body: Value = resp.json().await.unwrap();
    assert!(body["data"].is_array(), "the list still comes back");

    // A value outside the six kinds is refused by the pattern.
    let resp = app.get("/api/challenges?security_kind=not_a_kind").await;
    assert_eq!(resp.status(), 400, "an unknown security_kind is refused");
}
