//! Being put forward by the platform.
//!
//! What this suite guards is the scarcity. One person per domain per week,
//! with a written reason, resting on work somebody checked — those three
//! together are the whole value, and each of them is a rule that would be
//! easy to relax into meaninglessness.

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

/// A verified design deliverable, which is what a featuring has to rest on.
async fn a_verified_design_deliverable(app: &TestApp, user: Uuid) -> Uuid {
    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO projects (slug, name, owner_type, owner_id)
         VALUES ($1, 'Projet mis en avant', 'user', $2) RETURNING id",
    )
    .bind(format!("featured-{}", Uuid::new_v4()))
    .bind(user)
    .fetch_one(&app.db)
    .await
    .unwrap();

    let slice: Uuid = sqlx::query_scalar(
        "INSERT INTO project_slices
            (project_id, slice_type, title, description, primary_domain, difficulty,
             status, design_subtype, orientation_id)
         VALUES ($1, 'design_artifact', 'Identité', 'Un brief.', 'design', 2, 'validated',
                 'brand_kit', (SELECT id FROM orientations WHERE slug = 'design-brand-identity'))
         RETURNING id",
    )
    .bind(project)
    .fetch_one(&app.db)
    .await
    .unwrap();

    sqlx::query_scalar(
        "INSERT INTO deliverables
            (slice_id, user_id, artifact_type, artifact_url, verifiable_by,
             verification_status, verified_at, public)
         VALUES ($1, $2, 'design_artifact', 'https://figma.test/final', 'human_review',
                 'verified', NOW(), TRUE)
         RETURNING id",
    )
    .bind(slice)
    .bind(user)
    .fetch_one(&app.db)
    .await
    .unwrap()
}

/// The Monday of a week comfortably in the past, so no test depends on today.
const A_MONDAY: &str = "2026-06-01";
const THE_MONDAY_AFTER: &str = "2026-06-08";

fn a_reason() -> &'static str {
    "Trois identités menées à la validation ce trimestre, dont deux après quatre \
     tours de critique — et ses critiques aux autres sont les mieux écrites de la famille."
}

#[tokio::test]
async fn a_featuring_needs_work_somebody_checked() {
    let app = TestApp::spawn().await;
    app.register_user("feat_admin").await;
    app.register_user("feat_nothing").await;
    let admin = user_id(&app, "feat_admin").await;
    let nobody = user_id(&app, "feat_nothing").await;
    grant(&app, admin, "admin").await;

    // Being put forward for work nobody has checked is exactly the claim this
    // platform exists not to make.
    app.login("feat_admin").await;
    let resp = app
        .post(
            "/api/admin/featured",
            &json!({
                "skill_domain": "design",
                "week_of": A_MONDAY,
                "user_id": nobody,
                "reason_md": a_reason(),
            }),
        )
        .await;
    assert_eq!(resp.status().as_u16(), 400, "{:?}", resp.text().await);
}

#[tokio::test]
async fn a_featuring_says_why_and_leaves_an_attestation() {
    let app = TestApp::spawn().await;
    app.register_user("feat_admin2").await;
    app.register_user("feat_designer").await;
    let admin = user_id(&app, "feat_admin2").await;
    let designer = user_id(&app, "feat_designer").await;
    grant(&app, admin, "admin").await;
    let deliverable = a_verified_design_deliverable(&app, designer).await;

    app.login("feat_admin2").await;
    let resp = app
        .post(
            "/api/admin/featured",
            &json!({
                "skill_domain": "design",
                "week_of": A_MONDAY,
                "user_id": designer,
                "reason_md": a_reason(),
                "deliverable_id": deliverable,
            }),
        )
        .await;
    assert_eq!(resp.status().as_u16(), 201, "{:?}", resp.text().await);

    // The row keeps the sentence, published as written.
    let stored: String = sqlx::query_scalar(
        "SELECT reason_md FROM featured_talents WHERE user_id = $1 AND skill_domain = 'design'",
    )
    .bind(designer)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(stored.contains("quatre"), "{stored}");

    // And the editorial attestation exists, which is the point of keeping the
    // row rather than overwriting a flag.
    let basis: Option<String> = sqlx::query_scalar(
        "SELECT basis FROM attestations WHERE user_id = $1 AND basis = 'featured_designer'",
    )
    .bind(designer)
    .fetch_optional(&app.db)
    .await
    .unwrap();
    assert_eq!(basis.as_deref(), Some("featured_designer"));
}

#[tokio::test]
async fn a_reason_of_four_words_is_refused() {
    let app = TestApp::spawn().await;
    app.register_user("feat_admin3").await;
    app.register_user("feat_designer3").await;
    let admin = user_id(&app, "feat_admin3").await;
    let designer = user_id(&app, "feat_designer3").await;
    grant(&app, admin, "admin").await;
    a_verified_design_deliverable(&app, designer).await;

    // A featuring with no stated reason is a popularity contest.
    app.login("feat_admin3").await;
    let resp = app
        .post(
            "/api/admin/featured",
            &json!({
                "skill_domain": "design",
                "week_of": A_MONDAY,
                "user_id": designer,
                "reason_md": "Il est très fort.",
            }),
        )
        .await;
    assert_eq!(resp.status().as_u16(), 400);
}

