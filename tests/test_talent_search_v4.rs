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
        "INSERT INTO user_code_portfolios
            (user_id, platform, handle, profile_url, verified_at, verification_method)
         VALUES ($1, 'github', 'proved', 'https://github.com/proved', NOW(), 'oauth')",
    )
    .bind(proved)
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO user_code_portfolios (user_id, platform, handle, profile_url)
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
    // `register_admin`, not a raw `role = 'admin'`: since P21 the admin gate
    // reads `user_capabilities`, and the column alone opens nothing.
    app.register_admin("revenue_admin").await;
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

    // `fee_rate_bps` has no default on purpose: a revenue line that cannot
    // say what rate produced it is not auditable. 1000 bps is the ten percent
    // the mission marketplace takes.
    sqlx::query(
        "INSERT INTO platform_revenues (source, amount_credits, fee_rate_bps, notes)
         VALUES ('mission_marketplace', 120.00, 1000, 'test')",
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

// ═══════════════════════════════════════════════════════════════════
// What somebody has done, not only what they are
// ═══════════════════════════════════════════════════════════════════

/// A concluded contest in which `winner` came first and `runner_up` second.
async fn a_contest_won_by(app: &TestApp, slug: &str, winner: Uuid, runner_up: Option<Uuid>) {
    let tournament: Uuid = sqlx::query_scalar(
        "INSERT INTO tournaments (slug, name, skill_domain, kind, format, status,
                                  starts_at, ends_at)
         VALUES ($1, 'Concours', 'design', 'individual', 'ladder', 'concluded',
                 NOW() - INTERVAL '30 days', NOW() - INTERVAL '2 days')
         RETURNING id",
    )
    .bind(slug)
    .fetch_one(&app.db)
    .await
    .unwrap();

    for (user, rank) in [Some((winner, 1)), runner_up.map(|u| (u, 2))]
        .into_iter()
        .flatten()
    {
        sqlx::query(
            "INSERT INTO tournament_participants
                 (tournament_id, participant_type, participant_id, rank)
             VALUES ($1, 'user', $2, $3)",
        )
        .bind(tournament)
        .bind(user)
        .bind(rank)
        .execute(&app.db)
        .await
        .unwrap();
    }
}

#[tokio::test]
async fn a_family_narrows_to_a_group_of_trades() {
    let app = TestApp::spawn().await;
    let brand = a_talent(&app, "fam_brand").await;
    let motion = a_talent(&app, "fam_motion").await;

    // Two trades in two families. `reviewer_group` is the grouping the
    // platform maintains for drawing reviewers, which makes it the one a
    // recruiter can rely on being current.
    declares(&app, brand, "design-brand-identity", true).await;
    declares(&app, motion, "design-motion-2d", true).await;

    let found = usernames(&search(&app, "?family=brand").await);
    assert!(found.contains(&"fam_brand".to_string()), "{found:?}");
    assert!(!found.contains(&"fam_motion".to_string()), "{found:?}");
}

#[tokio::test]
async fn a_family_a_recruiter_invents_returns_nobody_not_everybody() {
    let app = TestApp::spawn().await;
    let someone = a_talent(&app, "fam_nobody").await;
    declares(&app, someone, "design-brand-identity", true).await;

    // Silently dropping an unmatched filter is how a recruiter concludes the
    // platform has nobody in a family, having actually been shown everybody.
    let body = search(&app, "?family=does-not-exist").await;
    assert!(usernames(&body).is_empty(), "{body}");
}

#[tokio::test]
async fn a_podium_is_not_a_win() {
    let app = TestApp::spawn().await;
    let first = a_talent(&app, "won_first").await;
    let second = a_talent(&app, "won_second").await;
    a_contest_won_by(&app, "concours-un", first, Some(second)).await;

    let found = usernames(&search(&app, "?min_contests_won=1").await);
    assert!(found.contains(&"won_first".to_string()), "{found:?}");
    assert!(
        !found.contains(&"won_second".to_string()),
        "second place is not a win: {found:?}"
    );
}

#[tokio::test]
async fn a_contest_still_running_has_no_result_to_report() {
    let app = TestApp::spawn().await;
    let leading = a_talent(&app, "won_running").await;

    let tournament: Uuid = sqlx::query_scalar(
        "INSERT INTO tournaments (slug, name, skill_domain, kind, format, status,
                                  starts_at, ends_at)
         VALUES ('concours-en-cours', 'Concours', 'design', 'individual', 'ladder',
                 'active', NOW() - INTERVAL '2 days', NOW() + INTERVAL '5 days')
         RETURNING id",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO tournament_participants
             (tournament_id, participant_type, participant_id, rank)
         VALUES ($1, 'user', $2, 1)",
    )
    .bind(tournament)
    .bind(leading)
    .execute(&app.db)
    .await
    .unwrap();

    // Leading a contest that has not finished is not having won one.
    let found = usernames(&search(&app, "?min_contests_won=1").await);
    assert!(!found.contains(&"won_running".to_string()), "{found:?}");
}

#[tokio::test]
async fn a_design_portfolio_counts_as_a_proved_platform() {
    let app = TestApp::spawn().await;
    let designer = a_talent(&app, "plat_behance").await;

    // Design portfolios live in `external_signals`, confirmed by a moderator
    // rather than by OAuth — the platform will not fetch arbitrary
    // user-supplied URLs. The search read only the forges and registries, so
    // a recruiter filtering on Behance was shown nobody.
    sqlx::query(
        "INSERT INTO external_signals (user_id, provider, url, title, verified_at,
                                       verification_method)
         VALUES ($1, 'behance', 'https://behance.net/exemple', 'Portfolio',
                 NOW(), 'manual_review')",
    )
    .bind(designer)
    .execute(&app.db)
    .await
    .unwrap();

    let body = search(&app, "?platform=behance").await;
    assert!(usernames(&body).contains(&"plat_behance".to_string()), "{body}");
    assert!(
        body["data"]["talents"][0]["verified_platforms"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p == "behance"),
        "{body}"
    );
}

