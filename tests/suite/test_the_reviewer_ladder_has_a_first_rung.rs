//! The cold start stops needing a steward for every entrance, forever.
//!
//! A rite verdict wants `admin`, `mentor` or `domain_curator`. Only `mentor`
//! is granted automatically, and it wants five attestations received or three
//! mentorship sessions given, both of which need work somebody already
//! validated. No reviewer, no completed rite, no attestation, no mentor.
//!
//! Nothing broke that circle. It was filed as a design problem and it never
//! was one: `routes/onboarding.rs` queues the code rite's pull request as a
//! `human_review` deliverable like the other eleven, so all twelve domains sat
//! in the same circle.
//!
//! Migration 0624 adds `rite_reviewer:{domain}`, granted when somebody passes
//! the rite of that domain. Person one still needs a steward; person two can
//! be read by person one. These tests pin that, and pin how narrow it is.

use crate::common::TestApp;
use reqwest::StatusCode;
use serde_json::json;

/// Grants a capability the way the fixtures elsewhere in the suite do.
async fn grant(app: &TestApp, username: &str, capability: &str) {
    sqlx::query(
        "INSERT INTO user_capabilities (user_id, capability, granted_reason)
         SELECT id, $2, 'test fixture' FROM users WHERE username = $1",
    )
    .bind(username)
    .bind(capability)
    .execute(&app.db)
    .await
    .unwrap_or_else(|e| panic!("grant {capability} to {username}: {e}"));
}

