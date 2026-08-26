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
    let data = json(&app, "/api/domains/ai/toolkit").await;
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
    let data = json(&app, "/api/domains/ai/toolkit?orientation=nlp-engineer").await;
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
    let data = json(&app, "/api/domains/ai/toolkit?category=inventee").await;
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
                 ai_subtype, ai_frameworks, published_artifact_url, difficulty,
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

// ═══════════════════════════════════════════════════════════════════
// Guides
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn the_guides_listing_honours_the_domain() {
    let app = TestApp::spawn().await;

    // `content_guides` carried a domain from the start and the endpoint
    // ignored it. Invisible while one domain had rows; the moment a second
    // one did, an AI onboarding guide answered under the code path.
    let ai = json(&app, "/api/guides?domain=ai&kind=onboarding").await;
    let slugs: Vec<&str> = ai
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["slug"].as_str().unwrap())
        .collect();

    assert!(slugs.contains(&"onboarding-ai-safety"), "{slugs:?}");
    assert!(
        !slugs.iter().any(|s| s.starts_with("onboarding-web")),
        "{slugs:?}"
    );
}

#[tokio::test]
async fn every_ai_reviewer_family_has_an_onboarding_guide() {
    let app = TestApp::spawn().await;

    let missing: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT o.reviewer_group FROM orientations o
          LEFT JOIN content_guides g
                 ON g.skill_domain = 'ai' AND g.kind = 'onboarding'
                AND g.reviewer_group = o.reviewer_group
          WHERE o.primary_domain = 'ai'
            AND o.reviewer_group IS NOT NULL
            AND g.id IS NULL",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert!(
        missing.is_empty(),
        "families with nowhere to start: {missing:?}"
    );
}

