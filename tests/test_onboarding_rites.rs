//! The first screen after signing up (SKI-360, SKI-361, SKI-362, SKI-364).
//!
//! Four tickets that are one path: register, be shown a first gesture, hand
//! something in, and have it read. Each piece of it was broken in a way the
//! next piece hid — eight domains had no gesture, so nobody noticed that
//! handing in a hundred characters passed; and every gesture there was
//! demanded a GitHub account, so nobody but a developer got that far.

mod common;

use reqwest::StatusCode;
use serde_json::json;
use skilluv_backend::validators::SKILL_DOMAINS;

/// Give somebody the capability a verdict needs.
///
/// Since the gate below, `mentor` is the platform's cross-domain reviewer
/// capability. Granted straight into `user_capabilities` because the promotion
/// engine that normally awards it is not what these tests are about.
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

/// Choose a trade in `domain`, the way the signup screen does.
///
/// The rite refuses to start without one since the trade gate: it is what
/// picks the starter, feeds the recommendations, and matches a reviewer.
/// Resolved from the catalogue rather than hardcoded so a renamed slug does
/// not silently turn these into tests of the gate.
async fn choose_trade_in(app: &common::TestApp, domain: &str) {
    let slug: String = sqlx::query_scalar(
        "SELECT slug FROM orientations
          WHERE primary_domain = $1 AND is_curated AND NOT is_archived
          ORDER BY slug LIMIT 1",
    )
    .bind(domain)
    .fetch_one(&app.db)
    .await
    .expect("the domain has a trade");

    let resp = app
        .post(
            "/api/users/me/orientations",
            &json!({ "slug": slug, "mode": "active", "is_primary": true }),
        )
        .await;
    assert_eq!(
        resp.status(),
        StatusCode::CREATED,
        "could not choose {slug}"
    );
}

/// Every domain a person may declare has a rite, with no exclusion list.
///
/// The loop is over `SKILL_DOMAINS` on purpose: SKI-360 asks for eleven and
/// names `soft_skills` as out of scope, but `soft_skills` is an active row of
/// `skill_domains` and a value `/auth/register` accepts, so excluding it by
/// hand would leave one domain landing on a 404 and put back the hand-written
/// list the ticket exists to remove.
#[tokio::test]
async fn every_declarable_domain_has_an_onboarding_challenge() {
    let app = common::TestApp::spawn().await;
    app.register_user("riteseeker").await;
    app.login("riteseeker").await;

    for domain in SKILL_DOMAINS {
        let resp = app
            .get(&format!("/api/challenges/onboarding?domain={domain}"))
            .await;
        let status = resp.status();
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(
            status,
            StatusCode::OK,
            "domain {domain} has no onboarding challenge: {body}"
        );
        assert_eq!(body["data"]["challenge"]["skill_domain"], *domain);
        assert_eq!(
            body["data"]["challenge"]["is_domain_rite"], true,
            "domain {domain} answered with something that is not its rite"
        );
    }
}

/// Two calls in a row return the same brief.
///
/// `code` carries fifteen `is_onboarding` templates, one per starter, so
/// `LIMIT 1` on the flag alone returned an arbitrary one of them.
#[tokio::test]
async fn the_code_rite_is_the_same_challenge_twice() {
    let app = common::TestApp::spawn().await;
    app.register_user("stableseeker").await;
    app.login("stableseeker").await;

    let first: serde_json::Value = app
        .get("/api/challenges/onboarding?domain=code")
        .await
        .json()
        .await
        .unwrap();
    let second: serde_json::Value = app
        .get("/api/challenges/onboarding?domain=code")
        .await
        .json()
        .await
        .unwrap();

    assert_eq!(
        first["data"]["challenge"]["id"], second["data"]["challenge"]["id"],
        "the code rite changed between two reads"
    );
}

