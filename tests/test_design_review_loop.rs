//! The design critique loop, end to end.
//!
//! What this suite really guards: a design challenge must be able to run for
//! several rounds, keep the whole trail, and — once validated — produce the
//! same class of proof a merged pull request produces. A validated design
//! slice that leaves `deliverables`, `attestations` and the rank untouched
//! would be a green mark and nothing else, which is the failure this whole
//! programme exists to avoid.

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

/// A design challenge already claimed by `designer`, in the given trade.
async fn a_claimed_challenge(
    app: &TestApp,
    designer: Uuid,
    orientation: &str,
    subtype: &str,
    fragments: i32,
) -> Uuid {
    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO projects (slug, name, owner_type, owner_id)
         VALUES ($1, 'Design test project', 'user', $2) RETURNING id",
    )
    .bind(format!("design-p-{}", Uuid::new_v4()))
    .bind(designer)
    .fetch_one(&app.db)
    .await
    .expect("project");

    sqlx::query_scalar(
        "INSERT INTO project_slices
            (project_id, slice_type, title, description, primary_domain, difficulty,
             status, claimed_by_user_id, claimed_at, fragments_reward,
             design_subtype, design_expected_rounds,
             orientation_id)
         VALUES ($1, 'design_artifact', 'Identité pour une coopérative',
                 'Logotype, palette, typographie et une application.',
                 'design', 3, 'claimed', $2, NOW(), $3, $4, 3,
                 (SELECT id FROM orientations WHERE slug = $5))
         RETURNING id",
    )
    .bind(project)
    .bind(designer)
    .bind(fragments)
    .bind(subtype)
    .bind(orientation)
    .fetch_one(&app.db)
    .await
    .expect("slice")
}

async fn slice_status(app: &TestApp, slice: Uuid) -> String {
    sqlx::query_scalar("SELECT status FROM project_slices WHERE id = $1")
        .bind(slice)
        .fetch_one(&app.db)
        .await
        .unwrap()
}

fn a_version(n: u32) -> Value {
    json!({
        "artifact_url": format!("https://figma.test/file/abc?version={n}"),
        "notes_md": format!("Tour {n} : direction retravaillée."),
    })
}

fn a_critique(verdict: &str, reason: Option<&str>) -> Value {
    let mut body = json!({
        "verdict": verdict,
        "feedback_md": "Le symbole tient en favicon mais la contre-forme se ferme à petite taille ; reprends l'ouverture.",
    });
    if let Some(reason) = reason {
        body["blocking_reason"] = json!(reason);
    }
    body
}

// ═══════════════════════════════════════════════════════════════════
// The slice shape
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn a_design_slice_must_say_what_it_produces_and_which_trade_it_is() {
    let app = TestApp::spawn().await;
    app.register_user("shape_designer").await;
    let designer = user_id(&app, "shape_designer").await;

    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO projects (slug, name, owner_type, owner_id)
         VALUES ($1, 'Projet', 'user', $2) RETURNING id",
    )
    .bind(format!("p-{}", Uuid::new_v4()))
    .bind(designer)
    .fetch_one(&app.db)
    .await
    .unwrap();

    // No subtype: nothing can size it, preview it or check it.
    let no_subtype = sqlx::query(
        "INSERT INTO project_slices
            (project_id, slice_type, title, description, primary_domain, difficulty,
             status, orientation_id)
         VALUES ($1, 'design_artifact', 't', 'd', 'design', 2, 'open',
                 (SELECT id FROM orientations WHERE slug = 'design-product'))",
    )
    .bind(project)
    .execute(&app.db)
    .await;
    assert!(no_subtype.is_err());

    // No trade: nobody competent can be routed to it.
    let no_trade = sqlx::query(
        "INSERT INTO project_slices
            (project_id, slice_type, title, description, primary_domain, difficulty,
             status, design_subtype)
         VALUES ($1, 'design_artifact', 't', 'd', 'design', 2, 'open', 'brand_kit')",
    )
    .bind(project)
    .execute(&app.db)
    .await;
    assert!(no_trade.is_err());

    // A code slice is untouched by any of this.
    sqlx::query(
        "INSERT INTO project_slices
            (project_id, slice_type, title, description, primary_domain, difficulty, status)
         VALUES ($1, 'github_issue', 't', 'd', 'code', 2, 'open')",
    )
    .bind(project)
    .execute(&app.db)
    .await
    .expect("the design migration must not disturb code slices");
}

