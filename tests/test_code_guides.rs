//! Guides, licences and the onboarding wizard.

mod common;
use common::TestApp;
use serde_json::{Value, json};
use uuid::Uuid;

async fn a_user(app: &TestApp, username: &str) -> Uuid {
    app.register_user(username).await;
    sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(username)
        .fetch_one(&app.db)
        .await
        .unwrap()
}

// ═══════════════════════════════════════════════════════════════════
// Guides
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn every_family_of_trades_has_a_guide_in_both_languages() {
    let app = TestApp::spawn().await;

    // The families come from the reviewer groups, so a trade added later
    // without a guide is caught here rather than by the person who arrives
    // and finds nothing.
    let families: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT reviewer_group FROM orientations
          WHERE reviewer_group IS NOT NULL AND primary_domain = 'code'",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();
    assert_eq!(families.len(), 8);

    for locale in ["fr", "en"] {
        let covered: Vec<String> = sqlx::query_scalar(
            "SELECT reviewer_group FROM content_guides
              WHERE kind = 'onboarding' AND locale = $1 AND reviewer_group IS NOT NULL",
        )
        .bind(locale)
        .fetch_all(&app.db)
        .await
        .unwrap();
        for family in &families {
            assert!(
                covered.contains(family),
                "no {locale} onboarding guide for {family}"
            );
        }
    }
}

/// The listing moved to `/api/guides` when a second domain got rows: the old
/// path ignored `skill_domain`, so an AI onboarding guide answered under the
/// code path. These assertions are about code, which is what `domain=code` now
/// says out loud instead of relying on code being the only content there.
#[tokio::test]
async fn the_listing_answers_in_the_requested_language() {
    let app = TestApp::spawn().await;

    let english: Value = reqwest::Client::new()
        .get(format!("{}/api/guides?domain=code&kind=onboarding", app.addr))
        .header("Accept-Language", "en")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let guides = english["data"].as_array().unwrap();
    assert_eq!(guides.len(), 8);
    assert!(guides.iter().all(|g| g["locale"] == "en"));

    // No header at all: French, the language these are written in first.
    let default: Value = app
        .get("/api/guides?domain=code&kind=onboarding")
        .await
        .json()
        .await
        .unwrap();
    assert!(default["data"].as_array().unwrap()[0]["locale"] == "fr");
}

#[tokio::test]
async fn a_guide_carries_a_body_somebody_can_act_on() {
    let app = TestApp::spawn().await;

    let body: Value = app
        .get("/api/guides/onboarding-systems")
        .await
        .json()
        .await
        .unwrap();
    let markdown = body["data"]["body_md"].as_str().unwrap();

    // The five things somebody arriving actually asks. A guide that answers
    // fewer is a welcome message.
    assert!(markdown.contains("Trente jours"), "no thirty-day path");
    assert!(markdown.contains("Outils"), "nothing to install");
    assert!(markdown.contains("Premier défi"), "nothing to attempt");
    assert!(markdown.contains("Où sont les gens"), "nowhere to go");
    assert!(markdown.len() > 800, "a guide of four lines is a stub");
}

#[tokio::test]
async fn the_toolkit_and_the_twelve_templates_are_served() {
    let app = TestApp::spawn().await;

    let toolkit: Value = app
        .get("/api/guides?domain=code&kind=toolkit")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(toolkit["data"].as_array().unwrap().len(), 1);

    let templates: Value = app
        .get("/api/guides?domain=code&kind=writeup_template")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(templates["data"].as_array().unwrap().len(), 12);
}

// ═══════════════════════════════════════════════════════════════════
// Licences
// ═══════════════════════════════════════════════════════════════════

async fn an_enterprise(app: &TestApp, company: &str) -> Uuid {
    app.register_enterprise(company).await;
    let username = company.to_lowercase().replace(' ', "");
    app.login(&username).await;
    app.enable_totp_for(&username).await;
    sqlx::query_scalar(
        "SELECT e.id FROM enterprises e JOIN users u ON u.id = e.owner_id
          WHERE u.username = $1",
    )
    .bind(&username)
    .fetch_one(&app.db)
    .await
    .unwrap()
}