#[tokio::test]
async fn one_person_per_domain_per_week() {
    let app = TestApp::spawn().await;
    app.register_user("feat_admin4").await;
    app.register_user("feat_first").await;
    app.register_user("feat_second").await;
    let admin = user_id(&app, "feat_admin4").await;
    let first = user_id(&app, "feat_first").await;
    let second = user_id(&app, "feat_second").await;
    grant(&app, admin, "admin").await;
    a_verified_design_deliverable(&app, first).await;
    a_verified_design_deliverable(&app, second).await;

    app.login("feat_admin4").await;
    let body = |who: Uuid| {
        json!({
            "skill_domain": "design",
            "week_of": A_MONDAY,
            "user_id": who,
            "reason_md": a_reason(),
        })
    };
    assert_eq!(
        app.post("/api/admin/featured", &body(first))
            .await
            .status()
            .as_u16(),
        201
    );

    // Two people featured in one week means neither was.
    assert_eq!(
        app.post("/api/admin/featured", &body(second))
            .await
            .status()
            .as_u16(),
        409
    );
}

#[tokio::test]
async fn the_same_person_does_not_come_back_next_week() {
    let app = TestApp::spawn().await;
    app.register_user("feat_admin5").await;
    app.register_user("feat_repeat").await;
    let admin = user_id(&app, "feat_admin5").await;
    let designer = user_id(&app, "feat_repeat").await;
    grant(&app, admin, "admin").await;
    a_verified_design_deliverable(&app, designer).await;

    app.login("feat_admin5").await;
    let body = |week: &str| {
        json!({
            "skill_domain": "design",
            "week_of": week,
            "user_id": designer,
            "reason_md": a_reason(),
        })
    };
    assert_eq!(
        app.post("/api/admin/featured", &body(A_MONDAY))
            .await
            .status()
            .as_u16(),
        201
    );

    // Otherwise a featuring is a rotation among the same four people, and the
    // attestation it produces says nothing.
    assert_eq!(
        app.post("/api/admin/featured", &body(THE_MONDAY_AFTER))
            .await
            .status()
            .as_u16(),
        409
    );
}

#[tokio::test]
async fn a_week_that_is_not_a_monday_is_refused_rather_than_rounded() {
    let app = TestApp::spawn().await;
    app.register_user("feat_admin6").await;
    app.register_user("feat_designer6").await;
    let admin = user_id(&app, "feat_admin6").await;
    let designer = user_id(&app, "feat_designer6").await;
    grant(&app, admin, "admin").await;
    a_verified_design_deliverable(&app, designer).await;

    // Rounding somebody's intent is how a featuring lands on the wrong week.
    app.login("feat_admin6").await;
    let resp = app
        .post(
            "/api/admin/featured",
            &json!({
                "skill_domain": "design",
                "week_of": "2026-06-03",
                "user_id": designer,
                "reason_md": a_reason(),
            }),
        )
        .await;
    assert_eq!(resp.status().as_u16(), 400);
}

#[tokio::test]
async fn only_an_admin_features_anybody() {
    let app = TestApp::spawn().await;
    app.register_user("feat_nobody").await;
    app.register_user("feat_target").await;
    let target = user_id(&app, "feat_target").await;
    a_verified_design_deliverable(&app, target).await;

    app.login("feat_nobody").await;
    let resp = app
        .post(
            "/api/admin/featured",
            &json!({
                "skill_domain": "design",
                "week_of": A_MONDAY,
                "user_id": target,
                "reason_md": a_reason(),
            }),
        )
        .await;
    assert_eq!(resp.status().as_u16(), 403);
}

#[tokio::test]
async fn a_quiet_week_reads_as_a_quiet_week_not_as_a_broken_page() {
    let app = TestApp::spawn().await;

    // Public, and null rather than 404: a week with nobody featured is a
    // normal week.
    let resp = app.get("/api/featured/design").await;
    assert_eq!(resp.status().as_u16(), 200);
    let body: Value = resp.json().await.unwrap();
    assert!(body["data"]["featured"].is_null(), "{body}");

    let unknown = app.get("/api/featured/charcuterie").await;
    assert_eq!(unknown.status().as_u16(), 400);
}

#[tokio::test]
async fn the_post_is_composed_and_not_sent() {
    let app = TestApp::spawn().await;
    app.register_user("feat_admin7").await;
    app.register_user("feat_card").await;
    let admin = user_id(&app, "feat_admin7").await;
    let designer = user_id(&app, "feat_card").await;
    grant(&app, admin, "admin").await;
    let deliverable = a_verified_design_deliverable(&app, designer).await;

    app.login("feat_admin7").await;
    app.post(
        "/api/admin/featured",
        &json!({
            "skill_domain": "design",
            "week_of": A_MONDAY,
            "user_id": designer,
            "reason_md": a_reason(),
            "deliverable_id": deliverable,
        }),
    )
    .await;

    // Everything a post needs, ready for a person to send. Publishing
    // somebody's name on a schedule with no human in between is not a feature.
    let body: Value = app
        .get(&format!("/api/admin/featured/design/{A_MONDAY}/card"))
        .await
        .json()
        .await
        .unwrap();
    let card = &body["data"]["card"];
    assert!(
        card["headline"].as_str().unwrap().contains("feat_card")
            || card["profile_url"].as_str().unwrap().contains("feat_card"),
        "{card}"
    );
    assert_eq!(card["deliverable_url"], "https://figma.test/final");
}
