//! `GET /api/forum/posts` ordering.
//!
//! Regression coverage for the `sort=hot` 500: the ORDER BY clause used the
//! `upvotes` / `downvotes` output aliases inside an arithmetic expression,
//! which Postgres resolves against the input relations rather than the
//! select list. Every `sort=hot` call failed with
//! `column "upvotes" does not exist`.

mod common;
use common::TestApp;
use uuid::Uuid;

/// Seeds one category and two posts, the second carrying `upvotes` upvotes.
/// Returns `(quiet_post_id, popular_post_id)`.
async fn seed_two_posts(app: &TestApp, upvotes: usize) -> (Uuid, Uuid) {
    let author: Uuid = sqlx::query_scalar(
        "INSERT INTO users (username, email, password_hash, display_name, first_name, last_name)
         VALUES ('forum_author', 'forum_author@test.dev', 'x', 'Forum Author', 'Forum', 'Author')
         RETURNING id",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    let category: Uuid = sqlx::query_scalar(
        "INSERT INTO forum_categories (slug, name) VALUES ('sorting', 'Sorting')
         RETURNING id",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    // The quiet post is created first, so `recent` and `hot` disagree on the
    // ordering and the assertions below are not both satisfied by accident.
    let quiet: Uuid = sqlx::query_scalar(
        "INSERT INTO posts (category_id, author_id, kind, title, body)
         VALUES ($1, $2, 'discussion', 'Quiet post', 'no reactions')
         RETURNING id",
    )
    .bind(category)
    .bind(author)
    .fetch_one(&app.db)
    .await
    .unwrap();

    let popular: Uuid = sqlx::query_scalar(
        "INSERT INTO posts (category_id, author_id, kind, title, body, bounty_fragments)
         VALUES ($1, $2, 'question', 'Popular post', 'many reactions', 50)
         RETURNING id",
    )
    .bind(category)
    .bind(author)
    .fetch_one(&app.db)
    .await
    .unwrap();

    // Reactions are unique per (target, user, kind), so each upvote needs a
    // distinct voter.
    for i in 0..upvotes {
        let voter: Uuid = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "INSERT INTO users (username, email, password_hash, display_name, first_name, last_name)
             VALUES ('forum_voter_{i}', 'forum_voter_{i}@test.dev', 'x', 'Voter {i}', 'Voter', '{i}')
             RETURNING id"
        )))
        .fetch_one(&app.db)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO reactions (target_type, target_id, user_id, kind)
             VALUES ('post', $1, $2, 'upvote')",
        )
        .bind(popular)
        .bind(voter)
        .execute(&app.db)
        .await
        .unwrap();
    }

    (quiet, popular)
}

async fn titles_for(app: &TestApp, sort: &str) -> Vec<String> {
    let resp = app.get(&format!("/api/forum/posts?sort={sort}")).await;
    let status = resp.status().as_u16();
    let body = resp.text().await.unwrap_or_default();
    assert_eq!(status, 200, "sort={sort} must not error: {body}");

    let body: serde_json::Value = serde_json::from_str(&body).expect("json body");
    body["data"]["posts"]
        .as_array()
        .expect("posts array")
        .iter()
        .map(|p| p["title"].as_str().unwrap_or_default().to_string())
        .collect()
}

#[tokio::test]
async fn every_sort_mode_answers_on_an_empty_forum() {
    let app = TestApp::spawn().await;

    for sort in ["recent", "hot", "top-bounty"] {
        let resp = app.get(&format!("/api/forum/posts?sort={sort}")).await;
        assert_eq!(resp.status().as_u16(), 200, "sort={sort}");
    }
}

#[tokio::test]
async fn hot_ranks_by_net_votes() {
    let app = TestApp::spawn().await;
    seed_two_posts(&app, 3).await;

    let titles = titles_for(&app, "hot").await;
    assert_eq!(titles.first().map(String::as_str), Some("Popular post"));
}

#[tokio::test]
async fn recent_ranks_by_creation_date() {
    let app = TestApp::spawn().await;
    seed_two_posts(&app, 3).await;

    // "Popular post" is inserted last, so it is also the most recent.
    let titles = titles_for(&app, "recent").await;
    assert_eq!(titles.first().map(String::as_str), Some("Popular post"));
    assert_eq!(titles.last().map(String::as_str), Some("Quiet post"));
}

#[tokio::test]
async fn top_bounty_ranks_by_fragments() {
    let app = TestApp::spawn().await;
    seed_two_posts(&app, 0).await;

    let titles = titles_for(&app, "top-bounty").await;
    assert_eq!(titles.first().map(String::as_str), Some("Popular post"));
}

#[tokio::test]
async fn hot_counts_downvotes_against_the_post() {
    let app = TestApp::spawn().await;
    let (_quiet, popular) = seed_two_posts(&app, 1).await;

    // Two downvotes take the popular post to a net of -1, below the quiet
    // post's 0. This is the arithmetic that used to be unreachable.
    for i in 0..2 {
        let voter: Uuid = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "INSERT INTO users (username, email, password_hash, display_name, first_name, last_name)
             VALUES ('forum_down_{i}', 'forum_down_{i}@test.dev', 'x', 'Down {i}', 'Down', '{i}')
             RETURNING id"
        )))
        .fetch_one(&app.db)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO reactions (target_type, target_id, user_id, kind)
             VALUES ('post', $1, $2, 'downvote')",
        )
        .bind(popular)
        .bind(voter)
        .execute(&app.db)
        .await
        .unwrap();
    }

    let titles = titles_for(&app, "hot").await;
    assert_eq!(titles.first().map(String::as_str), Some("Quiet post"));
}
