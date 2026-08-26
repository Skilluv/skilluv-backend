//! The security domain.
//!
//! Five trades whose shared question is "could somebody else get to the same
//! result". These tests hold the places where the answer is enforced by the
//! schema or by a state machine rather than by good intentions:
//!
//!   * a report against something nobody authorised is refused, not triaged;
//!   * a triager may not confirm, and only an administrator publishes;
//!   * a confirmed finding creates the deliverable that moves a rank, once;
//!   * a captured flag creates none, because the answer was planted;
//!   * a severity override keeps what the reporter claimed, and needs an
//!     argument;
//!   * a duplicate earns a co-credit and nothing is merged by a machine;
//!   * an offensive mission cannot leave draft without a written authorisation.

mod common;
use common::TestApp;
use serde_json::{Value, json};
use uuid::Uuid;

// ═══════════════════════════════════════════════════════════════════
// Fixtures
// ═══════════════════════════════════════════════════════════════════

async fn a_person(app: &TestApp, username: &str) -> Uuid {
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

/// A report that passes every shape check, so a test can vary one thing.
fn a_report(title: &str) -> Value {
    json!({
        "title": title,
        "description_md": "The search parameter reaches the query without being \
                           bound, so a quote changes the statement rather than \
                           the value.",
        "reproduction_steps_md": "1. Log in as any user\n2. Call \
                                  /api/users?search=' OR 1=1--\n3. Read the rows",
        "target_kind": "platform",
        "target_host": "staging.skill-uv.com",
        "affected_endpoint": "GET /api/users",
        "severity_tier": "high",
        "cwe_id": "CWE-89",
    })
}

async fn submit(app: &TestApp, body: &Value) -> (reqwest::StatusCode, Value) {
    let resp = app.post("/api/security/reports", body).await;
    let status = resp.status();
    let body: Value = resp.json().await.expect("json");
    (status, body)
}

/// Submit one report as `username` and return its id.
async fn a_finding(app: &TestApp, username: &str, title: &str) -> Uuid {
    app.login(username).await;
    let (status, body) = submit(app, &a_report(title)).await;
    assert_eq!(status, 200, "submit said: {body}");
    body["data"]["report"]["id"]
        .as_str()
        .expect("an id")
        .parse()
        .unwrap()
}

async fn transition(app: &TestApp, id: Uuid, body: &Value) -> (reqwest::StatusCode, Value) {
    let resp = app
        .post(
            &format!("/api/admin/security/findings/{id}/transition"),
            body,
        )
        .await;
    let status = resp.status();
    (status, resp.json().await.expect("json"))
}

// ═══════════════════════════════════════════════════════════════════
// The catalogue
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn security_is_an_open_domain_with_five_trades() {
    let app = TestApp::spawn().await;

    let active: bool =
        sqlx::query_scalar("SELECT is_active FROM skill_domains WHERE slug = 'security'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(active, "the domain has to be choosable");

    let trades: Vec<String> = sqlx::query_scalar(
        "SELECT slug FROM orientations
          WHERE primary_domain = 'security' AND is_curated AND NOT is_archived
          ORDER BY slug",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert_eq!(
        trades,
        vec![
            "security-blue-team",
            "security-code-audit",
            "security-governance",
            "security-purple-team",
            "security-red-team",
        ]
    );

    // The four that were there are archived rather than deleted, so the people
    // who chose one keep it in their history — and each says where it went.
    for legacy in [
        "security-engineer",
        "pentester-web",
        "pentester-mobile",
        "soc-analyst",
    ] {
        let (archived, replaced): (bool, Option<Uuid>) =
            sqlx::query_as("SELECT is_archived, replaced_by FROM orientations WHERE slug = $1")
                .bind(legacy)
                .fetch_one(&app.db)
                .await
                .unwrap();
        assert!(archived, "{legacy} is still choosable");
        assert!(replaced.is_some(), "{legacy} does not say what replaces it");
    }

    // And nothing live is left without a review family. A curated orientation
    // with a null `reviewer_group` is one nobody can be granted review rights
    // for, and this domain had four of them.
    let unreviewable: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM orientations
          WHERE primary_domain = 'security' AND is_curated
            AND NOT is_archived AND reviewer_group IS NULL",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(unreviewable, 0);
}

#[tokio::test]
async fn every_trade_makes_its_review_capability_grantable() {
    let app = TestApp::spawn().await;

    // Derived by the trigger of 0404 from the orientation rows, not written by
    // hand — so a sixth trade added later becomes grantable without anybody
    // editing a list.
    for family in [
        "red-team",
        "blue-team",
        "code-audit",
        "governance",
        "purple-team",
    ] {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM capability_catalog WHERE capability = $1)",
        )
        .bind(format!("security_reviewer:{family}"))
        .fetch_one(&app.db)
        .await
        .unwrap();
        assert!(exists, "security_reviewer:{family} is not grantable");
    }

    // And triage is its own capability, not the bottom of the reviewer ladder.
    let triager: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM capability_catalog
                         WHERE capability = 'security_triager')",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(triager);
}

