//! Contests read as one event.
//!
//! An awards edition and a design sprint were two backlog items that looked
//! like two formats. What this suite pins is the claim that they are one
//! mechanism: the same table serves thirteen parallel podiums and a weekend
//! run, and neither needs a bespoke route.

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

async fn a_contest(app: &TestApp, slug: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO tournaments
            (slug, name, kind, starts_at, ends_at, status, rules, scoring_direction, skill_domain)
         VALUES ($1, $1, 'brief_contest', NOW() - INTERVAL '1 day', NOW() + INTERVAL '7 days',
                 'active', '{}'::jsonb, 'higher_is_better', 'design')
         RETURNING id",
    )
    .bind(slug)
    .fetch_one(&app.db)
    .await
    .expect("contest")
}

fn an_edition() -> Value {
    json!({
        "slug": "skilluv-design-awards-2027",
        "name": "Skilluv Design Awards 2027",
        "description": "Treize catégories, une par famille de métiers.",
        "kind": "awards_edition",
        "skill_domain": "design",
        "starts_at": "2027-01-04T00:00:00Z",
        "ends_at": "2027-02-01T00:00:00Z",
    })
}

#[tokio::test]
async fn an_edition_gathers_its_categories() {
    let app = TestApp::spawn().await;
    app.register_user("series_admin").await;
    let admin = user_id(&app, "series_admin").await;
    grant(&app, admin, "admin").await;

    app.login("series_admin").await;
    let created = app.post("/api/admin/series", &an_edition()).await;
    assert_eq!(created.status().as_u16(), 201, "{:?}", created.text().await);

    let brand = a_contest(&app, "awards-brand").await;
    let motion = a_contest(&app, "awards-motion").await;
    for (id, category) in [(brand, "brand"), (motion, "motion")] {
        let resp = app
            .post(
                "/api/admin/series/skilluv-design-awards-2027/tournaments",
                &json!({ "tournament_id": id, "category": category }),
            )
            .await;
        assert_eq!(resp.status().as_u16(), 200, "{:?}", resp.text().await);
    }

    let body: Value = app
        .get("/api/series/skilluv-design-awards-2027/standings")
        .await
        .json()
        .await
        .unwrap();
    let categories = body["data"]["categories"].as_array().unwrap();
    assert_eq!(categories.len(), 2, "{body}");

    // No overall winner, and that is not an omission: summing places across
    // thirteen categories would rank somebody who entered all of them above
    // somebody who won the only category they work in.
    assert!(body["data"].get("overall_winner").is_none(), "{body}");
}

#[tokio::test]
async fn a_category_happens_once_in_an_edition() {
    let app = TestApp::spawn().await;
    app.register_user("series_admin2").await;
    let admin = user_id(&app, "series_admin2").await;
    grant(&app, admin, "admin").await;

    app.login("series_admin2").await;
    app.post("/api/admin/series", &an_edition()).await;

    let first = a_contest(&app, "awards-brand-1").await;
    let second = a_contest(&app, "awards-brand-2").await;
    let attach = |id: Uuid| json!({ "tournament_id": id, "category": "brand" });

    assert_eq!(
        app.post(
            "/api/admin/series/skilluv-design-awards-2027/tournaments",
            &attach(first)
        )
        .await
        .status()
        .as_u16(),
        200
    );

    // Two "best brand" categories in one edition is a mistake nobody notices
    // until the results page shows two winners of the same thing.
    assert_eq!(
        app.post(
            "/api/admin/series/skilluv-design-awards-2027/tournaments",
            &attach(second)
        )
        .await
        .status()
        .as_u16(),
        409
    );
}

#[tokio::test]
async fn a_sprint_is_one_contest_with_no_category() {
    let app = TestApp::spawn().await;
    app.register_user("series_admin3").await;
    let admin = user_id(&app, "series_admin3").await;
    grant(&app, admin, "admin").await;

    app.login("series_admin3").await;
    let created = app
        .post(
            "/api/admin/series",
            &json!({
                "slug": "sprint-2027-w06",
                "name": "Sprint du week-end — thème imposé",
                "kind": "sprint",
                "skill_domain": "design",
                "starts_at": "2027-02-05T17:00:00Z",
                "ends_at": "2027-02-07T22:00:00Z",
            }),
        )
        .await;
    assert_eq!(created.status().as_u16(), 201, "{:?}", created.text().await);

    let contest = a_contest(&app, "sprint-w06-contest").await;
    let resp = app
        .post(
            "/api/admin/series/sprint-2027-w06/tournaments",
            &json!({ "tournament_id": contest }),
        )
        .await;
    assert_eq!(resp.status().as_u16(), 200, "{:?}", resp.text().await);

    let body: Value = app
        .get("/api/series/sprint-2027-w06/standings")
        .await
        .json()
        .await
        .unwrap();
    let categories = body["data"]["categories"].as_array().unwrap();
    assert_eq!(categories.len(), 1);
    assert!(categories[0]["category"].is_null(), "{body}");
}

#[tokio::test]
async fn a_podium_in_a_series_carries_names() {
    let app = TestApp::spawn().await;
    app.register_user("series_admin4").await;
    app.register_user("series_winner").await;
    let admin = user_id(&app, "series_admin4").await;
    let winner = user_id(&app, "series_winner").await;
    grant(&app, admin, "admin").await;

    app.login("series_admin4").await;
    app.post("/api/admin/series", &an_edition()).await;
    let contest = a_contest(&app, "awards-illustration").await;
    app.post(
        "/api/admin/series/skilluv-design-awards-2027/tournaments",
        &json!({ "tournament_id": contest, "category": "illustration" }),
    )
    .await;

    sqlx::query(
        "INSERT INTO tournament_participants
            (tournament_id, participant_type, participant_id, rank, score)
         VALUES ($1, 'user', $2, 1, 92)",
    )
    .bind(contest)
    .bind(winner)
    .execute(&app.db)
    .await
    .unwrap();

    let body: Value = app
        .get("/api/series/skilluv-design-awards-2027/standings")
        .await
        .json()
        .await
        .unwrap();
    let podium = body["data"]["categories"][0]["podium"].as_array().unwrap();
    assert_eq!(podium.len(), 1);
    assert_eq!(podium[0]["username"], "series_winner");
    assert_eq!(podium[0]["rank"], 1);
}

#[tokio::test]
async fn a_series_that_ends_before_it_starts_is_refused() {
    let app = TestApp::spawn().await;
    app.register_user("series_admin5").await;
    let admin = user_id(&app, "series_admin5").await;
    grant(&app, admin, "admin").await;

    app.login("series_admin5").await;
    let resp = app
        .post(
            "/api/admin/series",
            &json!({
                "slug": "backwards",
                "name": "Une série à l'envers",
                "kind": "programme",
                "starts_at": "2027-03-01T00:00:00Z",
                "ends_at": "2027-02-01T00:00:00Z",
            }),
        )
        .await;
    assert_eq!(resp.status().as_u16(), 400);
}

#[tokio::test]
async fn composing_a_series_is_an_editorial_act() {
    let app = TestApp::spawn().await;
    app.register_user("series_nobody").await;
    app.login("series_nobody").await;

    let resp = app.post("/api/admin/series", &an_edition()).await;
    assert_eq!(resp.status().as_u16(), 403);

    // Reading is public: an edition nobody can read is a press release.
    let public = app.get("/api/series").await;
    assert_eq!(public.status().as_u16(), 200);
}
