//! The leadership domain.
//!
//! Six trades whose output is a document somebody else acts on, and whose
//! central problem is that almost all of it is written inside somebody's
//! organisation. These tests hold the four places where that is handled by
//! the schema rather than by good intentions:
//!
//!   * an anonymised document is not published until a **second person** has
//!     read it;
//!   * a retrospective is attested for its action items landing, not for the
//!     hour in the room;
//!   * a commitment counts once the project it commits has **acknowledged**
//!     it — the one term in this domain nobody can produce alone;
//!   * a cohort's graduation rate is computed over everybody who joined.

mod common;
use common::TestApp;
use serde_json::{Value, json};
use uuid::Uuid;

async fn a_talent(app: &TestApp, username: &str) -> Uuid {
    app.register_user(username).await;
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
    .expect("grant");
}

async fn a_project(app: &TestApp, owner: Uuid, slug: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO projects (slug, name, description, owner_type, owner_id)
         VALUES ($1, $1, 'A project', 'user', $2)
         RETURNING id",
    )
    .bind(slug)
    .bind(owner)
    .fetch_one(&app.db)
    .await
    .unwrap()
}

/// A leadership slice held by `owner`.
async fn a_leadership_slice(
    app: &TestApp,
    project: Uuid,
    owner: Uuid,
    subtype: &str,
    redaction: &str,
    orientation_slug: &str,
) -> Uuid {
    let context = if redaction == "confidential" {
        Some(json!({"industry": "fintech", "team_size": 12}))
    } else {
        None
    };

    sqlx::query_scalar(
        "INSERT INTO project_slices
             (project_id, slice_type, title, description, primary_domain,
              difficulty, leadership_subtype, redaction_state,
              leadership_context, target_domain, claimed_by_user_id, claimed_at,
              orientation_id)
         VALUES ($1, 'leadership_artifact', 'A leadership document', 'Description',
                 'leadership', 3, $2, $3, $4, 'code', $5, NOW(),
                 (SELECT id FROM orientations WHERE slug = $6))
         RETURNING id",
    )
    .bind(project)
    .bind(subtype)
    .bind(redaction)
    .bind(context)
    .bind(owner)
    .bind(orientation_slug)
    .fetch_one(&app.db)
    .await
    .unwrap()
}

async fn a_verified_deliverable(app: &TestApp, slice: Uuid, user: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO deliverables
             (slice_id, user_id, artifact_type, artifact_url, verifiable_by,
              verification_status, verified_at)
         VALUES ($1, $2, 'documentation', 'https://example.org/doc',
                 'human_review', 'verified', NOW())
         RETURNING id",
    )
    .bind(slice)
    .bind(user)
    .fetch_one(&app.db)
    .await
    .unwrap()
}

async fn a_cohort(app: &TestApp, creator: Uuid, slug: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO cohorts (slug, name, description, starts_at, ends_at, created_by)
         VALUES ($1, 'A cohort', 'Description',
                 NOW() - INTERVAL '60 days', NOW() + INTERVAL '30 days', $2)
         RETURNING id",
    )
    .bind(slug)
    .bind(creator)
    .fetch_one(&app.db)
    .await
    .unwrap()
}

async fn join_cohort(app: &TestApp, cohort: Uuid, user: Uuid) {
    sqlx::query(
        "INSERT INTO cohort_members (cohort_id, user_id, role)
         VALUES ($1, $2, 'member') ON CONFLICT DO NOTHING",
    )
    .bind(cohort)
    .bind(user)
    .execute(&app.db)
    .await
    .unwrap();
}

fn a_retro_body() -> Value {
    json!({
        "title": "The release that slipped twice",
        "format": "timeline",
        "participants_count": 6,
        "held_on": "2026-06-01",
        "insights_md": "The team agreed that the second slip was visible three weeks \
    before it was announced, and that nobody felt able to say so because the date had \
    already been repeated to the client. The estimate itself was not the problem: the \
    problem was that revising it publicly had become expensive. What the system allowed \
    was a plan whose date was quoted outside the team before its assumptions were written \
    down anywhere.",
    })
}

