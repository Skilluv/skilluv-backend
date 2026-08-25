//! Matching a design mentee with a design mentor.
//!
//! What this suite guards is the asymmetry: a mentee's families come from what
//! they declared, a mentor's from what they were validated in. Getting that
//! backwards would suggest mentors on the strength of an ambition, and an hour
//! would teach that the expensive way.

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

/// Declare what interests somebody, the way the onboarding does.
async fn declare(app: &TestApp, user: Uuid, trades: &[&str], tool: &str) {
    let answers = json!({
        "preferred_families": trades,
        "main_tool": tool,
    });
    sqlx::query(
        "INSERT INTO user_domain_profiles (user_id, domain, answers)
         VALUES ($1, 'design', $2)
         ON CONFLICT (user_id, domain) DO UPDATE SET answers = EXCLUDED.answers",
    )
    .bind(user)
    .bind(&answers)
    .execute(&app.db)
    .await
    .unwrap();
}

async fn set_craft_score(app: &TestApp, user: Uuid, score: i32) {
    sqlx::query(
        "INSERT INTO craft_scores (user_id, skill_domain, score, computed_at)
         VALUES ($1, 'design', $2, NOW())
         ON CONFLICT (user_id, skill_domain) DO UPDATE SET score = EXCLUDED.score",
    )
    .bind(user)
    .bind(score)
    .execute(&app.db)
    .await
    .unwrap();
}

async fn set_timezone(app: &TestApp, user: Uuid, tz: &str) {
    sqlx::query("UPDATE users SET timezone = $2 WHERE id = $1")
        .bind(user)
        .bind(tz)
        .execute(&app.db)
        .await
        .unwrap();
}

async fn become_mentor(app: &TestApp, user: Uuid, headline: &str) {
    sqlx::query(
        "INSERT INTO mentor_profiles (user_id, headline, bio, hourly_rate_eur_cents)
         VALUES ($1, $2, 'Bio.', 0)
         ON CONFLICT (user_id) DO UPDATE SET headline = EXCLUDED.headline, active = TRUE",
    )
    .bind(user)
    .bind(headline)
    .execute(&app.db)
    .await
    .unwrap();
}

/// A verified deliverable in a given trade — which is what makes somebody a
/// mentor *of that family* rather than an admirer of it.
async fn validated_in(app: &TestApp, user: Uuid, trade: &str) {
    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO projects (slug, name, owner_type, owner_id)
         VALUES ($1, 'Projet mentor', 'user', $2) RETURNING id",
    )
    .bind(format!("mentor-p-{}", Uuid::new_v4()))
    .bind(user)
    .fetch_one(&app.db)
    .await
    .unwrap();

    let slice: Uuid = sqlx::query_scalar(
        "INSERT INTO project_slices
            (project_id, slice_type, title, description, primary_domain, difficulty,
             status, design_subtype, orientation_id)
         VALUES ($1, 'design_artifact', 'Travail', 'Un brief.', 'design', 3, 'validated',
                 'brand_kit', (SELECT id FROM orientations WHERE slug = $2))
         RETURNING id",
    )
    .bind(project)
    .bind(trade)
    .fetch_one(&app.db)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO deliverables
            (slice_id, user_id, artifact_type, artifact_url, verifiable_by,
             verification_status, verified_at, public)
         VALUES ($1, $2, 'design_artifact', 'https://figma.test/x', 'human_review',
                 'verified', NOW(), TRUE)",
    )
    .bind(slice)
    .bind(user)
    .execute(&app.db)
    .await
    .unwrap();
}

#[tokio::test]
async fn a_mentor_is_suggested_for_what_they_delivered_not_what_they_declared() {
    let app = TestApp::spawn().await;
    app.register_user("mentee_brand").await;
    app.register_user("mentor_real").await;
    app.register_user("mentor_aspiring").await;

    let mentee = user_id(&app, "mentee_brand").await;
    let real = user_id(&app, "mentor_real").await;
    let aspiring = user_id(&app, "mentor_aspiring").await;

    declare(&app, mentee, &["design-brand-identity"], "figma").await;
    set_craft_score(&app, mentee, 100).await;

    // One has delivered brand work. The other only says they are interested in
    // it — which is exactly the trap this rule exists to avoid.
    become_mentor(&app, real, "Identités et systèmes de marque").await;
    set_craft_score(&app, real, 2000).await;
    validated_in(&app, real, "design-brand-identity").await;

    become_mentor(&app, aspiring, "Je m'intéresse à la marque").await;
    set_craft_score(&app, aspiring, 3000).await;
    declare(&app, aspiring, &["design-brand-identity"], "figma").await;

    app.login("mentee_brand").await;
    let body: Value = app
        .get("/api/domains/design/mentors/for-me")
        .await
        .json()
        .await
        .unwrap();

    let mentors = body["data"]["mentors"].as_array().unwrap();
    let names: Vec<&str> = mentors
        .iter()
        .map(|m| m["username"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"mentor_real"), "{body}");
    assert!(
        !names.contains(&"mentor_aspiring"),
        "a declared interest is not a family: {body}"
    );
}