/// Declares a trade, which `POST /onboarding/bonjour-skilluv/start` requires.
///
/// The same shape `test_onboarding_rites` uses. `mode` and `is_primary` are
/// not decoration: the gate looks for a row with `mode = 'active'`, so a POST
/// without them is accepted and then counts for nothing.
async fn choose_trade_in(app: &TestApp, domain: &str) {
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

/// Takes somebody from nothing to a rite awaiting a verdict.
///
/// Returns the deliverable a reviewer is asked to read.
async fn submit_the_rite(app: &TestApp, username: &str, domain: &str, body: &str) -> uuid::Uuid {
    app.register_user(username).await;
    app.login(username).await;
    choose_trade_in(app, domain).await;

    let start: serde_json::Value = app
        .post(
            &format!("/api/onboarding/bonjour-skilluv/start?domain={domain}"),
            &json!({}),
        )
        .await
        .json()
        .await
        .unwrap();
    let challenge_id = start["data"]["onboarding"]["challenge_id"]
        .as_str()
        .unwrap_or_else(|| panic!("no challenge_id for {username}: {start}"))
        .to_string();

    app.post(&format!("/api/challenges/{challenge_id}/start"), &json!({}))
        .await;
    app.post(
        &format!("/api/challenges/{challenge_id}/submit"),
        &json!({ "code": body }),
    )
    .await;

    sqlx::query_scalar(
        "SELECT d.id FROM deliverables d
           JOIN users u ON u.id = d.user_id
          WHERE u.username = $1",
    )
    .bind(username)
    .fetch_one(&app.db)
    .await
    .unwrap_or_else(|e| panic!("no deliverable for {username}: {e}"))
}

/// Waits for the capability the proof hooks grant after an approval.
///
/// `reviews::submit` spawns the recompute so a verdict does not wait on it, so
/// this is a real wait and not a formality. It fails loudly rather than
/// returning false, because a silent miss here reads as "the rung does not
/// exist" and that is the thing under test.
async fn await_capability(app: &TestApp, username: &str, capability: &str) {
    for _ in 0..50 {
        let held: bool = sqlx::query_scalar(
            "SELECT EXISTS (
               SELECT 1 FROM user_capabilities c
                 JOIN users u ON u.id = c.user_id
                WHERE u.username = $1 AND c.capability = $2
                  AND (c.revoked_at IS NULL))",
        )
        .bind(username)
        .bind(capability)
        .fetch_one(&app.db)
        .await
        .unwrap();
        if held {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    let all: Vec<String> = sqlx::query_scalar(
        "SELECT c.capability FROM user_capabilities c
           JOIN users u ON u.id = c.user_id WHERE u.username = $1",
    )
    .bind(username)
    .fetch_all(&app.db)
    .await
    .unwrap();
    panic!("{username} never received {capability}; holds {all:?}");
}

/// The circle, broken.
///
/// The first designer is read by a steward. The second is read by the first.
/// That second verdict is the whole point: before 0624 it was a 403, and the
/// only way past it was another steward, for every newcomer, forever.
#[tokio::test]
async fn whoever_passed_the_entrance_may_witness_the_next() {
    let app = TestApp::spawn().await;

    let first = submit_the_rite(
        &app,
        "firstdesigner",
        "design",
        "I am Ama. I draw. Here is a hand holding a hammer, in eight strokes.",
    )
    .await;

    // Person one needs a steward. That much is unavoidable and correct:
    // somebody has to read the first one.
    app.register_user("designsteward").await;
    grant(&app, "designsteward", "admin").await;
    app.login("designsteward").await;
    let resp = app
        .post(
            &format!("/api/deliverables/{first}/reviews"),
            &json!({ "verdict": "approve", "body": "It is hers, and it is drawn. Welcome." }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::OK, "the steward reads the first");

    await_capability(&app, "firstdesigner", "rite_reviewer:design").await;

    // Person two is read by person one.
    let second = submit_the_rite(
        &app,
        "seconddesigner",
        "design",
        "I am Kofi. I write interfaces. Ninety words, and none of them wasted.",
    )
    .await;

    app.login("firstdesigner").await;
    let resp = app
        .post(
            &format!("/api/deliverables/{second}/reviews"),
            &json!({ "verdict": "approve", "body": "Said in his own trade, and finished. In." }),
        )
        .await;
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "somebody who passed the design entrance must be able to read the next one"
    );

    let completed: bool = sqlx::query_scalar(
        "SELECT completed_at IS NOT NULL FROM onboarding_bonjour_skilluv
          WHERE user_id = (SELECT id FROM users WHERE username = 'seconddesigner')",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(
        completed,
        "a peer's verdict has to settle the rite for real"
    );
}

/// It opens one door and no other.
///
/// The capability names a domain, and the gate could have trusted that name.
/// It asks the template instead: what the holder passed is not what authorises
/// them, what they are being asked to read is. A rite reviewer let loose on
/// the ordinary review queue would be a newcomer signing off project work.
#[tokio::test]
async fn a_rite_reviewer_cannot_read_ordinary_work() {
    let app = TestApp::spawn().await;

    let rite = submit_the_rite(
        &app,
        "riterreader",
        "design",
        "I am Zoe. I animate. Ten seconds of a logo assembling itself.",
    )
    .await;
    app.register_user("steward2").await;
    grant(&app, "steward2", "admin").await;
    app.login("steward2").await;
    app.post(
        &format!("/api/deliverables/{rite}/reviews"),
        &json!({ "verdict": "approve", "body": "Hers, animated, done." }),
    )
    .await;
    await_capability(&app, "riterreader", "rite_reviewer:design").await;

    // An ordinary design deliverable, attached to no rite template.
    app.register_user("workhand").await;
    // An ordinary design brief, written here rather than looked up.
    //
    // Migration 0626 has since published six, so a lookup would now find one.
    // Writing it here anyway keeps this test about the gate: what it needs is
    // a deliverable that is not a rite, and depending on the ladder's contents
    // would make a change to that ladder able to break this.
    let brief: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO challenge_templates
             (title, description, instructions, skill_domain, difficulty,
              status, is_training)
         VALUES ('A real piece of design work',
                 'Not the entrance. Work delivered against a brief.',
                 'Do the work.', 'design', 3, 'published', TRUE)
         RETURNING id",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    // `deliverables_at_least_one_parent` refuses a row with neither a
    // challenge nor a slice, which is the schema saying a deliverable is
    // always delivered against something.
    let ordinary: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO deliverables
             (challenge_id, user_id, artifact_type, artifact_url, verifiable_by,
              verification_status, fragments_awarded, public, submitted_at)
         SELECT $1, id, 'other', 'https://example.com/a-real-piece-of-work',
                'human_review', 'pending', 0, TRUE, NOW()
           FROM users WHERE username = 'workhand'
         RETURNING id",
    )
    .bind(brief)
    .fetch_one(&app.db)
    .await
    .unwrap();

    app.login("riterreader").await;
    let resp = app
        .post(
            &format!("/api/deliverables/{ordinary}/reviews"),
            &json!({ "verdict": "approve", "body": "Looks fine to me." }),
        )
        .await;
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "passing an entrance is not a licence to sign off project work"
    );
}