fn a_mission(slug: &str, license: Option<&str>, ip_terms: &str) -> Value {
    json!({
        "slug": slug,
        "mission_type_slug": "backend_service_dev",
        "title": "Étendre un service existant",
        "description": "Reprendre le service et lui ajouter une fonctionnalité.",
        "acceptance_criteria": "La fonctionnalité marche, les tests passent.",
        "deliverable_format": "github_pr",
        "budget_eur": "3000.00",
        "ip_terms": ip_terms,
        "upstream_license_spdx": license,
    })
}

#[tokio::test]
async fn a_copyleft_mission_cannot_promise_client_ownership() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Copyleftco").await;

    // The accident this exists to prevent: nobody finds out until a lawyer
    // does, after the work is delivered and paid for.
    let refused = app
        .post(
            "/api/missions",
            &a_mission("gpl-owned", Some("GPL-3.0-only"), "full_ownership_client"),
        )
        .await;
    assert_eq!(refused.status(), 400);
    let body: Value = refused.json().await.unwrap();
    assert!(
        body.to_string().contains("GPL-3.0-only"),
        "the message must name the licence: {body}"
    );

    // The same work under honest terms is accepted.
    let accepted = app
        .post(
            "/api/missions",
            &a_mission("gpl-open", Some("GPL-3.0-only"), "open_source_output"),
        )
        .await;
    assert_eq!(accepted.status(), 200, "{}", accepted.text().await.unwrap());
}

#[tokio::test]
async fn a_permissive_licence_permits_ownership() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Permissiveco").await;

    for licence in ["MIT", "Apache-2.0", "BSD-3-Clause"] {
        let resp = app
            .post(
                "/api/missions",
                &a_mission(
                    &format!("permissive-{}", licence.to_lowercase().replace('.', "")),
                    Some(licence),
                    "full_ownership_client",
                ),
            )
            .await;
        assert_eq!(
            resp.status(),
            200,
            "{licence}: {}",
            resp.text().await.unwrap()
        );
    }
}

#[tokio::test]
async fn a_mission_with_no_upstream_is_not_interrogated() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Greenfieldco").await;

    // Most work has no upstream. Demanding a licence would block the ordinary
    // case to catch the rare one.
    let resp = app
        .post(
            "/api/missions",
            &a_mission("greenfield", None, "full_ownership_client"),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
}

// ═══════════════════════════════════════════════════════════════════
// AI disclosure
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn a_verified_artefact_with_nothing_declared_is_asked() {
    let app = TestApp::spawn().await;
    let user = a_user(&app, "disclose_asked").await;

    let challenge: Uuid = sqlx::query_scalar(
        "INSERT INTO challenge_templates
            (title, description, instructions, skill_domain, difficulty, status, is_training)
         VALUES ('x', 'x', 'x', 'code', 2, 'published', TRUE) RETURNING id",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    let deliverable: Uuid = sqlx::query_scalar(
        "INSERT INTO deliverables
            (user_id, challenge_id, artifact_type, artifact_url, verifiable_by)
         VALUES ($1, $2, 'pr_merged', 'https://x.test/pr', 'github_webhook') RETURNING id",
    )
    .bind(user)
    .bind(challenge)
    .fetch_one(&app.db)
    .await
    .unwrap();

    // The webhook verifies it. There is nobody to ask at that moment, which
    // is why this is a prompt and not a constraint.
    sqlx::query(
        "UPDATE deliverables SET verification_status = 'verified', verified_at = NOW()
          WHERE id = $1",
    )
    .bind(deliverable)
    .execute(&app.db)
    .await
    .unwrap();

    let deadline: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT ai_disclosure_deadline_at FROM deliverables WHERE id = $1")
            .bind(deliverable)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(deadline.is_some(), "nobody was asked");

    // Inside the window it still counts.
    let counted: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM countable_deliverables WHERE id = $1)")
            .bind(deliverable)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(counted, "the window has not closed yet");

    // Past the deadline with nothing declared, it stops counting — and it is
    // not revoked, because somebody on holiday is not somebody hiding
    // something.
    sqlx::query("UPDATE deliverables SET ai_disclosure_deadline_at = NOW() - INTERVAL '1 day' WHERE id = $1")
        .bind(deliverable)
        .execute(&app.db)
        .await
        .unwrap();
    let counted: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM countable_deliverables WHERE id = $1)")
            .bind(deliverable)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(!counted);
    let revoked: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT revoked_at FROM deliverables WHERE id = $1")
            .bind(deliverable)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(revoked.is_none(), "not declaring is not fraud");

    // Declaring it closes the question.
    sqlx::query("UPDATE deliverables SET ai_assistance_level = 'autocomplete' WHERE id = $1")
        .bind(deliverable)
        .execute(&app.db)
        .await
        .unwrap();
    let counted: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM countable_deliverables WHERE id = $1)")
            .bind(deliverable)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(counted);
}