#[tokio::test]
async fn a_suggestion_says_why_it_was_made() {
    let app = TestApp::spawn().await;
    app.register_user("mentee_reasons").await;
    app.register_user("mentor_reasons").await;
    let mentee = user_id(&app, "mentee_reasons").await;
    let mentor = user_id(&app, "mentor_reasons").await;

    declare(&app, mentee, &["design-brand-identity"], "figma").await;
    set_craft_score(&app, mentee, 200).await;
    set_timezone(&app, mentee, "+01:00").await;

    become_mentor(&app, mentor, "Direction artistique").await;
    set_craft_score(&app, mentor, 2200).await;
    set_timezone(&app, mentor, "+01:00").await;
    validated_in(&app, mentor, "design-brand-identity").await;

    app.login("mentee_reasons").await;
    let body: Value = app
        .get("/api/domains/design/mentors/for-me")
        .await
        .json()
        .await
        .unwrap();

    let top = &body["data"]["mentors"][0];
    let because: Vec<&str> = top["because"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r.as_str().unwrap())
        .collect();

    // Somebody who can read why a mentor was suggested can tell us it was
    // wrong. A list with no reasoning is a list nobody argues with.
    assert!(because.iter().any(|r| r.contains("brand")), "{because:?}");
    assert!(because.iter().any(|r| r.contains("2000")), "{because:?}");
    assert!(
        because.iter().any(|r| r.contains("Même fuseau")),
        "{because:?}"
    );
    assert_eq!(top["shared_families"][0], "brand");
}

#[tokio::test]
async fn somebody_adjacent_is_a_peer_and_not_a_mentor() {
    let app = TestApp::spawn().await;
    app.register_user("mentee_close").await;
    app.register_user("mentor_close").await;
    let mentee = user_id(&app, "mentee_close").await;
    let mentor = user_id(&app, "mentor_close").await;

    declare(&app, mentee, &["design-brand-identity"], "figma").await;
    set_craft_score(&app, mentee, 1000).await;

    // Four hundred points ahead: a conversation between peers, which is
    // valuable and is not mentorship.
    become_mentor(&app, mentor, "Presque au même niveau").await;
    set_craft_score(&app, mentor, 1400).await;
    validated_in(&app, mentor, "design-brand-identity").await;

    app.login("mentee_close").await;
    let body: Value = app
        .get("/api/domains/design/mentors/for-me")
        .await
        .json()
        .await
        .unwrap();
    assert!(
        body["data"]["mentors"].as_array().unwrap().is_empty(),
        "{body}"
    );
}

#[tokio::test]
async fn a_mentee_who_declared_nothing_is_told_what_to_do() {
    let app = TestApp::spawn().await;
    app.register_user("mentee_blank").await;
    app.login("mentee_blank").await;

    // Not an empty list: an empty list looks like "there is nobody", and the
    // person would wait rather than answer seven questions.
    let resp = app.get("/api/domains/design/mentors/for-me").await;
    assert_eq!(resp.status().as_u16(), 400);
    let body = resp.text().await.unwrap();
    assert!(body.contains("questionnaire"), "{body}");
}

#[tokio::test]
async fn being_stuck_is_answered_when_asked_rather_than_announced() {
    let app = TestApp::spawn().await;
    app.register_user("mentee_stuck").await;
    let mentee = user_id(&app, "mentee_stuck").await;
    declare(&app, mentee, &["design-brand-identity"], "figma").await;

    app.login("mentee_stuck").await;
    let before: Value = app
        .get("/api/domains/design/mentors/for-me")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(before["data"]["suggested"], json!(false));

    // Three challenges handed in, none validated.
    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO projects (slug, name, owner_type, owner_id)
         VALUES ($1, 'Projet', 'user', $2) RETURNING id",
    )
    .bind(format!("stuck-{}", Uuid::new_v4()))
    .bind(mentee)
    .fetch_one(&app.db)
    .await
    .unwrap();

    for _ in 0..3 {
        let slice: Uuid = sqlx::query_scalar(
            "INSERT INTO project_slices
                (project_id, slice_type, title, description, primary_domain, difficulty,
                 status, claimed_by_user_id, claimed_at, design_subtype, orientation_id)
             VALUES ($1, 'design_artifact', 'Essai', 'Un brief.', 'design', 2, 'in_iteration',
                     $2, NOW(), 'brand_kit',
                     (SELECT id FROM orientations WHERE slug = 'design-brand-identity'))
             RETURNING id",
        )
        .bind(project)
        .bind(mentee)
        .fetch_one(&app.db)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO slice_validation_decisions
                (slice_id, validator_id, decision, reason, blocking_reason)
             VALUES ($1, $2, 'iterate',
                     'La direction ne tient pas encore, reprends la contreforme.', 'craft_gap')",
        )
        .bind(slice)
        .bind(mentee)
        .execute(&app.db)
        .await
        .unwrap();
    }

    let after: Value = app
        .get("/api/domains/design/mentors/for-me")
        .await
        .json()
        .await
        .unwrap();
    // Telling somebody "you seem to be struggling" unprompted lands badly
    // however it is worded. This appears because they opened the page.
    assert_eq!(after["data"]["suggested"], json!(true));
}
