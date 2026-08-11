//! Integration tests for SKI-40 — time-boxed study cohorts.
//!
//! Focus is on the invariants that a CHECK constraint cannot express:
//! capacity, lifecycle freezing, organizer continuity, and the visibility
//! split between a public cohort and its private conversation.

mod common;

use common::TestApp;
use reqwest::StatusCode;
use serde_json::{Value, json};
use uuid::Uuid;

fn user_id_of(register_body: &Value) -> Uuid {
    register_body["data"]["user"]["id"]
        .as_str()
        .expect("register response carries a user id")
        .parse()
        .expect("user id is a uuid")
}

fn future(days: i64) -> String {
    (chrono::Utc::now() + chrono::Duration::days(days)).to_rfc3339()
}

async fn seed_orientation(app: &TestApp, slug: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO orientations (id, slug, name, primary_domain, is_curated)
         VALUES ($1, $2, 'Test orientation', 'code', TRUE)",
    )
    .bind(id)
    .bind(slug)
    .execute(&app.db)
    .await
    .expect("seed orientation");
    id
}

/// Create a cohort as the currently logged-in user and return its id.
async fn create_cohort(app: &TestApp, slug: &str, extra: Value) -> Value {
    let mut body = json!({
        "slug": slug,
        "name": "Rust bootcamp Q3",
        "starts_at": future(1),
        "ends_at": future(90),
    });
    if let (Some(base), Some(extra)) = (body.as_object_mut(), extra.as_object()) {
        for (k, v) in extra {
            base.insert(k.clone(), v.clone());
        }
    }
    let resp = app.post("/api/cohorts", &body).await;
    assert_eq!(resp.status(), StatusCode::CREATED, "cohort creation failed");
    resp.json().await.unwrap()
}

#[tokio::test]
async fn create_seats_the_creator_as_organizer() {
    let app = TestApp::spawn().await;
    let me = app.register_user("cohortcreator").await;
    app.login("cohortcreator").await;
    let my_id = user_id_of(&me);

    let created = create_cohort(&app, "rust-bootcamp-q3", json!({})).await;
    let cohort_id = created["data"]["cohort"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let role: String = sqlx::query_scalar(
        "SELECT role FROM cohort_members WHERE cohort_id = $1::UUID AND user_id = $2",
    )
    .bind(&cohort_id)
    .bind(my_id)
    .fetch_one(&app.db)
    .await
    .expect("creator is a member");
    assert_eq!(
        role, "organizer",
        "a cohort without an organizer would be unadministrable from birth"
    );

    let body: Value = app
        .get(&format!("/api/cohorts/{cohort_id}"))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["member_count"], 1);
    assert_eq!(body["data"]["my_role"], "organizer");
}

#[tokio::test]
async fn create_validates_its_window_and_bounds() {
    let app = TestApp::spawn().await;
    app.register_user("cohortvalid").await;
    app.login("cohortvalid").await;

    let cases = [
        // ends before it starts
        json!({ "slug": "bad-window", "name": "X cohort",
                "starts_at": future(30), "ends_at": future(10) }),
        // already over
        json!({ "slug": "past-cohort", "name": "X cohort",
                "starts_at": future(-90), "ends_at": future(-1) }),
        // slug shape
        json!({ "slug": "Not A Slug", "name": "X cohort",
                "starts_at": future(1), "ends_at": future(30) }),
        // name too short
        json!({ "slug": "short-name", "name": "X",
                "starts_at": future(1), "ends_at": future(30) }),
        // beyond the small-group cap
        json!({ "slug": "too-big", "name": "X cohort", "max_members": 200,
                "starts_at": future(1), "ends_at": future(30) }),
        json!({ "slug": "too-small", "name": "X cohort", "max_members": 1,
                "starts_at": future(1), "ends_at": future(30) }),
    ];

    for case in cases {
        let resp = app.post("/api/cohorts", &case).await;
        assert_eq!(
            resp.status(),
            StatusCode::BAD_REQUEST,
            "payload {case} should be rejected"
        );
    }
}