#[tokio::test]
async fn every_trade_has_a_review_grid_and_a_first_month() {
    let app = TestApp::spawn().await;

    let grids: i64 =
        sqlx::query_scalar("SELECT count(*) FROM review_grids WHERE domain = 'security'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    // Five families plus the domain default read when a submission arrives
    // without one.
    assert_eq!(grids, 6);

    // Both locales, per trade. A guide that exists in one language quietly
    // becomes the only one anybody reads.
    for family in [
        "red-team",
        "blue-team",
        "code-audit",
        "governance",
        "purple-team",
    ] {
        let locales: Vec<String> = sqlx::query_scalar(
            "SELECT locale FROM content_guides
              WHERE skill_domain = 'security' AND kind = 'onboarding'
                AND reviewer_group = $1
              ORDER BY locale",
        )
        .bind(family)
        .fetch_all(&app.db)
        .await
        .unwrap();
        assert_eq!(locales, vec!["en", "fr"], "{family} onboarding");
    }
}

#[tokio::test]
async fn the_craft_score_counts_something_for_every_basis_worth_counting() {
    let app = TestApp::spawn().await;

    let weights: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM craft_score_weights
          WHERE skill_domain = 'security' AND is_active",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(
        weights >= 18,
        "the formula is nearly empty: {weights} terms"
    );

    // The tier slugs are what the talent search filters on, and they are shared
    // across domains — a domain that renamed them would drop out of the filter.
    let slugs: Vec<String> = sqlx::query_scalar(
        "SELECT slug FROM craft_score_tiers
          WHERE skill_domain = 'security' ORDER BY min_score",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();
    assert_eq!(
        slugs,
        vec![
            "apprentice",
            "contributor",
            "engineer",
            "senior",
            "staff",
            "principal"
        ]
    );

    // And the editorial position, asserted rather than trusted: one confirmed
    // critical finding has to be worth more than twenty captured flags.
    let critical: f64 = sqlx::query_scalar::<_, bigdecimal::BigDecimal>(
        "SELECT weight FROM craft_score_weights
          WHERE skill_domain = 'security' AND term = 'findings_high_or_critical'",
    )
    .fetch_one(&app.db)
    .await
    .map(|w| w.to_string().parse().unwrap())
    .unwrap();
    let flag: f64 = sqlx::query_scalar::<_, bigdecimal::BigDecimal>(
        "SELECT weight FROM craft_score_weights
          WHERE skill_domain = 'security' AND term = 'ctf_solved'",
    )
    .fetch_one(&app.db)
    .await
    .map(|w| w.to_string().parse().unwrap())
    .unwrap();
    assert!(
        critical > flag * 10.0,
        "a planted answer is worth too much: {flag} against {critical}"
    );
}

#[tokio::test]
async fn the_practice_catalogue_is_seeded_as_drafts_and_claims_no_secret() {
    let app = TestApp::spawn().await;

    let seeded: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM challenge_templates
          WHERE skill_domain = 'security' AND security_kind IS NOT NULL",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(seeded >= 40, "only {seeded} security challenges seeded");

    // Drafts, like every other domain's seeds: a challenge nobody has reviewed
    // must not be offered to somebody learning.
    let published: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM challenge_templates
          WHERE skill_domain = 'security' AND security_kind IS NOT NULL
            AND status <> 'draft'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(published, 0);

    // And none of them claims a flag or an answer the author could not have
    // known. That is the failure mode 0558 refuses at length: a guessed hash
    // produces a challenge nobody can ever pass, and nothing errors.
    let machine_checked: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM challenge_templates
          WHERE skill_domain = 'security'
            AND (security_flag_hash IS NOT NULL
                 OR security_lab_questions IS NOT NULL)",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(
        machine_checked, 0,
        "a seeded challenge claims a secret the migration author invented"
    );

    // Every external target says where it is and under whose terms.
    let unattributed: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM challenge_templates
          WHERE skill_domain = 'security'
            AND security_kind IN ('machine_walkthrough', 'training_ground',
                                  'analysis_exercise')
            AND (security_external_url IS NULL
                 OR security_attribution_md IS NULL)",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(unattributed, 0);
}

#[tokio::test]
async fn the_wizard_asks_this_domain_its_own_questions() {
    let app = TestApp::spawn().await;
    a_person(&app, "wizardsec").await;
    app.login("wizardsec").await;

    let resp = app
        .get("/api/users/me/domain-profile/security/questions")
        .await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let text = body.to_string();

    // The question that has no equivalent anywhere else: what can you actually
    // run. Recommending a vulnerable virtual machine to somebody on a
    // locked-down laptop wastes their week.
    assert!(text.contains("security_lab_setup"), "{text}");
    assert!(text.contains("security_certifications"), "{text}");
    assert!(text.contains("security_tools"), "{text}");
}

#[tokio::test]
async fn the_scope_is_readable_without_an_account() {
    let app = TestApp::spawn().await;

    // No login. A researcher decides what to touch before they have an
    // account, and a scope behind a login is a scope nobody reads.
    let resp = app.get("/api/security/scope").await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();

    let hosts = body["data"]["in_scope_hosts"].as_array().expect("hosts");
    assert!(!hosts.is_empty());
    assert!(
        hosts
            .iter()
            .all(|h| h.as_str().unwrap().ends_with("skill-uv.com")),
        "the scope reaches somebody else's host: {hosts:?}"
    );
    // Denial of service is out of scope in the document as well as in fact.
    assert!(
        body["data"]["out_of_scope"].to_string().contains("denial"),
        "{body}"
    );
}