// ═══════════════════════════════════════════════════════════════════
// Onboarding
// ═══════════════════════════════════════════════════════════════════

fn wizard(level: &str, family: &str, objective: &str) -> Value {
    json!({
        "level": level,
        "preferred_families": [family],
        "weekly_hours": "5_to_15",
        "objective": objective,
        "main_languages": ["rust"],
        "challenge_preference": "upstream_contributions",
    })
}

#[tokio::test]
async fn the_wizard_answers_with_a_first_month() {
    let app = TestApp::spawn().await;
    a_user(&app, "wizard_beginner").await;
    app.login("wizard_beginner").await;

    let resp = app
        .post("/api/code/onboarding", &wizard("beginner", "web", "learn"))
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    let recommendation = &body["data"]["recommendation"];

    assert!(!recommendation["headline"].as_str().unwrap().is_empty());
    assert!(
        !recommendation["because"].as_str().unwrap().is_empty(),
        "advice nobody can argue with is advice nobody follows"
    );
    let guides = recommendation["guides"].as_array().unwrap();
    assert!(guides.iter().any(|g| g == "onboarding-web"));

    let stored: (Option<chrono::DateTime<chrono::Utc>>, Option<String>) = sqlx::query_as(
        "SELECT p.completed_at, p.answers ->> 'level'
           FROM user_domain_profiles p
           JOIN users u ON u.id = p.user_id
          WHERE u.username = 'wizard_beginner' AND p.domain = 'code'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(stored.0.is_some());
    assert_eq!(stored.1.as_deref(), Some("beginner"));
}

#[tokio::test]
async fn a_family_nobody_has_is_refused() {
    let app = TestApp::spawn().await;
    a_user(&app, "wizard_typo").await;
    app.login("wizard_typo").await;

    // A typo would send somebody to a guide that does not exist and quietly
    // recommend nothing.
    let resp = app
        .post(
            "/api/code/onboarding",
            &wizard("junior", "quantique", "learn"),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn skipping_is_recorded_as_skipping() {
    let app = TestApp::spawn().await;
    a_user(&app, "wizard_skipper").await;
    app.login("wizard_skipper").await;

    assert_eq!(
        app.post("/api/code/onboarding/skip", &json!({}))
            .await
            .status(),
        200
    );

    // Skipped is not unanswered: without the distinction the wizard would
    // reappear forever for the people who least wanted it.
    let (completed, skipped): (
        Option<chrono::DateTime<chrono::Utc>>,
        Option<chrono::DateTime<chrono::Utc>>,
    ) = sqlx::query_as(
        "SELECT p.completed_at, p.skipped_at
           FROM user_domain_profiles p
           JOIN users u ON u.id = p.user_id
          WHERE u.username = 'wizard_skipper' AND p.domain = 'code'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(completed.is_none());
    assert!(skipped.is_some());
}

#[tokio::test]
async fn a_github_username_given_to_the_wizard_is_claimed_not_proved() {
    let app = TestApp::spawn().await;
    a_user(&app, "wizard_github").await;
    app.login("wizard_github").await;

    let mut answers = wizard("mid", "systems", "build_portfolio");
    answers["github_username"] = json!("torvalds");
    assert_eq!(
        app.post("/api/code/onboarding", &answers).await.status(),
        200
    );

    let row: Option<(String, Option<chrono::DateTime<chrono::Utc>>)> = sqlx::query_as(
        "SELECT p.handle, p.verified_at FROM user_external_portfolios p
           JOIN users u ON u.id = p.user_id
          WHERE u.username = 'wizard_github' AND p.platform = 'github'",
    )
    .fetch_optional(&app.db)
    .await
    .unwrap();
    let (handle, verified) = row.expect("the username should have been recorded");
    assert_eq!(handle, "torvalds");
    // Typing a name proves nothing. Only the OAuth callback verifies.
    assert!(
        verified.is_none(),
        "typing a name must not make it a proved account"
    );
}

#[tokio::test]
async fn matching_needs_the_onboarding_answered_first() {
    let app = TestApp::spawn().await;
    a_user(&app, "match_unanswered").await;
    app.login("match_unanswered").await;

    let resp = app.get("/api/code/mentors/for-me").await;
    assert_eq!(
        resp.status(),
        400,
        "without a family there is nothing to match on"
    );
}

#[tokio::test]
async fn a_mentor_is_suggested_with_the_reasoning_attached() {
    let app = TestApp::spawn().await;

    let mentee = a_user(&app, "match_mentee").await;
    sqlx::query("UPDATE users SET timezone = '+01:00' WHERE id = $1")
        .bind(mentee)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO user_domain_profiles (user_id, domain, answers, completed_at)
         VALUES ($1, 'code',
                 jsonb_build_object('preferred_families', jsonb_build_array('systems'),
                                    'main_languages', jsonb_build_array('rust')),
                 NOW())",
    )
    .bind(mentee)
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO craft_scores (user_id, skill_domain, score, tier_slug)
         VALUES ($1, 'code', 100, 'contributor')",
    )
    .bind(mentee)
    .execute(&app.db)
    .await
    .unwrap();

    // One good match, and one in the wrong family who must not appear.
    for (name, family, score) in [
        ("match_mentor_good", "systems", 2000),
        ("match_mentor_wrong", "web", 4000),
    ] {
        let mentor = a_user(&app, name).await;
        sqlx::query("UPDATE users SET timezone = '+02:00' WHERE id = $1")
            .bind(mentor)
            .execute(&app.db)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO user_domain_profiles (user_id, domain, answers, completed_at)
             VALUES ($1, 'code',
                     jsonb_build_object('preferred_families', jsonb_build_array($2),
                                        'main_languages', jsonb_build_array('rust')),
                     NOW())",
        )
        .bind(mentor)
        .bind(family)
        .execute(&app.db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO craft_scores (user_id, skill_domain, score, tier_slug)
             VALUES ($1, 'code', $2, 'senior')",
        )
        .bind(mentor)
        .bind(score)
        .execute(&app.db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO mentor_profiles (user_id, headline, bio, hourly_rate_eur_cents)
             VALUES ($1, 'Systèmes et noyau', 'x', 5000)",
        )
        .bind(mentor)
        .execute(&app.db)
        .await
        .unwrap();
    }

    app.login("match_mentee").await;
    let body: Value = app
        .get("/api/code/mentors/for-me")
        .await
        .json()
        .await
        .unwrap();
    let mentors = body["data"]["mentors"].as_array().unwrap();

    assert_eq!(
        mentors.len(),
        1,
        "a kernel engineer is not the right person for somebody's first React component"
    );
    assert_eq!(mentors[0]["username"], "match_mentor_good");
    assert!(
        !mentors[0]["because"].as_array().unwrap().is_empty(),
        "a mentee who can read why somebody was suggested can tell us it was wrong"
    );
}