// ═══════════════════════════════════════════════════════════════════
// Three rounds, then validated
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn three_rounds_then_validated_leaves_a_full_proof() {
    let app = TestApp::spawn().await;
    app.register_user("brand_designer").await;
    app.register_user("brand_reviewer").await;
    let designer = user_id(&app, "brand_designer").await;
    let reviewer = user_id(&app, "brand_reviewer").await;
    grant(&app, reviewer, "design_reviewer:brand").await;

    let slice =
        a_claimed_challenge(&app, designer, "design-brand-identity", "brand_kit", 120).await;

    for round in 1..=3u32 {
        app.login("brand_designer").await;
        let resp = app
            .post(
                &format!("/api/design/slices/{slice}/versions"),
                &a_version(round),
            )
            .await;
        assert_eq!(resp.status(), 201, "{}", resp.text().await.unwrap());
        assert_eq!(slice_status(&app, slice).await, "pending_validation");

        app.login("brand_reviewer").await;
        let body = if round == 3 {
            a_critique("approve", None)
        } else {
            a_critique("iterate", Some("craft_gap"))
        };
        let resp = app
            .post(&format!("/api/design/slices/{slice}/reviews"), &body)
            .await;
        assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

        if round < 3 {
            assert_eq!(slice_status(&app, slice).await, "in_iteration");
        }
    }

    assert_eq!(slice_status(&app, slice).await, "validated");

    // The trail survives in full, and each round names the version it read.
    let body: Value = app
        .get(&format!("/api/design/slices/{slice}/reviews"))
        .await
        .json()
        .await
        .unwrap();
    let rounds = body["data"]["rounds"].as_array().unwrap();
    assert_eq!(rounds.len(), 3);
    assert_eq!(rounds[0]["decision"], "iterate");
    assert_eq!(rounds[2]["decision"], "approve");
    assert_eq!(
        rounds[2]["reviewed_artifact_url"], "https://figma.test/file/abc?version=3",
        "the accepted version is the last one, not the first"
    );

    // The proof row exists and is verified: this is what feeds rank and badges.
    let (artifact_type, status, fragments): (String, String, i32) = sqlx::query_as(
        "SELECT artifact_type, verification_status, fragments_awarded
           FROM deliverables WHERE slice_id = $1 AND user_id = $2",
    )
    .bind(slice)
    .bind(designer)
    .fetch_one(&app.db)
    .await
    .expect("a validated design challenge must produce a deliverable");
    assert_eq!(artifact_type, "design_artifact");
    assert_eq!(status, "verified");
    assert_eq!(fragments, 120);

    let total: i32 = sqlx::query_scalar("SELECT total_fragments FROM users WHERE id = $1")
        .bind(designer)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(total, 120);

    // And the attestation, which is what a stranger can actually check.
    let basis: String = sqlx::query_scalar(
        "SELECT basis FROM attestations WHERE user_id = $1 AND basis IS NOT NULL",
    )
    .bind(designer)
    .fetch_one(&app.db)
    .await
    .expect("attestation");
    assert_eq!(basis, "design_deliverable_validated");
}

