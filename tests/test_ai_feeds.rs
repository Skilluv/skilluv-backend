//! The four public doors into AI work, and what they are careful not to show.

mod common;
use common::TestApp;
use uuid::Uuid;

async fn json(app: &TestApp, path: &str) -> serde_json::Value {
    let resp = app.get(path).await;
    assert_eq!(resp.status().as_u16(), 200, "GET {path}");
    let body: serde_json::Value = resp.json().await.unwrap();
    body["data"].clone()
}

#[tokio::test]
async fn the_toolkit_answers_without_an_account() {
    let app = TestApp::spawn().await;
    let data = json(&app, "/api/ai/toolkit").await;
    let resources = data["resources"].as_array().unwrap();

    assert!(resources.len() >= 20, "got {}", resources.len());

    // The column that makes this useful to somebody without a credit card.
    let with_access_note = resources
        .iter()
        .filter(|r| !r["access_note"].as_str().unwrap_or("").is_empty())
        .count();
    assert_eq!(
        with_access_note,
        resources.len(),
        "every resource says what it takes to reach it"
    );
}

#[tokio::test]
async fn filtering_the_toolkit_keeps_what_serves_everyone() {
    let app = TestApp::spawn().await;
    let data = json(&app, "/api/ai/toolkit?orientation=nlp-engineer").await;
    let slugs: Vec<&str> = data["resources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["slug"].as_str().unwrap())
        .collect();

    // Tagged for this trade.
    assert!(slugs.contains(&"masakhane"), "{slugs:?}");
    // Tagged for none, so it serves every trade. Excluding it would hide
    // HuggingFace from somebody asking for the NLP toolkit.
    assert!(slugs.contains(&"huggingface"), "{slugs:?}");
    // Tagged for another trade only.
    assert!(!slugs.contains(&"evidently"), "{slugs:?}");
}

#[tokio::test]
async fn an_unknown_toolkit_category_returns_nothing_rather_than_everything() {
    let app = TestApp::spawn().await;
    let data = json(&app, "/api/ai/toolkit?category=inventee").await;
    assert!(data["resources"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn the_competition_feed_hides_what_is_over() {
    let app = TestApp::spawn().await;

    let curator: Uuid = {
        app.register_user("comp_curator").await;
        sqlx::query_scalar("SELECT id FROM users WHERE username = 'comp_curator'")
            .fetch_one(&app.db)
            .await
            .unwrap()
    };

    sqlx::query(
        "INSERT INTO external_ai_competitions
            (platform, title, url, why_this_one, deadline, reviewed_by_user_id, is_published)
         VALUES
           ('kaggle', 'Encore ouverte', 'https://kaggle.com/c/open',
            'Sujet tabulaire accessible sans GPU.', NOW() + INTERVAL '30 days', $1, TRUE),
           ('kaggle', 'Déjà close', 'https://kaggle.com/c/closed',
            'Bon sujet, trop tard.', NOW() - INTERVAL '1 day', $1, TRUE),
           ('huggingface_leaderboard', 'Classement permanent', 'https://huggingface.co/lb',
            'Pas de date de fin par nature.', NULL, $1, TRUE),
           ('kaggle', 'Pas encore relue', 'https://kaggle.com/c/draft',
            'En attente de curation.', NOW() + INTERVAL '30 days', NULL, FALSE)",
    )
    .bind(curator)
    .execute(&app.db)
    .await
    .unwrap();

    let titles: Vec<String> = json(&app, "/api/ai/competitions").await["competitions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["title"].as_str().unwrap().to_string())
        .collect();

    assert!(titles.contains(&"Encore ouverte".to_string()), "{titles:?}");
    // A rolling leaderboard has no deadline and is never closed.
    assert!(
        titles.contains(&"Classement permanent".to_string()),
        "{titles:?}"
    );
    // A listing that keeps showing closed entries teaches people to stop
    // reading it.
    assert!(!titles.contains(&"Déjà close".to_string()), "{titles:?}");
    // Unreviewed rows are not a feed.
    assert!(
        !titles.contains(&"Pas encore relue".to_string()),
        "{titles:?}"
    );
}

#[tokio::test]
async fn a_competition_cannot_publish_itself() {
    let app = TestApp::spawn().await;

    // A row arriving from an automated fetch must not reach the feed with
    // nobody answerable for it.
    let refused = sqlx::query(
        "INSERT INTO external_ai_competitions
            (platform, title, url, why_this_one, is_published)
         VALUES ('kaggle', 'Auto', 'https://kaggle.com/c/auto', 'x', TRUE)",
    )
    .execute(&app.db)
    .await;
    assert!(refused.is_err());
}

#[tokio::test]
async fn the_artifact_feed_shows_only_verified_public_work() {
    let app = TestApp::spawn().await;

    app.register_user("feed_author").await;
    let user: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'feed_author'")
        .fetch_one(&app.db)
        .await
        .unwrap();

    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO projects (slug, name, owner_type, owner_id)
         VALUES ('feed-proj', 'Projet', 'user', $1) RETURNING id",
    )
    .bind(user)
    .fetch_one(&app.db)
    .await
    .unwrap();

    for (title, status) in [("Vérifié", "verified"), ("En attente", "pending")] {
        let slice: Uuid = sqlx::query_scalar(
            "INSERT INTO project_slices
                (project_id, title, description, primary_domain, slice_type,
                 ai_subtype, ai_frameworks, ai_external_hosting_url, difficulty,
                 orientation_id)
             VALUES ($1, $2, 'x', 'ai', 'ai_artifact', 'ml_model', ARRAY['pytorch'],
                     'https://huggingface.co/skilluv/demo', 3,
                     (SELECT id FROM orientations WHERE slug = 'ml-engineer'))
             RETURNING id",
        )
        .bind(project)
        .bind(title)
        .fetch_one(&app.db)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO deliverables
                (user_id, slice_id, artifact_type, artifact_url, verifiable_by,
                 verification_status, verified_at)
             VALUES ($1, $2, 'other', 'https://example.test/a', 'human_review', $3,
                     CASE WHEN $3 = 'verified' THEN NOW() ELSE NULL END)",
        )
        .bind(user)
        .bind(slice)
        .bind(status)
        .execute(&app.db)
        .await
        .unwrap();
    }

    let titles: Vec<String> = json(&app, "/api/ai/artifacts").await["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["title"].as_str().unwrap().to_string())
        .collect();

    // The feed exists to answer "does this platform produce anything real",
    // and a pending submission answers it wrongly.
    assert!(titles.contains(&"Vérifié".to_string()), "{titles:?}");
    assert!(!titles.contains(&"En attente".to_string()), "{titles:?}");
}

#[tokio::test]
async fn the_artifact_feed_filters_on_the_framework_the_slice_declares() {
    let app = TestApp::spawn().await;
    let data = json(&app, "/api/ai/artifacts?framework=cobol").await;
    assert!(data["artifacts"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn an_oversized_limit_is_refused_rather_than_silently_capped() {
    let app = TestApp::spawn().await;
    // Silently capping teaches a caller that their parameter works.
    let resp = app.get("/api/ai/artifacts?limit=5000").await;
    assert_eq!(resp.status().as_u16(), 400);
}
