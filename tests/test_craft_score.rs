//! The craft score and the attestations it counts.

mod common;
use common::TestApp;
use serde_json::Value;
use uuid::Uuid;

async fn a_user(app: &TestApp, username: &str) -> Uuid {
    app.register_user(username).await;
    sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(username)
        .fetch_one(&app.db)
        .await
        .unwrap()
}

/// A verified deliverable belonging to `user`.
async fn a_verified_artifact(app: &TestApp, user: Uuid, language: &str) -> Uuid {
    let challenge: Uuid = sqlx::query_scalar(
        "INSERT INTO challenge_templates
            (title, description, instructions, skill_domain, difficulty, language, status, is_training)
         VALUES ($1, 'x', 'x', 'code', 2, $2, 'published', TRUE)
         RETURNING id",
    )
    .bind(format!("chal {} {language}", Uuid::new_v4()))
    .bind(language)
    .fetch_one(&app.db)
    .await
    .unwrap();

    sqlx::query_scalar(
        "INSERT INTO deliverables
            (user_id, challenge_id, artifact_type, artifact_url, verifiable_by,
             verification_status, verified_at)
         VALUES ($1, $2, 'pr_merged', 'https://github.com/x/y/pull/1', 'github_webhook',
                 'verified', NOW())
         RETURNING id",
    )
    .bind(user)
    .bind(challenge)
    .fetch_one(&app.db)
    .await
    .unwrap()
}