/// And it opens it in one trade.
///
/// Against `ops` rather than `code`: the code rite is a fork rite and refuses
/// to start without a connected GitHub account, so it cannot be driven from
/// here. The rule under test is the domain scope, and `ops` exercises it the
/// same way.
#[tokio::test]
async fn a_design_rite_reviewer_is_not_an_ops_rite_reviewer() {
    let app = TestApp::spawn().await;

    let design_rite = submit_the_rite(
        &app,
        "crossdesigner",
        "design",
        "I am Nadia. I name things. Here are six names for one product, and why the fourth wins.",
    )
    .await;
    app.register_user("steward3").await;
    grant(&app, "steward3", "admin").await;
    app.login("steward3").await;
    app.post(
        &format!("/api/deliverables/{design_rite}/reviews"),
        &json!({ "verdict": "approve", "body": "Named, argued, hers." }),
    )
    .await;
    await_capability(&app, "crossdesigner", "rite_reviewer:design").await;

    let ops_rite = submit_the_rite(
        &app,
        "opsnewcomer",
        "ops",
        "I am Yao. The availability SLO says nothing about how stale the data is.",
    )
    .await;

    app.login("crossdesigner").await;
    let resp = app
        .post(
            &format!("/api/deliverables/{ops_rite}/reviews"),
            &json!({ "verdict": "approve", "body": "Seems right." }),
        )
        .await;
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "a design entrance says nothing about whether somebody can read ops work"
    );
}

/// Nobody lets themselves in, rung or no rung.
#[tokio::test]
async fn the_rung_does_not_let_anybody_sign_off_their_own_entrance() {
    let app = TestApp::spawn().await;

    let first = submit_the_rite(
        &app,
        "solohand",
        "design",
        "I am Sena. I illustrate. A goat, unimpressed, in three colours.",
    )
    .await;
    app.register_user("steward4").await;
    grant(&app, "steward4", "admin").await;
    app.login("steward4").await;
    app.post(
        &format!("/api/deliverables/{first}/reviews"),
        &json!({ "verdict": "approve", "body": "Unimpressed indeed. In." }),
    )
    .await;
    await_capability(&app, "solohand", "rite_reviewer:design").await;

    // A second pending rite deliverable of her own, written directly because
    // nobody passes the same rite twice. The already approved one would have
    // been refused as settled, by a rule that is not the one under test here,
    // and a 400 would have read as proof of something it does not prove.
    let own_pending: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO deliverables
             (challenge_id, user_id, artifact_type, artifact_url, verifiable_by,
              verification_status, fragments_awarded, public, submitted_at)
         SELECT (SELECT id FROM challenge_templates
                  WHERE skill_domain = 'design' AND status = 'published'
                    AND is_domain_rite LIMIT 1),
                id, 'other', 'https://example.com/a-second-goat',
                'human_review', 'pending', 0, TRUE, NOW()
           FROM users WHERE username = 'solohand'
         RETURNING id",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    app.login("solohand").await;
    let resp = app
        .post(
            &format!("/api/deliverables/{own_pending}/reviews"),
            &json!({ "verdict": "approve", "body": "I approve of me." }),
        )
        .await;
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "the self review rule comes first and the new rung must not slip under it"
    );
}

/// The catalogue carries one rung per declarable domain.
///
/// `validators::SKILL_DOMAINS` is the authority on which domains somebody may
/// declare, and migration 0624 writes the twelve out by hand. A domain added
/// to the constant without a rung would give that trade a rite nobody but an
/// admin can ever read, which is the bug this file exists to close.
#[tokio::test]
async fn every_declarable_domain_has_a_rung() {
    let app = TestApp::spawn().await;

    let listed: Vec<String> = sqlx::query_scalar(
        "SELECT scope FROM capability_catalog WHERE family = 'rite_reviewer' ORDER BY scope",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    let mut expected: Vec<String> = skilluv_backend::validators::SKILL_DOMAINS
        .iter()
        .map(|d| d.to_string())
        .collect();
    expected.sort();

    assert_eq!(
        listed, expected,
        "the rungs and the declarable domains have drifted apart"
    );
}
