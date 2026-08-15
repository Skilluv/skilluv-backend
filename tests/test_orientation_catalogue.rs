//! The code catalogue: thirty-three trades, two languages, and a lineage
//! that survives a rename.
//!
//! These assert the things a migration can get wrong silently — a JOIN that
//! drops rows on a mistyped slug, a translation nobody reads, an archived
//! orientation whose people become unreachable.

mod common;
use common::TestApp;

#[tokio::test]
async fn the_code_catalogue_names_thirty_three_trades() {
    let app = TestApp::spawn().await;

    let active: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM orientations
          WHERE primary_domain = 'code' AND NOT is_archived",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    assert_eq!(
        active, 33,
        "'Développeur Backend' is a family of trades, not a trade"
    );
}

#[tokio::test]
async fn no_orientation_ships_without_a_description() {
    let app = TestApp::spawn().await;

    let blank: Vec<String> = sqlx::query_scalar(
        "SELECT slug FROM orientations
          WHERE primary_domain = 'code' AND btrim(description) = ''",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert!(
        blank.is_empty(),
        "a blank description renders as a blank line in the catalogue: {blank:?}"
    );
}

#[tokio::test]
async fn every_active_trade_has_an_english_name() {
    let app = TestApp::spawn().await;

    let missing: Vec<String> = sqlx::query_scalar(
        "SELECT o.slug FROM orientations o
          WHERE o.primary_domain = 'code' AND NOT o.is_archived
            AND NOT EXISTS (
                SELECT 1 FROM orientation_translations t
                 WHERE t.orientation_id = o.id AND t.locale = 'en')",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert!(
        missing.is_empty(),
        "an orientation with no English name is invisible to half the audience: {missing:?}"
    );
}

#[tokio::test]
async fn the_default_locale_is_never_stored_twice() {
    let app = TestApp::spawn().await;

    // The base row carries French. A French row here would be a second copy
    // with nothing saying which one wins.
    let id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM orientations LIMIT 1")
        .fetch_one(&app.db)
        .await
        .unwrap();

    let refused = sqlx::query(
        "INSERT INTO orientation_translations (orientation_id, locale, name)
         VALUES ($1, 'fr', 'doublon')",
    )
    .bind(id)
    .execute(&app.db)
    .await;

    assert!(
        refused.is_err(),
        "the default locale must not be storable here"
    );
}

#[tokio::test]
async fn an_archived_trade_says_what_it_became() {
    let app = TestApp::spawn().await;

    let orphaned: Vec<String> = sqlx::query_scalar(
        "SELECT slug FROM orientations
          WHERE primary_domain = 'code' AND is_archived AND replaced_by IS NULL",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert!(
        orphaned.is_empty(),
        "a recruiter filtering on the new slug misses these profiles: {orphaned:?}"
    );

    // And the lineage resolves rather than pointing into nothing.
    let dangling: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM orientations o
          WHERE o.replaced_by IS NOT NULL
            AND NOT EXISTS (SELECT 1 FROM orientations n WHERE n.id = o.replaced_by)",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(dangling, 0);
}

#[tokio::test]
async fn a_live_trade_cannot_claim_a_successor() {
    let app = TestApp::spawn().await;

    // Only something archived has been replaced. Otherwise the catalogue
    // would offer an orientation while telling search to look elsewhere.
    let refused = sqlx::query(
        "UPDATE orientations SET replaced_by = (
             SELECT id FROM orientations WHERE slug = 'web-backend-developer')
          WHERE slug = 'web-frontend-developer'",
    )
    .execute(&app.db)
    .await;

    assert!(refused.is_err());
}

#[tokio::test]
async fn every_trade_knows_what_it_is_made_of() {
    let app = TestApp::spawn().await;

    let empty: Vec<String> = sqlx::query_scalar(
        "SELECT o.slug FROM orientations o
          WHERE o.primary_domain = 'code' AND NOT o.is_archived
            AND NOT EXISTS (
                SELECT 1 FROM orientation_skill_map m WHERE m.orientation_id = o.id)",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert!(
        empty.is_empty(),
        "an orientation with no skills looks supported and recommends nothing: {empty:?}"
    );

    let no_core: Vec<String> = sqlx::query_scalar(
        "SELECT o.slug FROM orientations o
          WHERE o.primary_domain = 'code' AND NOT o.is_archived
            AND NOT EXISTS (
                SELECT 1 FROM orientation_skill_map m
                 WHERE m.orientation_id = o.id AND m.is_core)",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert!(
        no_core.is_empty(),
        "without a core skill nothing says what to learn first: {no_core:?}"
    );
}

#[tokio::test]
async fn the_catalogue_answers_in_the_language_it_was_asked_in() {
    let app = TestApp::spawn().await;

    let english = app
        .client
        .get(format!(
            "{}/api/orientations?domain=code&limit=200",
            app.addr
        ))
        .header("Accept-Language", "en-GB,en;q=0.9")
        .send()
        .await
        .expect("GET catalogue");
    assert_eq!(english.status().as_u16(), 200);
    let english: serde_json::Value = english.json().await.unwrap();

    let french = app
        .client
        .get(format!(
            "{}/api/orientations?domain=code&limit=200",
            app.addr
        ))
        .header("Accept-Language", "fr-FR,fr;q=0.9")
        .send()
        .await
        .expect("GET catalogue");
    let french: serde_json::Value = french.json().await.unwrap();

    let name_of = |body: &serde_json::Value, slug: &str| -> String {
        body["data"]["orientations"]
            .as_array()
            .expect("orientations array")
            .iter()
            .find(|o| o["slug"] == slug)
            .unwrap_or_else(|| panic!("{slug} missing from the catalogue"))["name"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    };

    assert_eq!(
        name_of(&english, "kernel-driver-developer"),
        "Kernel and Driver Developer"
    );
    assert_eq!(
        name_of(&french, "kernel-driver-developer"),
        "Développeur Noyau et Pilotes"
    );
}

#[tokio::test]
async fn asking_for_a_language_we_do_not_have_still_answers() {
    let app = TestApp::spawn().await;

    // Japanese has no translations. The reader gets a catalogue rather than
    // an error or a page of blanks.
    let resp = app
        .client
        .get(format!("{}/api/orientations?domain=code&limit=5", app.addr))
        .header("Accept-Language", "ja-JP,ja;q=0.9")
        .send()
        .await
        .expect("GET catalogue");
    assert_eq!(resp.status().as_u16(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    let names: Vec<&str> = body["data"]["orientations"]
        .as_array()
        .expect("orientations array")
        .iter()
        .map(|o| o["name"].as_str().unwrap_or_default())
        .collect();

    assert!(!names.is_empty());
    assert!(
        names.iter().all(|n| !n.trim().is_empty()),
        "a missing translation must fall back, not blank out: {names:?}"
    );
}