// ═══════════════════════════════════════════════════════════════════
// Reporting
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn a_report_against_something_nobody_authorised_is_refused() {
    let app = TestApp::spawn().await;
    a_person(&app, "hunter").await;
    app.login("hunter").await;

    let mut out_of_scope = a_report("Something on a host that is not ours");
    out_of_scope["target_host"] = json!("example.com");

    let (status, body) = submit(&app, &out_of_scope).await;
    assert_eq!(status, 400, "{body}");
    assert!(
        body.to_string().contains("scope"),
        "the refusal has to say why: {body}"
    );

    // And nothing was written. An unauthorised report accepted and then
    // rejected is still a record of an unauthorised test.
    let stored: i64 = sqlx::query_scalar("SELECT count(*) FROM security_findings")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(stored, 0);
}

#[tokio::test]
async fn a_report_a_stranger_could_not_follow_is_refused() {
    let app = TestApp::spawn().await;
    a_person(&app, "terse").await;
    app.login("terse").await;

    let mut thin = a_report("It breaks");
    thin["reproduction_steps_md"] = json!("it broke");
    let (status, body) = submit(&app, &thin).await;
    assert_eq!(status, 400, "{body}");

    let mut vague = a_report("A real title, and nothing behind it");
    vague["description_md"] = json!("sqli");
    let (status, body) = submit(&app, &vague).await;
    assert_eq!(status, 400, "{body}");
}

#[tokio::test]
async fn a_vector_decides_the_severity_and_a_claim_does_not() {
    let app = TestApp::spawn().await;
    a_person(&app, "vectorist").await;
    app.login("vectorist").await;

    let mut with_vector = a_report("Total compromise, understated on purpose");
    // The reporter claims `low` and sends the vector of a 9.8. The vector wins:
    // one is an argument, the other is an adjective.
    with_vector["severity_tier"] = json!("low");
    with_vector["cvss_vector"] = json!("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H");

    let (status, body) = submit(&app, &with_vector).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["data"]["report"]["severity_tier"], "critical");
    assert_eq!(body["data"]["report"]["cvss_score"], 9.8);

    // A vector that does not parse is refused rather than silently becoming a
    // zero, which would read as harmless on a report that might be critical.
    let mut bad = a_report("A vector nobody can read");
    bad["cvss_vector"] = json!("CVSS:3.1/AV:Q/AC:L");
    let (status, body) = submit(&app, &bad).await;
    assert_eq!(status, 400, "{body}");
}