// ═══════════════════════════════════════════════════════════════════
// The catalogue
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn leadership_is_an_open_domain_with_six_trades_in_five_families() {
    let app = TestApp::spawn().await;

    let active: bool =
        sqlx::query_scalar("SELECT is_active FROM skill_domains WHERE slug = 'leadership'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(active);

    let resp = app.get("/api/leadership/reference").await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    let orientations = body["data"]["orientations"].as_array().unwrap();

    assert_eq!(orientations.len(), 6);

    let groups: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT slug, reviewer_group FROM orientations WHERE primary_domain = 'leadership'",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    let group_of = |slug: &str| -> String {
        groups
            .iter()
            .find(|(s, _)| s == slug)
            .and_then(|(_, g)| g.clone())
            .unwrap_or_default()
    };

    // Somebody who can read a roadmap can read a delivery plan: both are a
    // sequence of commitments with dependencies and a stated cost of being
    // wrong.
    assert_eq!(group_of("lead-product"), group_of("lead-project"));
    // A career ladder and a curriculum are not the same object, and reading
    // one well says nothing about reading the other.
    assert_ne!(group_of("lead-people"), group_of("lead-mentor"));

    assert!(groups.iter().all(|(_, g)| g.is_some()));
}

#[tokio::test]
async fn the_domain_list_learned_about_leadership() {
    assert!(skilluv_backend::validators::SKILL_DOMAINS.contains(&"leadership"));
}

#[tokio::test]
async fn every_trade_makes_its_review_capability_grantable() {
    let app = TestApp::spawn().await;

    for capability in [
        "leadership_reviewer:delivery",
        "leadership_reviewer:technical",
        "leadership_reviewer:people",
        "leadership_reviewer:community",
        "leadership_reviewer:teaching",
        "leadership_reviewer:all",
        "challenge_validator:leadership",
    ] {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM capability_catalog WHERE capability = $1)",
        )
        .bind(capability)
        .fetch_one(&app.db)
        .await
        .unwrap();
        assert!(exists, "{capability} is not grantable");
    }
}

#[tokio::test]
async fn the_skill_map_reuses_what_the_tree_already_had() {
    let app = TestApp::spawn().await;

    let total: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM orientation_skill_map m
           JOIN orientations o ON o.id = m.orientation_id
          WHERE o.primary_domain = 'leadership'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(
        total, 71,
        "the map is short — a skill slug in 0466 does not exist"
    );

    // `soft_skills` already held adr-writing, roadmap-thinking,
    // technical-decision-making, mentoring-junior and nine others. Re-creating
    // them under a `lead-` prefix would give a profile two skills for one
    // competence.
    let borrowed: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM orientation_skill_map m
           JOIN orientations o ON o.id = m.orientation_id
           JOIN skill_nodes s ON s.id = m.skill_id
          WHERE o.primary_domain = 'leadership' AND s.domain <> 'leadership'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(
        borrowed >= 12,
        "the leadership map re-created skills the tree already had: {borrowed} borrowed"
    );
}

// ═══════════════════════════════════════════════════════════════════
// Redaction
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn a_leadership_artefact_has_to_say_how_much_of_it_can_be_shown() {
    let app = TestApp::spawn().await;
    let user = a_talent(&app, "lead_noredact").await;
    let project = a_project(&app, user, "lead-noredact-project").await;

    // No redaction state. "Unset" and "public" reading the same is how an
    // internal roadmap reaches a public profile.
    let refused = sqlx::query(
        "INSERT INTO project_slices
             (project_id, slice_type, title, description, primary_domain,
              difficulty, leadership_subtype)
         VALUES ($1, 'leadership_artifact', 'A document', 'x', 'leadership', 3, 'roadmap')",
    )
    .bind(project)
    .execute(&app.db)
    .await;

    assert!(
        refused.is_err(),
        "a leadership artefact was accepted without saying what can be shown"
    );
}

#[tokio::test]
async fn a_confidential_artefact_has_to_describe_itself() {
    let app = TestApp::spawn().await;
    let user = a_talent(&app, "lead_noctx").await;
    let project = a_project(&app, user, "lead-noctx-project").await;

    // Confidential and no context: the attestation it earns would claim
    // nothing at all.
    let refused = sqlx::query(
        "INSERT INTO project_slices
             (project_id, slice_type, title, description, primary_domain,
              difficulty, leadership_subtype, redaction_state)
         VALUES ($1, 'leadership_artifact', 'A document', 'x', 'leadership', 3,
                 'roadmap', 'confidential')",
    )
    .bind(project)
    .execute(&app.db)
    .await;

    assert!(refused.is_err());
}