#[tokio::test]
async fn duplicate_slug_is_a_conflict() {
    let app = TestApp::spawn().await;
    app.register_user("cohortslug").await;
    app.login("cohortslug").await;

    create_cohort(&app, "taken-slug", json!({})).await;

    let resp = app
        .post(
            "/api/cohorts",
            &json!({ "slug": "taken-slug", "name": "Another cohort",
                     "starts_at": future(1), "ends_at": future(30) }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn join_respects_capacity() {
    let app = TestApp::spawn().await;
    app.register_user("caporganizer").await;
    app.login("caporganizer").await;
    // max_members 2: the organizer plus exactly one joiner.
    let created = create_cohort(&app, "tiny-cohort", json!({ "max_members": 2 })).await;
    let cohort_id = created["data"]["cohort"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    app.register_user("capjoiner1").await;
    app.login("capjoiner1").await;
    let resp = app
        .post(&format!("/api/cohorts/{cohort_id}/join"), &json!({}))
        .await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Joining twice is a conflict, not a second seat.
    let resp = app
        .post(&format!("/api/cohorts/{cohort_id}/join"), &json!({}))
        .await;
    assert_eq!(resp.status(), StatusCode::CONFLICT);

    app.register_user("capjoiner2").await;
    app.login("capjoiner2").await;
    let resp = app
        .post(&format!("/api/cohorts/{cohort_id}/join"), &json!({}))
        .await;
    assert_eq!(
        resp.status(),
        StatusCode::CONFLICT,
        "the third person must not get a seat in a 2-person cohort"
    );

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM cohort_members WHERE cohort_id = $1::UUID")
            .bind(&cohort_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(count, 2);
}

#[tokio::test]
async fn private_cohort_is_invisible_and_invite_only() {
    let app = TestApp::spawn().await;
    let organizer = app.register_user("privorganizer").await;
    app.login("privorganizer").await;
    let organizer_id = user_id_of(&organizer);
    let created = create_cohort(&app, "private-cohort", json!({ "is_public": false })).await;
    let cohort_id = created["data"]["cohort"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let outsider = app.register_user("privoutsider").await;
    let outsider_id = user_id_of(&outsider);
    app.login("privoutsider").await;

    let resp = app.get(&format!("/api/cohorts/{cohort_id}")).await;
    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "a private cohort answers 404 so its id cannot be probed"
    );

    let resp = app
        .post(&format!("/api/cohorts/{cohort_id}/join"), &json!({}))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND, "no self-serve join");

    // It also stays out of discovery.
    let body: Value = app.get("/api/cohorts").await.json().await.unwrap();
    assert!(body["data"]["cohorts"].as_array().unwrap().is_empty());

    // The organizer can add them directly.
    app.login("privorganizer").await;
    let resp = app
        .post(
            &format!("/api/cohorts/{cohort_id}/members"),
            &json!({ "user_id": outsider_id }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Now it shows up in their own list.
    app.login("privoutsider").await;
    let body: Value = app.get("/api/users/me/cohorts").await.json().await.unwrap();
    assert_eq!(body["data"]["cohorts"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["cohorts"][0]["my_role"], "member");
    let _ = organizer_id;
}

#[tokio::test]
async fn last_organizer_cannot_leave() {
    let app = TestApp::spawn().await;
    app.register_user("soloorganizer").await;
    app.login("soloorganizer").await;
    let created = create_cohort(&app, "solo-cohort", json!({})).await;
    let cohort_id = created["data"]["cohort"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let resp = app.delete(&format!("/api/cohorts/{cohort_id}/leave")).await;
    assert_eq!(
        resp.status(),
        StatusCode::CONFLICT,
        "leaving would strand the cohort with no one able to administer it"
    );

    // Promote someone else, then leaving is allowed.
    let second = app.register_user("secondorganizer").await;
    let second_id = user_id_of(&second);
    app.login("soloorganizer").await;
    app.post(
        &format!("/api/cohorts/{cohort_id}/members"),
        &json!({ "user_id": second_id, "role": "organizer" }),
    )
    .await;

    let resp = app.delete(&format!("/api/cohorts/{cohort_id}/leave")).await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn chat_is_members_only_and_persisted() {
    let app = TestApp::spawn().await;
    app.register_user("chatorganizer").await;
    app.login("chatorganizer").await;
    let created = create_cohort(&app, "chat-cohort", json!({})).await;
    let cohort_id = created["data"]["cohort"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let resp = app
        .post(
            &format!("/api/cohorts/{cohort_id}/messages"),
            &json!({ "body": "bienvenue tout le monde" }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    let body: Value = app
        .get(&format!("/api/cohorts/{cohort_id}/messages"))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["messages"].as_array().unwrap().len(), 1);

    // A non-member cannot read or write, even though the cohort is public.
    app.register_user("chatoutsider").await;
    app.login("chatoutsider").await;

    let resp = app.get(&format!("/api/cohorts/{cohort_id}/messages")).await;
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "a public cohort's existence is public; its conversation is not"
    );
    let resp = app
        .post(
            &format!("/api/cohorts/{cohort_id}/messages"),
            &json!({ "body": "hello" }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // Empty and oversized bodies are rejected for members.
    app.login("chatorganizer").await;
    let resp = app
        .post(
            &format!("/api/cohorts/{cohort_id}/messages"),
            &json!({ "body": "   " }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let resp = app
        .post(
            &format!("/api/cohorts/{cohort_id}/messages"),
            &json!({ "body": "x".repeat(4001) }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn archived_cohort_is_frozen_but_readable() {
    let app = TestApp::spawn().await;
    app.register_user("archorganizer").await;
    app.login("archorganizer").await;
    let created = create_cohort(&app, "arch-cohort", json!({})).await;
    let cohort_id = created["data"]["cohort"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let resp = app
        .client
        .patch(format!("{}/api/cohorts/{cohort_id}", app.addr))
        .json(&json!({ "archive": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Still readable — the archive is the point.
    let resp = app.get(&format!("/api/cohorts/{cohort_id}")).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = app
        .get(&format!("/api/cohorts/{cohort_id}/messages"))
        .await
        .json()
        .await
        .unwrap();
    assert!(body["data"]["messages"].as_array().unwrap().is_empty());

    // But frozen for writes.
    let resp = app
        .post(
            &format!("/api/cohorts/{cohort_id}/messages"),
            &json!({ "body": "still here?" }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::CONFLICT);

    app.register_user("archjoiner").await;
    app.login("archjoiner").await;
    let resp = app
        .post(&format!("/api/cohorts/{cohort_id}/join"), &json!({}))
        .await;
    assert_eq!(resp.status(), StatusCode::CONFLICT);

    // And gone from discovery.
    let body: Value = app.get("/api/cohorts").await.json().await.unwrap();
    assert!(body["data"]["cohorts"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn milestones_are_organizer_only() {
    let app = TestApp::spawn().await;
    app.register_user("msorganizer").await;
    app.login("msorganizer").await;
    let created = create_cohort(&app, "ms-cohort", json!({})).await;
    let cohort_id = created["data"]["cohort"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let target = (chrono::Utc::now() + chrono::Duration::days(20))
        .date_naive()
        .to_string();
    let resp = app
        .post(
            &format!("/api/cohorts/{cohort_id}/milestones"),
            &json!({ "title": "Ship the parser", "target_date": target }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let created_ms: Value = resp.json().await.unwrap();
    let ms_id = created_ms["data"]["milestone"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    app.register_user("msmember").await;
    app.login("msmember").await;
    app.post(&format!("/api/cohorts/{cohort_id}/join"), &json!({}))
        .await;

    // A member reads milestones but cannot write them.
    let body: Value = app
        .get(&format!("/api/cohorts/{cohort_id}/milestones"))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["milestones"].as_array().unwrap().len(), 1);

    let resp = app
        .post(
            &format!("/api/cohorts/{cohort_id}/milestones"),
            &json!({ "title": "My own milestone", "target_date": target }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    let resp = app
        .delete(&format!("/api/cohorts/{cohort_id}/milestones/{ms_id}"))
        .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    app.login("msorganizer").await;
    let resp = app
        .delete(&format!("/api/cohorts/{cohort_id}/milestones/{ms_id}"))
        .await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn discovery_filters_by_orientation_and_reports_seats() {
    let app = TestApp::spawn().await;
    app.register_user("discorganizer").await;
    app.login("discorganizer").await;

    let orientation_id = seed_orientation(&app, "rust-backend").await;
    create_cohort(
        &app,
        "rust-cohort",
        json!({ "orientation_id": orientation_id, "max_members": 5 }),
    )
    .await;
    create_cohort(&app, "generic-cohort", json!({})).await;

    let body: Value = app.get("/api/cohorts").await.json().await.unwrap();
    assert_eq!(body["data"]["cohorts"].as_array().unwrap().len(), 2);

    let body: Value = app
        .get("/api/cohorts?orientation=rust-backend")
        .await
        .json()
        .await
        .unwrap();
    let items = body["data"]["cohorts"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["orientation_slug"], "rust-backend");
    assert_eq!(items[0]["member_count"], 1);
    assert_eq!(
        items[0]["seats_left"], 4,
        "seats_left accounts for the organizer already in the room"
    );
}

#[tokio::test]
async fn max_members_cannot_be_lowered_below_current_headcount() {
    let app = TestApp::spawn().await;
    app.register_user("shrinkorganizer").await;
    app.login("shrinkorganizer").await;
    let created = create_cohort(&app, "shrink-cohort", json!({ "max_members": 10 })).await;
    let cohort_id = created["data"]["cohort"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    for name in ["shrinkm1", "shrinkm2"] {
        app.register_user(name).await;
        app.login(name).await;
        app.post(&format!("/api/cohorts/{cohort_id}/join"), &json!({}))
            .await;
    }

    app.login("shrinkorganizer").await;
    let resp = app
        .client
        .patch(format!("{}/api/cohorts/{cohort_id}", app.addr))
        .json(&json!({ "max_members": 2 }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::CONFLICT,
        "a cohort permanently over capacity could never be reconciled"
    );

    let resp = app
        .client
        .patch(format!("{}/api/cohorts/{cohort_id}", app.addr))
        .json(&json!({ "max_members": 3 }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}
