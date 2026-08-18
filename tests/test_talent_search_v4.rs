//! The recruiter search, and the revenue catalogue behind the business model.

mod common;
use common::TestApp;
use serde_json::Value;
use uuid::Uuid;

async fn a_talent(app: &TestApp, username: &str) -> Uuid {
    app.register_user(username).await;
    let id: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(username)
        .fetch_one(&app.db)
        .await
        .unwrap();
    // Registration leaves the profile inactive until somebody fills it in;
    // an inactive profile is correctly invisible to a recruiter.
    sqlx::query("UPDATE users SET profile_active = TRUE WHERE id = $1")
        .bind(id)
        .execute(&app.db)
        .await
        .unwrap();
    id
}

async fn score(app: &TestApp, user: Uuid, domain: &str, score: i32, tier: &str) {
    sqlx::query(
        "INSERT INTO craft_scores (user_id, skill_domain, score, tier_slug)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (user_id, skill_domain) DO UPDATE
             SET score = EXCLUDED.score, tier_slug = EXCLUDED.tier_slug",
    )
    .bind(user)
    .bind(domain)
    .bind(score)
    .bind(tier)
    .execute(&app.db)
    .await
    .unwrap();
}

async fn declares(app: &TestApp, user: Uuid, orientation: &str, primary: bool) {
    sqlx::query(
        "INSERT INTO user_orientations (user_id, orientation_id, mode, is_primary)
         SELECT $1, resolve_orientation($2), 'active', $3",
    )
    .bind(user)
    .bind(orientation)
    .bind(primary)
    .execute(&app.db)
    .await
    .unwrap();
}

async fn search(app: &TestApp, query: &str) -> Value {
    let resp = app.get(&format!("/api/talents/search{query}")).await;
    assert_eq!(resp.status(), 200, "search must answer");
    resp.json().await.unwrap()
}

