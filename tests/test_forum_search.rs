//! `GET /api/forum/search` and the characters a search box actually receives.
//!
//! Regression coverage for a 500 the contract fuzzer found and no test had:
//! the query was joined with ` & ` and handed to `to_tsquery`, which parses
//! its argument as a query expression. Every tsquery operator — `&`, `|`,
//! `!`, `(`, `)`, `:` — therefore reached the parser as syntax, and anything
//! that did not happen to form a valid expression raised
//! `syntax error in tsquery` and became a database error at the client.
//!
//! Which means searching the forum for `C++`, `R&D` or `(brouillon)` was a
//! server error. The fuzzer found it with random bytes; a user would have
//! found it with a plus sign.

mod common;
use common::TestApp;
use serde_json::Value;

/// Percent-encode everything that is not unreserved.
///
/// Written here rather than pulled in as a dependency: the point of the test
/// is the characters, and a crate that encodes them correctly would be one
/// more thing between the test and what it is asserting.
fn q(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

/// Every one of these is a plausible thing to type and a tsquery operator.
///
/// Asserted as a list rather than as one case each, because what is under test
/// is not any particular character: it is that the endpoint does not parse its
/// input as an expression at all.
const WHAT_PEOPLE_TYPE: &[&str] = &[
    "C++",
    "R&D",
    "(brouillon)",
    "!important",
    "a | b",
    "o'brien",
    "design:system",
    "<script>",
    "&&&",
    "\"une phrase\"",
    "-exclu",
    ":",
];

#[tokio::test]
async fn a_search_box_accepts_what_a_search_box_receives() {
    let app = TestApp::spawn().await;

    for term in WHAT_PEOPLE_TYPE {
        let resp = app.get(&format!("/api/forum/search?q={}", q(term))).await;
        let status = resp.status();
        let body: Value = resp.json().await.unwrap_or(Value::Null);
        assert_eq!(
            status, 200,
            "searching for {term:?} was not a search but an error: {body}"
        );
    }
}

/// And it still finds things.
///
/// The fix swaps `to_tsquery` for `websearch_to_tsquery`, which is a different
/// parser and not merely a more forgiving one. Bare words still AND together,
/// so a two-word search still narrows rather than widens — without this, "no
/// 500" could be bought by matching nothing at all.
#[tokio::test]
async fn two_words_still_narrow_the_search() {
    let app = TestApp::spawn().await;
    seed_a_post(
        &app,
        "Guide du logotype monochrome",
        "Comment tenir en une seule couleur.",
    )
    .await;
    seed_a_post(&app, "Guide des couleurs", "La palette et ses contrastes.").await;

    let both = hits(&app, "guide logotype").await;
    assert_eq!(
        both.len(),
        1,
        "two words should narrow, not widen: {both:?}"
    );

    let one = hits(&app, "guide").await;
    assert_eq!(one.len(), 2, "one word should match both: {one:?}");
}

async fn hits(app: &TestApp, term: &str) -> Vec<String> {
    let body: Value = app
        .get(&format!("/api/forum/search?q={}", q(term)))
        .await
        .json()
        .await
        .unwrap();
    body["data"]["hits"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|p| p["title"].as_str().unwrap_or_default().to_string())
        .collect()
}

async fn seed_a_post(app: &TestApp, title: &str, body: &str) {
    let author: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO users (username, email, password_hash, display_name,
                            first_name, last_name)
         VALUES ('fs_' || substr(md5(random()::text), 1, 10),
                 'fs_' || substr(md5(random()::text), 1, 10) || '@test.dev',
                 'x', 'Auteur', 'A', 'B')
         RETURNING id",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    let category: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO forum_categories (slug, name, description)
         VALUES ('fs-' || substr(md5(random()::text), 1, 10), 'Catégorie', 'Test')
         RETURNING id",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO posts (category_id, author_id, kind, title, body)
         VALUES ($1, $2, 'discussion', $3, $4)",
    )
    .bind(category)
    .bind(author)
    .bind(title)
    .bind(body)
    .execute(&app.db)
    .await
    .unwrap();
}