#[tokio::test]
async fn a_reporter_can_withdraw_and_nothing_else() {
    let app = TestApp::spawn().await;
    a_person(&app, "reporter1").await;
    let id = a_finding(&app, "reporter1", "A finding to take back").await;

    // Not a transition of their own choosing.
    let (status, _) = transition(&app, id, &json!({ "to": "confirmed" })).await;
    assert_ne!(status, 200, "a reporter confirmed their own finding");

    let resp = app
        .post(&format!("/api/security/reports/{id}/withdraw"), &json!({}))
        .await;
    assert_eq!(resp.status(), 200);

    let status: String = sqlx::query_scalar("SELECT status FROM security_findings WHERE id = $1")
        .bind(id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(status, "withdrawn");
}

#[tokio::test]
async fn a_triager_reads_and_does_not_confirm() {
    let app = TestApp::spawn().await;
    a_person(&app, "reporter2").await;
    let id = a_finding(&app, "reporter2", "A finding for a triager").await;

    let triager = a_person(&app, "triager").await;
    grant(&app, triager, "security_triager").await;
    app.login("triager").await;

    // Triage: allowed.
    let (status, body) = transition(
        &app,
        id,
        &json!({ "to": "triaged", "triage_notes_md": "worth reproducing" }),
    )
    .await;
    assert_eq!(status, 200, "{body}");

    // Confirmation: not. Confirming asserts publicly that a vulnerability is
    // real, which is a different judgement from whether it deserves an hour.
    let (status, body) = transition(&app, id, &json!({ "to": "confirmed" })).await;
    assert_eq!(status, 409, "a triager confirmed a finding: {body}");
}

#[tokio::test]
async fn confirming_creates_the_deliverable_that_moves_a_rank_once() {
    let app = TestApp::spawn().await;
    let reporter = a_person(&app, "reporter3").await;
    let id = a_finding(&app, "reporter3", "A finding worth confirming").await;

    let reviewer = a_person(&app, "reviewer").await;
    grant(&app, reviewer, "security_reviewer:red-team").await;
    app.login("reviewer").await;

    let (status, body) = transition(&app, id, &json!({ "to": "triaged" })).await;
    assert_eq!(status, 200, "{body}");
    let (status, body) = transition(&app, id, &json!({ "to": "confirmed" })).await;
    assert_eq!(status, 200, "{body}");

    // The deliverable is the whole point: a vulnerability counts towards a rank
    // exactly as a merged contribution does.
    let (count, fragments): (i64, i64) = sqlx::query_as(
        "SELECT count(*), COALESCE(sum(fragments_awarded), 0)
           FROM deliverables WHERE security_finding_id = $1",
    )
    .bind(id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(count, 1);
    assert_eq!(fragments, 300, "a high finding is worth 300 fragments");

    let credited: i32 = sqlx::query_scalar("SELECT total_fragments FROM users WHERE id = $1")
        .bind(reporter)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert!(credited >= 300, "the reporter was not paid: {credited}");

    // And the embargo clock started, without anybody asking for it.
    let (stage, ends): (Option<String>, Option<chrono::DateTime<chrono::Utc>>) = sqlx::query_as(
        "SELECT disclosure_stage, embargo_ends_at FROM security_findings WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(stage.as_deref(), Some("embargoed"));
    assert!(ends.is_some(), "an embargo with no end is a promise");

    // The attestation, and only one of it.
    let attestations: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM attestations
          WHERE security_finding_id = $1 AND basis = 'security_finding_confirmed'",
    )
    .bind(id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(attestations, 1);
}

#[tokio::test]
async fn only_an_administrator_publishes() {
    let app = TestApp::spawn().await;
    a_person(&app, "reporter4").await;
    let id = a_finding(&app, "reporter4", "A finding on its way to public").await;

    let reviewer = a_person(&app, "reviewer2").await;
    grant(&app, reviewer, "security_reviewer:red-team").await;
    app.login("reviewer2").await;
    transition(&app, id, &json!({ "to": "triaged" })).await;
    transition(&app, id, &json!({ "to": "confirmed" })).await;

    // A reviewer cannot. Publication is irreversible in the way that matters,
    // because the internet keeps a copy.
    let (status, body) = transition(
        &app,
        id,
        &json!({ "to": "published", "writeup_url": "https://skill-uv.com/w/1" }),
    )
    .await;
    assert_eq!(status, 409, "a reviewer published a finding: {body}");

    let admin = a_person(&app, "adminsec").await;
    grant(&app, admin, "admin").await;
    app.login("adminsec").await;

    // And not without a write-up: the point of the last transition is that
    // somebody can read what happened.
    let (status, body) = transition(&app, id, &json!({ "to": "published" })).await;
    assert_eq!(status, 400, "{body}");

    let (status, body) = transition(
        &app,
        id,
        &json!({ "to": "published", "writeup_url": "https://skill-uv.com/w/1" }),
    )
    .await;
    assert_eq!(status, 200, "{body}");

    let published: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM attestations
          WHERE security_finding_id = $1 AND basis = 'security_finding_published'",
    )
    .bind(id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(published, 1);
}

#[tokio::test]
async fn nothing_skips_the_middle() {
    let app = TestApp::spawn().await;
    a_person(&app, "reporter5").await;
    let id = a_finding(&app, "reporter5", "A finding nobody has read").await;

    let admin = a_person(&app, "adminsec2").await;
    grant(&app, admin, "admin").await;
    app.login("adminsec2").await;

    // Even an administrator cannot publish an untriaged report.
    for to in ["published", "confirmed", "fixed"] {
        let (status, body) = transition(
            &app,
            id,
            &json!({ "to": to, "writeup_url": "https://skill-uv.com/w/2",
                     "fix_url": "https://github.com/x/y/pull/1" }),
        )
        .await;
        assert_eq!(status, 409, "submitted -> {to} was allowed: {body}");
    }
}

#[tokio::test]
async fn a_severity_override_keeps_the_claim_and_needs_an_argument() {
    let app = TestApp::spawn().await;
    a_person(&app, "reporter6").await;
    let id = a_finding(&app, "reporter6", "A finding rated generously").await;

    let reviewer = a_person(&app, "reviewer3").await;
    grant(&app, reviewer, "security_reviewer:red-team").await;
    app.login("reviewer3").await;

    // No argument, no override. An unexplained downgrade is the thing
    // researchers leave a platform over.
    let resp = app
        .post(
            &format!("/api/admin/security/findings/{id}/severity"),
            &json!({ "severity_tier": "low", "reason": "no" }),
        )
        .await;
    assert_eq!(resp.status(), 400);

    let resp = app
        .post(
            &format!("/api/admin/security/findings/{id}/severity"),
            &json!({
                "severity_tier": "low",
                "reason": "The endpoint requires an authenticated session with \
                           the same role, so PR:N does not hold."
            }),
        )
        .await;
    assert_eq!(resp.status(), 200);

    let (reported, settled): (String, String) = sqlx::query_as(
        "SELECT severity_reported_tier, severity_tier
           FROM security_findings WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(reported, "high", "the reporter's claim was overwritten");
    assert_eq!(settled, "low");

    // The disagreement is on the record with its reasoning.
    let events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM security_finding_events
          WHERE finding_id = $1 AND event = 'severity_changed' AND reason IS NOT NULL",
    )
    .bind(id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(events, 1);
}

#[tokio::test]
async fn the_public_card_withholds_the_title_until_publication() {
    let app = TestApp::spawn().await;
    a_person(&app, "reporter7").await;
    let id = a_finding(&app, "reporter7", "Injection in the export endpoint").await;

    // Not readable at all before confirmation.
    let resp = app.get(&format!("/api/security/findings/{id}")).await;
    assert_eq!(resp.status(), 404);

    let reviewer = a_person(&app, "reviewer4").await;
    grant(&app, reviewer, "security_reviewer:red-team").await;
    app.login("reviewer4").await;
    transition(&app, id, &json!({ "to": "triaged" })).await;
    transition(&app, id, &json!({ "to": "confirmed" })).await;

    let resp = app.get(&format!("/api/security/findings/{id}")).await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let card = &body["data"]["finding"];

    // The severity and the class are quotable — that is what an attestation on
    // this finding claims, so it has to be readable. The title is not: "SQL
    // injection in the export endpoint" is half the disclosure.
    assert_eq!(card["severity_tier"], "high");
    assert_eq!(card["cwe_id"], "CWE-89");
    assert!(card["title"].is_null(), "the title leaked: {card}");
    assert!(card["description_md"].is_null());
}

#[tokio::test]
async fn an_anonymous_reporter_is_not_named() {
    let app = TestApp::spawn().await;
    a_person(&app, "ghost").await;
    app.login("ghost").await;

    let mut anonymous = a_report("A finding by somebody who would rather not");
    anonymous["anonymous"] = json!(true);
    let (status, body) = submit(&app, &anonymous).await;
    assert_eq!(status, 200, "{body}");
    let id: Uuid = body["data"]["report"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    let reviewer = a_person(&app, "reviewer5").await;
    grant(&app, reviewer, "security_reviewer:red-team").await;
    app.login("reviewer5").await;
    transition(&app, id, &json!({ "to": "triaged" })).await;
    transition(&app, id, &json!({ "to": "confirmed" })).await;

    let resp = app.get("/api/security/hall-of-fame").await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let text = body.to_string();

    assert!(!text.contains("ghost"), "the alias did not hold: {text}");
    assert!(text.contains("anonymous-"), "{text}");
}

#[tokio::test]
async fn a_duplicate_earns_a_co_credit_and_no_deliverable() {
    let app = TestApp::spawn().await;
    a_person(&app, "first").await;
    let original = a_finding(&app, "first", "Injection in the search parameter").await;

    a_person(&app, "second").await;
    let duplicate = a_finding(&app, "second", "Injection in the search parameter").await;

    let reviewer = a_person(&app, "reviewer6").await;
    grant(&app, reviewer, "security_reviewer:red-team").await;
    app.login("reviewer6").await;

    let (status, body) = transition(
        &app,
        duplicate,
        &json!({ "to": "duplicate", "duplicate_of": original }),
    )
    .await;
    assert_eq!(status, 200, "{body}");

    // A co-credit, which records the work without pretending it was the
    // original — and no deliverable, because there is no fix to its name.
    let co_credits: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM attestations
          WHERE security_finding_id = $1
            AND basis = 'security_finding_co_credit'",
    )
    .bind(duplicate)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(co_credits, 1);

    let deliverables: i64 =
        sqlx::query_scalar("SELECT count(*) FROM deliverables WHERE security_finding_id = $1")
            .bind(duplicate)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(deliverables, 0, "a duplicate was paid like an original");

    // The status and the dedup state cannot disagree: two ways to say one thing
    // is what the constraint refuses.
    let (status_, state): (String, String) =
        sqlx::query_as("SELECT status, dedup_state FROM security_findings WHERE id = $1")
            .bind(duplicate)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(status_, "duplicate");
    assert_eq!(state, "duplicate_confirmed");
}

#[tokio::test]
async fn the_similarity_scan_proposes_and_never_merges() {
    let app = TestApp::spawn().await;
    a_person(&app, "one").await;
    a_finding(
        &app,
        "one",
        "Reflected cross-site scripting in the search box",
    )
    .await;

    a_person(&app, "two").await;
    let second = a_finding(
        &app,
        "two",
        "Reflected cross-site scripting in the search box",
    )
    .await;

    let reviewer = a_person(&app, "reviewer7").await;
    grant(&app, reviewer, "security_reviewer:red-team").await;
    app.login("reviewer7").await;

    let resp = app
        .post(
            &format!("/api/admin/security/findings/{second}/rescan"),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert!(
        body["data"]["candidates"].as_u64().unwrap() >= 1,
        "the same title on the same endpoint was not noticed: {body}"
    );

    // Flagged, not merged. A merge decides who is credited, and a trigram
    // score does not get to.
    let (state, status_): (String, String) =
        sqlx::query_as("SELECT dedup_state, status FROM security_findings WHERE id = $1")
            .bind(second)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(state, "suspected");
    assert_eq!(status_, "submitted", "the scan moved a finding by itself");
}

#[tokio::test]
async fn a_round_puts_the_ball_back_and_five_is_the_limit() {
    let app = TestApp::spawn().await;
    a_person(&app, "reporter8").await;
    let id = a_finding(&app, "reporter8", "A finding that needs work").await;

    let reviewer = a_person(&app, "reviewer8").await;
    grant(&app, reviewer, "security_reviewer:red-team").await;

    for round in 1..=5 {
        app.login("reviewer8").await;
        let resp = app
            .post(
                &format!("/api/admin/security/findings/{id}/rounds"),
                &json!({
                    "kind": "sec_repro_insufficient",
                    "notes_md": "Step two does not get me to the same response."
                }),
            )
            .await;
        assert_eq!(resp.status(), 200, "round {round} was refused");

        // A second round while one is open is refused: the ball is with the
        // researcher.
        let resp = app
            .post(
                &format!("/api/admin/security/findings/{id}/rounds"),
                &json!({
                    "kind": "sec_impact_unclear",
                    "notes_md": "And the impact is the class restated."
                }),
            )
            .await;
        assert_eq!(resp.status(), 409);

        app.login("reporter8").await;
        let resp = app
            .post(
                &format!("/api/security/reports/{id}/answer-round"),
                &json!({ "answer_md": "Corrected: the parameter is `q`, not `search`." }),
            )
            .await;
        assert_eq!(resp.status(), 200);

        app.login("reviewer8").await;
        let resp = app
            .post(
                &format!("/api/admin/security/findings/{id}/rounds/resolve"),
                &json!({ "resolution": "insufficient" }),
            )
            .await;
        assert_eq!(resp.status(), 200);
    }

    // The sixth is refused. A report iterated five times and still not
    // reproducible is a decision, not another round.
    let resp = app
        .post(
            &format!("/api/admin/security/findings/{id}/rounds"),
            &json!({ "kind": "sec_repro_insufficient",
                     "notes_md": "Still cannot get there from here." }),
        )
        .await;
    assert_eq!(resp.status(), 409);
}

// ═══════════════════════════════════════════════════════════════════
// Practice
// ═══════════════════════════════════════════════════════════════════

/// A machine-checked challenge, created the only way one can be: by somebody
/// who knows the answer.
async fn a_flag_challenge(app: &TestApp, admin_username: &str, flag: &str) -> Uuid {
    app.login(admin_username).await;
    let resp = app
        .post(
            "/api/admin/security/challenges",
            &json!({
                "title": "Log in as the administrator",
                "description": "Authenticate as the administrator without the password.",
                "instructions": "On the range. The login form is the way in.",
                "kind": "ctf_flag",
                "difficulty": 2,
                "difficulty_tier": "easy",
                "reward_fragments": 40,
                "flag": flag,
                "flag_format": "SKILLUV{lower_snake_case}",
                "target_url": "https://ctf.skill-uv.com",
            }),
        )
        .await;
    let status = resp.status();
    let body: Value = resp.json().await.unwrap();
    assert_eq!(status, 200, "create said: {body}");
    let id: Uuid = body["data"]["id"].as_str().unwrap().parse().unwrap();

    // Created as a draft, on purpose. Published here so the rest of the test
    // can attempt it.
    sqlx::query("UPDATE challenge_templates SET status = 'published' WHERE id = $1")
        .bind(id)
        .execute(&app.db)
        .await
        .unwrap();
    id
}

#[tokio::test]
async fn a_flag_is_checked_against_a_hash_and_the_plaintext_is_never_stored() {
    let app = TestApp::spawn().await;
    let admin = a_person(&app, "curator").await;
    grant(&app, admin, "admin").await;
    let challenge = a_flag_challenge(&app, "curator", "SKILLUV{the_real_one}").await;

    // The flag is nowhere in the row.
    let row: String =
        sqlx::query_scalar("SELECT to_jsonb(c)::TEXT FROM challenge_templates c WHERE c.id = $1")
            .bind(challenge)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(
        !row.contains("the_real_one"),
        "the flag was stored in plaintext"
    );

    a_person(&app, "solver").await;
    app.login("solver").await;

    // A wrong flag with the right shape says nothing about the shape.
    let resp = app
        .post(
            &format!("/api/security/challenges/{challenge}/flag"),
            &json!({ "flag": "SKILLUV{not_the_real_one}" }),
        )
        .await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["outcome"]["correct"], false);
    assert!(body["data"]["outcome"]["hint"].is_null(), "{body}");

    // A wrong *shape* gets the one hint worth giving.
    let resp = app
        .post(
            &format!("/api/security/challenges/{challenge}/flag"),
            &json!({ "flag": "flag{wrong_prefix}" }),
        )
        .await;
    let body: Value = resp.json().await.unwrap();
    assert!(
        body["data"]["outcome"]["hint"]
            .as_str()
            .unwrap_or_default()
            .contains("format"),
        "{body}"
    );

    // And the attempts are stored hashed: a log of near-miss guesses is a hint.
    let plaintext: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM security_flag_attempts
          WHERE submitted_hash LIKE '%SKILLUV%'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(plaintext, 0);
}

#[tokio::test]
async fn the_first_solve_is_the_first_and_a_solve_happens_once() {
    let app = TestApp::spawn().await;
    let admin = a_person(&app, "curator2").await;
    grant(&app, admin, "admin").await;
    let challenge = a_flag_challenge(&app, "curator2", "SKILLUV{found_it}").await;

    a_person(&app, "quick").await;
    app.login("quick").await;
    let resp = app
        .post(
            &format!("/api/security/challenges/{challenge}/flag"),
            &json!({ "flag": "SKILLUV{found_it}" }),
        )
        .await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let outcome = &body["data"]["outcome"];
    assert_eq!(outcome["correct"], true);
    assert_eq!(outcome["first_solve"], true);
    // First blood is worth half again: 40 plus 20.
    assert_eq!(outcome["fragments_awarded"], 60);
    assert!(outcome["attestation_code"].is_string(), "{body}");

    // Twice is refused rather than counted twice.
    let resp = app
        .post(
            &format!("/api/security/challenges/{challenge}/flag"),
            &json!({ "flag": "SKILLUV{found_it}" }),
        )
        .await;
    assert_eq!(resp.status(), 409);

    a_person(&app, "later").await;
    app.login("later").await;
    let resp = app
        .post(
            &format!("/api/security/challenges/{challenge}/flag"),
            &json!({ "flag": "SKILLUV{found_it}" }),
        )
        .await;
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["outcome"]["first_solve"], false);
    assert_eq!(body["data"]["outcome"]["fragments_awarded"], 40);

    // A planted answer produces an attestation and no deliverable: a weekend on
    // a range must not outrank a year of merged work.
    let deliverables: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM deliverables d
           JOIN attestations a ON a.challenge_template_id = $1
          WHERE d.id = ANY(a.linked_deliverable_ids)",
    )
    .bind(challenge)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(deliverables, 0);

    let attested: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM attestations
          WHERE challenge_template_id = $1 AND basis = 'security_ctf_solved'",
    )
    .bind(challenge)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(attested, 2);
}

#[tokio::test]
async fn a_flag_challenge_cannot_exist_without_a_flag() {
    let app = TestApp::spawn().await;

    // The constraint, not the service. A flag challenge with no hash would be
    // published, claimable and permanently unanswerable, and nothing would
    // fail.
    let refused = sqlx::query(
        "INSERT INTO challenge_templates
             (title, description, instructions, skill_domain, difficulty,
              status, ai_policy, security_kind)
         VALUES ('No flag', 'x', 'y', 'security', 2, 'draft',
                 'disclosure_required', 'ctf_flag')",
    )
    .execute(&app.db)
    .await;
    assert!(
        refused.is_err(),
        "a flag challenge with no flag was accepted"
    );
}

// ═══════════════════════════════════════════════════════════════════
// Research mode
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn a_research_token_is_shown_once_and_replaces_the_last_one() {
    let app = TestApp::spawn().await;
    let person = a_person(&app, "researcher").await;
    app.login("researcher").await;

    let resp = app
        .post("/api/security/research-token", &json!({ "label": "burp" }))
        .await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let first = body["data"]["token"].as_str().expect("a token").to_string();
    assert!(first.starts_with("srt_"));

    // Only the hash is kept.
    let stored: i64 =
        sqlx::query_scalar("SELECT count(*) FROM security_research_tokens WHERE token_hash = $1")
            .bind(&first)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(stored, 0, "the plaintext was stored");

    // Reading it back never shows the secret again.
    let resp = app.get("/api/security/research-token").await;
    let body: Value = resp.json().await.unwrap();
    assert!(!body.to_string().contains(&first));

    // A second issue supersedes the first, so a revocation actually stops the
    // traffic.
    let resp = app
        .post(
            "/api/security/research-token",
            &json!({ "label": "laptop" }),
        )
        .await;
    assert_eq!(resp.status(), 200);

    let live: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM security_research_tokens
          WHERE user_id = $1 AND revoked_at IS NULL",
    )
    .bind(person)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(live, 1);

    let superseded: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM security_research_tokens
          WHERE user_id = $1 AND revoked_reason = 'superseded'",
    )
    .bind(person)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(superseded, 1);
}