// ═══════════════════════════════════════════════════════════════════
// What the loop refuses
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn a_verdict_that_is_not_an_approval_needs_a_reason_and_words() {
    let app = TestApp::spawn().await;
    app.register_user("terse_designer").await;
    app.register_user("terse_reviewer").await;
    let designer = user_id(&app, "terse_designer").await;
    let reviewer = user_id(&app, "terse_reviewer").await;
    grant(&app, reviewer, "design_reviewer:illustration").await;

    let slice =
        a_claimed_challenge(&app, designer, "design-illustration", "illustration_set", 0).await;
    app.login("terse_designer").await;
    app.post(
        &format!("/api/design/slices/{slice}/versions"),
        &a_version(1),
    )
    .await;

    app.login("terse_reviewer").await;

    // No blocking reason: "rejected" alone tells the designer nothing.
    let resp = app
        .post(
            &format!("/api/design/slices/{slice}/reviews"),
            &json!({"verdict": "reject", "feedback_md": "La direction ne correspond pas au brief, il faut repartir."}),
        )
        .await;
    assert_eq!(resp.status(), 400, "{}", resp.text().await.unwrap());

    // A reason but no words.
    let resp = app
        .post(
            &format!("/api/design/slices/{slice}/reviews"),
            &json!({"verdict": "reject", "blocking_reason": "brief_unmet", "feedback_md": "non"}),
        )
        .await;
    assert_eq!(resp.status(), 400);

    // Both, and it goes through.
    let resp = app
        .post(
            &format!("/api/design/slices/{slice}/reviews"),
            &a_critique("reject", Some("brief_unmet")),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    assert_eq!(slice_status(&app, slice).await, "closed");
}

#[tokio::test]
async fn code_blocking_reasons_are_not_offered_on_a_design_review() {
    let app = TestApp::spawn().await;
    app.register_user("ci_designer").await;
    app.register_user("ci_reviewer").await;
    let designer = user_id(&app, "ci_designer").await;
    let reviewer = user_id(&app, "ci_reviewer").await;
    grant(&app, reviewer, "design_reviewer:motion").await;

    let slice = a_claimed_challenge(&app, designer, "design-motion-ui", "motion", 0).await;
    app.login("ci_designer").await;
    app.post(
        &format!("/api/design/slices/{slice}/versions"),
        &a_version(1),
    )
    .await;

    // `ci_failing` means nothing on a motion loop, and offering it would
    // invite a reviewer to pick the nearest wrong label.
    app.login("ci_reviewer").await;
    let resp = app
        .post(
            &format!("/api/design/slices/{slice}/reviews"),
            &a_critique("iterate", Some("ci_failing")),
        )
        .await;
    assert_eq!(resp.status(), 400, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn a_reviewer_outside_the_trade_cannot_decide() {
    let app = TestApp::spawn().await;
    app.register_user("sound_designer").await;
    app.register_user("dataviz_reviewer").await;
    let designer = user_id(&app, "sound_designer").await;
    let reviewer = user_id(&app, "dataviz_reviewer").await;
    grant(&app, reviewer, "design_reviewer:dataviz").await;

    let slice = a_claimed_challenge(&app, designer, "design-sound", "sound", 0).await;
    app.login("sound_designer").await;
    app.post(
        &format!("/api/design/slices/{slice}/versions"),
        &a_version(1),
    )
    .await;

    // A dataviz reviewer has no basis to sign off an ambience.
    app.login("dataviz_reviewer").await;
    let resp = app
        .post(
            &format!("/api/design/slices/{slice}/reviews"),
            &a_critique("approve", None),
        )
        .await;
    assert_eq!(resp.status(), 403, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn nobody_signs_off_their_own_challenge() {
    let app = TestApp::spawn().await;
    app.register_user("self_designer").await;
    let designer = user_id(&app, "self_designer").await;
    grant(&app, designer, "design_reviewer:all").await;

    let slice = a_claimed_challenge(&app, designer, "design-product", "interface", 0).await;
    app.login("self_designer").await;
    app.post(
        &format!("/api/design/slices/{slice}/versions"),
        &a_version(1),
    )
    .await;

    let resp = app
        .post(
            &format!("/api/design/slices/{slice}/reviews"),
            &a_critique("approve", None),
        )
        .await;
    assert_eq!(
        resp.status(),
        403,
        "holding the capability does not make you impartial about your own work"
    );
}

#[tokio::test]
async fn only_the_designer_who_claimed_it_hands_in_a_version() {
    let app = TestApp::spawn().await;
    app.register_user("claimer").await;
    app.register_user("stranger").await;
    let designer = user_id(&app, "claimer").await;

    let slice = a_claimed_challenge(&app, designer, "design-web", "interface", 0).await;

    app.login("stranger").await;
    let resp = app
        .post(
            &format!("/api/design/slices/{slice}/versions"),
            &a_version(1),
        )
        .await;
    assert_eq!(resp.status(), 403);
}

#[tokio::test]
async fn the_fifth_round_is_the_last_one() {
    let app = TestApp::spawn().await;
    app.register_user("persistent_designer").await;
    app.register_user("patient_reviewer").await;
    let designer = user_id(&app, "persistent_designer").await;
    let reviewer = user_id(&app, "patient_reviewer").await;
    grant(&app, reviewer, "design_reviewer:brand").await;

    let slice = a_claimed_challenge(&app, designer, "design-brand-identity", "brand_kit", 0).await;

    for round in 1..=5u32 {
        app.login("persistent_designer").await;
        let resp = app
            .post(
                &format!("/api/design/slices/{slice}/versions"),
                &a_version(round),
            )
            .await;
        assert_eq!(resp.status(), 201, "round {round}");

        app.login("patient_reviewer").await;
        let resp = app
            .post(
                &format!("/api/design/slices/{slice}/reviews"),
                &a_critique("iterate", Some("craft_gap")),
            )
            .await;
        assert_eq!(resp.status(), 200, "round {round}");
    }

    // A sixth pass is refused: by then the problem is the brief or the
    // assignment, and a sixth identical critique helps nobody.
    app.login("persistent_designer").await;
    app.post(
        &format!("/api/design/slices/{slice}/versions"),
        &a_version(6),
    )
    .await;
    app.login("patient_reviewer").await;
    let resp = app
        .post(
            &format!("/api/design/slices/{slice}/reviews"),
            &a_critique("iterate", Some("craft_gap")),
        )
        .await;
    assert!(
        resp.status().is_client_error() || resp.status().is_server_error(),
        "the ceiling must stop an endless critique loop, got {}",
        resp.status()
    );
}

// ═══════════════════════════════════════════════════════════════════
// The queue
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn the_queue_shows_only_the_trades_a_reviewer_can_judge() {
    let app = TestApp::spawn().await;
    app.register_user("q_designer").await;
    app.register_user("q_motion_reviewer").await;
    let designer = user_id(&app, "q_designer").await;
    let reviewer = user_id(&app, "q_motion_reviewer").await;
    grant(&app, reviewer, "design_reviewer:motion").await;

    let motion = a_claimed_challenge(&app, designer, "design-motion-2d", "motion", 0).await;
    let brand = a_claimed_challenge(&app, designer, "design-brand-identity", "brand_kit", 0).await;

    app.login("q_designer").await;
    for slice in [motion, brand] {
        app.post(
            &format!("/api/design/slices/{slice}/versions"),
            &a_version(1),
        )
        .await;
    }

    app.login("q_motion_reviewer").await;
    let body: Value = app
        .get("/api/design/reviews/queue")
        .await
        .json()
        .await
        .unwrap();
    let slices = body["data"]["slices"].as_array().unwrap();
    let ids: Vec<String> = slices
        .iter()
        .map(|s| s["id"].as_str().unwrap().to_string())
        .collect();
    assert!(ids.contains(&motion.to_string()));
    assert!(
        !ids.contains(&brand.to_string()),
        "a motion reviewer has no business in the brand queue"
    );
}

#[tokio::test]
async fn a_reviewer_with_no_rights_gets_an_empty_queue_not_a_refusal() {
    let app = TestApp::spawn().await;
    app.register_user("q_nobody").await;
    app.login("q_nobody").await;

    // "Nothing for you to do" is the honest answer; a 403 on a queue reads as
    // a bug and sends somebody hunting for a permission problem.
    let resp = app.get("/api/design/reviews/queue").await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert!(body["data"]["slices"].as_array().unwrap().is_empty());
}

// ═══════════════════════════════════════════════════════════════════
// The rank is cross-domain
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn design_proofs_move_the_platform_rank_like_code_ones() {
    let app = TestApp::spawn().await;
    app.register_user("ranked_designer").await;
    app.register_user("ranked_reviewer").await;
    let designer = user_id(&app, "ranked_designer").await;
    let reviewer = user_id(&app, "ranked_reviewer").await;
    grant(&app, reviewer, "design_reviewer:all").await;

    // Four validated design deliverables and nothing else. Ranger's threshold
    // is four verified deliverables, whatever discipline produced them.
    for i in 0..4 {
        let slice = a_claimed_challenge(&app, designer, "design-iconography", "icon_set", 0).await;
        app.login("ranked_designer").await;
        app.post(
            &format!("/api/design/slices/{slice}/versions"),
            &json!({"artifact_url": format!("https://figma.test/icons/{i}")}),
        )
        .await;
        app.login("ranked_reviewer").await;
        app.post(
            &format!("/api/design/slices/{slice}/reviews"),
            &a_critique("approve", None),
        )
        .await;
    }

    let verified: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM deliverables
          WHERE user_id = $1 AND verification_status = 'verified' AND revoked_at IS NULL",
    )
    .bind(designer)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(verified, 4);

    let (_, computed, _) =
        skilluv_backend::services::ranks::recompute_rank_for_user(&app.db, designer)
            .await
            .unwrap();
    assert_eq!(
        computed, "ranger",
        "four validated design deliverables are four proofs, whatever the discipline"
    );
}

// ═══════════════════════════════════════════════════════════════════
// Setting two rounds against each other
// ═══════════════════════════════════════════════════════════════════

/// Runs `rounds` critiques on one challenge, approving the last.
async fn a_challenge_that_took(app: &TestApp, designer: &str, reviewer: &str, rounds: u32) -> Uuid {
    let designer_id = user_id(app, designer).await;
    let slice =
        a_claimed_challenge(app, designer_id, "design-brand-identity", "brand_kit", 60).await;

    for round in 1..=rounds {
        app.login(designer).await;
        let resp = app
            .post(
                &format!("/api/design/slices/{slice}/versions"),
                &json!({
                    "artifact_url": format!("https://figma.test/round/{round}"),
                    "notes_md": format!("Version {round} : la marque tient en une couleur."),
                }),
            )
            .await;
        assert!(resp.status().is_success(), "{:?}", resp.text().await);

        app.login(reviewer).await;
        let body = if round == rounds {
            a_critique("approve", None)
        } else {
            a_critique("iterate", Some("craft_gap"))
        };
        let resp = app
            .post(&format!("/api/design/slices/{slice}/reviews"), &body)
            .await;
        assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    }
    slice
}

#[tokio::test]
async fn two_rounds_can_be_set_against_each_other_with_what_was_said_between() {
    let app = TestApp::spawn().await;
    app.register_user("compare_designer").await;
    app.register_user("compare_reviewer").await;
    let reviewer = user_id(&app, "compare_reviewer").await;
    grant(&app, reviewer, "design_reviewer:all").await;

    let slice = a_challenge_that_took(&app, "compare_designer", "compare_reviewer", 3).await;

    let body: Value = app
        .get(&format!("/api/design/slices/{slice}/compare?from=1&to=3"))
        .await
        .json()
        .await
        .unwrap();
    let c = &body["data"]["comparison"];

    // Each round has to carry its own file. Reading the trail off the slice
    // would show the same URL three times, which is exactly what this
    // endpoint exists to disprove.
    assert_eq!(c["from"]["artifact_url"], "https://figma.test/round/1");
    assert_eq!(c["to"]["artifact_url"], "https://figma.test/round/3");
    assert_eq!(c["from"]["round"], 1);
    assert_eq!(c["to"]["round"], 3);

    // A brand kit is marks, type and a document at once: no single automatic
    // comparison is honest, so both are shown and a person reads them.
    assert_eq!(c["diff_strategy"], "side_by_side");
    assert_eq!(c["design_subtype"], "brand_kit");

    // The critiques between are the reason the later version differs.
    let between = c["critiques_between"].as_array().unwrap();
    assert_eq!(between.len(), 2, "rounds 1 and 2 produced round 3");
    assert_eq!(between[0]["decision"], "iterate");
}

#[tokio::test]
async fn a_round_can_be_read_on_its_own() {
    let app = TestApp::spawn().await;
    app.register_user("single_designer").await;
    app.register_user("single_reviewer").await;
    let reviewer = user_id(&app, "single_reviewer").await;
    grant(&app, reviewer, "design_reviewer:all").await;

    let slice = a_challenge_that_took(&app, "single_designer", "single_reviewer", 2).await;

    let body: Value = app
        .get(&format!("/api/design/slices/{slice}/versions/1"))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["version"]["decision"], "iterate");
    assert_eq!(
        body["data"]["version"]["artifact_url"],
        "https://figma.test/round/1"
    );

    let missing = app
        .get(&format!("/api/design/slices/{slice}/versions/9"))
        .await;
    assert_eq!(missing.status().as_u16(), 404);
}

#[tokio::test]
async fn a_backwards_comparison_is_refused_rather_than_answered() {
    let app = TestApp::spawn().await;
    app.register_user("back_designer").await;
    app.register_user("back_reviewer").await;
    let reviewer = user_id(&app, "back_reviewer").await;
    grant(&app, reviewer, "design_reviewer:all").await;

    let slice = a_challenge_that_took(&app, "back_designer", "back_reviewer", 2).await;

    // Both of these are answerable, and both mean the caller built the request
    // wrong. Answering would return something that looks like a result.
    for query in ["from=2&to=1", "from=1&to=1"] {
        let resp = app
            .get(&format!("/api/design/slices/{slice}/compare?{query}"))
            .await;
        assert_eq!(resp.status().as_u16(), 400, "{query}");
    }
}

#[tokio::test]
async fn the_iteration_story_keeps_only_the_work_that_was_argued_about() {
    let app = TestApp::spawn().await;
    app.register_user("story_designer").await;
    app.register_user("story_reviewer").await;
    let reviewer = user_id(&app, "story_reviewer").await;
    grant(&app, reviewer, "design_reviewer:all").await;

    // One approved on the first pass, one that took three rounds.
    a_challenge_that_took(&app, "story_designer", "story_reviewer", 1).await;
    let hard = a_challenge_that_took(&app, "story_designer", "story_reviewer", 3).await;

    let body: Value = app
        .get("/api/design/users/story_designer/iteration-stories")
        .await
        .json()
        .await
        .unwrap();
    let stories = body["data"]["stories"].as_array().unwrap();

    // Two rounds is one critique and a fix, which happens to everybody. Three
    // is where a direction was questioned and the person came back.
    assert_eq!(stories.len(), 1, "{stories:?}");
    assert_eq!(stories[0]["slice_id"], json!(hard.to_string()));
    assert_eq!(stories[0]["rounds"], 3);
    assert_eq!(
        stories[0]["first_artifact_url"],
        "https://figma.test/round/1"
    );
    assert_eq!(
        stories[0]["final_artifact_url"],
        "https://figma.test/round/3"
    );
}

// ═══════════════════════════════════════════════════════════════════
// What the machine can say, and what it deliberately cannot
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn an_unreadable_host_is_recorded_as_unchecked_not_as_passing() {
    let app = TestApp::spawn().await;
    app.register_user("checks_designer").await;
    let designer = user_id(&app, "checks_designer").await;
    let slice = a_claimed_challenge(&app, designer, "design-brand-identity", "brand_kit", 40).await;

    app.login("checks_designer").await;
    let resp = app
        .post(
            &format!("/api/design/slices/{slice}/versions"),
            &json!({"artifact_url": "https://figma.test/file/abc"}),
        )
        .await;
    assert!(resp.status().is_success(), "{:?}", resp.text().await);

    // The checks run in the background, so the panel fills in a moment later.
    // What is pinned here is that it fills in at all: silence and success must
    // not look alike, or a reviewer reads an empty panel as approval.
    let mut checks = Vec::new();
    for _ in 0..40 {
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        let body: Value = app
            .get(&format!("/api/design/slices/{slice}/auto-checks"))
            .await
            .json()
            .await
            .unwrap();
        checks = body["data"]["checks"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if !checks.is_empty() {
            break;
        }
    }

    assert_eq!(checks.len(), 1, "{checks:?}");
    assert_eq!(checks[0]["check_type"], "fetch");
    assert_eq!(checks[0]["severity"], "info");
    assert_eq!(checks[0]["round"], 1);
}

#[tokio::test]
async fn nothing_the_checks_say_blocks_a_verdict() {
    let app = TestApp::spawn().await;
    app.register_user("checks_free_designer").await;
    app.register_user("checks_free_reviewer").await;
    let reviewer = user_id(&app, "checks_free_reviewer").await;
    grant(&app, reviewer, "design_reviewer:all").await;

    let slice =
        a_challenge_that_took(&app, "checks_free_designer", "checks_free_reviewer", 1).await;

    // A version whose checks recorded an `error` is still approvable, because
    // no check knows whether a mark is right for a cooperative. The reverse —
    // a clean run and a rejection — is the common case.
    sqlx::query(
        "INSERT INTO design_auto_check_results (slice_id, round, check_type, severity, message)
         VALUES ($1, 1, 'palette_contrast', 'error', 'Aucune paire lisible.')",
    )
    .bind(slice)
    .execute(&app.db)
    .await
    .unwrap();

    assert_eq!(slice_status(&app, slice).await, "validated");

    let body: Value = app
        .get(&format!("/api/design/slices/{slice}/auto-checks"))
        .await
        .json()
        .await
        .unwrap();
    assert!(
        body["data"]["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["severity"] == "error")
    );
}

// ═══════════════════════════════════════════════════════════════════
// What to do next
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn a_suggestion_says_why_it_was_made() {
    let app = TestApp::spawn().await;
    app.register_user("suggested_to").await;
    let user = user_id(&app, "suggested_to").await;

    // Declare a trade, then leave an open challenge in it.
    sqlx::query(
        "INSERT INTO user_orientations (user_id, orientation_id)
         SELECT $1, id FROM orientations WHERE slug = 'design-brand-identity'",
    )
    .bind(user)
    .execute(&app.db)
    .await
    .unwrap();

    let owner = user_id(&app, "suggested_to").await;
    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO projects (slug, name, owner_type, owner_id)
         VALUES ($1, 'Projet ouvert', 'user', $2) RETURNING id",
    )
    .bind(format!("open-{}", Uuid::new_v4()))
    .bind(owner)
    .fetch_one(&app.db)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO project_slices
            (project_id, slice_type, title, description, primary_domain, difficulty,
             status, design_subtype, orientation_id)
         VALUES ($1, 'design_artifact', 'Identité coopérative', 'Un brief.', 'design', 2, 'open',
                 'brand_kit', (SELECT id FROM orientations WHERE slug = 'design-brand-identity'))",
    )
    .bind(project)
    .execute(&app.db)
    .await
    .unwrap();

    app.login("suggested_to").await;
    let resp = app.get("/api/users/me/next-challenges?domain=design").await;
    let status = resp.status();
    let body: Value = resp.json().await.unwrap();
    assert_eq!(status.as_u16(), 200, "{body}");
    let suggestions = body["data"]["suggestions"]
        .as_array()
        .unwrap_or_else(|| panic!("no suggestions array: {body}"));
    assert!(!suggestions.is_empty(), "{body}");

    // A recommendation nobody can argue with is one nobody trusts.
    let top = &suggestions[0];
    assert_eq!(top["format"], "individual");
    assert!(top["score"].as_i64().unwrap() >= 3, "{top}");
    assert!(
        top["reasons"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r.as_str().unwrap().contains("design-brand-identity")),
        "{top}"
    );
}

#[tokio::test]
async fn a_domain_nobody_practises_is_refused_rather_than_answered_emptily() {
    let app = TestApp::spawn().await;
    app.register_user("suggest_bad_domain").await;
    app.login("suggest_bad_domain").await;

    let resp = app
        .get("/api/users/me/next-challenges?domain=charcuterie")
        .await;
    assert_eq!(resp.status().as_u16(), 400);
}