#[tokio::test]
async fn an_unconfirmed_portfolio_is_still_not_evidence() {
    let app = TestApp::spawn().await;
    let claimer = a_talent(&app, "plat_claimed").await;
    sqlx::query(
        "INSERT INTO external_signals (user_id, provider, url, title)
         VALUES ($1, 'dribbble', 'https://dribbble.com/quelquun', 'Portfolio')",
    )
    .bind(claimer)
    .execute(&app.db)
    .await
    .unwrap();

    // Anybody can type anybody's handle. The rule that holds for the forges
    // holds here.
    let found = usernames(&search(&app, "?platform=dribbble").await);
    assert!(found.is_empty(), "{found:?}");
}

#[tokio::test]
async fn sorting_by_wins_reorders_and_paginates_on_wins() {
    let app = TestApp::spawn().await;
    let strong = a_talent(&app, "sort_strong").await;
    let winner = a_talent(&app, "sort_winner").await;

    // The high scorer has never won; the winner scores nothing. Under the
    // default sort the first leads, under `contests_won` the second does.
    score(&app, strong, "design", 4000, "senior").await;
    a_contest_won_by(&app, "concours-deux", winner, None).await;

    let by_score = usernames(&search(&app, "?limit=2").await);
    assert_eq!(by_score.first().unwrap(), "sort_strong", "{by_score:?}");

    let by_wins = search(&app, "?sort=contests_won&limit=1").await;
    assert_eq!(usernames(&by_wins).first().unwrap(), "sort_winner");

    // The cursor carries the key that was sorted on. Carrying the craft score
    // instead would skip or repeat rows here, silently.
    let cursor = by_wins["data"]["next_cursor"].as_str().unwrap().to_string();
    assert!(cursor.starts_with("1|"), "cursor holds the win count: {cursor}");

    let page_two = usernames(&search(&app, &format!("?sort=contests_won&limit=5&after={cursor}")).await);
    assert!(!page_two.contains(&"sort_winner".to_string()), "{page_two:?}");
}

#[tokio::test]
async fn never_featured_sorts_below_everybody_who_ever_was() {
    let app = TestApp::spawn().await;
    let featured = a_talent(&app, "feat_yes").await;
    let never = a_talent(&app, "feat_no").await;
    score(&app, never, "design", 9000, "principal").await;

    sqlx::query(
        "INSERT INTO featured_talents (skill_domain, week_of, user_id, reason_md)
         VALUES ('design', date_trunc('week', CURRENT_DATE)::DATE, $1,
                 'Une identité qui tient sur un tampon encreur, et qui reste lisible gravée dans le bois.')",
    )
    .bind(featured)
    .execute(&app.db)
    .await
    .unwrap();

    // Never featured is not "featured in 1970". A high score does not buy a
    // place in an editorial ranking.
    let ordered = usernames(&search(&app, "?sort=recently_featured&limit=5").await);
    assert_eq!(ordered.first().unwrap(), "feat_yes", "{ordered:?}");
}

#[tokio::test]
async fn a_featuring_ages_out_of_the_filter() {
    let app = TestApp::spawn().await;
    let old = a_talent(&app, "feat_old").await;
    sqlx::query(
        "INSERT INTO featured_talents (skill_domain, week_of, user_id, reason_md)
         VALUES ('design', (CURRENT_DATE - INTERVAL '400 days')::DATE, $1,
                 'Une mise en avant qui date, gardée pour montrer que le filtre par ancienneté fonctionne.')",
    )
    .bind(old)
    .execute(&app.db)
    .await
    .unwrap();

    // A featuring is somebody's judgement on a given week. Two years later it
    // says what they thought then.
    assert!(
        !usernames(&search(&app, "?featured_within_days=30").await)
            .contains(&"feat_old".to_string())
    );
    assert!(
        usernames(&search(&app, "?featured_within_days=3000").await)
            .contains(&"feat_old".to_string())
    );
}

#[tokio::test]
async fn a_sort_nobody_defined_is_refused_rather_than_ignored() {
    let app = TestApp::spawn().await;
    // Falling back to the default would answer a different question than the
    // one asked, and look like it answered the right one.
    let resp = app.get("/api/talents/search?sort=whatever").await;
    assert_eq!(resp.status(), 400);

    let resp = app.get("/api/talents/search?min_contests_won=0").await;
    assert_eq!(resp.status(), 400, "at least nothing is not a filter");
}

#[tokio::test]
async fn the_track_record_is_in_every_row() {
    let app = TestApp::spawn().await;
    let person = a_talent(&app, "record_all").await;
    declares(&app, person, "design-brand-identity", true).await;
    a_contest_won_by(&app, "concours-trois", person, None).await;

    let body = search(&app, "?q=record_all").await;
    let row = &body["data"]["talents"][0];
    assert_eq!(row["contests_won"], 1, "{body}");
    assert_eq!(row["missions_delivered"], 0);
    assert!(row["last_featured_on"].is_null());
    assert_eq!(row["families"][0], "brand", "{body}");
    // The sort key is pagination machinery, not something a caller reasons
    // about; it stays out of the answer.
    assert!(row.get("sort_key").is_none(), "{body}");
}