#[tokio::test]
async fn a_token_nobody_issued_is_ignored_rather_than_refused() {
    let app = TestApp::spawn().await;

    // Answering 401 to a bad token would turn the header into an oracle for
    // which tokens exist.
    let resp = app
        .get_with_header(
            "/api/security/scope",
            "X-Security-Research-Token",
            "srt_0000000000000000000000000000000000000000000000000000000000000000",
        )
        .await;
    assert_eq!(resp.status(), 200);
}

// ═══════════════════════════════════════════════════════════════════
// Paid work
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn an_offensive_mission_cannot_leave_draft_unauthorised() {
    let app = TestApp::spawn().await;
    let owner = a_person(&app, "client").await;

    let enterprise: Uuid = sqlx::query_scalar(
        "INSERT INTO enterprises (company_name, slug, owner_id, company_size)
         VALUES ('A client', 'a-client', $1, '1-10')
         RETURNING id",
    )
    .bind(owner)
    .fetch_one(&app.db)
    .await
    .unwrap();

    let mission_type: Uuid =
        sqlx::query_scalar("SELECT id FROM mission_types WHERE slug = 'sec_pentest_web'")
            .fetch_one(&app.db)
            .await
            .unwrap();

    // A draft is allowed to be incomplete: that is what a draft is for.
    let mission: Uuid = sqlx::query_scalar(
        "INSERT INTO missions
             (slug, enterprise_id, mission_type_id, skill_domain, title,
              description, acceptance_criteria, deliverable_format, ip_terms,
              payment_model, budget_eur, status, created_by)
         VALUES ('a-pentest', $1, $2, 'security', 'A penetration test',
                 'Test the thing', 'A report with reproducible findings',
                 'sec_pentest_report', 'retain_reusable_components',
                 'fixed_price', 2000, 'draft', $3)
         RETURNING id",
    )
    .bind(enterprise)
    .bind(mission_type)
    .bind(owner)
    .fetch_one(&app.db)
    .await
    .unwrap();

    // Publishing it without a written authorisation is refused by the trigger.
    let refused = sqlx::query("UPDATE missions SET status = 'published' WHERE id = $1")
        .bind(mission)
        .execute(&app.db)
        .await;
    assert!(
        refused.is_err(),
        "an offensive engagement was published with no rules of engagement"
    );

    // With one, it publishes.
    sqlx::query(
        "UPDATE missions
            SET rules_of_engagement_url = 'https://skill-uv.com/roe/1',
                status = 'published'
          WHERE id = $1",
    )
    .bind(mission)
    .execute(&app.db)
    .await
    .expect("an authorised engagement publishes");
}