#[tokio::test]
async fn a_guide_is_served_with_its_body() {
    let app = TestApp::spawn().await;

    let resp = app.get("/api/guides/template-red-team-report").await;
    assert_eq!(resp.status().as_u16(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();

    assert_eq!(body["data"]["skill_domain"], "ai");
    assert_eq!(
        body["data"]["locale"], "en",
        "English with no Accept-Language asked for: migration 0304 gave this guide an English row"
    );
    assert!(
        body["data"]["body_md"]
            .as_str()
            .unwrap()
            .contains("Dual use"),
        "the template must carry the section the disclosure policy requires"
    );

    // And the French row still carries the same section. This assertion used
    // to be the only one, made against the default locale, so it stopped
    // testing the French guide the moment an English one existed beside it.
    let fr: serde_json::Value = app
        .get_with_header(
            "/api/guides/template-red-team-report",
            "accept-language",
            "fr",
        )
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(fr["data"]["locale"], "fr");
    assert!(
        fr["data"]["body_md"]
            .as_str()
            .unwrap()
            .contains("Double usage")
    );
}

#[tokio::test]
async fn the_ai_award_categories_join_the_existing_ceremony() {
    let app = TestApp::spawn().await;

    // One ceremony, not two: an AI researcher and a library author named on
    // the same evening is what makes the AI categories visible to people who
    // would never have looked for them.
    // Named rather than matched. This was `slug LIKE '%ai%'`, which is a
    // substring and therefore also catches `best-blockchain-project`,
    // `best-trainer` and `cross-domain-educator` — three letters in the
    // middle of an unrelated word. It passed while those categories did not
    // exist and broke the day two domains added theirs, which is the wrong
    // reason for a test about the AI categories to fail.
    const AI_CATEGORIES: &[&str] = &[
        "best-ai-application",
        "best-ai-model",
        "best-ai-safety-research",
        "best-dataset-published",
        "rookie-ai-researcher",
    ];

    let ai: Vec<String> =
        sqlx::query_scalar("SELECT slug FROM award_categories WHERE slug = ANY($1) ORDER BY slug")
            .bind(AI_CATEGORIES)
            .fetch_all(&app.db)
            .await
            .unwrap();
    assert_eq!(ai.len(), AI_CATEGORIES.len(), "got {ai:?}");

    // The claim is that the AI categories live in the same table as the code
    // ones — one ceremony — not that the table has a particular size. Every
    // domain added since brings its own, and a fixed total tests only that
    // somebody ran the seed.
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM award_categories")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert!(
        total >= 13,
        "eight code categories plus the five AI ones, at least: got {total}"
    );

    let inactive: Vec<String> = sqlx::query_scalar(
        "SELECT slug FROM award_categories
          WHERE slug = ANY($1) AND is_active = FALSE",
    )
    .bind(AI_CATEGORIES)
    .fetch_all(&app.db)
    .await
    .unwrap();
    assert!(
        inactive.is_empty(),
        "an AI category nobody can be nominated in is not on the evening: {inactive:?}"
    );
}

#[tokio::test]
async fn every_ai_guide_exists_in_both_languages() {
    let app = TestApp::spawn().await;

    // F-01, F-05 and G-01 each say "FR + EN". A slug with no English row does
    // not 404 — it falls back to French — which is why nothing would have
    // surfaced half a domain being untranslated.
    let untranslated: Vec<String> = sqlx::query_scalar(
        "SELECT fr.slug FROM content_guides fr
          LEFT JOIN content_guides en
                 ON en.slug = fr.slug AND en.locale = 'en'
          WHERE fr.skill_domain = 'ai' AND fr.locale = 'fr' AND en.id IS NULL",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert!(untranslated.is_empty(), "French only: {untranslated:?}");
}

#[tokio::test]
async fn an_english_reader_gets_the_english_guide() {
    let app = TestApp::spawn().await;

    let resp = app
        .client
        .get(format!("{}/api/guides/onboarding-ai-safety", app.addr))
        .header("Accept-Language", "en")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["locale"], "en");
    assert_eq!(body["data"]["title"], "Getting started in safety");
}

// ═══════════════════════════════════════════════════════════════════
// Mentor matching
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn ai_mentor_matching_needs_a_family_to_match_on() {
    let app = TestApp::spawn().await;
    app.register_user("ai_mentee_bare").await;

    // Refused with a message that names the fix, rather than an empty list
    // that reads as "there is nobody".
    let resp = app.get("/api/domains/ai/mentors/for-me").await;
    assert_eq!(resp.status().as_u16(), 400);
    let body = resp.text().await.unwrap_or_default();
    assert!(
        body.contains("ai"),
        "the message must name the domain: {body}"
    );
}

#[tokio::test]
async fn an_ai_mentor_is_suggested_with_the_reasoning_attached() {
    let app = TestApp::spawn().await;

    let mentee = a_user(&app, "ai_mentee").await;
    profile_for(&app, mentee, "ai", "ml", "pytorch", "+01:00").await;
    score_for(&app, mentee, "ai", 100).await;

    // One good match, and one in the wrong family who must not appear.
    for (name, family, score) in [
        ("ai_mentor_good", "ml", 2000),
        ("ai_mentor_wrong", "safety", 4000),
    ] {
        let mentor = a_user(&app, name).await;
        profile_for(&app, mentor, "ai", family, "pytorch", "+02:00").await;
        score_for(&app, mentor, "ai", score).await;
        // A mentor's families come from what they delivered, not from what
        // they declared. Setting a profile and a score describes somebody who
        // has said what interests them and shown nothing, which is exactly
        // who the matcher is supposed to leave out.
        common::delivered_in(&app, mentor, "ai", family).await;
        sqlx::query(
            "INSERT INTO mentor_profiles
                (user_id, headline, bio, hourly_rate_eur_cents, active)
             VALUES ($1, 'Je relis des entraînements',
                     'Je relis des entraînements et des évaluations.', 0, TRUE)",
        )
        .bind(mentor)
        .execute(&app.db)
        .await
        .unwrap();
    }

    app.login("ai_mentee").await;
    let resp = app.get("/api/domains/ai/mentors/for-me").await;
    assert_eq!(resp.status().as_u16(), 200, "{:?}", resp.text().await);
    let body: serde_json::Value = resp.json().await.unwrap();
    let mentors = body["data"]["mentors"].as_array().unwrap();

    assert_eq!(
        mentors.len(),
        1,
        "the wrong family must not appear: {mentors:?}"
    );
    assert_eq!(mentors[0]["username"], "ai_mentor_good");

    // The reasoning is the point: a mentee who can read why somebody was
    // suggested can tell us it was wrong.
    let because = mentors[0]["because"].as_array().unwrap();
    assert!(!because.is_empty());
    assert!(
        because
            .iter()
            .any(|b| b.as_str().unwrap_or("").contains("outils")),
        "the AI wording says tools, not languages: {because:?}"
    );
}

async fn profile_for(
    app: &TestApp,
    user: Uuid,
    domain: &str,
    family: &str,
    tool: &str,
    timezone: &str,
) {
    sqlx::query("UPDATE users SET timezone = $2 WHERE id = $1")
        .bind(user)
        .bind(timezone)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO user_domain_profiles (user_id, domain, answers, completed_at)
         VALUES ($1, $2,
                 jsonb_build_object('preferred_families', jsonb_build_array($3),
                                    'main_frameworks', jsonb_build_array($4)),
                 NOW())",
    )
    .bind(user)
    .bind(domain)
    .bind(family)
    .bind(tool)
    .execute(&app.db)
    .await
    .unwrap();
}

async fn score_for(app: &TestApp, user: Uuid, domain: &str, score: i32) {
    sqlx::query(
        "INSERT INTO craft_scores (user_id, skill_domain, score, tier_slug)
         VALUES ($1, $2, $3, 'apprentice')
         ON CONFLICT (user_id, skill_domain) DO UPDATE SET score = EXCLUDED.score",
    )
    .bind(user)
    .bind(domain)
    .bind(score)
    .execute(&app.db)
    .await
    .unwrap();
}

async fn a_user(app: &TestApp, username: &str) -> Uuid {
    app.register_user(username).await;
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT id FROM users WHERE username = '{username}'"
    )))
    .fetch_one(&app.db)
    .await
    .unwrap()
}
