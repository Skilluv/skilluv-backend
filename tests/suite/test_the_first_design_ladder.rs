//! A designer who finishes the rite is handed something to do.
//!
//! Migration 0615 wrote six code exercises because the catalogue is drafts and
//! `GET /api/challenges` answered a new account with an empty list. Design had
//! that hole untouched: its 130 seeded briefs are all `draft`, written as
//! portfolio pieces for somebody with three years of practice, in French only.
//!
//! These tests pin the six that replace the empty list, and pin the properties
//! that make them a ladder rather than six more drafts.

use crate::common::TestApp;

const LADDER: [&str; 6] = [
    "One icon for the Skilluv set",
    "The screen when there is nothing yet",
    "Three error messages somebody can act on",
    "Ten seconds of motion, inside a budget",
    "A poster for one Skilluv event",
    "One screen redrawn, and why",
];

/// The list a designer sees is not empty.
///
/// Through the API a client actually calls, and not against the table: the
/// hole was never that the rows did not exist, it was that `status = 'draft'`
/// meant nothing reached anybody.
#[tokio::test]
async fn the_catalogue_answers_a_designer_with_something_to_do() {
    let app = TestApp::spawn().await;
    app.register_user("laddernewcomer").await;
    app.login("laddernewcomer").await;

    // `per_page`, not `limit`: `ListQuery` names the page size that way and
    // `test_unknown_query_params` makes an unknown parameter a 400 rather
    // than something silently ignored.
    let body: serde_json::Value = app
        .get("/api/challenges?domain=design&per_page=50")
        .await
        .json()
        .await
        .unwrap();

    // `data` is a list of `{ "challenge": ... }` wrappers, which is what the
    // handler builds and what a client destructures.
    let titles: Vec<String> = body["data"]
        .as_array()
        .unwrap_or_else(|| panic!("the challenge list is an array: {body}"))
        .iter()
        .filter_map(|c| c["challenge"]["title"].as_str().map(str::to_string))
        .collect();

    assert!(
        !titles.is_empty(),
        "a designer who finished the rite must not be handed an empty list"
    );
    for expected in LADDER {
        assert!(
            titles.iter().any(|t| t == expected),
            "{expected} is not in what a designer is offered: {titles:?}"
        );
    }
}

/// Each one says what it is not.
///
/// The commonest way a beginner loses a week is doing more than was asked, so
/// an explicit out of scope is what separates these from the seeded drafts.
/// Checked in both languages because a French reader gets the French field.
#[tokio::test]
async fn every_exercise_says_what_is_out_of_scope() {
    let app = TestApp::spawn().await;

    for title in LADDER {
        let (en, fr): (Option<String>, Option<String>) = sqlx::query_as(
            "SELECT instructions_i18n ->> 'en', instructions_i18n ->> 'fr'
               FROM challenge_templates WHERE title = $1",
        )
        .bind(title)
        .fetch_one(&app.db)
        .await
        .unwrap_or_else(|e| panic!("{title} is not in the catalogue: {e}"));

        let en = en.unwrap_or_default();
        let fr = fr.unwrap_or_default();
        assert!(
            en.contains("Out of scope:"),
            "{title} does not say what it is not, in English"
        );
        assert!(
            fr.contains("Hors périmètre :"),
            "{title} does not say what it is not, in French"
        );
        assert!(
            !fr.is_empty() && fr != en,
            "{title} is not really written in French"
        );
    }
}

/// Nobody reaches the end of the ladder and finds nothing after it.
///
/// `challenge_prerequisites` was empty across the platform before 0615, so
/// "what do I do after this" had no answer that was not a search box. Every
/// exercise but the first is somebody's next one.
#[tokio::test]
async fn the_ladder_is_a_chain_and_not_six_loose_briefs() {
    let app = TestApp::spawn().await;

    let without_a_parent: Vec<String> = sqlx::query_scalar(
        "SELECT c.title
           FROM challenge_templates c
          WHERE c.skill_domain = 'design' AND c.status = 'published'
            AND c.is_training AND NOT c.is_domain_rite
            AND NOT EXISTS (
                SELECT 1 FROM challenge_prerequisites p
                 WHERE p.challenge_id = c.id)",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert_eq!(
        without_a_parent,
        vec!["One icon for the Skilluv set".to_string()],
        "exactly one exercise starts the chain, and the rest follow something"
    );
}

/// The chain advises, it does not gate.
///
/// A UX writer should not have to draw an icon before being allowed to write,
/// and `check_eligibility` only blocks on required edges. Recommended is the
/// whole point of a ladder that people arrive at from different trades.
#[tokio::test]
async fn no_edge_of_the_ladder_locks_anybody_out() {
    let app = TestApp::spawn().await;

    let enforced: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM challenge_prerequisites p
           JOIN challenge_templates c ON c.id = p.challenge_id
          WHERE c.skill_domain = 'design' AND p.required",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    assert_eq!(
        enforced, 0,
        "the design ladder is advice; a required edge turns it into a gate"
    );
}

/// No exercise points at a link nobody minted.
///
/// Migration 0616 deleted an invented `discord.gg/skilluv` from this table and
/// wrote the reason: a dead link in the first list a beginner is handed is
/// worse than an empty list, because it teaches them the guidance is
/// decorative. Four of the six carry only the repository, and that is the
/// honest state rather than a gap.
#[tokio::test]
async fn the_reading_list_invents_nothing() {
    let app = TestApp::spawn().await;

    let invented: Vec<String> = sqlx::query_scalar(
        "SELECT r.url FROM challenge_resources r
           JOIN challenge_templates c ON c.id = r.challenge_id
          WHERE c.skill_domain = 'design'
            AND (r.url LIKE '%discord.gg%' OR r.url LIKE '%skill-uv.com%')",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert!(
        invented.is_empty(),
        "these point at addresses nobody has minted: {invented:?}"
    );

    // And every one of them is reachable through the guidance a client reads,
    // rather than sitting in a table nothing queries.
    let with_a_repository: i64 = sqlx::query_scalar(
        "SELECT count(DISTINCT c.id) FROM challenge_templates c
           JOIN challenge_resources r ON r.challenge_id = c.id
          WHERE c.skill_domain = 'design' AND c.status = 'published'
            AND c.is_training AND NOT c.is_domain_rite
            AND r.kind = 'repository'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(
        with_a_repository, 6,
        "every exercise names where the thing it works on actually lives"
    );
}

/// Each one names a trade, so the recommendation engine can offer it.
///
/// A published brief with a null `orientation_id` is the silent half of the
/// empty list: it exists, and nobody is ever pointed at it.
#[tokio::test]
async fn every_exercise_belongs_to_a_trade_that_exists() {
    let app = TestApp::spawn().await;

    let orphans: Vec<String> = sqlx::query_scalar(
        "SELECT c.title FROM challenge_templates c
          WHERE c.skill_domain = 'design' AND c.status = 'published'
            AND c.is_training AND NOT c.is_domain_rite
            AND c.orientation_id IS NULL",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert!(
        orphans.is_empty(),
        "these are offered to nobody: {orphans:?}"
    );
}
