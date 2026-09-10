//! The design rite asks one thing of 26 trades.
//!
//! What it asked before only fitted an interface designer, against a brief
//! that did not exist, and told the person their screen would be "lu par
//! trois relecteurs" when it is read by one. These tests pin the replacement
//! and the three claims that had to go.

use crate::common::TestApp;

/// Fetches the design rite the way a newcomer does.
///
/// Registered and logged in, because `GET /api/challenges/onboarding` is
/// behind `AuthUser` and refuses anybody whose `profile_active` is already
/// true: it is the brief you are shown before you have a profile, not a
/// public catalogue page.
async fn design_rite(app: &TestApp, who: &str, locale: &str) -> serde_json::Value {
    app.register_user(who).await;
    app.login(who).await;

    let resp = app
        .get_with_header(
            "/api/challenges/onboarding?domain=design",
            "Accept-Language",
            locale,
        )
        .await;
    let status = resp.status();
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(
        status.is_success(),
        "the design rite must be served, it is the first screen a designer sees: {status} {body}"
    );
    body
}

/// The gesture is the same one the code rite asks, in another medium.
#[tokio::test]
async fn the_rite_asks_for_a_hello() {
    let app = TestApp::spawn().await;

    let fr = design_rite(&app, "hellofr", "fr").await;
    let challenge = &fr["data"]["challenge"];

    assert_eq!(challenge["title"], "Ton HELLO");

    let instructions = challenge["instructions"].as_str().expect("instructions");
    for expected in ["ton nom", "ton métier", "une chose que tu sais faire"] {
        assert!(
            instructions.contains(expected),
            "the three things have to be named: {expected} missing from {instructions}"
        );
    }
    assert!(
        instructions.contains("Un seul artefact"),
        "one artefact, not a series, is what makes it finishable"
    );
}

/// It is served in English too, with the same shape.
#[tokio::test]
async fn the_rite_is_bilingual() {
    let app = TestApp::spawn().await;

    let en = design_rite(&app, "helloen", "en").await;
    let challenge = &en["data"]["challenge"];

    assert_eq!(challenge["title"], "Your HELLO");
    let instructions = challenge["instructions"].as_str().expect("instructions");
    for expected in ["your name", "your trade", "one thing you can do"] {
        assert!(
            instructions.contains(expected),
            "{expected} missing from the English instructions"
        );
    }
}

/// The three trades the old copy left out are named in the new one.
///
/// Not decoration: "design one screen" meant nothing to an illustrator, a
/// motion designer or a UX writer, and those are three of the 26 orientations
/// the design domain actually carries. Naming them is how somebody knows the
/// rite is addressed to them.
#[tokio::test]
async fn the_instructions_reach_past_the_interface_designer() {
    let app = TestApp::spawn().await;
    let fr = design_rite(&app, "hellotrades", "fr").await;
    let instructions = fr["data"]["challenge"]["instructions"]
        .as_str()
        .unwrap()
        .to_lowercase();

    for trade in ["illustrateur", "motion designer", "ux writer"] {
        assert!(
            instructions.contains(trade),
            "the rite has to speak to {trade}: {instructions}"
        );
    }
}

/// The promise of three reviewers is gone, in both languages.
///
/// All twelve rites are read once, in the generic queue. `continues_in` on the
/// rite catalogue is documentation and not routing, and the copy shown to the
/// person used to say otherwise. Somebody expecting three verdicts and
/// receiving one has been told something untrue by the product.
#[tokio::test]
async fn nothing_promises_three_reviewers() {
    let app = TestApp::spawn().await;

    let stale: Vec<String> = sqlx::query_scalar(
        "SELECT s.text
           FROM challenge_templates ct,
                LATERAL (VALUES
                    (ct.title_i18n ->> 'fr'), (ct.title_i18n ->> 'en'),
                    (ct.description_i18n ->> 'fr'), (ct.description_i18n ->> 'en'),
                    (ct.instructions_i18n ->> 'fr'), (ct.instructions_i18n ->> 'en'),
                    (ct.title), (ct.description), (ct.instructions)
                ) AS s(text)
          WHERE ct.is_domain_rite = TRUE
            AND ct.skill_domain = 'design'
            AND ct.status = 'published'
            AND (s.text LIKE '%trois relecteurs%' OR s.text LIKE '%three reviewers%')",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert!(
        stale.is_empty(),
        "the design rite still promises three reviewers: {stale:?}"
    );
}

/// The rite catalogue and the brief say the same thing.
///
/// `GET /api/onboarding/rites` serves `gesture` from the Rust table in
/// `services::onboarding_rite`, while the challenge screen serves the copy
/// from `challenge_templates`. Two sources, one screen apart. The catalogue
/// still described "one screen against the entry brief of your trade" after
/// the brief had been removed from the template for not existing, which is
/// how a person reads one promise on the rite list and another on the page
/// the rite list links to.
#[tokio::test]
async fn the_catalogue_promises_what_the_brief_asks() {
    let app = TestApp::spawn().await;

    let body: serde_json::Value = app.get("/api/onboarding/rites").await.json().await.unwrap();

    let design = body["data"]["rites"]
        .as_array()
        .expect("rites")
        .iter()
        .find(|r| r["domain"] == "design")
        .expect("the design rite is one of the twelve");

    let gesture = design["gesture"].as_str().expect("gesture");
    assert!(
        !gesture.contains("brief"),
        "the catalogue still sends somebody to a brief that was never written: {gesture}"
    );
    assert_eq!(
        design["challenge_title"], "Your HELLO",
        "the catalogue must point at the rewritten rite"
    );
}

/// The brief says where the artefact will end up, before it is uploaded.
///
/// A `design_upload_sessions` row is private: its owner reads it, and a
/// reviewer only while the review task is open. The wall of HELLOs breaks
/// that for one kind of artefact, which is defensible because a self
/// introduction is made to be seen, and indefensible in silence. Somebody who
/// uploads a file to a platform that told them uploads are private has not
/// agreed to publish it.
#[tokio::test]
async fn the_rite_says_the_artefact_will_be_shown_publicly() {
    let app = TestApp::spawn().await;

    for (locale, expected) in [
        (
            "fr",
            vec!["mur des HELLO", "sous ton nom", "appartienne à un client"],
        ),
        (
            "en",
            vec!["wall of HELLOs", "under your name", "belongs to a client"],
        ),
    ] {
        let rite = design_rite(&app, &format!("warned{locale}"), locale).await;
        let instructions = rite["data"]["challenge"]["instructions"]
            .as_str()
            .expect("instructions");
        for phrase in expected {
            assert!(
                instructions.contains(phrase),
                "the {locale} brief does not warn: {phrase} missing from {instructions}"
            );
        }
    }
}
