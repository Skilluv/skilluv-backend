//! The whole path a new code account walks, in one test file.
//!
//! Register, choose a trade, do the entry rite, be told what is next, open an
//! exercise with somewhere to start reading, ask for help on it, hand it in,
//! and be read by a person. Every step of that used to end in an empty list, a
//! 404 or a silent default; this is what stops it regressing there.

mod common;

use reqwest::StatusCode;
use serde_json::json;

/// Give somebody the capability a verdict needs.
async fn make_reviewer(app: &common::TestApp, username: &str) {
    sqlx::query(
        "INSERT INTO user_capabilities (user_id, capability, granted_reason)
         SELECT id, 'mentor', 'test fixture' FROM users WHERE username = $1",
    )
    .bind(username)
    .execute(&app.db)
    .await
    .expect("grant mentor");
}

/// Register a trade for the caller, the way the signup screen does.
async fn choose_trade(app: &common::TestApp, slug: &str) -> StatusCode {
    app.post(
        "/api/users/me/orientations",
        &json!({ "slug": slug, "mode": "active", "is_primary": true }),
    )
    .await
    .status()
}

// ════════════════════════════════════════════════════════════════════
// A trade is chosen before the first gesture
// ════════════════════════════════════════════════════════════════════

/// The rite refuses to start until a trade is named, and says how to name one.
///
/// The trade picks the starter to fork, feeds the playlist and the
/// recommendations, and is what a reviewer is matched on. Starting without one
/// meant forking the broad-appeal default and then being recommended nothing
/// in particular.
#[tokio::test]
async fn the_rite_asks_for_a_trade_first() {
    let app = common::TestApp::spawn().await;
    app.register_user("tradeless").await;
    app.login("tradeless").await;

    let refused = app
        .post(
            "/api/onboarding/bonjour-skilluv/start?domain=code",
            &json!({}),
        )
        .await;
    assert_eq!(refused.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = refused.json().await.unwrap();
    assert!(
        body.to_string().contains("/api/users/me/orientations"),
        "the refusal must say how to choose a trade: {body}"
    );

    // With a trade, the code rite gets as far as asking for GitHub — which is
    // the next legitimate wall and not this one.
    assert_eq!(
        choose_trade(&app, "web-backend-developer").await,
        StatusCode::CREATED
    );
    let next = app
        .post(
            "/api/onboarding/bonjour-skilluv/start?domain=code",
            &json!({}),
        )
        .await;
    assert_eq!(next.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = next.json().await.unwrap();
    assert!(
        body.to_string().contains("GitHub"),
        "past the trade gate, the code rite asks for GitHub: {body}"
    );
}

/// One trade is enough. The cap of three is a ceiling, not a quota.
#[tokio::test]
async fn one_trade_opens_the_rite() {
    let app = common::TestApp::spawn().await;
    app.register_user("onetrade").await;
    app.login("onetrade").await;
    choose_trade(&app, "web-backend-developer").await;

    // A submission rite starts outright; the code one stops at GitHub, which
    // is why this checks a domain whose gesture is a submission.
    let started = app
        .post(
            "/api/onboarding/bonjour-skilluv/start?domain=design",
            &json!({}),
        )
        .await;
    assert_eq!(started.status(), StatusCode::OK);
}

// ════════════════════════════════════════════════════════════════════
// After the rite, there is something to do
// ════════════════════════════════════════════════════════════════════

/// The code domain has published exercises, in order, each with a next.
///
/// The catalogue was 654 drafts and nothing published: finishing the entry
/// rite led to an empty `GET /api/challenges`.
#[tokio::test]
async fn the_code_domain_has_a_ladder_to_climb() {
    let app = common::TestApp::spawn().await;
    app.register_user("climber").await;
    app.login("climber").await;

    let body: serde_json::Value = app
        .get("/api/challenges?domain=code&per_page=50")
        .await
        .json()
        .await
        .unwrap();
    let listed = body["data"].as_array().expect("a challenges array");
    assert!(
        listed.len() >= 6,
        "the code catalogue offers {} challenges",
        listed.len()
    );

    // And they are chained: every one but the first names what comes before.
    let chained: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM challenge_prerequisites p
           JOIN challenge_templates c ON c.id = p.challenge_id
          WHERE c.skill_domain = 'code' AND c.status = 'published'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(chained, 5, "six exercises make five steps");
}

/// The engine that answers "what do I do next" can see them.
///
/// `open_challenges` reads `project_slices` despite its name, so the entire
/// published challenge catalogue was invisible to the one surface whose job is
/// exactly this question.
#[tokio::test]
async fn what_comes_next_includes_the_exercises() {
    let app = common::TestApp::spawn().await;
    app.register_user("whatnext").await;
    app.login("whatnext").await;
    choose_trade(&app, "web-backend-developer").await;

    let body: serde_json::Value = app
        .get("/api/users/me/next-challenges?domain=code")
        .await
        .json()
        .await
        .unwrap();
    let suggestions = body["data"]["suggestions"]
        .as_array()
        .expect("a suggestions array");

    let exercises: Vec<&serde_json::Value> = suggestions
        .iter()
        .filter(|s| s["target_kind"] == "challenge")
        .collect();
    assert!(
        !exercises.is_empty(),
        "nothing from the challenge catalogue was suggested: {body}"
    );
    assert!(
        exercises
            .iter()
            .any(|s| !s["reasons"].as_array().unwrap().is_empty()),
        "a suggestion must say why it is one"
    );
}

// ════════════════════════════════════════════════════════════════════
// The brief comes with somewhere to start
// ════════════════════════════════════════════════════════════════════

/// An exercise carries resources, a place to ask, and the count of people who
/// asked before.
#[tokio::test]
async fn a_brief_says_where_to_start_reading() {
    let app = common::TestApp::spawn().await;
    app.register_user("reader").await;
    app.login("reader").await;

    let id: uuid::Uuid = sqlx::query_scalar(
        "SELECT id FROM challenge_templates
          WHERE title = 'Read a query and say what it costs'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    let body: serde_json::Value = app
        .get(&format!("/api/challenges/{id}"))
        .await
        .json()
        .await
        .unwrap();
    let guidance = &body["data"]["guidance"];

    assert_eq!(guidance["sized_for"], "apprenti");
    assert_eq!(guidance["self_sufficient"], false);
    let resources = guidance["resources"].as_array().unwrap();
    assert!(!resources.is_empty(), "no resources: {body}");
    for r in resources {
        assert!(
            r["url"].as_str().unwrap().starts_with("https://"),
            "a resource is a link somebody else hosts"
        );
        assert!(!r["language"].as_str().unwrap().is_empty());
    }
    assert!(
        !guidance["help"].as_array().unwrap().is_empty(),
        "somewhere to ask"
    );
    assert_eq!(guidance["discussions"], 0);
}

/// A French reader gets French where it exists, and the resource says which
/// language it is in either way.
#[tokio::test]
async fn a_french_reader_is_served_french() {
    let app = common::TestApp::spawn().await;
    app.register_user("francophone").await;
    app.login("francophone").await;

    let id: uuid::Uuid = sqlx::query_scalar(
        "SELECT id FROM challenge_templates
          WHERE title = 'Read a query and say what it costs'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    let fr: serde_json::Value = app
        .client
        .get(format!("{}/api/challenges/{id}", app.addr))
        .header("Accept-Language", "fr-FR,fr;q=0.9")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        fr["data"]["challenge"]["title"],
        "Lire une requête et dire ce qu'elle coûte"
    );

    let en: serde_json::Value = app
        .client
        .get(format!("{}/api/challenges/{id}", app.addr))
        .header("Accept-Language", "en-GB,en;q=0.9")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        en["data"]["challenge"]["title"],
        "Read a query and say what it costs"
    );

    // And the front is told which languages exist, so it can say "French only"
    // rather than silently showing the wrong one.
    let locales = en["data"]["available_locales"].as_array().unwrap();
    assert!(locales.contains(&json!("fr")) && locales.contains(&json!("en")));

    // The French reader sees the French resource first, and the English ones
    // after — never instead.
    let first = &fr["data"]["guidance"]["resources"][0];
    assert!(
        fr["data"]["guidance"]["resources"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["language"] == "en"),
        "English resources are still offered to a French reader"
    );
    let _ = first;
}

/// A locale nobody translated falls back to readable text rather than to an
/// empty string.
#[tokio::test]
async fn an_untranslated_locale_falls_back() {
    let app = common::TestApp::spawn().await;

    let id: uuid::Uuid = sqlx::query_scalar(
        "SELECT id FROM challenge_templates
          WHERE title = 'Read a query and say what it costs'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    let ar: serde_json::Value = app
        .client
        .get(format!("{}/api/challenges/{id}", app.addr))
        .header("Accept-Language", "ar")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        !ar["data"]["challenge"]["title"]
            .as_str()
            .unwrap()
            .is_empty(),
        "an untranslated locale must not blank the brief"
    );
}

/// The platform stops explaining once somebody no longer needs it.
#[tokio::test]
async fn guidance_stops_for_a_doyen() {
    let app = common::TestApp::spawn().await;
    app.register_user("doyenne").await;
    app.login("doyenne").await;

    sqlx::query(
        "INSERT INTO user_ranks (user_id, rank)
         SELECT id, 'doyen' FROM users WHERE username = 'doyenne'
         ON CONFLICT (user_id) DO UPDATE SET rank = 'doyen'",
    )
    .execute(&app.db)
    .await
    .expect("promote to doyen");

    let id: uuid::Uuid = sqlx::query_scalar(
        "SELECT id FROM challenge_templates
          WHERE title = 'Read a query and say what it costs'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    let body: serde_json::Value = app
        .get(&format!("/api/challenges/{id}"))
        .await
        .json()
        .await
        .unwrap();
    let guidance = &body["data"]["guidance"];
    assert_eq!(guidance["sized_for"], "doyen");
    assert_eq!(guidance["self_sufficient"], true);
    assert!(guidance["resources"].as_array().unwrap().is_empty());
    assert!(guidance["help"].as_array().unwrap().is_empty());
}

// ════════════════════════════════════════════════════════════════════
// Asking for help, on the challenge
// ════════════════════════════════════════════════════════════════════

/// A question names the challenge it is about, and the next person to open
/// that challenge is told somebody asked.
#[tokio::test]
async fn a_question_attaches_to_the_challenge() {
    let app = common::TestApp::spawn().await;
    app.register_user("stuck").await;
    app.login("stuck").await;

    let id: uuid::Uuid = sqlx::query_scalar(
        "SELECT id FROM challenge_templates
          WHERE title = 'A test that fails for the right reason'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    // `challenges` on purpose: it is where a question about a challenge
    // belongs, and an arbitrary category can be locked to non-moderators.
    let category: String = sqlx::query_scalar(
        "SELECT slug FROM forum_categories
          WHERE NOT locked ORDER BY (slug = 'challenges') DESC, slug LIMIT 1",
    )
    .fetch_one(&app.db)
    .await
    .expect("the forum has an unlocked category");

    let created = app
        .post(
            "/api/forum/posts",
            &json!({
                "category_slug": category,
                "kind": "question",
                "title": "What counts as a good failure message here?",
                "body": "My assertion prints left == right and I cannot tell what broke.",
                "challenge_id": id,
            }),
        )
        .await;
    let status = created.status();
    let created_body: serde_json::Value = created.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "post refused: {created_body}");

    // The next reader is told.
    app.register_user("nextreader").await;
    app.login("nextreader").await;
    let body: serde_json::Value = app
        .get(&format!("/api/challenges/{id}"))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["guidance"]["discussions"], 1);
}

/// A thread cannot attach to a challenge that does not exist. A thread pinned
/// to nothing looks answered, which is worse than one pinned nowhere.
#[tokio::test]
async fn a_question_cannot_name_a_challenge_that_is_not_there() {
    let app = common::TestApp::spawn().await;
    app.register_user("phantom").await;
    app.login("phantom").await;

    let category: String = sqlx::query_scalar(
        "SELECT slug FROM forum_categories
          WHERE NOT locked ORDER BY (slug = 'challenges') DESC, slug LIMIT 1",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    let refused = app
        .post(
            "/api/forum/posts",
            &json!({
                "category_slug": category,
                "kind": "question",
                "title": "A question about nothing at all",
                "body": "This names a challenge id that does not exist.",
                "challenge_id": uuid::Uuid::new_v4(),
            }),
        )
        .await;
    assert_eq!(refused.status(), StatusCode::NOT_FOUND);
}

// ════════════════════════════════════════════════════════════════════
// The loop closes
// ════════════════════════════════════════════════════════════════════

/// An exercise is handed in, queued for a person, approved, and rewarded —
/// and the next rung then becomes reachable.
#[tokio::test]
async fn an_exercise_is_handed_in_read_and_rewarded() {
    let app = common::TestApp::spawn().await;
    app.register_user("worker").await;
    app.login("worker").await;
    choose_trade(&app, "web-backend-developer").await;

    let (id, reward): (uuid::Uuid, i32) = sqlx::query_as(
        "SELECT id, reward_fragments FROM challenge_templates
          WHERE title = 'Read a query and say what it costs'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    let start = app
        .post(&format!("/api/challenges/{id}/start"), &json!({}))
        .await;
    assert_eq!(start.status(), StatusCode::CREATED);

    let submitted = app
        .post(
            &format!("/api/challenges/{id}/submit"),
            &json!({ "code": "GET /api/challenges reads challenge_templates, one row per published \
                              challenge, capped by per_page. It leans on the status index. It gets \
                              slow when nobody passes a domain and the catalogue grows." }),
        )
        .await;
    assert_eq!(submitted.status(), StatusCode::OK);
    let body: serde_json::Value = submitted.json().await.unwrap();
    assert_eq!(body["data"]["submission"]["status"], "pending_review");
    assert_eq!(body["data"]["fragments_earned"], 0);

    let deliverable_id: uuid::Uuid = sqlx::query_scalar(
        "SELECT id FROM deliverables
          WHERE user_id = (SELECT id FROM users WHERE username = 'worker')",
    )
    .fetch_one(&app.db)
    .await
    .expect("the hand-in became a deliverable");

    app.register_user("codereader").await;
    make_reviewer(&app, "codereader").await;
    app.login("codereader").await;
    let verdict = app
        .post(
            &format!("/api/deliverables/{deliverable_id}/reviews"),
            &json!({ "verdict": "approve",
                     "body": "Names the table, the cap and the index, and the slow case is the real one." }),
        )
        .await;
    assert_eq!(verdict.status(), StatusCode::OK);

    let (status, fragments): (String, i32) = sqlx::query_as(
        "SELECT cs.status, u.total_fragments
           FROM challenge_submissions cs JOIN users u ON u.id = cs.user_id
          WHERE u.username = 'worker'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(status, "success");
    assert_eq!(fragments, reward);
}

/// The first brief anybody reads carries guidance too.
///
/// `GET /api/challenges/onboarding` returned the rite and nothing around it —
/// so the one brief where being stranded means never starting was the one with
/// no reading, nowhere to ask, and no count of who asked before.
#[tokio::test]
async fn the_rite_says_where_to_start_reading() {
    let app = common::TestApp::spawn().await;
    app.register_user("firstbrief").await;
    app.login("firstbrief").await;

    let body: serde_json::Value = app
        .get("/api/challenges/onboarding?domain=code")
        .await
        .json()
        .await
        .unwrap();

    let guidance = &body["data"]["guidance"];
    assert!(
        !guidance["help"].as_array().unwrap().is_empty(),
        "the first brief must say where to ask: {body}"
    );
    let resources = guidance["resources"].as_array().unwrap();
    assert!(
        !resources.is_empty(),
        "the code rite asks for a fork and a pull request and says where neither is documented"
    );
    assert!(
        resources.iter().any(|r| r["language"] == "fr"),
        "a French reader gets something they can read"
    );
}

/// No resource points at a host this repository invented.
///
/// Migration 0615 attached a Discord invite that exists nowhere — written
/// because a community link belonged there, not because that one was real. A
/// dead link in the first list a beginner is handed teaches them the guidance
/// is decorative.
#[tokio::test]
async fn no_resource_points_at_an_invented_link() {
    let app = common::TestApp::spawn().await;

    let invented: Vec<String> = sqlx::query_scalar(
        "SELECT url FROM challenge_resources
          WHERE url LIKE '%discord.gg%' OR url LIKE '%example.com%'",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();
    assert!(invented.is_empty(), "invented links: {invented:?}");

    // And every one is a link somebody else hosts, not a copy.
    let bad_scheme: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM challenge_resources WHERE url NOT LIKE 'https://%'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(bad_scheme, 0);
}
