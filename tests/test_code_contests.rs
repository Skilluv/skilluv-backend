//! Code contests: what each format asks for, what a participant hands in,
//! and which end of the scale it is won at.

mod common;
use common::TestApp;
use serde_json::{Value, json};
use uuid::Uuid;

async fn user_id(app: &TestApp, username: &str) -> Uuid {
    sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(username)
        .fetch_one(&app.db)
        .await
        .unwrap()
}

/// A contest of `kind`, already running, created directly: the admin endpoint
/// is gated on 2FA and that is not what these tests are about.
async fn a_contest(app: &TestApp, slug: &str, kind: &str, rules: Value) -> Uuid {
    let direction = if kind == "code_golf" {
        "lower_is_better"
    } else {
        "higher_is_better"
    };
    sqlx::query_scalar(
        "INSERT INTO tournaments
            (slug, name, kind, starts_at, ends_at, status, rules, scoring_direction, skill_domain)
         VALUES ($1, $1, $2, NOW() - INTERVAL '1 day', NOW() + INTERVAL '7 days',
                 'active', $3, $4, 'code')
         RETURNING id",
    )
    .bind(slug)
    .bind(kind)
    .bind(&rules)
    .bind(direction)
    .fetch_one(&app.db)
    .await
    .expect("contest")
}

async fn enter(app: &TestApp, contest: Uuid, user: Uuid) {
    sqlx::query(
        "INSERT INTO tournament_participants (tournament_id, participant_type, participant_id)
         VALUES ($1, 'user', $2) ON CONFLICT DO NOTHING",
    )
    .bind(contest)
    .bind(user)
    .execute(&app.db)
    .await
    .unwrap();
}

async fn grant(app: &TestApp, user: Uuid, capability: &str) {
    sqlx::query(
        "INSERT INTO user_capabilities (user_id, capability, granted_reason)
         VALUES ($1, $2, 'test') ON CONFLICT DO NOTHING",
    )
    .bind(user)
    .bind(capability)
    .execute(&app.db)
    .await
    .unwrap();
}

fn a_golf_entry(chars: i32) -> Value {
    json!({
        "artifact_url": "https://gist.github.com/x/1",
        "artifact_type": "gist",
        "summary": "one expression, no imports",
        "language": "python",
        "measured_value": chars,
    })
}

// ═══════════════════════════════════════════════════════════════════
// Rules
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn a_golf_is_ranked_by_the_smallest_number() {
    let app = TestApp::spawn().await;
    let contest = a_contest(
        &app,
        "golf-week-1",
        "code_golf",
        json!({"language": "python", "problem_url": "https://x.test/p"}),
    )
    .await;

    for (name, chars) in [("golfer_long", 300), ("golfer_short", 42)] {
        app.register_user(name).await;
        let id = user_id(&app, name).await;
        enter(&app, contest, id).await;
        app.login(name).await;
        let resp = app
            .post(
                "/api/tournaments/golf-week-1/submissions",
                &a_golf_entry(chars),
            )
            .await;
        assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    }

    let body: Value = app
        .get("/api/tournaments/golf-week-1/leaderboard")
        .await
        .json()
        .await
        .unwrap();
    let board = body["data"]["leaderboard"].as_array().unwrap();
    let short = user_id(&app, "golfer_short").await;
    assert_eq!(
        board[0]["participant_id"].as_str().unwrap(),
        short.to_string(),
        "a leaderboard that crowns the longest solution is worse than none"
    );
}