/// No rite carries a countdown.
///
/// `start_challenge` turns `duration_minutes` into an `expires_at`, and
/// `submit_challenge` marks anything arriving after it `failure`. Sixty
/// minutes — what migration 0607 first wrote — is impossible on the code rite,
/// whose gesture is a fork and a pull request, and arbitrary on the other
/// eleven, which are gestures somebody fits into their week (0610).
#[tokio::test]
async fn no_rite_puts_a_clock_on_the_first_gesture() {
    let app = common::TestApp::spawn().await;
    app.register_user("unhurried").await;
    app.login("unhurried").await;

    let timed: Vec<String> = sqlx::query_scalar(
        "SELECT skill_domain FROM challenge_templates
         WHERE is_domain_rite AND duration_minutes IS NOT NULL",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();
    assert!(timed.is_empty(), "these rites are on a clock: {timed:?}");

    // And the attempt it opens carries no deadline either.
    let challenge: serde_json::Value = app
        .get("/api/challenges/onboarding?domain=design")
        .await
        .json()
        .await
        .unwrap();
    let challenge_id = challenge["data"]["challenge"]["id"].as_str().unwrap();
    let start: serde_json::Value = app
        .post(&format!("/api/challenges/{challenge_id}/start"), &json!({}))
        .await
        .json()
        .await
        .unwrap();
    assert!(
        start["data"]["submission"]["expires_at"].is_null(),
        "the attempt was opened with a deadline: {start}"
    );
}

/// A domain nothing recognises is refused, rather than answered "no challenge
/// found" — which reads as "that trade does not exist here".
#[tokio::test]
async fn an_unknown_domain_is_a_bad_request_not_a_missing_challenge() {
    let app = common::TestApp::spawn().await;
    app.register_user("typoseeker").await;
    app.login("typoseeker").await;

    let resp = app.get("/api/challenges/onboarding?domain=desgin").await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ════════════════════════════════════════════════════════════════════
// SKI-361 — a hundred characters is not a pass
// ════════════════════════════════════════════════════════════════════

/// One filler submission per non-code domain never produces `success`, and
/// never credits a fragment.
///
/// This is the recette condition of SKI-361, run against the rite of each
/// domain — which is the challenge a new account is actually sent to, and
/// therefore the one the hole was reachable through.
#[tokio::test]
async fn filler_never_passes_a_domain_without_an_evaluator() {
    let app = common::TestApp::spawn().await;

    for domain in SKILL_DOMAINS.iter().filter(|d| **d != "code") {
        let username = format!("filler{domain}");
        app.register_user(&username).await;
        app.login(&username).await;

        let challenge: serde_json::Value = app
            .get(&format!("/api/challenges/onboarding?domain={domain}"))
            .await
            .json()
            .await
            .unwrap();
        let challenge_id = challenge["data"]["challenge"]["id"].as_str().unwrap();

        let start = app
            .post(&format!("/api/challenges/{challenge_id}/start"), &json!({}))
            .await;
        assert_eq!(
            start.status(),
            StatusCode::CREATED,
            "{domain}: start refused"
        );

        // A hundred and twenty characters of nothing — comfortably over the
        // old `code.len() >= 100` bar.
        let filler = "x".repeat(120);
        let resp = app
            .post(
                &format!("/api/challenges/{challenge_id}/submit"),
                &json!({ "code": filler }),
            )
            .await;
        assert_eq!(resp.status(), StatusCode::OK, "{domain}: submit refused");
        let body: serde_json::Value = resp.json().await.unwrap();

        assert_eq!(
            body["data"]["submission"]["status"], "pending_review",
            "{domain}: filler was scored instead of queued — {body}"
        );
        assert_eq!(
            body["data"]["fragments_earned"], 0,
            "{domain}: filler earned fragments — {body}"
        );
        assert_eq!(
            body["data"]["user"]["total_fragments"], 0,
            "{domain}: filler moved the user's total — {body}"
        );
        assert_eq!(
            body["data"]["user"]["profile_active"], false,
            "{domain}: filler activated a profile — {body}"
        );
    }
}

/// The submission is not lost: it becomes a deliverable a person is queued to
/// read. Without this, `pending_review` would be a polite way of dropping the
/// work on the floor.
#[tokio::test]
async fn a_pending_submission_reaches_the_review_queue() {
    let app = common::TestApp::spawn().await;
    app.register_user("designhand").await;
    app.login("designhand").await;

    let challenge: serde_json::Value = app
        .get("/api/challenges/onboarding?domain=design")
        .await
        .json()
        .await
        .unwrap();
    let challenge_id = challenge["data"]["challenge"]["id"].as_str().unwrap();

    app.post(&format!("/api/challenges/{challenge_id}/start"), &json!({}))
        .await;
    app.post(
        &format!("/api/challenges/{challenge_id}/submit"),
        &json!({ "code": "A login screen: one field, one button, one way back. ".repeat(3) }),
    )
    .await;

    let (status, tasks): (String, i64) = sqlx::query_as(
        "SELECT d.verification_status, COUNT(rt.id)
         FROM deliverables d
         LEFT JOIN review_tasks rt ON rt.deliverable_id = d.id
         WHERE d.user_id = (SELECT id FROM users WHERE username = 'designhand')
         GROUP BY d.verification_status",
    )
    .fetch_one(&app.db)
    .await
    .expect("the submission produced no deliverable");

    assert_eq!(status, "pending");
    assert_eq!(tasks, 1, "the deliverable is not in front of a reviewer");
}

/// An `approve` verdict is what turns the submission into a pass: fragments
/// arrive, the profile activates, and the rite completes.
///
/// The fragments come from the challenge template, not from a slice — a
/// challenge deliverable has no slice, so reading the reward only from
/// `project_slices` would have approved the work and awarded nothing.
#[tokio::test]
async fn an_approved_review_is_what_awards_the_fragments() {
    let app = common::TestApp::spawn().await;

    app.register_user("audiohand").await;
    app.login("audiohand").await;

    let challenge: serde_json::Value = app
        .get("/api/challenges/onboarding?domain=audio")
        .await
        .json()
        .await
        .unwrap();
    let challenge_id = challenge["data"]["challenge"]["id"].as_str().unwrap();
    let reward = challenge["data"]["challenge"]["reward_fragments"]
        .as_i64()
        .unwrap();

    app.post(&format!("/api/challenges/{challenge_id}/start"), &json!({}))
        .await;
    app.post(
        &format!("/api/challenges/{challenge_id}/submit"),
        &json!({ "code": "Twenty seconds of kora, recorded at home; the pad is my own synth patch." }),
    )
    .await;

    let deliverable_id: uuid::Uuid = sqlx::query_scalar(
        "SELECT id FROM deliverables
         WHERE user_id = (SELECT id FROM users WHERE username = 'audiohand')",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    app.register_user("audioreviewer").await;
    make_reviewer(&app, "audioreviewer").await;
    app.login("audioreviewer").await;
    let verdict = app
        .post(
            &format!("/api/deliverables/{deliverable_id}/reviews"),
            &json!({
                "verdict": "approve",
                "body": "The source list is complete and the signature is legible at low volume.",
            }),
        )
        .await;
    assert_eq!(verdict.status(), StatusCode::OK);

    let (submission_status, fragments, active): (String, i32, bool) = sqlx::query_as(
        "SELECT cs.status, u.total_fragments, u.profile_active
         FROM challenge_submissions cs
         JOIN users u ON u.id = cs.user_id
         WHERE u.username = 'audiohand'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    assert_eq!(submission_status, "success");
    assert_eq!(i64::from(fragments), reward);
    assert!(
        active,
        "an approved first gesture must activate the profile"
    );
}

// ════════════════════════════════════════════════════════════════════
// SKI-362 — eleven of the twelve gestures are not a fork
// ════════════════════════════════════════════════════════════════════

/// The catalogue the signup screen renders: twelve gestures, one of which
/// wants GitHub.
#[tokio::test]
async fn the_rite_catalogue_covers_every_domain_and_only_code_wants_github() {
    let app = common::TestApp::spawn().await;
    app.register_user("catalogueseeker").await;
    app.login("catalogueseeker").await;

    let body: serde_json::Value = app.get("/api/onboarding/rites").await.json().await.unwrap();
    let rites = body["data"]["rites"].as_array().expect("a rites array");
    assert_eq!(rites.len(), SKILL_DOMAINS.len());

    for rite in rites {
        let domain = rite["domain"].as_str().unwrap();
        assert!(SKILL_DOMAINS.contains(&domain));
        assert_eq!(
            rite["requires_github"],
            domain == "code",
            "{domain} has the wrong GitHub requirement"
        );
        assert!(
            rite["challenge_id"].is_string(),
            "{domain} has no published brief"
        );
        assert!(!rite["expected_artifact"].as_str().unwrap().is_empty());
    }
}

/// The whole ticket, in one assertion: a designer with no GitHub account
/// starts their rite.
#[tokio::test]
async fn a_designer_starts_the_rite_without_a_github_account() {
    let app = common::TestApp::spawn().await;
    app.register_user("designernogh").await;
    app.login("designernogh").await;
    choose_trade_in(&app, "design").await;

    let resp = app
        .post(
            "/api/onboarding/bonjour-skilluv/start?domain=design",
            &json!({}),
        )
        .await;
    let status = resp.status();
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "design rite refused: {body}");

    assert_eq!(body["data"]["onboarding"]["rite_form"], "submission");
    assert_eq!(body["data"]["onboarding"]["skill_domain"], "design");
    assert_eq!(body["data"]["onboarding"]["status"], "started");
    assert!(body["data"]["onboarding"]["fork_full_name"].is_null());
    assert!(body["data"]["next_steps"]["clone_url"].is_null());
    assert!(body["data"]["next_steps"]["challenge_id"].is_string());
}

/// `code` still forks, and still says so when there is no token — the one
/// domain where "connect GitHub" is the right answer.
#[tokio::test]
async fn the_code_rite_still_asks_for_github() {
    let app = common::TestApp::spawn().await;
    app.register_user("codernogh").await;
    app.login("codernogh").await;
    choose_trade_in(&app, "code").await;

    let resp = app
        .post(
            "/api/onboarding/bonjour-skilluv/start?domain=code",
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(
        body.to_string().contains("GitHub"),
        "the code rite should name GitHub: {body}"
    );
}

/// Starting twice does not start twice.
#[tokio::test]
async fn starting_a_submission_rite_is_idempotent() {
    let app = common::TestApp::spawn().await;
    app.register_user("twicehand").await;
    app.login("twicehand").await;
    choose_trade_in(&app, "quality").await;

    app.post(
        "/api/onboarding/bonjour-skilluv/start?domain=quality",
        &json!({}),
    )
    .await;
    let second: serde_json::Value = app
        .post(
            "/api/onboarding/bonjour-skilluv/start?domain=quality",
            &json!({}),
        )
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(second["data"]["already_started"], true);

    let rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM onboarding_bonjour_skilluv
         WHERE user_id = (SELECT id FROM users WHERE username = 'twicehand')",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(rows, 1);
}

/// Status describes the rite before anything is started — which is what lets
/// the front stop hardcoding eleven shapes.
#[tokio::test]
async fn status_describes_the_rite_before_it_starts() {
    let app = common::TestApp::spawn().await;
    app.register_user("beforestart").await;
    app.login("beforestart").await;

    let body: serde_json::Value = app
        .get("/api/onboarding/bonjour-skilluv/status")
        .await
        .json()
        .await
        .unwrap();

    assert_eq!(body["data"]["started"], false);
    assert!(body["data"]["onboarding"].is_null());
    // register_user declares `code`, so this caller's rite is the fork.
    assert_eq!(body["data"]["rite"]["domain"], "code");
    assert_eq!(body["data"]["rite"]["form"], "fork");
    assert_eq!(body["data"]["rite"]["requires_github"], true);
}

/// Handing in the brief moves the rite on, and the reviewer's verdict finishes
/// it — the submission form's equivalent of the webhook.
#[tokio::test]
async fn handing_in_the_brief_advances_then_completes_the_rite() {
    let app = common::TestApp::spawn().await;
    app.register_user("leadhand").await;
    app.login("leadhand").await;
    choose_trade_in(&app, "leadership").await;

    let start: serde_json::Value = app
        .post(
            "/api/onboarding/bonjour-skilluv/start?domain=leadership",
            &json!({}),
        )
        .await
        .json()
        .await
        .unwrap();
    let challenge_id = start["data"]["onboarding"]["challenge_id"]
        .as_str()
        .unwrap()
        .to_string();

    app.post(&format!("/api/challenges/{challenge_id}/start"), &json!({}))
        .await;
    app.post(
        &format!("/api/challenges/{challenge_id}/submit"),
        &json!({ "code": "The retry storm was possible because the client had no ceiling. Owner: ops." }),
    )
    .await;

    let status: String = sqlx::query_scalar(
        "SELECT status FROM onboarding_bonjour_skilluv
         WHERE user_id = (SELECT id FROM users WHERE username = 'leadhand')",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(status, "submitted");

    let deliverable_id: uuid::Uuid = sqlx::query_scalar(
        "SELECT id FROM deliverables
         WHERE user_id = (SELECT id FROM users WHERE username = 'leadhand')",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    app.register_user("leadreviewer").await;
    make_reviewer(&app, "leadreviewer").await;
    app.login("leadreviewer").await;
    app.post(
        &format!("/api/deliverables/{deliverable_id}/reviews"),
        &json!({ "verdict": "approve", "body": "Names the cause, and the owner is somebody." }),
    )
    .await;

    let (status, completed_at): (String, Option<chrono::DateTime<chrono::Utc>>) = sqlx::query_as(
        "SELECT status, completed_at FROM onboarding_bonjour_skilluv
         WHERE user_id = (SELECT id FROM users WHERE username = 'leadhand')",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(status, "completed");
    assert!(completed_at.is_some());
}

// ════════════════════════════════════════════════════════════════════
// SKI-364 — the catalogue says how big it is
// ════════════════════════════════════════════════════════════════════

/// The page says how many rows the filter matches, so a client can tell a full
/// page from the end of the catalogue. The default page is 50 against ~255
/// curated orientations, and without this it looked complete.
#[tokio::test]
async fn the_orientations_catalogue_carries_its_total() {
    let app = common::TestApp::spawn().await;

    let body: serde_json::Value = app.get("/api/orientations").await.json().await.unwrap();
    let page = body["data"]["orientations"].as_array().unwrap().len();
    let total = body["data"]["total"].as_i64().unwrap();

    assert_eq!(page, 50, "the default page size changed");
    assert!(
        total > page as i64,
        "the total ({total}) should exceed one page — otherwise this test proves nothing"
    );

    let db_total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM orientations WHERE is_curated AND NOT is_archived",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(total, db_total);
}

/// The total follows the filter rather than describing the whole table.
#[tokio::test]
async fn the_total_is_counted_under_the_same_filter_as_the_page() {
    let app = common::TestApp::spawn().await;

    let body: serde_json::Value = app
        .get("/api/orientations?domain=security&limit=200")
        .await
        .json()
        .await
        .unwrap();
    let page = body["data"]["orientations"].as_array().unwrap().len() as i64;
    assert_eq!(
        body["data"]["total"], page,
        "one unpaginated page of a filter must equal that filter's total"
    );

    let db_total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM orientations
         WHERE is_curated AND NOT is_archived AND primary_domain = 'security'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(body["data"]["total"], db_total);
}

/// The eleven class counts in one call rather than eleven, and never
/// hardcoded — which is what the front was doing, from a count of the
/// migrations, and which is wrong on the next orientation added.
#[tokio::test]
async fn the_counts_endpoint_answers_the_whole_catalogue_in_one_call() {
    let app = common::TestApp::spawn().await;

    let body: serde_json::Value = app
        .get("/api/orientation-counts")
        .await
        .json()
        .await
        .unwrap();
    let domains = body["data"]["domains"].as_array().unwrap();
    assert!(!domains.is_empty());

    let summed: i64 = domains.iter().map(|d| d["total"].as_i64().unwrap()).sum();
    assert_eq!(body["data"]["total"], summed);

    for entry in domains {
        let domain = entry["domain"].as_str().unwrap();
        assert!(
            SKILL_DOMAINS.contains(&domain),
            "{domain} is not a domain anybody can declare"
        );
        let expected: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM orientations
             WHERE is_curated AND NOT is_archived AND primary_domain = $1",
        )
        .bind(domain)
        .fetch_one(&app.db)
        .await
        .unwrap();
        assert_eq!(entry["total"], expected, "{domain} is miscounted");
    }

    // And it is the same number the catalogue reports, so the two endpoints
    // cannot disagree about the size of the same thing.
    let catalogue: serde_json::Value = app.get("/api/orientations").await.json().await.unwrap();
    assert_eq!(body["data"]["total"], catalogue["data"]["total"]);
}

/// The counts live off the `{slug}` namespace, so no orientation slug can ever
/// shadow them or be shadowed by them.
///
/// It was `/api/orientations/counts`, which reached this handler for the slug
/// spelled `counts` and handed the caller a payload the detail operation never
/// described. `/challenges/onboarding` has the same shape safely, because its
/// sibling parameter is a UUID; a slug is a free string.
#[tokio::test]
async fn the_counts_do_not_shadow_a_slug() {
    let app = common::TestApp::spawn().await;

    let counts: serde_json::Value = app
        .get("/api/orientation-counts")
        .await
        .json()
        .await
        .unwrap();
    assert!(counts["data"]["domains"].is_array());

    // And the detail route answers for the word that used to collide, the way
    // it answers for any slug nobody registered.
    let shadowed = app.get("/api/orientations/counts").await;
    assert_eq!(shadowed.status(), StatusCode::NOT_FOUND);
}

// ════════════════════════════════════════════════════════════════════
// Who may hand down a verdict
// ════════════════════════════════════════════════════════════════════

/// Nobody signs off their own rite.
///
/// Sharper since `approve` became what awards the fragments, settles the
/// submission and activates the profile: an unguarded self-review is a person
/// handing themselves the proof the platform exists to vouch for.
#[tokio::test]
async fn nobody_signs_off_their_own_rite() {
    let app = common::TestApp::spawn().await;
    app.register_user("selfjudge").await;
    make_reviewer(&app, "selfjudge").await;
    app.login("selfjudge").await;

    let challenge: serde_json::Value = app
        .get("/api/challenges/onboarding?domain=quality")
        .await
        .json()
        .await
        .unwrap();
    let challenge_id = challenge["data"]["challenge"]["id"].as_str().unwrap();
    app.post(&format!("/api/challenges/{challenge_id}/start"), &json!({}))
        .await;
    app.post(
        &format!("/api/challenges/{challenge_id}/submit"),
        &json!({ "code": "Steps: open the page, press save twice. Expected one row, got two." }),
    )
    .await;

    let deliverable_id: uuid::Uuid = sqlx::query_scalar(
        "SELECT id FROM deliverables
         WHERE user_id = (SELECT id FROM users WHERE username = 'selfjudge')",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    let resp = app
        .post(
            &format!("/api/deliverables/{deliverable_id}/reviews"),
            &json!({ "verdict": "approve", "body": "Looks good to me, obviously." }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    let (status, fragments, active): (String, i32, bool) = sqlx::query_as(
        "SELECT cs.status, u.total_fragments, u.profile_active
         FROM challenge_submissions cs JOIN users u ON u.id = cs.user_id
         WHERE u.username = 'selfjudge'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(status, "pending_review");
    assert_eq!(fragments, 0);
    assert!(!active);
}

/// A verdict is a competence, not a login.
///
/// P2.2 opened this to every authenticated user for the cold start and
/// deferred the gate. It stops being acceptable the moment a verdict is worth
/// something.
#[tokio::test]
async fn a_verdict_needs_more_than_an_account() {
    let app = common::TestApp::spawn().await;
    app.register_user("opshand").await;
    app.login("opshand").await;

    let challenge: serde_json::Value = app
        .get("/api/challenges/onboarding?domain=ops")
        .await
        .json()
        .await
        .unwrap();
    let challenge_id = challenge["data"]["challenge"]["id"].as_str().unwrap();
    app.post(&format!("/api/challenges/{challenge_id}/start"), &json!({}))
        .await;
    app.post(
        &format!("/api/challenges/{challenge_id}/submit"),
        &json!({ "code": "The availability SLO says nothing about how stale the data is." }),
    )
    .await;

    let deliverable_id: uuid::Uuid = sqlx::query_scalar(
        "SELECT id FROM deliverables
         WHERE user_id = (SELECT id FROM users WHERE username = 'opshand')",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    // A stranger with nothing but an account.
    app.register_user("passerby").await;
    app.login("passerby").await;
    let refused = app
        .post(
            &format!("/api/deliverables/{deliverable_id}/reviews"),
            &json!({ "verdict": "approve", "body": "Sure." }),
        )
        .await;
    assert_eq!(refused.status(), StatusCode::FORBIDDEN);

    // The same person, once they hold the capability.
    make_reviewer(&app, "passerby").await;
    let accepted = app
        .post(
            &format!("/api/deliverables/{deliverable_id}/reviews"),
            &json!({ "verdict": "approve", "body": "Names what the SLO misses, and what it costs." }),
        )
        .await;
    assert_eq!(accepted.status(), StatusCode::OK);
}

/// An approved pull request is what completes the code rite.
///
/// The webhook takes it to `pr_opened` and attaches the deliverable; nothing
/// used to move it further, so `badge_rules.bonjour_skilluv` — which fires on
/// `completed_at IS NOT NULL` — was unreachable on the only path that had
/// shipped. This exercises the settlement, not the webhook itself: the webhook
/// reads HELLO.md off GitHub, so the rows it writes are written here directly.
#[tokio::test]
async fn an_approved_pull_request_is_what_completes_the_code_rite() {
    let app = common::TestApp::spawn().await;
    app.register_user("forkhand").await;
    app.login("forkhand").await;

    let user_id: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM users WHERE username = 'forkhand'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    let (challenge_id, reward): (uuid::Uuid, i32) = sqlx::query_as(
        "SELECT id, reward_fragments FROM challenge_templates
         WHERE is_domain_rite AND skill_domain = 'code' AND status = 'published'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    let deliverable_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO deliverables
            (challenge_id, user_id, artifact_type, artifact_url, artifact_hash,
             verifiable_by, verification_status, fragments_awarded, public,
             submitted_at, created_at)
         VALUES ($1, $2, 'other', 'https://github.com/forkhand/starter-fullstack-rust/pull/1',
                 'deadbeef', 'human_review', 'pending', 0, TRUE, NOW(), NOW())
         RETURNING id",
    )
    .bind(challenge_id)
    .bind(user_id)
    .fetch_one(&app.db)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO onboarding_bonjour_skilluv
            (user_id, skill_domain, rite_form, challenge_id, deliverable_id,
             starter_slug, fork_full_name, fork_html_url, github_fork_id,
             status, pr_number, pr_url, pr_opened_at)
         VALUES ($1, 'code', 'fork', $2, $3,
                 'starter-fullstack-rust', 'forkhand/starter-fullstack-rust',
                 'https://github.com/forkhand/starter-fullstack-rust', 987654,
                 'pr_opened', 1,
                 'https://github.com/forkhand/starter-fullstack-rust/pull/1', NOW())",
    )
    .bind(user_id)
    .bind(challenge_id)
    .bind(deliverable_id)
    .execute(&app.db)
    .await
    .expect("the fork rite row must be writable in its own shape");

    app.register_user("codereviewer").await;
    make_reviewer(&app, "codereviewer").await;
    app.login("codereviewer").await;
    let verdict = app
        .post(
            &format!("/api/deliverables/{deliverable_id}/reviews"),
            &json!({ "verdict": "approve",
                     "body": "The HELLO.md says what they came to build, and the PR is on the right branch." }),
        )
        .await;
    assert_eq!(verdict.status(), StatusCode::OK);

    let (status, completed_at, fragments, active): (
        String,
        Option<chrono::DateTime<chrono::Utc>>,
        i32,
        bool,
    ) = sqlx::query_as(
        "SELECT o.status, o.completed_at, u.total_fragments, u.profile_active
         FROM onboarding_bonjour_skilluv o JOIN users u ON u.id = o.user_id
         WHERE u.username = 'forkhand'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    assert_eq!(status, "completed");
    assert!(completed_at.is_some(), "the badge rule reads completed_at");
    assert_eq!(fragments, reward);
    assert!(active, "an approved first gesture activates the profile");
}

// ════════════════════════════════════════════════════════════════════
// What a submission hands in besides its text
// ════════════════════════════════════════════════════════════════════

/// An upload somebody does not own cannot be attached to their submission.
///
/// The whole reason attachments are references rather than URLs: without the
/// ownership check, a reference is a URL with extra steps and anybody could be
/// reviewed on another candidate's screen.
#[tokio::test]
async fn a_submission_cannot_attach_what_it_does_not_own() {
    let app = common::TestApp::spawn().await;

    // One designer uploads.
    app.register_user("owner").await;
    app.login("owner").await;
    let owner_id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'owner'")
        .fetch_one(&app.db)
        .await
        .unwrap();
    let upload_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO design_upload_sessions
            (user_id, design_subtype, filename, content_type, declared_bytes,
             stored_bytes, part_size, part_count, storage_key, s3_upload_id,
             status, completed_at, expires_at)
         VALUES ($1, 'screen', 'screen.png', 'image/png', 4096, 4096,
                 5242880, 1, 'uploads/owner/screen.png', 's3-1',
                 'completed', NOW(), NOW() + INTERVAL '1 day')
         RETURNING id",
    )
    .bind(owner_id)
    .fetch_one(&app.db)
    .await
    .unwrap();

    // Somebody else tries to hand it in as their own.
    app.register_user("borrower").await;
    app.login("borrower").await;
    let challenge: serde_json::Value = app
        .get("/api/challenges/onboarding?domain=design")
        .await
        .json()
        .await
        .unwrap();
    let challenge_id = challenge["data"]["challenge"]["id"].as_str().unwrap();
    app.post(&format!("/api/challenges/{challenge_id}/start"), &json!({}))
        .await;

    let resp = app
        .post(
            &format!("/api/challenges/{challenge_id}/submit"),
            &json!({
                "code": "A sign-in screen: one field, one button, one way back.",
                "attachments": [format!("design_upload:{upload_id}")],
            }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    // And nothing was written: the refusal comes before the evaluation.
    let submissions: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM challenge_submissions cs JOIN users u ON u.id = cs.user_id
         WHERE u.username = 'borrower' AND cs.status <> 'in_progress'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(submissions, 0);
}

/// An owned upload rides along, and the reviewer can open it.
///
/// Both halves matter. Attaching it was impossible before; and `design_uploads`
/// is owner-scoped, so even attached, the reviewer got a 404 on the only thing
/// they were being asked to judge.
#[tokio::test]
async fn an_owned_attachment_reaches_the_reviewer() {
    let app = common::TestApp::spawn().await;
    app.register_user("screenhand").await;
    app.login("screenhand").await;

    let user_id: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM users WHERE username = 'screenhand'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    let upload_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO design_upload_sessions
            (user_id, design_subtype, filename, content_type, declared_bytes,
             stored_bytes, part_size, part_count, storage_key, s3_upload_id,
             status, completed_at, expires_at)
         VALUES ($1, 'screen', 'signin.png', 'image/png', 4096, 4096,
                 5242880, 1, 'uploads/screenhand/signin.png', 's3-2',
                 'completed', NOW(), NOW() + INTERVAL '1 day')
         RETURNING id",
    )
    .bind(user_id)
    .fetch_one(&app.db)
    .await
    .unwrap();

    let challenge: serde_json::Value = app
        .get("/api/challenges/onboarding?domain=design")
        .await
        .json()
        .await
        .unwrap();
    let challenge_id = challenge["data"]["challenge"]["id"].as_str().unwrap();
    app.post(&format!("/api/challenges/{challenge_id}/start"), &json!({}))
        .await;
    let submitted = app
        .post(
            &format!("/api/challenges/{challenge_id}/submit"),
            &json!({
                "code": "A sign-in screen: one field, one button, one way back.",
                "attachments": [format!("design_upload:{upload_id}")],
            }),
        )
        .await;
    assert_eq!(submitted.status(), StatusCode::OK);

    // The deliverable the reviewer opens names the artifact.
    let attachments: serde_json::Value = sqlx::query_scalar(
        "SELECT artifact_metadata -> 'attachments' FROM deliverables WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(attachments, json!([format!("design_upload:{upload_id}")]));

    // A stranger still cannot open it.
    app.register_user("nosy").await;
    app.login("nosy").await;
    let refused = app
        .get(&format!("/api/design/uploads/{upload_id}/download-url"))
        .await;
    assert_eq!(refused.status(), StatusCode::NOT_FOUND);

    // The reviewer can, because a task on it is open and they hold the
    // capability a verdict needs.
    app.register_user("screenreviewer").await;
    make_reviewer(&app, "screenreviewer").await;
    app.login("screenreviewer").await;
    let allowed = app
        .get(&format!("/api/design/uploads/{upload_id}/download-url"))
        .await;
    assert_eq!(
        allowed.status(),
        StatusCode::OK,
        "the reviewer cannot open what they are asked to judge"
    );
}

/// The rite catalogue says where a trade goes next, and does not pretend the
/// rite is routed there.
#[tokio::test]
async fn the_catalogue_names_where_the_trade_continues() {
    let app = common::TestApp::spawn().await;

    let body: serde_json::Value = app.get("/api/onboarding/rites").await.json().await.unwrap();
    for rite in body["data"]["rites"].as_array().unwrap() {
        assert!(
            rite["continues_in"].as_str().is_some_and(|v| !v.is_empty()),
            "{} says nothing about what comes next",
            rite["domain"]
        );
        assert!(
            rite.get("review_loop").is_none(),
            "review_loop claimed a routing that does not exist and is gone"
        );
    }
}

// ════════════════════════════════════════════════════════════════════
// What there is to do once the rite is passed
// ════════════════════════════════════════════════════════════════════

/// Every seeded challenge names the trade it belongs to.
///
/// `POST /admin/orientations/{slug}/challenges/publish` — the one surface that
/// opens a catalogue, one trade at a time — selects on `orientation_id`. Only
/// the 130 design seeds carried one, so it published nothing for eleven
/// domains of twelve, and a person who finished their first gesture met an
/// empty `GET /api/challenges` (migration 0612).
///
/// The rites themselves are deliberately excluded: a rite belongs to its
/// domain, not to one trade.
#[tokio::test]
async fn every_seeded_challenge_names_its_trade() {
    let app = common::TestApp::spawn().await;

    let orphans: Vec<(String, String)> = sqlx::query_as(
        "SELECT skill_domain, title FROM challenge_templates
          WHERE orientation_id IS NULL
            AND NOT is_domain_rite
            AND NOT is_onboarding
            AND status <> 'archived'
          ORDER BY skill_domain, title",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert!(
        orphans.is_empty(),
        "{} challenges no curator can open: {:?}",
        orphans.len(),
        &orphans[..orphans.len().min(8)]
    );
}

// ════════════════════════════════════════════════════════════════════
// Which starter a trade forks
// ════════════════════════════════════════════════════════════════════

/// Every curated trade resolves to a starter, read from the table.
///
/// The check this replaces lived in `onboarding.rs` and looped over a constant
/// of 32 slugs commented "snapshot au 2026-07-22" — a snapshot of what the
/// mapping already covered, so it compared the list to itself and could not
/// fail. It stayed green while the table grew to 150 and the other 118
/// orientations silently forked `starter-fullstack-node`, handing a
/// `compiler-language-developer` a Node fullstack app.
///
/// It is an integration test now, and that is the point: a unit test that
/// cannot fail is not worth the Postgres connection it saves.
#[tokio::test]
async fn every_curated_orientation_resolves_to_a_starter() {
    use skilluv_backend::routes::onboarding::resolve_starter;

    let app = common::TestApp::spawn().await;

    let trades: Vec<(String, Option<String>, String)> = sqlx::query_as(
        "SELECT slug, reviewer_group, primary_domain
           FROM orientations WHERE is_curated AND NOT is_archived
          ORDER BY primary_domain, slug",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert!(
        trades.len() > 100,
        "only {} trades read — the catalogue is not being walked",
        trades.len()
    );

    let unresolved: Vec<&str> = trades
        .iter()
        .filter(|(slug, group, domain)| resolve_starter(slug, group.as_deref(), domain).is_none())
        .map(|(slug, _, _)| slug.as_str())
        .collect();

    assert!(
        unresolved.is_empty(),
        "{} trades would fork the blind default: {:?}",
        unresolved.len(),
        &unresolved[..unresolved.len().min(10)]
    );
}

/// A starter that is named must be one that exists.
///
/// The resolver hands its answer to `gh::fork_repo` as
/// `skilluv-community/<slug>`, so a typo is a 404 at GitHub on somebody's
/// first gesture, discovered by them rather than by us.
#[tokio::test]
async fn every_starter_named_is_one_of_the_fifteen() {
    use skilluv_backend::routes::onboarding::resolve_starter;

    const STARTERS: &[&str] = &[
        "starter-data-python",
        "starter-devops",
        "starter-frontend-htmx",
        "starter-frontend-react",
        "starter-frontend-svelte",
        "starter-fullstack-go",
        "starter-fullstack-node",
        "starter-fullstack-python",
        "starter-fullstack-rust",
        "starter-game-bevy",
        "starter-game-godot",
        "starter-iot-esp32",
        "starter-mobile-flutter",
        "starter-mobile-kotlin",
        "starter-mobile-react-native",
    ];

    let app = common::TestApp::spawn().await;
    let trades: Vec<(String, Option<String>, String)> = sqlx::query_as(
        "SELECT slug, reviewer_group, primary_domain
           FROM orientations WHERE is_curated AND NOT is_archived",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    for (slug, group, domain) in &trades {
        if let Some(starter) = resolve_starter(slug, group.as_deref(), domain) {
            assert!(
                STARTERS.contains(&starter),
                "{slug} would fork '{starter}', which is not a starter repository"
            );
        }
    }
}