#[tokio::test]
async fn an_anonymised_document_is_not_attested_until_a_second_person_reads_it() {
    let app = TestApp::spawn().await;
    let author = a_talent(&app, "lead_anon_author").await;
    let reviewer = a_talent(&app, "lead_anon_rev").await;
    grant(&app, reviewer, "leadership_reviewer:delivery").await;

    let project = a_project(&app, author, "lead-anon-project").await;
    let slice = a_leadership_slice(
        &app,
        project,
        author,
        "roadmap",
        "anonymised",
        "lead-product",
    )
    .await;
    a_verified_deliverable(&app, slice, author).await;

    // Verified, and nobody has confirmed the redaction. Nothing is attested:
    // publishing a document because its author ticked a box is a harm they
    // cannot take back, on behalf of people who are not here.
    let issued =
        skilluv_backend::services::leadership_attestations::issue_for_user(&app.db, author)
            .await
            .unwrap();
    assert!(
        issued.is_empty(),
        "an unconfirmed anonymised document was attested: {issued:?}"
    );

    app.login("lead_anon_author").await;
    let resp = app
        .post(
            &format!("/api/leadership/slices/{slice}/redaction/declare"),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    // Declaring is not confirming. The author saying it is fine is the claim,
    // not the check.
    let issued =
        skilluv_backend::services::leadership_attestations::issue_for_user(&app.db, author)
            .await
            .unwrap();
    assert!(issued.is_empty(), "a declaration alone produced a proof");

    app.login("lead_anon_rev").await;
    let resp = app
        .post(
            &format!("/api/leadership/slices/{slice}/redaction/confirm"),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM attestations
          WHERE user_id = $1 AND basis = 'leadership_roadmap_validated'
            AND revoked_at IS NULL",
    )
    .bind(author)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(count, 1, "the confirmation did not produce the attestation");
}

#[tokio::test]
async fn nobody_confirms_their_own_redaction() {
    let app = TestApp::spawn().await;
    let author = a_talent(&app, "lead_selfconfirm").await;
    // Even holding the capability. The value of the confirmation is that a
    // second person looked.
    grant(&app, author, "leadership_reviewer:all").await;

    let project = a_project(&app, author, "lead-selfconfirm-project").await;
    let slice = a_leadership_slice(
        &app,
        project,
        author,
        "roadmap",
        "anonymised",
        "lead-product",
    )
    .await;

    app.login("lead_selfconfirm").await;
    app.post(
        &format!("/api/leadership/slices/{slice}/redaction/declare"),
        &json!({}),
    )
    .await;

    let resp = app
        .post(
            &format!("/api/leadership/slices/{slice}/redaction/confirm"),
            &json!({}),
        )
        .await;
    assert_eq!(
        resp.status(),
        400,
        "an author confirmed their own redaction"
    );
}

#[tokio::test]
async fn a_confidential_artefact_is_counted_and_never_shown() {
    let app = TestApp::spawn().await;
    let author = a_talent(&app, "lead_conf").await;
    let project = a_project(&app, author, "lead-conf-project").await;

    let slice = a_leadership_slice(
        &app,
        project,
        author,
        "roadmap",
        "confidential",
        "lead-product",
    )
    .await;
    a_verified_deliverable(&app, slice, author).await;

    // No confirmation needed: nothing is published.
    let issued =
        skilluv_backend::services::leadership_attestations::issue_for_user(&app.db, author)
            .await
            .unwrap();
    assert!(issued.contains(&"leadership_roadmap_validated".to_string()));

    let body: Value = app
        .get("/api/users/lead_conf/leadership-profile")
        .await
        .json()
        .await
        .unwrap();

    let artefacts = body["data"]["profile"]["artefacts"].as_array().unwrap();
    assert!(
        artefacts.is_empty(),
        "a confidential document appeared on a public profile"
    );

    // But it exists, in the abstract, and it counts towards the score. That is
    // the point of the state: somebody whose five years are internal has a
    // score that says so.
    let summary = body["data"]["profile"]["confidential_summary"]
        .as_array()
        .unwrap();
    assert_eq!(summary.len(), 1);
    assert!(summary[0]["context"]["industry"].is_string());
    assert!(
        summary[0].get("title").is_none(),
        "a confidential artefact published its title — often enough to identify a product"
    );
    assert!(body["data"]["profile"]["score"]["score"].as_i64().unwrap() > 0);
}

// ═══════════════════════════════════════════════════════════════════
// Decisions and adoption
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn a_written_decision_is_attested_whether_or_not_it_passed() {
    let app = TestApp::spawn().await;
    let author = a_talent(&app, "lead_rfc").await;
    let project = a_project(&app, author, "lead-rfc-project").await;

    let slice = a_leadership_slice(&app, project, author, "rfc", "public", "lead-tech").await;
    a_verified_deliverable(&app, slice, author).await;

    let issued =
        skilluv_backend::services::leadership_attestations::issue_for_user(&app.db, author)
            .await
            .unwrap();

    // A domain that only attests accepted proposals teaches people to propose
    // what will pass.
    assert_eq!(
        issued,
        vec!["leadership_decision_recorded".to_string()],
        "a rejected proposal earned nothing"
    );

    app.login("lead_rfc").await;
    let resp = app
        .post(
            &format!("/api/leadership/slices/{slice}/adoption"),
            &json!({"evidence_url": "https://github.com/example/repo/pull/9"}),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    let bases: Vec<String> = sqlx::query_scalar(
        "SELECT basis FROM attestations
          WHERE user_id = $1 AND revoked_at IS NULL ORDER BY basis",
    )
    .bind(author)
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert!(bases.contains(&"leadership_decision_recorded".to_string()));
    assert!(
        bases.contains(&"leadership_rfc_accepted".to_string()),
        "adoption did not produce the second attestation: {bases:?}"
    );
}

#[tokio::test]
async fn adoption_of_a_public_document_names_where_it_landed() {
    let app = TestApp::spawn().await;
    let author = a_talent(&app, "lead_noevidence").await;
    let project = a_project(&app, author, "lead-noevidence-project").await;
    let slice = a_leadership_slice(&app, project, author, "rfc", "public", "lead-tech").await;

    app.login("lead_noevidence").await;
    let resp = app
        .post(
            &format!("/api/leadership/slices/{slice}/adoption"),
            &json!({}),
        )
        .await;
    assert_eq!(
        resp.status(),
        400,
        "an adoption was recorded with nothing to point at"
    );
}

#[tokio::test]
async fn only_a_written_decision_can_be_adopted() {
    let app = TestApp::spawn().await;
    let author = a_talent(&app, "lead_adoptroadmap").await;
    let project = a_project(&app, author, "lead-adoptroadmap-project").await;
    let slice =
        a_leadership_slice(&app, project, author, "roadmap", "public", "lead-product").await;

    app.login("lead_adoptroadmap").await;
    let resp = app
        .post(
            &format!("/api/leadership/slices/{slice}/adoption"),
            &json!({"evidence_url": "https://example.org/x"}),
        )
        .await;
    assert_eq!(
        resp.status(),
        400,
        "a roadmap was 'adopted' — it is followed or it is not, and no moment says so"
    );
}

// ═══════════════════════════════════════════════════════════════════
// Retrospectives
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn a_retrospective_is_attested_for_its_actions_not_for_the_hour() {
    let app = TestApp::spawn().await;
    let facilitator = a_talent(&app, "lead_retro").await;
    app.login("lead_retro").await;

    let project = a_project(&app, facilitator, "lead-retro-project").await;
    let slice = a_leadership_slice(
        &app,
        project,
        facilitator,
        "retrospective",
        "public",
        "lead-project",
    )
    .await;
    a_verified_deliverable(&app, slice, facilitator).await;

    let mut body = a_retro_body();
    body["slice_id"] = json!(slice);
    let resp = app.post("/api/leadership/retrospectives", &body).await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let created: Value = resp.json().await.unwrap();
    let retro_id = created["data"]["retrospective"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // The hour happened and the deliverable is verified. Nothing is attested:
    // every scheme in this trade rewards the meeting, and this one does not.
    let issued =
        skilluv_backend::services::leadership_attestations::issue_for_user(&app.db, facilitator)
            .await
            .unwrap();
    assert!(
        issued.is_empty(),
        "a retrospective was attested for having happened: {issued:?}"
    );

    // Three actions. Two closed and one dropped with a reason is 100 % resolved
    // — dropping in writing is a decision, not a failure.
    let mut action_ids = Vec::new();
    for what in [
        "Write the date assumptions down before quoting the date",
        "Move the client update to the day after the internal one",
        "Introduce a weekly confidence check",
    ] {
        let a: Value = app
            .post(
                &format!("/api/leadership/retrospectives/{retro_id}/actions"),
                &json!({"description": what, "owner_label": "The delivery lead"}),
            )
            .await
            .json()
            .await
            .unwrap();
        action_ids.push(a["data"]["action"]["id"].as_str().unwrap().to_string());
    }

    // Still nothing: the actions exist and none is resolved.
    let issued =
        skilluv_backend::services::leadership_attestations::issue_for_user(&app.db, facilitator)
            .await
            .unwrap();
    assert!(issued.is_empty(), "open actions produced a proof");

    app.post(
        &format!("/api/leadership/actions/{}/resolve", action_ids[0]),
        &json!({}),
    )
    .await;
    app.post(
        &format!("/api/leadership/actions/{}/resolve", action_ids[1]),
        &json!({}),
    )
    .await;
    app.post(
        &format!("/api/leadership/actions/{}/resolve", action_ids[2]),
        &json!({"abandoned_reason": "The confidence check duplicated the standup"}),
    )
    .await;

    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM attestations
          WHERE user_id = $1 AND revoked_at IS NULL
            AND basis = 'leadership_retrospective_facilitated'",
    )
    .bind(facilitator)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(
        count, 1,
        "resolving the actions did not produce the attestation"
    );
}

#[tokio::test]
async fn an_action_with_nobody_on_it_is_refused() {
    let app = TestApp::spawn().await;
    let facilitator = a_talent(&app, "lead_noowner").await;
    app.login("lead_noowner").await;

    let created: Value = app
        .post("/api/leadership/retrospectives", &a_retro_body())
        .await
        .json()
        .await
        .unwrap();
    let id = created["data"]["retrospective"]["id"].as_str().unwrap();
    let _ = facilitator;

    let resp = app
        .post(
            &format!("/api/leadership/retrospectives/{id}/actions"),
            &json!({"description": "Improve communication"}),
        )
        .await;
    assert_eq!(
        resp.status(),
        400,
        "an action with no owner was accepted — that is an intention"
    );
}

#[tokio::test]
async fn a_meeting_is_not_a_retrospective() {
    let app = TestApp::spawn().await;
    a_talent(&app, "lead_thin").await;
    app.login("lead_thin").await;

    let mut body = a_retro_body();
    body["insights_md"] = json!("## What went well\n- Stuff\n\n## What did not\n- Other stuff");

    let resp = app.post("/api/leadership/retrospectives", &body).await;
    assert_eq!(resp.status(), 400, "{}", resp.text().await.unwrap());
}

// ═══════════════════════════════════════════════════════════════════
// Coordination
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn a_commitment_counts_once_the_other_side_has_agreed() {
    let app = TestApp::spawn().await;
    let author = a_talent(&app, "lead_commit").await;
    let steward = a_talent(&app, "lead_steward").await;

    let project = a_project(&app, author, "lead-commit-project").await;
    let other = a_project(&app, steward, "lead-commit-other").await;
    let slice =
        a_leadership_slice(&app, project, author, "roadmap", "public", "lead-product").await;
    a_verified_deliverable(&app, slice, author).await;

    app.login("lead_commit").await;

    // A commitment with nothing written down is a commitment nobody can
    // dispute.
    let resp = app
        .post(
            &format!("/api/leadership/slices/{slice}/links"),
            &json!({"linked_project_id": other, "link_kind": "commits"}),
        )
        .await;
    assert_eq!(resp.status(), 400, "{}", resp.text().await.unwrap());

    let created: Value = app
        .post(
            &format!("/api/leadership/slices/{slice}/links"),
            &json!({
                "linked_project_id": other,
                "link_kind": "commits",
                "note": "Two weeks of their time in the second half of the quarter"
            }),
        )
        .await
        .json()
        .await
        .unwrap();
    let link_id = created["data"]["link"]["id"].as_str().unwrap().to_string();

    let before = score_of(&app, "lead_commit").await;

    // The author cannot agree with themselves.
    let resp = app
        .post(
            &format!("/api/leadership/links/{link_id}/acknowledge"),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), 400, "an author acknowledged their own plan");

    app.login("lead_steward").await;
    let resp = app
        .post(
            &format!("/api/leadership/links/{link_id}/acknowledge"),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    let after = score_of(&app, "lead_commit").await;
    assert!(
        after > before,
        "the acknowledgement did not reach the author's score ({before} → {after})"
    );
}

async fn score_of(app: &TestApp, username: &str) -> i64 {
    let body: Value = app
        .get(&format!("/api/users/{username}/leadership-profile"))
        .await
        .json()
        .await
        .unwrap();
    body["data"]["profile"]["score"]["score"].as_i64().unwrap()
}

#[tokio::test]
async fn only_a_leadership_document_coordinates_other_projects() {
    let app = TestApp::spawn().await;
    let user = a_talent(&app, "lead_wrongslice").await;
    let project = a_project(&app, user, "lead-wrongslice-project").await;

    let code_slice: Uuid = sqlx::query_scalar(
        "INSERT INTO project_slices
             (project_id, slice_type, title, description, primary_domain, difficulty)
         VALUES ($1, 'documentation', 'Some docs', 'x', 'code', 2)
         RETURNING id",
    )
    .bind(project)
    .fetch_one(&app.db)
    .await
    .unwrap();

    let refused = sqlx::query(
        "INSERT INTO leadership_artifact_links
             (leadership_slice_id, linked_project_id, link_kind, note)
         VALUES ($1, $2, 'commits', 'x')",
    )
    .bind(code_slice)
    .bind(project)
    .execute(&app.db)
    .await;

    assert!(
        refused.is_err(),
        "a documentation slice was allowed to commit a project"
    );
}

// ═══════════════════════════════════════════════════════════════════
// Cohorts
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn a_graduation_rate_is_computed_over_everybody_who_joined() {
    let app = TestApp::spawn().await;
    let lead = a_talent(&app, "lead_cohort").await;
    let cohort = a_cohort(&app, lead, "lead-cohort-run").await;

    let mut members = Vec::new();
    for n in 0..5 {
        let m = a_talent(&app, &format!("cohort_m{n}")).await;
        join_cohort(&app, cohort, m).await;
        members.push(m);
    }

    app.login("lead_cohort").await;
    let resp = app
        .post(
            &format!("/api/leadership/cohorts/{cohort}/lead"),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    // Three finished, two left — one because the schedule did not work, one
    // because they found a job.
    for m in &members[0..3] {
        let resp = app
            .post(
                &format!("/api/leadership/cohorts/{cohort}/graduate"),
                &json!({"member_id": m}),
            )
            .await;
        assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    }
    app.post(
        &format!("/api/leadership/cohorts/{cohort}/departure"),
        &json!({"member_id": members[3], "reason": "schedule"}),
    )
    .await;
    app.post(
        &format!("/api/leadership/cohorts/{cohort}/departure"),
        &json!({"member_id": members[4], "reason": "found_work"}),
    )
    .await;

    let body: Value = app
        .post(
            &format!("/api/leadership/cohorts/{cohort}/conclude"),
            &json!({"note": "Ran to the end"}),
        )
        .await
        .json()
        .await
        .unwrap();
    let outcomes = &body["data"]["outcomes"];

    assert_eq!(outcomes["joined_total"], 5);
    assert_eq!(outcomes["graduated_total"], 3);
    assert_eq!(outcomes["left_for_work"], 1);

    // Three of the four who were not lost to a job: seventy-five per cent,
    // which clears the threshold. Over the survivors it would have been a
    // hundred, which is the figure this refuses to publish.
    assert_eq!(outcomes["led_to_the_end"], true);

    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM attestations
          WHERE user_id = $1 AND revoked_at IS NULL
            AND basis = 'leadership_cohort_completed'",
    )
    .bind(lead)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(count, 1, "concluding did not produce the attestation");
}

#[tokio::test]
async fn a_cohort_that_lost_most_of_its_people_earns_nothing() {
    let app = TestApp::spawn().await;
    let lead = a_talent(&app, "lead_badcohort").await;
    let cohort = a_cohort(&app, lead, "lead-badcohort-run").await;

    let mut members = Vec::new();
    for n in 0..5 {
        let m = a_talent(&app, &format!("badcohort_m{n}")).await;
        join_cohort(&app, cohort, m).await;
        members.push(m);
    }

    app.login("lead_badcohort").await;
    app.post(
        &format!("/api/leadership/cohorts/{cohort}/lead"),
        &json!({}),
    )
    .await;

    app.post(
        &format!("/api/leadership/cohorts/{cohort}/graduate"),
        &json!({"member_id": members[0]}),
    )
    .await;
    for m in &members[1..5] {
        app.post(
            &format!("/api/leadership/cohorts/{cohort}/departure"),
            &json!({"member_id": m, "reason": "level_mismatch"}),
        )
        .await;
    }

    let body: Value = app
        .post(
            &format!("/api/leadership/cohorts/{cohort}/conclude"),
            &json!({}),
        )
        .await
        .json()
        .await
        .unwrap();

    assert_eq!(body["data"]["outcomes"]["led_to_the_end"], false);

    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM attestations
          WHERE user_id = $1 AND basis = 'leadership_cohort_completed'",
    )
    .bind(lead)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(count, 0, "a cohort that lost four of five was attested");
}

#[tokio::test]
async fn a_cohort_abandoned_mid_run_is_not_a_cohort_that_finished() {
    let app = TestApp::spawn().await;
    let lead = a_talent(&app, "lead_abandoned").await;
    let cohort = a_cohort(&app, lead, "lead-abandoned-run").await;

    for n in 0..3 {
        let m = a_talent(&app, &format!("abandoned_m{n}")).await;
        join_cohort(&app, cohort, m).await;
        app.login("lead_abandoned").await;
        app.post(
            &format!("/api/leadership/cohorts/{cohort}/lead"),
            &json!({}),
        )
        .await;
        app.post(
            &format!("/api/leadership/cohorts/{cohort}/graduate"),
            &json!({"member_id": m}),
        )
        .await;
    }

    // Everybody graduated and nobody concluded it. `ends_at` in the past means
    // the planned window closed, which is not the same as somebody bringing it
    // to an end.
    let led: bool = sqlx::query_scalar(
        "SELECT led_to_the_end FROM leadership_cohort_outcomes WHERE cohort_id = $1",
    )
    .bind(cohort)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(!led, "an unconcluded cohort counted as led to the end");
}

#[tokio::test]
async fn a_departure_says_why() {
    let app = TestApp::spawn().await;
    let lead = a_talent(&app, "lead_noreason").await;
    let cohort = a_cohort(&app, lead, "lead-noreason-run").await;
    let member = a_talent(&app, "noreason_member").await;
    join_cohort(&app, cohort, member).await;

    app.login("lead_noreason").await;
    app.post(
        &format!("/api/leadership/cohorts/{cohort}/lead"),
        &json!({}),
    )
    .await;

    let resp = app
        .post(
            &format!("/api/leadership/cohorts/{cohort}/departure"),
            &json!({"member_id": member, "reason": "because"}),
        )
        .await;
    assert_eq!(resp.status(), 400, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn the_wizard_asks_this_domain_its_own_questions() {
    let app = TestApp::spawn().await;
    a_talent(&app, "lead_wizard").await;
    app.login("lead_wizard").await;

    let resp = app
        .get("/api/users/me/domain-profile/leadership/questions")
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let text = resp.text().await.unwrap();

    for key in [
        "leadership_level",
        "leadership_context",
        "leadership_target_domains",
        "leadership_tools",
    ] {
        assert!(text.contains(key), "the wizard does not ask {key}");
    }
}