#[tokio::test]
async fn a_golf_entry_without_its_number_is_not_rankable() {
    let app = TestApp::spawn().await;
    let contest = a_contest(
        &app,
        "golf-nonumber",
        "code_golf",
        json!({"language": "python", "problem_url": "https://x.test/p"}),
    )
    .await;
    app.register_user("golfer_vague").await;
    enter(&app, contest, user_id(&app, "golfer_vague").await).await;
    app.login("golfer_vague").await;

    let resp = app
        .post(
            "/api/tournaments/golf-nonumber/submissions",
            &json!({
                "artifact_url": "https://gist.github.com/x/2",
                "artifact_type": "gist",
                "summary": "shortish",
            }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn a_judged_contest_refuses_a_number_that_looks_like_a_score() {
    let app = TestApp::spawn().await;
    let contest = a_contest(
        &app,
        "tdd-one",
        "tdd_contest",
        json!({"problem_url": "https://x.test/p", "judging_criteria": ["tests first"]}),
    )
    .await;
    app.register_user("tdd_entrant").await;
    enter(&app, contest, user_id(&app, "tdd_entrant").await).await;
    app.login("tdd_entrant").await;

    let resp = app
        .post(
            "/api/tournaments/tdd-one/submissions",
            &json!({
                "artifact_url": "https://github.com/x/y",
                "artifact_type": "repository",
                "summary": "red, green, refactor",
                "measured_value": 12,
            }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

// ═══════════════════════════════════════════════════════════════════
// Submissions
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn submitting_without_entering_is_refused() {
    let app = TestApp::spawn().await;
    a_contest(
        &app,
        "golf-unregistered",
        "code_golf",
        json!({"language": "python", "problem_url": "https://x.test/p"}),
    )
    .await;
    app.register_user("golfer_ghost").await;
    app.login("golfer_ghost").await;

    let resp = app
        .post(
            "/api/tournaments/golf-unregistered/submissions",
            &a_golf_entry(50),
        )
        .await;
    assert_eq!(resp.status(), 400);
    let body: Value = resp.json().await.unwrap();
    let message = body.to_string();
    assert!(
        message.contains("register"),
        "the message must say what to do: {message}"
    );
}

#[tokio::test]
async fn a_revision_replaces_the_entry_and_clears_its_judgement() {
    let app = TestApp::spawn().await;
    let contest = a_contest(
        &app,
        "tdd-revised",
        "tdd_contest",
        json!({"problem_url": "https://x.test/p", "judging_criteria": ["tests first"]}),
    )
    .await;
    app.register_user("tdd_reviser").await;
    let entrant = user_id(&app, "tdd_reviser").await;
    enter(&app, contest, entrant).await;
    app.login("tdd_reviser").await;

    let first = json!({
        "artifact_url": "https://github.com/x/v1",
        "artifact_type": "repository",
        "summary": "first attempt",
    });
    assert_eq!(
        app.post("/api/tournaments/tdd-revised/submissions", &first)
            .await
            .status(),
        200
    );

    // A juror scores it.
    app.register_user("tdd_juror").await;
    let juror = user_id(&app, "tdd_juror").await;
    grant(&app, juror, "jury_tournament").await;
    let submission: Uuid =
        sqlx::query_scalar("SELECT id FROM tournament_submissions WHERE tournament_id = $1")
            .bind(contest)
            .fetch_one(&app.db)
            .await
            .unwrap();
    app.login("tdd_juror").await;
    let judged = app
        .post(
            &format!("/api/submissions/{submission}/judge"),
            &json!({"status": "accepted", "judge_score": 80}),
        )
        .await;
    assert_eq!(judged.status(), 200, "{}", judged.text().await.unwrap());

    let score: i32 = sqlx::query_scalar(
        "SELECT score FROM tournament_participants
          WHERE tournament_id = $1 AND participant_id = $2",
    )
    .bind(contest)
    .bind(entrant)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(score, 80, "a judgement must reach the leaderboard");

    // The entrant revises. The score belonged to what they replaced.
    app.login("tdd_reviser").await;
    let second = json!({
        "artifact_url": "https://github.com/x/v2",
        "artifact_type": "repository",
        "summary": "second attempt",
    });
    assert_eq!(
        app.post("/api/tournaments/tdd-revised/submissions", &second)
            .await
            .status(),
        200
    );

    let (count, status, judge_score): (i64, String, Option<i16>) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM tournament_submissions WHERE tournament_id = $1),
                status, judge_score
           FROM tournament_submissions WHERE tournament_id = $1",
    )
    .bind(contest)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(count, 1, "one answer per participant, revised in place");
    assert_eq!(status, "submitted");
    assert!(judge_score.is_none(), "a score belongs to what it judged");
}

#[tokio::test]
async fn refusing_an_entry_requires_a_reason() {
    let app = TestApp::spawn().await;
    let contest = a_contest(
        &app,
        "tdd-refused",
        "tdd_contest",
        json!({"problem_url": "https://x.test/p", "judging_criteria": ["tests first"]}),
    )
    .await;
    app.register_user("tdd_refused").await;
    enter(&app, contest, user_id(&app, "tdd_refused").await).await;
    app.login("tdd_refused").await;
    app.post(
        "/api/tournaments/tdd-refused/submissions",
        &json!({
            "artifact_url": "https://github.com/x/z",
            "artifact_type": "repository",
            "summary": "an attempt",
        }),
    )
    .await;

    app.register_user("tdd_judge2").await;
    let juror = user_id(&app, "tdd_judge2").await;
    grant(&app, juror, "jury_tournament").await;
    let submission: Uuid =
        sqlx::query_scalar("SELECT id FROM tournament_submissions WHERE tournament_id = $1")
            .bind(contest)
            .fetch_one(&app.db)
            .await
            .unwrap();
    app.login("tdd_judge2").await;

    let silent = app
        .post(
            &format!("/api/submissions/{submission}/judge"),
            &json!({"status": "rejected"}),
        )
        .await;
    assert_eq!(
        silent.status(),
        400,
        "refusing somebody's work without saying why is the one thing a contest must not do"
    );

    let spoken = app
        .post(
            &format!("/api/submissions/{submission}/judge"),
            &json!({"status": "rejected", "judge_notes": "les tests ne compilent pas"}),
        )
        .await;
    assert_eq!(spoken.status(), 200);
}

#[tokio::test]
async fn judging_is_a_competence_not_an_office() {
    let app = TestApp::spawn().await;
    let contest = a_contest(
        &app,
        "tdd-gated",
        "tdd_contest",
        json!({"problem_url": "https://x.test/p", "judging_criteria": ["tests first"]}),
    )
    .await;
    app.register_user("tdd_player").await;
    enter(&app, contest, user_id(&app, "tdd_player").await).await;
    app.login("tdd_player").await;
    app.post(
        "/api/tournaments/tdd-gated/submissions",
        &json!({
            "artifact_url": "https://github.com/x/w",
            "artifact_type": "repository",
            "summary": "an attempt",
        }),
    )
    .await;

    let submission: Uuid =
        sqlx::query_scalar("SELECT id FROM tournament_submissions WHERE tournament_id = $1")
            .bind(contest)
            .fetch_one(&app.db)
            .await
            .unwrap();

    // Still logged in as the entrant, who is not a juror.
    let resp = app
        .post(
            &format!("/api/submissions/{submission}/judge"),
            &json!({"status": "accepted", "judge_score": 100}),
        )
        .await;
    assert_eq!(resp.status(), 403);
}

// ═══════════════════════════════════════════════════════════════════
// Marathon
// ═══════════════════════════════════════════════════════════════════

/// A verified merged pull request, `days_ago` before now.
///
/// Every deliverable answers something — a slice or a challenge — so the
/// marathon work hangs off one shared training challenge rather than
/// floating free.
async fn a_merged_pr(app: &TestApp, user: Uuid, days_ago: i32) {
    let challenge: Uuid = sqlx::query_scalar(
        "INSERT INTO challenge_templates
            (title, description, instructions, skill_domain, difficulty, status, is_training)
         VALUES ('marathon upstream work', 'x', 'x', 'code', 2, 'published', TRUE)
         ON CONFLICT DO NOTHING
         RETURNING id",
    )
    .fetch_optional(&app.db)
    .await
    .unwrap()
    .unwrap_or(Uuid::nil());
    let challenge = if challenge.is_nil() {
        sqlx::query_scalar(
            "SELECT id FROM challenge_templates WHERE title = 'marathon upstream work'",
        )
        .fetch_one(&app.db)
        .await
        .unwrap()
    } else {
        challenge
    };

    sqlx::query(
        "INSERT INTO deliverables
            (user_id, challenge_id, artifact_type, artifact_url, verifiable_by,
             verification_status, verified_at)
         VALUES ($1, $2, 'pr_merged', 'https://github.com/x/y/pull/1', 'github_webhook',
                 'verified', NOW() - ($3 || ' days')::INTERVAL)",
    )
    .bind(user)
    .bind(challenge)
    .bind(days_ago.to_string())
    .execute(&app.db)
    .await
    .unwrap();
}

#[tokio::test]
async fn a_marathon_counts_what_was_merged_inside_its_window() {
    let app = TestApp::spawn().await;
    let contest = a_contest(
        &app,
        "marathon-2026",
        "marathon",
        json!({"target_merged_prs": 3}),
    )
    .await;
    app.register_user("marathoner").await;
    let runner = user_id(&app, "marathoner").await;
    enter(&app, contest, runner).await;

    // Two inside the window, one from before it opened.
    a_merged_pr(&app, runner, 0).await;
    a_merged_pr(&app, runner, 0).await;
    a_merged_pr(&app, runner, 40).await;

    let body: Value = app
        .get("/api/tournaments/marathon-2026/leaderboard")
        .await
        .json()
        .await
        .unwrap();
    let board = body["data"]["leaderboard"].as_array().unwrap();
    assert_eq!(
        board[0]["score"], 2,
        "work merged before the marathon opened is not marathon work"
    );
}

#[tokio::test]
async fn a_revoked_contribution_stops_counting() {
    let app = TestApp::spawn().await;
    let contest = a_contest(
        &app,
        "marathon-revoked",
        "marathon",
        json!({"target_merged_prs": 2}),
    )
    .await;
    app.register_user("marathoner_two").await;
    let runner = user_id(&app, "marathoner_two").await;
    enter(&app, contest, runner).await;
    a_merged_pr(&app, runner, 0).await;
    a_merged_pr(&app, runner, 0).await;

    sqlx::query(
        "UPDATE deliverables SET revoked_at = NOW()
          WHERE user_id = $1 AND id = (SELECT id FROM deliverables WHERE user_id = $1 LIMIT 1)",
    )
    .bind(runner)
    .execute(&app.db)
    .await
    .unwrap();

    let body: Value = app
        .get("/api/tournaments/marathon-revoked/leaderboard")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["leaderboard"][0]["score"], 1);
}

#[tokio::test]
async fn the_marathon_badge_says_what_it_was_for() {
    let app = TestApp::spawn().await;
    let contest = a_contest(
        &app,
        "marathon-badged",
        "marathon",
        json!({"target_merged_prs": 2}),
    )
    .await;
    app.register_user("marathoner_hero").await;
    let hero = user_id(&app, "marathoner_hero").await;
    app.register_user("marathoner_short").await;
    let short = user_id(&app, "marathoner_short").await;
    enter(&app, contest, hero).await;
    enter(&app, contest, short).await;

    a_merged_pr(&app, hero, 0).await;
    a_merged_pr(&app, hero, 0).await;
    a_merged_pr(&app, short, 0).await;

    app.register_user("marathon_admin").await;
    let admin = user_id(&app, "marathon_admin").await;

    skilluv_backend::services::contest::recompute_marathon_scores(&app.db, contest)
        .await
        .unwrap();
    let granted =
        skilluv_backend::services::contest::grant_marathon_badges(&app.db, contest, admin)
            .await
            .unwrap();
    assert_eq!(granted, 1, "one runner reached the target, one did not");

    let reason: String = sqlx::query_scalar(
        "SELECT ub.grant_reason FROM user_badges ub
           JOIN badge_rules r ON r.id = ub.rule_id
          WHERE ub.user_id = $1 AND r.slug = 'code-oss-marathon-hero'",
    )
    .bind(hero)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(
        reason.contains("marathon-badged") && reason.contains('2'),
        "a badge whose reason says nothing cannot be questioned: {reason}"
    );
}

// ═══════════════════════════════════════════════════════════════════
// Awards
// ═══════════════════════════════════════════════════════════════════

async fn an_edition(app: &TestApp, year: i16, status: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO award_editions (year, status) VALUES ($1, $2) RETURNING id")
        .bind(year)
        .bind(status)
        .fetch_one(&app.db)
        .await
        .unwrap()
}

#[tokio::test]
async fn the_eight_categories_are_public() {
    let app = TestApp::spawn().await;
    let body: Value = app
        .get("/api/awards/categories")
        .await
        .json()
        .await
        .unwrap();
    let categories = body["data"]["categories"].as_array().unwrap();
    assert_eq!(categories.len(), 8);
    for c in categories {
        assert!(!c["description"].as_str().unwrap().is_empty());
    }
}

#[tokio::test]
async fn a_nomination_must_make_a_case() {
    let app = TestApp::spawn().await;
    an_edition(&app, 2026, "nominations").await;
    app.register_user("nominator").await;
    let subject = user_id(&app, "nominator").await;
    app.login("nominator").await;

    let silent = app
        .post(
            "/api/awards/2026/nominations",
            &json!({
                "category_slug": "rookie-coder",
                "subject_id": subject,
                "citation": "   ",
            }),
        )
        .await;
    assert_eq!(silent.status(), 400, "voters cannot weigh a name");

    let argued = app
        .post(
            "/api/awards/2026/nominations",
            &json!({
                "category_slug": "rookie-coder",
                "subject_id": subject,
                "citation": "quatre contributions fusionnées en six mois, sans expérience préalable",
            }),
        )
        .await;
    assert_eq!(argued.status(), 200, "{}", argued.text().await.unwrap());
}

#[tokio::test]
async fn a_category_nominates_what_it_says_it_nominates() {
    let app = TestApp::spawn().await;
    an_edition(&app, 2027, "nominations").await;
    app.register_user("nominator_wrong").await;
    let person = user_id(&app, "nominator_wrong").await;
    app.login("nominator_wrong").await;

    // "Best Library" nominates a project. A person is not a library.
    let resp = app
        .post(
            "/api/awards/2027/nominations",
            &json!({
                "category_slug": "best-library-published",
                "subject_id": person,
                "citation": "une personne, pas une bibliothèque",
            }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn voting_opens_on_the_shortlist_not_on_every_nomination() {
    let app = TestApp::spawn().await;
    let edition = an_edition(&app, 2028, "nominations").await;
    app.register_user("award_subject").await;
    let subject = user_id(&app, "award_subject").await;
    app.login("award_subject").await;
    let created = app
        .post(
            "/api/awards/2028/nominations",
            &json!({
                "category_slug": "rookie-coder",
                "subject_id": subject,
                "citation": "une première année remarquable",
            }),
        )
        .await;
    let body: Value = created.json().await.unwrap();
    let nominee = body["data"]["nominee_id"].as_str().unwrap().to_string();

    sqlx::query("UPDATE award_editions SET status = 'voting' WHERE id = $1")
        .bind(edition)
        .execute(&app.db)
        .await
        .unwrap();

    app.register_user("award_voter").await;
    app.login("award_voter").await;
    let early = app
        .post(&format!("/api/awards/nominees/{nominee}/vote"), &json!({}))
        .await;
    assert_eq!(
        early.status(),
        400,
        "nothing is votable before the shortlist"
    );

    // A curator fixes the shortlist.
    app.register_user("award_curator").await;
    let curator = user_id(&app, "award_curator").await;
    grant(&app, curator, "community_curator").await;
    app.login("award_curator").await;
    let shortlisted = app
        .post(
            "/api/awards/nominees/shortlist",
            &json!({"nominee_ids": [nominee]}),
        )
        .await;
    assert_eq!(shortlisted.status(), 200);

    app.login("award_voter").await;
    let first = app
        .post(&format!("/api/awards/nominees/{nominee}/vote"), &json!({}))
        .await;
    assert_eq!(first.status(), 200, "{}", first.text().await.unwrap());

    let twice = app
        .post(&format!("/api/awards/nominees/{nominee}/vote"), &json!({}))
        .await;
    assert_eq!(twice.status(), 400, "one vote per person per category");
}

#[tokio::test]
async fn a_juror_is_also_a_member_of_the_community() {
    let app = TestApp::spawn().await;
    let edition = an_edition(&app, 2029, "nominations").await;
    app.register_user("juror_subject").await;
    let subject = user_id(&app, "juror_subject").await;
    app.login("juror_subject").await;
    let created: Value = app
        .post(
            "/api/awards/2029/nominations",
            &json!({
                "category_slug": "rookie-coder",
                "subject_id": subject,
                "citation": "une première année remarquable",
            }),
        )
        .await
        .json()
        .await
        .unwrap();
    let nominee = created["data"]["nominee_id"].as_str().unwrap().to_string();

    sqlx::query("UPDATE award_nominees SET shortlisted_at = NOW() WHERE id = $1::UUID")
        .bind(&nominee)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("UPDATE award_editions SET status = 'voting' WHERE id = $1")
        .bind(edition)
        .execute(&app.db)
        .await
        .unwrap();

    app.register_user("award_juror").await;
    let juror = user_id(&app, "award_juror").await;
    grant(&app, juror, "jury_tournament").await;
    app.login("award_juror").await;

    for query in ["", "?jury=true"] {
        let resp = app
            .post(
                &format!("/api/awards/nominees/{nominee}/vote{query}"),
                &json!({}),
            )
            .await;
        assert_eq!(
            resp.status(),
            200,
            "a juror keeps their community vote: {}",
            resp.text().await.unwrap()
        );
    }

    let (community, jury): (i64, i64) = sqlx::query_as(
        "SELECT community_votes, jury_votes FROM award_results WHERE nominee_id = $1::UUID",
    )
    .bind(&nominee)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!((community, jury), (1, 1));
}

#[tokio::test]
async fn eight_jurors_are_not_drowned_by_four_thousand_votes() {
    let app = TestApp::spawn().await;
    let edition = an_edition(&app, 2030, "voting").await;
    let category: Uuid =
        sqlx::query_scalar("SELECT id FROM award_categories WHERE slug = 'rookie-coder'")
            .fetch_one(&app.db)
            .await
            .unwrap();

    // Two nominees. The community prefers one; the jury, unanimously, the
    // other. With the weights applied to raw counts the jury would vanish.
    let mut nominees = Vec::new();
    for name in ["award_popular", "award_respected"] {
        app.register_user(name).await;
        let subject = user_id(&app, name).await;
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO award_nominees
                (edition_id, category_id, subject_type, subject_id, citation, shortlisted_at)
             VALUES ($1, $2, 'user', $3, 'x', NOW()) RETURNING id",
        )
        .bind(edition)
        .bind(category)
        .bind(subject)
        .fetch_one(&app.db)
        .await
        .unwrap();
        nominees.push(id);
    }

    for (index, (nominee, ballot, count)) in [
        (nominees[0], "community", 60),
        (nominees[1], "community", 40),
        (nominees[1], "jury", 5),
    ]
    .into_iter()
    .enumerate()
    {
        for n in 0..count {
            let voter = format!("voter_{index}_{n}");
            app.register_user(&voter).await;
            let voter_id = user_id(&app, &voter).await;
            sqlx::query(
                "INSERT INTO award_votes (nominee_id, voter_id, ballot, edition_id, category_id)
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(nominee)
            .bind(voter_id)
            .bind(ballot)
            .bind(edition)
            .bind(category)
            .execute(&app.db)
            .await
            .unwrap();
        }
    }

    let scores: Vec<(Uuid, bigdecimal::BigDecimal)> = sqlx::query_as(
        "SELECT nominee_id, weighted_score FROM award_results
          WHERE edition_id = $1 ORDER BY weighted_score DESC",
    )
    .bind(edition)
    .fetch_all(&app.db)
    .await
    .unwrap();

    // 0.6 * 70 = 42 against 0.4 * 70 + 1.0 * 30 = 58. The jury decides,
    // which is what a 30% weight is supposed to mean.
    assert_eq!(scores[0].0, nominees[1]);
}