fn usernames(body: &Value) -> Vec<String> {
    body["data"]["talents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["username"].as_str().unwrap().to_string())
        .collect()
}

// ═══════════════════════════════════════════════════════════════════
// The old versions are gone
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn the_three_old_versions_no_longer_answer() {
    let app = TestApp::spawn().await;

    // Three endpoints answering the same question differently is three places
    // for a filter to be subtly wrong.
    for gone in ["/api/talents/search/v2", "/api/talents/search/v3"] {
        assert_eq!(
            app.get(gone).await.status(),
            404,
            "{gone} should have been removed with its module"
        );
    }
    assert_eq!(app.get("/api/talents/search").await.status(), 200);
}

// ═══════════════════════════════════════════════════════════════════
// Filters
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn results_come_back_strongest_first() {
    let app = TestApp::spawn().await;
    let junior = a_talent(&app, "search_junior").await;
    let senior = a_talent(&app, "search_senior").await;
    score(&app, junior, "code", 200, "contributor").await;
    score(&app, senior, "code", 4000, "staff").await;

    let found = usernames(&search(&app, "?skill_domain=code").await);
    assert_eq!(found, vec!["search_senior", "search_junior"]);
}

#[tokio::test]
async fn a_tier_is_a_floor_not_an_exact_match() {
    let app = TestApp::spawn().await;
    let contributor = a_talent(&app, "tier_contributor").await;
    let staff = a_talent(&app, "tier_staff").await;
    score(&app, contributor, "code", 200, "contributor").await;
    score(&app, staff, "code", 4000, "staff").await;

    // "Senior and above" must include Staff. A recruiter asking for Senior is
    // stating a minimum, not describing a band.
    let found = usernames(&search(&app, "?skill_domain=code&min_tier=senior").await);
    assert_eq!(found, vec!["tier_staff"]);
}

#[tokio::test]
async fn a_trade_narrows_and_an_old_slug_still_works() {
    let app = TestApp::spawn().await;
    let front = a_talent(&app, "search_front").await;
    let kernel = a_talent(&app, "search_kernel").await;
    score(&app, front, "code", 1000, "engineer").await;
    score(&app, kernel, "code", 1000, "engineer").await;
    declares(&app, front, "web-frontend-developer", true).await;
    declares(&app, kernel, "kernel-driver-developer", true).await;

    assert_eq!(
        usernames(&search(&app, "?orientation=web-frontend-developer").await),
        vec!["search_front"]
    );
    // The catalogue was renamed in migration 0173; a bookmarked search must
    // not silently return nobody.
    assert_eq!(
        usernames(&search(&app, "?orientation=dev-frontend").await),
        vec!["search_front"]
    );
}

#[tokio::test]
async fn a_trade_nobody_has_is_a_404_not_the_whole_catalogue() {
    let app = TestApp::spawn().await;
    let someone = a_talent(&app, "search_typo").await;
    score(&app, someone, "code", 1000, "engineer").await;

    // Silently dropping the filter shows a recruiter a full page of people
    // who do not do the job they searched for.
    assert_eq!(
        app.get("/api/talents/search?orientation=metier-invente")
            .await
            .status(),
        404
    );
}

#[tokio::test]
async fn a_capability_is_a_filter() {
    let app = TestApp::spawn().await;
    let reviewer = a_talent(&app, "search_reviewer").await;
    let other = a_talent(&app, "search_notreviewer").await;
    score(&app, reviewer, "code", 1000, "engineer").await;
    score(&app, other, "code", 1000, "engineer").await;

    sqlx::query(
        "INSERT INTO user_capabilities (user_id, capability, granted_reason)
         VALUES ($1, 'code_reviewer:systems', 'test')",
    )
    .bind(reviewer)
    .execute(&app.db)
    .await
    .unwrap();

    assert_eq!(
        usernames(&search(&app, "?capability=code_reviewer:systems").await),
        vec!["search_reviewer"]
    );
}

#[tokio::test]
async fn a_claimed_account_is_not_evidence() {
    let app = TestApp::spawn().await;
    let proved = a_talent(&app, "search_proved").await;
    let claimed = a_talent(&app, "search_claimed").await;
    score(&app, proved, "code", 1000, "engineer").await;
    score(&app, claimed, "code", 1000, "engineer").await;

    sqlx::query(
        "INSERT INTO user_portfolios
            (user_id, platform, handle, profile_url, verified_at, verification_method)
         VALUES ($1, 'github', 'proved', 'https://github.com/proved', NOW(), 'oauth')",
    )
    .bind(proved)
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO user_portfolios (user_id, platform, handle, profile_url)
         VALUES ($1, 'github', 'claimed', 'https://github.com/claimed')",
    )
    .bind(claimed)
    .execute(&app.db)
    .await
    .unwrap();

    // Anybody can type anybody's handle. On a recruiter surface that
    // distinction is the whole point.
    let body = search(&app, "?platform=github").await;
    assert_eq!(usernames(&body), vec!["search_proved"]);
    assert_eq!(
        body["data"]["talents"][0]["verified_platforms"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn every_named_skill_must_be_proved_not_just_one() {
    let app = TestApp::spawn().await;
    let both = a_talent(&app, "skills_both").await;
    let one = a_talent(&app, "skills_one").await;
    score(&app, both, "code", 1000, "engineer").await;
    score(&app, one, "code", 1000, "engineer").await;

    let skills: Vec<(Uuid, String)> =
        sqlx::query_as("SELECT id, slug FROM skill_nodes WHERE slug IN ('rust', 'typescript')")
            .fetch_all(&app.db)
            .await
            .unwrap();
    if skills.len() < 2 {
        // The seed catalogue does not carry both; nothing to assert.
        return;
    }

    for (skill_id, slug) in &skills {
        sqlx::query(
            "INSERT INTO user_skills (user_id, skill_id, proficiency_level)
             VALUES ($1, $2, 3) ON CONFLICT DO NOTHING",
        )
        .bind(both)
        .bind(skill_id)
        .execute(&app.db)
        .await
        .unwrap();
        if slug == "rust" {
            sqlx::query(
                "INSERT INTO user_skills (user_id, skill_id, proficiency_level)
                 VALUES ($1, $2, 3) ON CONFLICT DO NOTHING",
            )
            .bind(one)
            .bind(skill_id)
            .execute(&app.db)
            .await
            .unwrap();
        }
    }

    // A recruiter asking for Rust and TypeScript wants somebody who has both.
    assert_eq!(
        usernames(&search(&app, "?skills=rust,typescript").await),
        vec!["skills_both"]
    );
}

#[tokio::test]
async fn the_answer_says_which_filters_it_honoured() {
    let app = TestApp::spawn().await;
    let body = search(&app, "?skill_domain=code&available_only=true").await;
    let applied = body["data"]["filters_applied"].as_array().unwrap();

    // A silently dropped filter reads as "nobody matches".
    assert!(applied.iter().any(|f| f == "skill_domain"));
    assert!(applied.iter().any(|f| f == "available_only"));
}

#[tokio::test]
async fn a_hidden_profile_is_not_searchable() {
    let app = TestApp::spawn().await;
    let hidden = a_talent(&app, "search_hidden").await;
    score(&app, hidden, "code", 5000, "staff").await;
    sqlx::query("UPDATE users SET profile_hidden = TRUE WHERE id = $1")
        .bind(hidden)
        .execute(&app.db)
        .await
        .unwrap();

    assert!(usernames(&search(&app, "?skill_domain=code").await).is_empty());
}

// ═══════════════════════════════════════════════════════════════════
// Pagination
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn pagination_neither_skips_nor_repeats() {
    let app = TestApp::spawn().await;
    for n in 0..5 {
        let user = a_talent(&app, &format!("page_talent_{n}")).await;
        score(&app, user, "code", 1000 - n * 10, "engineer").await;
    }

    let first = search(&app, "?skill_domain=code&limit=2").await;
    let first_names = usernames(&first);
    assert_eq!(first_names.len(), 2);
    let cursor = first["data"]["next_cursor"].as_str().expect("a full page");

    let second = search(&app, &format!("?skill_domain=code&limit=2&after={cursor}")).await;
    for name in usernames(&second) {
        assert!(!first_names.contains(&name), "{name} appeared twice");
    }

    // The last page carries no cursor.
    let all = search(&app, "?skill_domain=code&limit=50").await;
    assert!(all["data"]["next_cursor"].is_null());
}

#[tokio::test]
async fn unscored_people_still_paginate() {
    let app = TestApp::spawn().await;
    // Everybody unscored shares a score of zero, so the id is what separates
    // them. A cursor on the score alone would loop on the first page forever.
    for n in 0..4 {
        a_talent(&app, &format!("unscored_{n}")).await;
    }

    let first = search(&app, "?limit=2").await;
    let cursor = first["data"]["next_cursor"].as_str().expect("a full page");
    let second = search(&app, &format!("?limit=2&after={cursor}")).await;
    let first_names = usernames(&first);
    for name in usernames(&second) {
        assert!(!first_names.contains(&name));
    }
}

#[tokio::test]
async fn an_unusable_cursor_is_refused() {
    let app = TestApp::spawn().await;
    assert_eq!(
        app.get("/api/talents/search?after=nonsense").await.status(),
        400
    );
}

// ═══════════════════════════════════════════════════════════════════
// The card
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn a_card_shows_every_domain_somebody_is_scored_in() {
    let app = TestApp::spawn().await;
    let polymath = a_talent(&app, "card_polymath").await;
    score(&app, polymath, "code", 2000, "senior").await;
    score(&app, polymath, "design", 800, "engineer").await;

    let body: Value = app
        .get("/api/talents/card_polymath/card")
        .await
        .json()
        .await
        .unwrap();
    let scores = body["data"]["craft_scores"].as_array().unwrap();

    // A card that shows one score for somebody who works across two says
    // less than it knows.
    assert_eq!(scores.len(), 2);
    assert_eq!(scores[0]["skill_domain"], "code");
    assert_eq!(scores[0]["score"], 2000);
}

// ═══════════════════════════════════════════════════════════════════
// Revenue streams
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn the_catalogue_is_honest_about_what_is_live() {
    let app = TestApp::spawn().await;
    app.register_user("revenue_admin").await;
    sqlx::query("UPDATE users SET role = 'admin' WHERE username = 'revenue_admin'")
        .execute(&app.db)
        .await
        .unwrap();
    app.login("revenue_admin").await;

    let resp = app.get("/api/admin/revenue/streams").await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();

    let streams = body["data"]["streams"].as_array().unwrap();
    assert!(
        streams.len() >= 25,
        "the catalogue should cover the pillars"
    );

    // The gap between these two is the honest measure of how much of the
    // business model is a business and how much is a plan.
    let live = body["data"]["live_streams"].as_i64().unwrap();
    let planned = body["data"]["planned_streams"].as_i64().unwrap();
    assert!(live >= 4, "the four that already earn");
    assert!(planned > live, "most of it is still a plan, and says so");

    for stream in streams {
        assert!(!stream["description"].as_str().unwrap().is_empty());
        assert!(!stream["pillar"].as_str().unwrap().is_empty());
    }
}

#[tokio::test]
async fn booking_revenue_marks_a_stream_live() {
    let app = TestApp::spawn().await;

    let live_before: bool = sqlx::query_scalar(
        "SELECT is_live FROM revenue_streams WHERE slug = 'mission_marketplace'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(!live_before, "nothing has earned there yet");

    sqlx::query(
        "INSERT INTO platform_revenues (source, amount_credits, notes)
         VALUES ('mission_marketplace', 120.00, 'test')",
    )
    .execute(&app.db)
    .await
    .unwrap();

    // Maintained by a trigger: whoever wires a flow has no reason to remember
    // a catalogue flag.
    let live_after: bool = sqlx::query_scalar(
        "SELECT is_live FROM revenue_streams WHERE slug = 'mission_marketplace'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(live_after);
}

#[tokio::test]
async fn a_revenue_line_must_name_a_known_stream() {
    let app = TestApp::spawn().await;

    // A foreign key rather than a CHECK, so adding the twenty-third is an
    // INSERT — but an unknown one is still refused.
    let refused = sqlx::query(
        "INSERT INTO platform_revenues (source, amount_credits) VALUES ('vibes', 10.00)",
    )
    .execute(&app.db)
    .await;
    assert!(refused.is_err());
}

#[tokio::test]
async fn the_revenue_figures_are_not_public() {
    let app = TestApp::spawn().await;
    app.register_user("revenue_nosy").await;
    app.login("revenue_nosy").await;

    assert_eq!(app.get("/api/admin/revenue/streams").await.status(), 403);
}
