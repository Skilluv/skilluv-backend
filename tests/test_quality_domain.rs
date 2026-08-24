//! The quality domain.
//!
//! Five trades whose only shared question is "what would have to be true for
//! this to be wrong, and did anybody check". These tests hold the places where
//! that question is enforced by the schema rather than by good intentions:
//!
//!   * a defect report becomes a proof only when the fix shipped **and** the
//!     person who found it went back to look;
//!   * the severity that counts is the reviewer's;
//!   * an imported figure is not evidence until somebody opened the link;
//!   * review rights are granted per family and reach no other.

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
         VALUES ($1, $2, 'test')
         ON CONFLICT DO NOTHING",
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

/// A quality slice held by `owner`, of one subtype, aimed at one domain.
async fn a_qa_slice(
    app: &TestApp,
    project: Uuid,
    owner: Uuid,
    subtype: &str,
    target_domain: Option<&str>,
    orientation_slug: &str,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO project_slices
             (project_id, slice_type, title, description, primary_domain,
              difficulty, qa_subtype, target_domain, claimed_by_user_id,
              claimed_at, orientation_id)
         VALUES ($1, 'qa_report', 'A piece of quality work', 'Description',
                 'quality', 2, $2, $3, $4, NOW(),
                 (SELECT id FROM orientations WHERE slug = $5))
         RETURNING id",
    )
    .bind(project)
    .bind(subtype)
    .bind(target_domain)
    .bind(owner)
    .bind(orientation_slug)
    .fetch_one(&app.db)
    .await
    .unwrap()
}

/// A verified deliverable on a slice, which is what an attestation rests on.
async fn a_verified_deliverable(app: &TestApp, slice: Uuid, user: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO deliverables
             (slice_id, user_id, artifact_type, artifact_url, verifiable_by,
              verification_status, verified_at)
         VALUES ($1, $2, 'documentation', 'https://example.org/report',
                 'human_review', 'verified', NOW())
         RETURNING id",
    )
    .bind(slice)
    .bind(user)
    .fetch_one(&app.db)
    .await
    .unwrap()
}

fn a_bug_body(slice: Uuid) -> Value {
    json!({
        "slice_id": slice,
        "title": "The invoice list never resolves for accounts with no orders",
        "repro_steps_md": "1. Sign in as a brand new account\n2. Open /invoices\n3. Wait",
        "expected_md": "An empty state saying there are no invoices yet",
        "observed_md": "A spinner that never resolves, and no network activity after 2s",
        "environment": {"os": "Windows 11", "browser": "Firefox 128", "build": "2026.8.1"},
        "severity": "high",
        "reproducibility": "always"
    })
}

// ═══════════════════════════════════════════════════════════════════
// The catalogue
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn quality_is_an_open_domain_with_five_trades() {
    let app = TestApp::spawn().await;

    let active: bool =
        sqlx::query_scalar("SELECT is_active FROM skill_domains WHERE slug = 'quality'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(
        active,
        "the domain row existed since 0400 with is_active = FALSE; opening it is what \
         this branch does"
    );

    let resp = app.get("/api/quality/reference").await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    let orientations = body["data"]["orientations"].as_array().unwrap();

    assert_eq!(orientations.len(), 5);
    for slug in ["qa-code", "qa-cyber", "qa-design", "qa-game", "qa-lead"] {
        assert!(
            orientations.iter().any(|o| o["slug"] == slug),
            "{slug} is missing"
        );
    }
    assert!(
        orientations.iter().all(|o| o["reviewer_group"].is_string()),
        "every trade belongs to a review family, or nobody can be given rights over it"
    );
}

/// The mirror in `validators.rs` and the table agree.
///
/// `test_skill_domains.rs` asserts this for every domain. It is repeated here
/// because opening a domain is exactly the moment the two drift, and a failure
/// in this file names the domain that caused it.
#[tokio::test]
async fn the_domain_list_learned_about_quality() {
    assert!(skilluv_backend::validators::SKILL_DOMAINS.contains(&"quality"));
}