#[tokio::test]
async fn a_signature_names_the_document_it_agreed_to() {
    let app = TestApp::spawn().await;
    let owner = a_person(&app, "client2").await;

    let enterprise: Uuid = sqlx::query_scalar(
        "INSERT INTO enterprises (company_name, slug, owner_id, company_size)
         VALUES ('Another client', 'another-client', $1, '1-10')
         RETURNING id",
    )
    .bind(owner)
    .fetch_one(&app.db)
    .await
    .unwrap();

    let mission_type: Uuid =
        sqlx::query_scalar("SELECT id FROM mission_types WHERE slug = 'sec_code_audit'")
            .fetch_one(&app.db)
            .await
            .unwrap();

    sqlx::query(
        "INSERT INTO missions
             (slug, enterprise_id, mission_type_id, skill_domain, title,
              description, acceptance_criteria, deliverable_format, ip_terms,
              payment_model, budget_eur, status, created_by, nda_required,
              nda_template)
         VALUES ('an-audit', $1, $2, 'security', 'An audit',
                 'Read the thing', 'Findings with their paths traced',
                 'sec_audit_report', 'retain_reusable_components',
                 'fixed_price', 3000, 'published', $3, TRUE, 'mutual_standard')",
    )
    .bind(enterprise)
    .bind(mission_type)
    .bind(owner)
    .execute(&app.db)
    .await
    .unwrap();

    a_person(&app, "auditor").await;
    app.login("auditor").await;

    let resp = app.get("/api/missions/an-audit/nda").await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let agreement = &body["data"]["agreement"];
    let hash = agreement["sha256"].as_str().expect("a hash").to_string();
    assert_eq!(hash.len(), 64);
    // Said in the response, not only in a document nobody opens.
    assert_eq!(agreement["is_reviewed"], false);
    assert!(
        body["data"]["notice"].as_str().unwrap().contains("draft"),
        "{body}"
    );

    // A hash that is not what would be served now is refused: a signature has
    // to name the text it agreed to.
    let resp = app
        .post(
            "/api/missions/an-audit/nda",
            &json!({ "typed_name": "A Uditor", "document_sha256": "0".repeat(64) }),
        )
        .await;
    assert_eq!(resp.status(), 409);

    let resp = app
        .post(
            "/api/missions/an-audit/nda",
            &json!({ "typed_name": "A Uditor", "document_sha256": hash }),
        )
        .await;
    assert_eq!(resp.status(), 200);

    let signed: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM mission_nda_signatures WHERE document_sha256 = $1",
    )
    .bind(&hash)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(signed, 1);
}

// ═══════════════════════════════════════════════════════════════════
// Proofs
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn somebody_elses_proof_is_not_yours_to_read() {
    let app = TestApp::spawn().await;
    let mine = a_person(&app, "owner").await;
    a_person(&app, "nosy").await;

    // No object store is needed to check the access rule: the key names its
    // uploader, which is what makes the ownership check a string comparison.
    app.login("nosy").await;
    let resp = app
        .get(&format!(
            "/api/security/proofs/security-proofs/{mine}/x.png"
        ))
        .await;
    assert_eq!(resp.status(), 403);

    // And a key that is not a proof key is refused before anything is signed.
    let resp = app.get("/api/security/proofs/kyc/secret.pdf").await;
    assert_ne!(resp.status(), 200);
}

#[tokio::test]
async fn a_proof_key_from_somewhere_else_cannot_be_attached_to_a_report() {
    let app = TestApp::spawn().await;
    a_person(&app, "forger").await;
    app.login("forger").await;

    let mut report = a_report("A finding with a borrowed proof");
    report["proof_keys"] = json!(["https://example.com/screenshot.png"]);
    let (status, body) = submit(&app, &report).await;
    assert_eq!(status, 400, "{body}");
    assert!(body.to_string().contains("upload"), "{body}");
}