// ═══════════════════════════════════════════════════════════════════
// Attestations
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn a_merged_pull_request_becomes_an_attestation() {
    let app = TestApp::spawn().await;
    let user = a_user(&app, "attest_merger").await;
    let deliverable = a_verified_artifact(&app, user, "rust").await;

    let issued = skilluv_backend::services::code_attestations::pr_merged_upstream(
        &app.db,
        user,
        deliverable,
    )
    .await
    .expect("issue");

    assert_eq!(issued.basis, "code_pr_merged_upstream");
    assert_eq!(issued.verification_code.len(), 10);

    // The link is in the description, because an attestation nobody can
    // check is worth nothing.
    let description: String =
        sqlx::query_scalar("SELECT description FROM attestations WHERE id = $1")
            .bind(issued.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(description.contains("https://"));
}

#[tokio::test]
async fn an_attestation_cannot_be_issued_from_somebody_elses_work() {
    let app = TestApp::spawn().await;
    let owner = a_user(&app, "attest_owner").await;
    let other = a_user(&app, "attest_other").await;
    let deliverable = a_verified_artifact(&app, owner, "rust").await;

    let stolen = skilluv_backend::services::code_attestations::pr_merged_upstream(
        &app.db,
        other,
        deliverable,
    )
    .await;
    assert!(stolen.is_err());
}

#[tokio::test]
async fn a_revoked_artifact_cannot_be_attested() {
    let app = TestApp::spawn().await;
    let user = a_user(&app, "attest_revoked").await;
    let deliverable = a_verified_artifact(&app, user, "rust").await;
    sqlx::query("UPDATE deliverables SET revoked_at = NOW() WHERE id = $1")
        .bind(deliverable)
        .execute(&app.db)
        .await
        .unwrap();

    let refused = skilluv_backend::services::code_attestations::pr_merged_upstream(
        &app.db,
        user,
        deliverable,
    )
    .await;
    assert!(
        refused.is_err(),
        "the platform has already taken this artefact back"
    );
}

#[tokio::test]
async fn a_standard_contribution_names_the_room_it_happened_in() {
    let app = TestApp::spawn().await;
    let user = a_user(&app, "attest_standards").await;

    let invented = skilluv_backend::services::code_attestations::standard_contribution(
        &app.db,
        user,
        "https://github.com/myself/my-proposal",
        "une idée",
    )
    .await;
    assert!(
        invented.is_err(),
        "the most valuable attestation must not be reachable by pasting any URL"
    );

    let real = skilluv_backend::services::code_attestations::standard_contribution(
        &app.db,
        user,
        "https://github.com/tc39/proposal-decorators/pull/1",
        "décorateurs",
    )
    .await
    .expect("a tc39 proposal is a standards contribution");
    assert_eq!(real.basis, "code_standard_contribution");
}

#[tokio::test]
async fn featuring_somebody_requires_saying_why() {
    let app = TestApp::spawn().await;
    let user = a_user(&app, "attest_featured").await;

    let silent = skilluv_backend::services::code_attestations::featured_coder(
        &app.db,
        user,
        "https://skill-uv.com/@attest_featured",
        "   ",
    )
    .await;
    assert!(silent.is_err());

    let argued = skilluv_backend::services::code_attestations::featured_coder(
        &app.db,
        user,
        "https://skill-uv.com/@attest_featured",
        "a repris seul la maintenance d'une bibliothèque abandonnée",
    )
    .await;
    assert!(argued.is_ok());
}

// ═══════════════════════════════════════════════════════════════════
// The score
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn somebody_with_nothing_scores_nothing_and_is_an_apprentice() {
    let app = TestApp::spawn().await;
    let user = a_user(&app, "score_empty").await;

    let score = skilluv_backend::services::craft_score::compute(&app.db, user)
        .await
        .unwrap();
    assert_eq!(score.score, 0);
    assert_eq!(score.tier_slug, "apprentice");
    assert!(
        score.breakdown.is_empty(),
        "a breakdown of zeroes says nothing"
    );
    assert_eq!(score.next_tier_at, Some(100));
}

#[tokio::test]
async fn every_point_has_a_line_explaining_it() {
    let app = TestApp::spawn().await;
    let user = a_user(&app, "score_explained").await;
    let deliverable = a_verified_artifact(&app, user, "rust").await;
    skilluv_backend::services::code_attestations::pr_merged_upstream(&app.db, user, deliverable)
        .await
        .unwrap();

    let score = skilluv_backend::services::craft_score::compute(&app.db, user)
        .await
        .unwrap();

    // One attestation (5) + one merged PR (15) + one language (20) = 40.
    assert_eq!(score.score, 40);
    let terms: Vec<&str> = score.breakdown.iter().map(|t| t.term.as_str()).collect();
    assert!(terms.contains(&"attestations_code"));
    assert!(terms.contains(&"prs_merged_upstream"));
    assert!(terms.contains(&"languages_distinct"));
    for term in &score.breakdown {
        assert!(
            !term.explanation.is_empty(),
            "a score nobody can explain is a score nobody trusts"
        );
    }
}

#[tokio::test]
async fn an_unscored_person_is_not_a_badly_scored_one() {
    let app = TestApp::spawn().await;
    let user = a_user(&app, "score_unreviewed").await;
    let deliverable = a_verified_artifact(&app, user, "rust").await;
    skilluv_backend::services::code_attestations::pr_merged_upstream(&app.db, user, deliverable)
        .await
        .unwrap();

    let score = skilluv_backend::services::craft_score::compute(&app.db, user)
        .await
        .unwrap();
    // Nobody has scored their work against a grid. The term is absent, not
    // negative: counting an unscored average as zero would subtract the whole
    // baseline from the total.
    assert!(
        !score
            .breakdown
            .iter()
            .any(|t| t.term == "review_grid_average")
    );
    assert!(score.score > 0);
}

#[tokio::test]
async fn a_grid_scoring_must_answer_every_criterion() {
    let app = TestApp::spawn().await;
    let author = a_user(&app, "grid_author").await;
    let reviewer = a_user(&app, "grid_reviewer").await;
    let deliverable = a_verified_artifact(&app, author, "rust").await;

    let review: Uuid = sqlx::query_scalar(
        "INSERT INTO reviews (deliverable_id, reviewer_user_id, verdict, body)
         VALUES ($1, $2, 'approve', 'ok') RETURNING id",
    )
    .bind(deliverable)
    .bind(reviewer)
    .fetch_one(&app.db)
    .await
    .unwrap();

    let grid: Uuid = sqlx::query_scalar(
        "SELECT id FROM review_grids WHERE domain = 'code' AND reviewer_group IS NULL",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    // Two criteria out of five: this is the reviewer skipping the part they
    // did not want to look at, which is what the table exists to prevent.
    let partial = sqlx::query(
        "INSERT INTO review_grid_scores (review_id, grid_id, scores, average)
         VALUES ($1, $2, '{\"Correction\": 4, \"Tests\": 3}'::JSONB, 0)",
    )
    .bind(review)
    .bind(grid)
    .execute(&app.db)
    .await;
    assert!(partial.is_err());

    // A criterion the grid does not have is a reviewer inventing one.
    let invented = sqlx::query(
        "INSERT INTO review_grid_scores (review_id, grid_id, scores, average)
         VALUES ($1, $2, '{\"Correction\": 4, \"Tests\": 3, \"Documentation\": 3,
                           \"Lisibilité\": 4, \"Transparence sur l''IA\": 5,
                           \"Élégance\": 5}'::JSONB, 0)",
    )
    .bind(review)
    .bind(grid)
    .execute(&app.db)
    .await;
    assert!(invented.is_err());

    let whole = sqlx::query(
        "INSERT INTO review_grid_scores (review_id, grid_id, scores, average)
         VALUES ($1, $2, '{\"Correction\": 4, \"Tests\": 3, \"Documentation\": 3,
                           \"Lisibilité\": 4, \"Transparence sur l''IA\": 5}'::JSONB, 0)",
    )
    .bind(review)
    .bind(grid)
    .execute(&app.db)
    .await;
    assert!(whole.is_ok(), "a complete scoring must be accepted");

    // The average is derived, never taken from the caller: 19/5 = 3.8, not
    // the zero that was passed in.
    let average: f64 =
        sqlx::query_scalar("SELECT average::FLOAT8 FROM review_grid_scores WHERE review_id = $1")
            .bind(review)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(average, 3.8);
}

#[tokio::test]
async fn a_grid_score_above_the_middle_moves_the_craft_score() {
    let app = TestApp::spawn().await;
    let author = a_user(&app, "grid_scored_author").await;
    let reviewer = a_user(&app, "grid_scored_reviewer").await;
    let deliverable = a_verified_artifact(&app, author, "rust").await;

    let before = skilluv_backend::services::craft_score::compute(&app.db, author)
        .await
        .unwrap()
        .score;

    let review: Uuid = sqlx::query_scalar(
        "INSERT INTO reviews (deliverable_id, reviewer_user_id, verdict, body)
         VALUES ($1, $2, 'approve', 'ok') RETURNING id",
    )
    .bind(deliverable)
    .bind(reviewer)
    .fetch_one(&app.db)
    .await
    .unwrap();
    let grid: Uuid = sqlx::query_scalar(
        "SELECT id FROM review_grids WHERE domain = 'code' AND reviewer_group IS NULL",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO review_grid_scores (review_id, grid_id, scores, average)
         VALUES ($1, $2, '{\"Correction\": 5, \"Tests\": 5, \"Documentation\": 4,
                           \"Lisibilité\": 4, \"Transparence sur l''IA\": 4}'::JSONB, 0)",
    )
    .bind(review)
    .bind(grid)
    .execute(&app.db)
    .await
    .unwrap();

    let after = skilluv_backend::services::craft_score::compute(&app.db, author)
        .await
        .unwrap();
    // Average 4.4, baseline 3, weight 200 → 280 points.
    assert_eq!(after.score - before, 280);
}

#[tokio::test]
async fn the_profile_publishes_the_formula_it_scores_with() {
    let app = TestApp::spawn().await;

    let body: Value = app.get("/api/code/tiers").await.json().await.unwrap();
    let tiers = body["data"]["tiers"].as_array().unwrap();
    assert_eq!(tiers.len(), 6);
    assert_eq!(tiers[0]["slug"], "apprentice");

    // A score computed from a secret formula is one people game by guessing
    // rather than by doing the work.
    let formula = body["data"]["formula"].as_array().unwrap();
    assert!(formula.len() >= 10);
    for term in formula {
        assert!(!term["explanation"].as_str().unwrap().is_empty());
    }
}

#[tokio::test]
async fn the_public_profile_carries_the_artefacts_behind_the_score() {
    let app = TestApp::spawn().await;
    let user = a_user(&app, "profile_public").await;
    let deliverable = a_verified_artifact(&app, user, "rust").await;
    skilluv_backend::services::code_attestations::pr_merged_upstream(&app.db, user, deliverable)
        .await
        .unwrap();

    let resp = app.get("/api/users/profile_public/code-profile").await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();

    assert_eq!(body["data"]["craft_score"]["score"], 40);
    assert_eq!(
        body["data"]["attestations"].as_array().unwrap().len(),
        1,
        "the score is a summary of these, and they must be readable"
    );
    let languages = body["data"]["languages"].as_array().unwrap();
    assert_eq!(languages[0]["language"], "rust");
}

#[tokio::test]
async fn a_hidden_profile_answers_nothing_rather_than_an_empty_page() {
    let app = TestApp::spawn().await;
    let user = a_user(&app, "profile_hidden").await;
    sqlx::query("UPDATE users SET profile_hidden = TRUE WHERE id = $1")
        .bind(user)
        .execute(&app.db)
        .await
        .unwrap();

    // An empty profile would read as "this person has done nothing".
    assert_eq!(
        app.get("/api/users/profile_hidden/code-profile")
            .await
            .status(),
        404
    );
}

#[tokio::test]
async fn recomputing_stores_what_the_page_computes() {
    let app = TestApp::spawn().await;
    let user = a_user(&app, "score_stored").await;
    let deliverable = a_verified_artifact(&app, user, "rust").await;
    skilluv_backend::services::code_attestations::pr_merged_upstream(&app.db, user, deliverable)
        .await
        .unwrap();

    let stored_before: i32 = sqlx::query_scalar("SELECT craft_score_code FROM users WHERE id = $1")
        .bind(user)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(stored_before, 0, "nothing has recomputed yet");

    let done = skilluv_backend::services::craft_score::sweep(&app.db, 100)
        .await
        .unwrap();
    assert!(done > 0);

    let stored_after: i32 = sqlx::query_scalar("SELECT craft_score_code FROM users WHERE id = $1")
        .bind(user)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(stored_after, 40);
}