#[tokio::test]
async fn every_trade_makes_its_review_capability_grantable() {
    let app = TestApp::spawn().await;

    // Nothing in this branch's migrations names these. Migration 0404's
    // trigger derives them from the orientations, which is the whole point of
    // that migration and the reason the backlog's `qa_reviewer:*` would have
    // been ungrantable.
    for capability in [
        "quality_reviewer:automation",
        "quality_reviewer:intrusion",
        "quality_reviewer:usability",
        "quality_reviewer:playtest",
        "quality_reviewer:strategy",
        "quality_reviewer:all",
        "challenge_validator:quality",
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
async fn the_skill_map_lost_nothing_on_the_way_in() {
    let app = TestApp::spawn().await;

    // Migration 0454 joins on slugs. A slug that does not exist drops out
    // silently, which is exactly the failure mode a JOIN has and a subquery
    // does not — so the count is asserted rather than trusted.
    let mapped: Vec<(String, i64)> = sqlx::query_as(
        "SELECT o.slug, count(*)
           FROM orientation_skill_map m
           JOIN orientations o ON o.id = m.orientation_id
          WHERE o.primary_domain = 'quality'
          GROUP BY o.slug ORDER BY o.slug",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert_eq!(mapped.len(), 5, "a trade came out of the map with nothing");
    let total: i64 = mapped.iter().map(|(_, n)| n).sum();
    assert_eq!(
        total, 66,
        "the map is short — a skill slug in 0454 does not exist: {mapped:?}"
    );

    // The map points at nodes other domains declared rather than re-creating
    // them under a `qa-` prefix. Two nodes meaning one competence is worse
    // than a missing one.
    let borrowed: i64 = sqlx::query_scalar(
        "SELECT count(*)
           FROM orientation_skill_map m
           JOIN orientations o ON o.id = m.orientation_id
           JOIN skill_nodes s ON s.id = m.skill_id
          WHERE o.primary_domain = 'quality' AND s.domain <> 'quality'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(
        borrowed >= 10,
        "the quality map re-created skills the tree already had: {borrowed} borrowed"
    );
}

/// Every seeded badge rule counts something the engine implements.
///
/// The proof-type half of this is checked for design; the dimension half was
/// not checked anywhere, and a rule naming an unimplemented `distinct_over`
/// fails at recompute time on somebody's account rather than in CI.
#[tokio::test]
async fn every_seeded_rule_counts_something_real() {
    let app = TestApp::spawn().await;

    let rules: Vec<(String, Value)> =
        sqlx::query_as("SELECT slug, conditions FROM badge_rules WHERE deprecated_at IS NULL")
            .fetch_all(&app.db)
            .await
            .unwrap();

    for (slug, conditions) in rules {
        if let Some(dimension) = conditions.get("distinct_over").and_then(Value::as_str) {
            assert!(
                skilluv_backend::services::badge_engine::DISTINCT_DIMENSIONS.contains(&dimension),
                "badge rule '{slug}' counts distinct '{dimension}', which nothing implements"
            );
        }
        if let Some(basis) = conditions.get("attestation_basis").and_then(Value::as_str) {
            let known: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM attestation_bases WHERE basis = $1)",
            )
            .bind(basis)
            .fetch_one(&app.db)
            .await
            .unwrap();
            assert!(
                known,
                "badge rule '{slug}' rests on '{basis}', which no attestation can carry"
            );
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// Defect reports
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn a_report_a_stranger_could_not_follow_is_refused() {
    let app = TestApp::spawn().await;
    let user = a_talent(&app, "qa_short").await;
    app.login("qa_short").await;
    let project = a_project(&app, user, "qa-short-project").await;
    let slice = a_qa_slice(&app, project, user, "bug_report", Some("code"), "qa-code").await;

    let mut body = a_bug_body(slice);
    body["repro_steps_md"] = json!("it does not work");

    let resp = app.post("/api/quality/bugs", &body).await;
    assert_eq!(resp.status(), 400, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn a_report_nobody_can_situate_is_refused() {
    let app = TestApp::spawn().await;
    let user = a_talent(&app, "qa_noenv").await;
    app.login("qa_noenv").await;
    let project = a_project(&app, user, "qa-noenv-project").await;
    let slice = a_qa_slice(&app, project, user, "bug_report", Some("code"), "qa-code").await;

    let mut body = a_bug_body(slice);
    body["environment"] = json!({});

    let resp = app.post("/api/quality/bugs", &body).await;
    assert_eq!(resp.status(), 400, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn a_bug_report_cannot_hang_off_a_slice_that_is_not_one() {
    let app = TestApp::spawn().await;
    let user = a_talent(&app, "qa_wrongslice").await;
    app.login("qa_wrongslice").await;
    let project = a_project(&app, user, "qa-wrongslice-project").await;

    // A test plan slice, not a bug report one. The trigger refuses, because
    // the slice type is what decides the reviewer routing: a report hanging
    // off the wrong slice sits in a queue nobody reads.
    let slice = a_qa_slice(&app, project, user, "test_plan", Some("code"), "qa-code").await;

    let resp = app.post("/api/quality/bugs", &a_bug_body(slice)).await;
    assert!(
        resp.status().is_client_error() || resp.status().is_server_error(),
        "a bug report was accepted on a test-plan slice"
    );
}

#[tokio::test]
async fn a_report_cannot_be_filed_under_somebody_elses_slice() {
    let app = TestApp::spawn().await;
    let owner = a_talent(&app, "qa_owner").await;
    a_talent(&app, "qa_intruder").await;
    let project = a_project(&app, owner, "qa-owner-project").await;
    let slice = a_qa_slice(&app, project, owner, "bug_report", Some("code"), "qa-code").await;

    // Filing under somebody else's slice would hand them the attestation.
    app.login("qa_intruder").await;
    let resp = app.post("/api/quality/bugs", &a_bug_body(slice)).await;
    assert_eq!(resp.status(), 400, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn a_fix_is_confirmed_by_the_person_who_found_the_defect() {
    let app = TestApp::spawn().await;
    let reporter = a_talent(&app, "qa_reporter").await;
    a_talent(&app, "qa_bystander").await;
    app.login("qa_reporter").await;

    let project = a_project(&app, reporter, "qa-confirm-project").await;
    let slice = a_qa_slice(
        &app,
        project,
        reporter,
        "bug_report",
        Some("code"),
        "qa-code",
    )
    .await;

    let resp = app.post("/api/quality/bugs", &a_bug_body(slice)).await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    let report_id = body["data"]["report"]["id"].as_str().unwrap().to_string();

    // Nothing to confirm yet: no fix has been named.
    let resp = app
        .post(
            &format!("/api/quality/bugs/{report_id}/confirm"),
            &json!({}),
        )
        .await;
    assert_eq!(
        resp.status(),
        400,
        "a defect with no fix link was confirmable"
    );

    let resp = app
        .post(
            &format!("/api/quality/bugs/{report_id}/fix"),
            &json!({"fix_url": "https://github.com/example/repo/pull/42"}),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    // Somebody else cannot confirm it. A confirmation by anybody but the
    // reporter is the claim restated by a person with an interest in it being
    // true, which is what the column exists to distinguish from a merge.
    app.login("qa_bystander").await;
    let resp = app
        .post(
            &format!("/api/quality/bugs/{report_id}/confirm"),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), 400, "a bystander confirmed somebody's fix");

    app.login("qa_reporter").await;
    let resp = app
        .post(
            &format!("/api/quality/bugs/{report_id}/confirm"),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    assert!(body["data"]["report"]["fix_confirmed_at"].is_string());
}

#[tokio::test]
async fn confirming_twice_does_not_move_the_date_of_the_check() {
    let app = TestApp::spawn().await;
    let reporter = a_talent(&app, "qa_twice").await;
    app.login("qa_twice").await;
    let project = a_project(&app, reporter, "qa-twice-project").await;
    let slice = a_qa_slice(
        &app,
        project,
        reporter,
        "bug_report",
        Some("code"),
        "qa-code",
    )
    .await;

    let body: Value = app
        .post("/api/quality/bugs", &a_bug_body(slice))
        .await
        .json()
        .await
        .unwrap();
    let id = body["data"]["report"]["id"].as_str().unwrap().to_string();

    app.post(
        &format!("/api/quality/bugs/{id}/fix"),
        &json!({"fix_url": "https://github.com/example/repo/pull/1"}),
    )
    .await;

    let first: Value = app
        .post(&format!("/api/quality/bugs/{id}/confirm"), &json!({}))
        .await
        .json()
        .await
        .unwrap();
    let second: Value = app
        .post(&format!("/api/quality/bugs/{id}/confirm"), &json!({}))
        .await
        .json()
        .await
        .unwrap();

    assert_eq!(
        first["data"]["report"]["fix_confirmed_at"], second["data"]["report"]["fix_confirmed_at"],
        "a double-clicked button rewrote when the check happened"
    );
}

#[tokio::test]
async fn a_reviewers_severity_is_kept_beside_the_reporters_not_instead_of_it() {
    let app = TestApp::spawn().await;
    let reporter = a_talent(&app, "qa_sev_rep").await;
    let reviewer = a_talent(&app, "qa_sev_rev").await;
    grant(&app, reviewer, "quality_reviewer:automation").await;

    app.login("qa_sev_rep").await;
    let project = a_project(&app, reporter, "qa-sev-project").await;
    let slice = a_qa_slice(
        &app,
        project,
        reporter,
        "bug_report",
        Some("code"),
        "qa-code",
    )
    .await;
    let body: Value = app
        .post("/api/quality/bugs", &a_bug_body(slice))
        .await
        .json()
        .await
        .unwrap();
    let id = body["data"]["report"]["id"].as_str().unwrap().to_string();

    app.login("qa_sev_rev").await;
    let resp = app
        .post(
            &format!("/api/quality/bugs/{id}/review"),
            &json!({"decision": "accept", "severity_adjusted_to": "medium"}),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    let row: (String, Option<String>) = sqlx::query_as(
        "SELECT severity, severity_adjusted_to FROM quality_bug_reports WHERE id = $1::UUID",
    )
    .bind(&id)
    .fetch_one(&app.db)
    .await
    .unwrap();

    assert_eq!(row.0, "high", "the reporter's figure was overwritten");
    assert_eq!(row.1.as_deref(), Some("medium"));
}

#[tokio::test]
async fn a_rejection_says_why() {
    let app = TestApp::spawn().await;
    let reporter = a_talent(&app, "qa_rej_rep").await;
    let reviewer = a_talent(&app, "qa_rej_rev").await;
    grant(&app, reviewer, "quality_reviewer:automation").await;

    app.login("qa_rej_rep").await;
    let project = a_project(&app, reporter, "qa-rej-project").await;
    let slice = a_qa_slice(
        &app,
        project,
        reporter,
        "bug_report",
        Some("code"),
        "qa-code",
    )
    .await;
    let body: Value = app
        .post("/api/quality/bugs", &a_bug_body(slice))
        .await
        .json()
        .await
        .unwrap();
    let id = body["data"]["report"]["id"].as_str().unwrap().to_string();

    app.login("qa_rej_rev").await;
    let resp = app
        .post(
            &format!("/api/quality/bugs/{id}/review"),
            &json!({"decision": "reject"}),
        )
        .await;
    assert_eq!(
        resp.status(),
        400,
        "a rejection with no reason is a refusal with no appeal"
    );
}

#[tokio::test]
async fn review_rights_are_granted_per_family_and_reach_no_other() {
    let app = TestApp::spawn().await;
    let reporter = a_talent(&app, "qa_fam_rep").await;
    let reviewer = a_talent(&app, "qa_fam_rev").await;
    // Can read a test suite. Says nothing about reading a usability protocol.
    grant(&app, reviewer, "quality_reviewer:automation").await;

    app.login("qa_fam_rep").await;
    let project = a_project(&app, reporter, "qa-fam-project").await;
    let slice = a_qa_slice(
        &app,
        project,
        reporter,
        "bug_report",
        Some("design"),
        "qa-design",
    )
    .await;
    let body: Value = app
        .post("/api/quality/bugs", &a_bug_body(slice))
        .await
        .json()
        .await
        .unwrap();
    let id = body["data"]["report"]["id"].as_str().unwrap().to_string();

    app.login("qa_fam_rev").await;
    let resp = app
        .post(
            &format!("/api/quality/bugs/{id}/review"),
            &json!({"decision": "accept"}),
        )
        .await;
    assert_eq!(
        resp.status(),
        403,
        "automation review rights reached a usability report"
    );
}

// ═══════════════════════════════════════════════════════════════════
// Imported test runs
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn an_imported_run_has_a_link_somebody_can_open() {
    let app = TestApp::spawn().await;
    let user = a_talent(&app, "qa_run_link").await;
    app.login("qa_run_link").await;
    let project = a_project(&app, user, "qa-run-link-project").await;
    let slice = a_qa_slice(
        &app,
        project,
        user,
        "test_automation",
        Some("code"),
        "qa-code",
    )
    .await;

    let resp = app
        .post(
            "/api/quality/test-runs",
            &json!({
                "slice_id": slice,
                "source": "codecov",
                "report_url": "not-a-url",
                "tests_total": 100
            }),
        )
        .await;
    assert_eq!(resp.status(), 400, "a figure with no source was accepted");
}

#[tokio::test]
async fn a_run_cannot_report_more_failures_than_tests() {
    let app = TestApp::spawn().await;
    let user = a_talent(&app, "qa_run_math").await;
    app.login("qa_run_math").await;
    let project = a_project(&app, user, "qa-run-math-project").await;
    let slice = a_qa_slice(
        &app,
        project,
        user,
        "test_automation",
        Some("code"),
        "qa-code",
    )
    .await;

    let resp = app
        .post(
            "/api/quality/test-runs",
            &json!({
                "slice_id": slice,
                "source": "junit_xml",
                "report_url": "https://example.org/report.xml",
                "tests_total": 10,
                "tests_failed": 8,
                "tests_skipped": 5
            }),
        )
        .await;
    assert_eq!(resp.status(), 400, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn re_importing_a_run_replaces_it_and_drops_its_verification() {
    let app = TestApp::spawn().await;
    let importer = a_talent(&app, "qa_reimport").await;
    let reviewer = a_talent(&app, "qa_reimport_rev").await;
    grant(&app, reviewer, "quality_reviewer:automation").await;

    app.login("qa_reimport").await;
    let project = a_project(&app, importer, "qa-reimport-project").await;
    let slice = a_qa_slice(
        &app,
        project,
        importer,
        "test_automation",
        Some("code"),
        "qa-code",
    )
    .await;

    let run = json!({
        "slice_id": slice,
        "source": "codecov",
        "report_url": "https://example.org/run/1",
        "tests_total": 100,
        "tests_failed": 2
    });
    let body: Value = app
        .post("/api/quality/test-runs", &run)
        .await
        .json()
        .await
        .unwrap();
    let id = body["data"]["run"]["id"].as_str().unwrap().to_string();

    app.login("qa_reimport_rev").await;
    let resp = app
        .post(&format!("/api/quality/test-runs/{id}/verify"), &json!({}))
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    // The same run re-imported is one run, not two — and it is new data, so
    // what the reviewer checked no longer describes the row.
    app.login("qa_reimport").await;
    let mut updated = run.clone();
    updated["tests_total"] = json!(120);
    let body: Value = app
        .post("/api/quality/test-runs", &updated)
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["run"]["id"].as_str().unwrap(), id);
    assert!(
        body["data"]["run"]["verified_at"].is_null(),
        "a re-import kept a verification of figures that have changed"
    );

    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM quality_test_runs WHERE slice_id = $1")
            .bind(slice)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(count, 1, "a re-import doubled somebody's figures");
}

#[tokio::test]
async fn nobody_verifies_their_own_import() {
    let app = TestApp::spawn().await;
    let user = a_talent(&app, "qa_selfverify").await;
    grant(&app, user, "quality_reviewer:all").await;
    app.login("qa_selfverify").await;

    let project = a_project(&app, user, "qa-selfverify-project").await;
    let slice = a_qa_slice(
        &app,
        project,
        user,
        "test_automation",
        Some("code"),
        "qa-code",
    )
    .await;

    let body: Value = app
        .post(
            "/api/quality/test-runs",
            &json!({
                "slice_id": slice,
                "source": "codecov",
                "report_url": "https://example.org/run/self",
                "tests_total": 10
            }),
        )
        .await
        .json()
        .await
        .unwrap();
    let id = body["data"]["run"]["id"].as_str().unwrap().to_string();

    let resp = app
        .post(&format!("/api/quality/test-runs/{id}/verify"), &json!({}))
        .await;
    assert_eq!(
        resp.status(),
        404,
        "somebody vouched for their own green badge"
    );
}

// ═══════════════════════════════════════════════════════════════════
// Attestations and the score
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn a_defect_report_is_not_a_proof_until_the_fix_is_confirmed() {
    let app = TestApp::spawn().await;
    let reporter = a_talent(&app, "qa_proof").await;
    app.login("qa_proof").await;

    let project = a_project(&app, reporter, "qa-proof-project").await;
    let slice = a_qa_slice(
        &app,
        project,
        reporter,
        "bug_report",
        Some("code"),
        "qa-code",
    )
    .await;
    a_verified_deliverable(&app, slice, reporter).await;

    // Verified deliverable, no confirmed fix: nothing to attest yet. This is
    // the one basis in the domain whose condition cannot be met alone.
    let issued = skilluv_backend::services::quality_attestations::issue_for_user(&app.db, reporter)
        .await
        .unwrap();
    assert!(
        issued.is_empty(),
        "a defect report attested before anybody fixed anything: {issued:?}"
    );

    let body: Value = app
        .post("/api/quality/bugs", &a_bug_body(slice))
        .await
        .json()
        .await
        .unwrap();
    let id = body["data"]["report"]["id"].as_str().unwrap().to_string();
    app.post(
        &format!("/api/quality/bugs/{id}/fix"),
        &json!({"fix_url": "https://github.com/example/repo/pull/7"}),
    )
    .await;
    app.post(&format!("/api/quality/bugs/{id}/confirm"), &json!({}))
        .await;

    // The endpoint recomputes the proof itself, so the attestation is already
    // there. Asserting on the row rather than on a second call's return value:
    // a generator that is idempotent — which this one has to be — reports
    // nothing on the second pass, and a test reading that as failure would be
    // testing the harness rather than the behaviour.
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM attestations
          WHERE user_id = $1 AND revoked_at IS NULL
            AND basis = 'quality_bug_report_validated'",
    )
    .bind(reporter)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(count, 1, "the confirmation did not produce the attestation");

    // Another pass issues nothing: the uniqueness index makes re-running free,
    // which is what lets the sweep run on everybody without doubling anything.
    let again = skilluv_backend::services::quality_attestations::issue_for_user(&app.db, reporter)
        .await
        .unwrap();
    assert!(again.is_empty(), "a second pass issued a duplicate");

    let still: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM attestations
          WHERE user_id = $1 AND revoked_at IS NULL
            AND basis = 'quality_bug_report_validated'",
    )
    .bind(reporter)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(still, 1);
}

#[tokio::test]
async fn a_document_artefact_attests_on_verification_alone() {
    let app = TestApp::spawn().await;
    let author = a_talent(&app, "qa_plan_author").await;

    let project = a_project(&app, author, "qa-plan-project").await;
    let slice = a_qa_slice(&app, project, author, "test_plan", Some("game"), "qa-code").await;
    a_verified_deliverable(&app, slice, author).await;

    let issued = skilluv_backend::services::quality_attestations::issue_for_user(&app.db, author)
        .await
        .unwrap();
    assert_eq!(issued, vec!["quality_test_plan_validated".to_string()]);
}

#[tokio::test]
async fn the_profile_breaks_down_what_was_tested_and_where() {
    let app = TestApp::spawn().await;
    let author = a_talent(&app, "qa_breakdown").await;
    let project = a_project(&app, author, "qa-breakdown-project").await;

    for (subtype, target, orientation) in [
        ("test_plan", "code", "qa-code"),
        ("playtest_report", "game", "qa-game"),
        ("a11y_audit", "design", "qa-design"),
    ] {
        let slice = a_qa_slice(&app, project, author, subtype, Some(target), orientation).await;
        a_verified_deliverable(&app, slice, author).await;
    }

    let resp = app.get("/api/users/qa_breakdown/quality-profile").await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();

    let breakdown = body["data"]["profile"]["target_domain_breakdown"]
        .as_array()
        .unwrap();
    assert_eq!(
        breakdown.len(),
        3,
        "the cross-domain breakdown lost a domain: {breakdown:?}"
    );

    // The score exists and is explained. A number nobody can decompose is a
    // number somebody has to trust.
    assert!(body["data"]["profile"]["score"]["breakdown"].is_array());
    assert!(body["data"]["profile"]["score"]["tier_slug"].is_string());
}

#[tokio::test]
async fn the_quality_tiers_kept_the_slugs_the_search_filters_on() {
    let app = TestApp::spawn().await;

    // Migration 0452 gives this domain its own words and keeps the slugs.
    // `craft_scores.tier_slug` is what the recruiter search filters on, and a
    // domain whose second tier is called something else is a domain the
    // "Senior and above" filter silently reads differently.
    let quality: Vec<String> = sqlx::query_scalar(
        "SELECT slug FROM craft_score_tiers WHERE skill_domain = 'quality' ORDER BY min_score",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();
    let code: Vec<String> = sqlx::query_scalar(
        "SELECT slug FROM craft_score_tiers WHERE skill_domain = 'code' ORDER BY min_score",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert_eq!(quality, code);

    let renamed: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM craft_score_tiers t
          WHERE t.skill_domain = 'quality'
            AND t.name <> (SELECT name FROM craft_score_tiers
                            WHERE skill_domain = 'code' AND slug = t.slug)",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    // Three of the six differ: `contributor` reads Tester, `engineer` reads
    // Quality Engineer, `staff` reads Quality Lead. Apprentice, Senior and
    // Principal are the same word in both domains, which is correct — they
    // are positions on a scale, not job titles.
    assert!(
        renamed >= 3,
        "the tiers kept the code vocabulary: only {renamed} were reworded"
    );
}

// ═══════════════════════════════════════════════════════════════════
// Cross-domain routing
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn reports_can_be_read_by_the_domain_they_were_aimed_at() {
    let app = TestApp::spawn().await;
    let author = a_talent(&app, "qa_routing").await;
    let project = a_project(&app, author, "qa-routing-project").await;

    for (subtype, target, orientation) in [
        ("test_plan", "code", "qa-code"),
        ("playtest_report", "game", "qa-game"),
    ] {
        let slice = a_qa_slice(&app, project, author, subtype, Some(target), orientation).await;
        a_verified_deliverable(&app, slice, author).await;
    }

    let body: Value = app
        .get("/api/quality/reports?target_domain=game")
        .await
        .json()
        .await
        .unwrap();
    let reports = body["data"]["reports"].as_array().unwrap();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0]["target_domain"], "game");

    let body: Value = app.get("/api/quality/reports").await.json().await.unwrap();
    assert_eq!(body["data"]["reports"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn a_domain_nothing_declares_is_refused_rather_than_matching_nothing() {
    let app = TestApp::spawn().await;

    // The backlog's own CHECK listed `cyber`, which is not a domain slug. A
    // filter on it would have matched nothing, silently, and read as an empty
    // platform.
    let resp = app.get("/api/quality/reports?target_domain=cyber").await;
    assert_eq!(resp.status(), 400, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn the_wizard_asks_this_domain_its_own_questions() {
    let app = TestApp::spawn().await;
    a_talent(&app, "qa_wizard").await;
    app.login("qa_wizard").await;

    let resp = app
        .get("/api/users/me/domain-profile/quality/questions")
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    let text = body.to_string();

    for key in [
        "quality_background",
        "quality_target_domains",
        "quality_tools",
    ] {
        assert!(text.contains(key), "the wizard does not ask {key}");
    }
}
