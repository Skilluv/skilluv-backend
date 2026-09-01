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
        .get("/api/orientations/counts")
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

/// `/counts` sits in front of `/{slug}` in the router; without this, a route
/// ordering change turns it into a lookup for an orientation called "counts".
#[tokio::test]
async fn counts_is_not_read_as_an_orientation_slug() {
    let app = common::TestApp::spawn().await;
    let body: serde_json::Value = app
        .get("/api/orientations/counts")
        .await
        .json()
        .await
        .unwrap();
    assert!(body["data"]["domains"].is_array());
}
