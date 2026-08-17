//! Ten AI trades, and the three things that make one usable: a name in both
//! languages, skills it is made of, and somebody allowed to review it.

mod common;
use common::TestApp;

const AI_TRADES: [&str; 10] = [
    "data-engineer",
    "data-analyst",
    "ml-engineer",
    "prompt-engineer",
    "llm-engineer",
    "mlops-engineer",
    "computer-vision-engineer",
    "nlp-engineer",
    "ai-safety-researcher",
    "generative-ai-artist",
];

#[tokio::test]
async fn the_catalogue_holds_ten_live_ai_trades() {
    let app = TestApp::spawn().await;

    let slugs: Vec<String> = sqlx::query_scalar(
        "SELECT slug FROM orientations
          WHERE primary_domain = 'ai' AND NOT is_archived
          ORDER BY slug",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    let mut expected: Vec<String> = AI_TRADES.iter().map(|s| s.to_string()).collect();
    expected.sort();
    assert_eq!(slugs, expected);
}

#[tokio::test]
async fn every_ai_trade_has_an_english_name() {
    let app = TestApp::spawn().await;

    // The four seeded in 0088 had none for two years, which made half the
    // catalogue invisible to an English reader with nothing to surface it.
    let missing: Vec<String> = sqlx::query_scalar(
        "SELECT o.slug FROM orientations o
          LEFT JOIN orientation_translations t
                 ON t.orientation_id = o.id AND t.locale = 'en'
          WHERE o.primary_domain = 'ai' AND NOT o.is_archived
            AND (t.name IS NULL OR btrim(t.description) = '')",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert!(missing.is_empty(), "no English name or blurb: {missing:?}");
}

#[tokio::test]
async fn the_catalogue_answers_in_the_readers_language() {
    let app = TestApp::spawn().await;

    let resp = app.get("/api/orientations?domain=ai&limit=50").await;
    assert!(resp.status().is_success());
    let body: serde_json::Value = resp.json().await.unwrap();
    let rows = body["data"]["orientations"]
        .as_array()
        .expect("orientations array");

    let llm = rows
        .iter()
        .find(|o| o["slug"] == "llm-engineer")
        .expect("llm-engineer in the catalogue");

    // No Accept-Language header: the API answers English, which is the
    // default `resolve_from_accept_language` picks for machine readers.
    assert_eq!(llm["name"], "LLM Engineer");
}

#[tokio::test]
async fn every_ai_trade_is_made_of_something() {
    let app = TestApp::spawn().await;

    // An orientation with an empty skill map looks supported and is not:
    // nothing can be recommended and nothing verified. Four of these had
    // exactly that state since 0088.
    let unmapped: Vec<String> = sqlx::query_scalar(
        "SELECT o.slug FROM orientations o
          LEFT JOIN orientation_skill_map m ON m.orientation_id = o.id
          WHERE o.primary_domain = 'ai' AND NOT o.is_archived
          GROUP BY o.slug
         HAVING count(m.skill_id) = 0",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert!(unmapped.is_empty(), "no skills attached to: {unmapped:?}");
}

#[tokio::test]
async fn every_ai_trade_says_what_to_learn_first() {
    let app = TestApp::spawn().await;

    // A map where nothing is core says nothing about where to start, which
    // is the only thing it is read for.
    let coreless: Vec<String> = sqlx::query_scalar(
        "SELECT o.slug FROM orientations o
          LEFT JOIN orientation_skill_map m
                 ON m.orientation_id = o.id AND m.is_core
          WHERE o.primary_domain = 'ai' AND NOT o.is_archived
          GROUP BY o.slug
         HAVING count(m.skill_id) < 3",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert!(coreless.is_empty(), "fewer than three core skills: {coreless:?}");
}

#[tokio::test]
async fn the_map_reaches_outside_the_domain_on_purpose() {
    let app = TestApp::spawn().await;

    // `sql`, `python`, `containers` live under code and ops. Duplicating them
    // under `ai` would give two answers to whether somebody knows Python.
    let crossing: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM orientation_skill_map m
           JOIN orientations o ON o.id = m.orientation_id
           JOIN skill_nodes s ON s.id = m.skill_id
          WHERE o.primary_domain = 'ai' AND s.domain <> 'ai'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    assert!(crossing > 0, "the AI map must reuse the code and ops nodes");
}

#[tokio::test]
async fn no_ai_skill_duplicates_one_that_already_existed() {
    let app = TestApp::spawn().await;

    // Two nodes for one skill means two answers to the same question. The
    // SQL nodes are the case this nearly happened to: `sql-window-functions`
    // already lived under `code`, and an `analytical-sql` under `ai` would
    // have shadowed it.
    let dupes: Vec<String> = sqlx::query_scalar(
        "SELECT ai.slug FROM skill_nodes ai
           JOIN skill_nodes other
             ON lower(other.display_name) = lower(ai.display_name)
            AND other.domain <> 'ai'
          WHERE ai.domain = 'ai'",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert!(
        dupes.is_empty(),
        "these AI nodes shadow one that already existed elsewhere: {dupes:?}"
    );
}

#[tokio::test]
async fn ai_has_a_default_review_grid() {
    let app = TestApp::spawn().await;

    // Without it, an AI challenge carrying no rubric reached the verifier
    // with its instructions alone — asking a model whether work is good with
    // no statement of what good means.
    let criteria: Option<serde_json::Value> = sqlx::query_scalar(
        "SELECT criteria FROM review_grids
          WHERE domain = 'ai' AND reviewer_group IS NULL",
    )
    .fetch_optional(&app.db)
    .await
    .unwrap();

    let criteria = criteria.expect("the ai domain needs a fallback grid");
    let list = criteria.as_array().expect("criteria is a list");
    assert!(list.len() >= 5, "a grid of {} criteria is thin", list.len());
}

#[tokio::test]
async fn every_reviewer_family_has_its_own_grid() {
    let app = TestApp::spawn().await;

    let ungridded: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT o.reviewer_group FROM orientations o
          LEFT JOIN review_grids g
                 ON g.domain = 'ai' AND g.reviewer_group = o.reviewer_group
          WHERE o.primary_domain = 'ai'
            AND o.reviewer_group IS NOT NULL
            AND g.id IS NULL",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert!(ungridded.is_empty(), "families with no grid: {ungridded:?}");
}
