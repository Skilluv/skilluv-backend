//! SKI-291 — `GET /api/u/{username}/projects`.
//!
//! The route gated on `profile_active`, which only records whether the user
//! cleared onboarding. `GET /api/profile/{username}` gates on
//! `profile_hidden` instead — the same fix was applied there under SKI-70 and
//! never propagated here. The result: one username, two routes, 200 on one
//! and 404 "user not found" on the other.
//!
//! Also covers the payload: the profile page renders one badge per maintained
//! repository, so `github_repo_owner` / `github_repo_name` must reach the
//! client. They were on the table since migration 0055 but absent from the
//! struct, so serde silently dropped them.

mod common;
use common::TestApp;
use uuid::Uuid;

/// Creates a user. `profile_active` defaults to FALSE, which is exactly the
/// state of the e2e account in the ticket.
async fn make_user(app: &TestApp, username: &str) -> Uuid {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "INSERT INTO users (username, email, password_hash, display_name, first_name, last_name)
         VALUES ('{username}', '{username}@test.dev', 'x', '{username}', 'F', 'L')
         RETURNING id"
    )))
    .fetch_one(&app.db)
    .await
    .unwrap()
}

async fn make_project(app: &TestApp, owner: Uuid, slug: &str, repo: Option<(&str, &str)>) {
    sqlx::query(
        "INSERT INTO projects (slug, name, owner_type, owner_id, github_repo_owner, github_repo_name)
         VALUES ($1, $2, 'user', $3, $4, $5)",
    )
    .bind(slug)
    .bind(format!("Project {slug}"))
    .bind(owner)
    .bind(repo.map(|r| r.0))
    .bind(repo.map(|r| r.1))
    .execute(&app.db)
    .await
    .unwrap();
}

async fn projects_of(app: &TestApp, username: &str) -> (u16, serde_json::Value) {
    let resp = app.get(&format!("/api/u/{username}/projects")).await;
    let status = resp.status().as_u16();
    let body = resp.text().await.unwrap_or_default();
    let json = serde_json::from_str(&body).unwrap_or(serde_json::Value::Null);
    (status, json)
}

#[tokio::test]
async fn a_user_who_has_not_finished_onboarding_still_answers_200() {
    let app = TestApp::spawn().await;
    let uid = make_user(&app, "jz_onboarding").await;
    make_project(&app, uid, "onboarding-proj", None).await;

    // The regression: profile_active is FALSE here, as on any fresh account.
    let (status, body) = projects_of(&app, "jz_onboarding").await;
    assert_eq!(status, 200, "body: {body}");
    assert_eq!(body["data"]["projects"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn both_routes_agree_on_the_same_username() {
    let app = TestApp::spawn().await;
    make_user(&app, "jz_agree").await;

    let resp = app.get("/api/profile/jz_agree").await;
    let profile = resp.status().as_u16();
    let profile_body = resp.text().await.unwrap_or_default();
    let (projects, _) = projects_of(&app, "jz_agree").await;
    assert_eq!(
        profile, projects,
        "profile answered {profile} and projects answered {projects} for one \
         username. profile body: {profile_body}"
    );
}

#[tokio::test]
async fn a_user_without_projects_gets_200_and_an_empty_list() {
    let app = TestApp::spawn().await;
    make_user(&app, "jz_empty").await;

    let (status, body) = projects_of(&app, "jz_empty").await;
    assert_eq!(status, 200);
    assert_eq!(
        body["data"]["projects"].as_array().map(Vec::len),
        Some(0),
        "an empty list is 200 with [], not 404 — the front cannot tell \
         'owns nothing' from 'no such account' otherwise"
    );
}

#[tokio::test]
async fn an_unknown_username_is_still_404() {
    let app = TestApp::spawn().await;
    let (status, _) = projects_of(&app, "jz_nobody_here").await;
    assert_eq!(status, 404);
}

#[tokio::test]
async fn a_hidden_profile_is_404() {
    let app = TestApp::spawn().await;
    let uid = make_user(&app, "jz_hidden").await;
    make_project(&app, uid, "hidden-proj", None).await;
    sqlx::query("UPDATE users SET profile_hidden = TRUE WHERE id = $1")
        .bind(uid)
        .execute(&app.db)
        .await
        .unwrap();

    let (status, _) = projects_of(&app, "jz_hidden").await;
    assert_eq!(status, 404);
}

#[tokio::test]
async fn a_banned_user_is_404() {
    let app = TestApp::spawn().await;
    let uid = make_user(&app, "jz_banned").await;
    make_project(&app, uid, "banned-proj", None).await;
    sqlx::query("UPDATE users SET is_banned = TRUE WHERE id = $1")
        .bind(uid)
        .execute(&app.db)
        .await
        .unwrap();

    let (status, _) = projects_of(&app, "jz_banned").await;
    assert_eq!(status, 404);
}

#[tokio::test]
async fn the_payload_carries_the_github_coordinates() {
    let app = TestApp::spawn().await;
    let uid = make_user(&app, "jz_repos").await;
    make_project(&app, uid, "with-repo", Some(("skilluv", "skilluv-backend"))).await;
    make_project(&app, uid, "without-repo", None).await;

    let (status, body) = projects_of(&app, "jz_repos").await;
    assert_eq!(status, 200);

    let projects = body["data"]["projects"].as_array().unwrap();
    assert_eq!(
        projects.len(),
        2,
        "projects without a repo are returned too"
    );

    let with_repo = projects
        .iter()
        .find(|p| p["slug"] == "with-repo")
        .expect("with-repo present");
    assert_eq!(with_repo["github_repo_owner"], "skilluv");
    assert_eq!(with_repo["github_repo_name"], "skilluv-backend");
    assert_eq!(with_repo["name"], "Project with-repo");

    let without_repo = projects
        .iter()
        .find(|p| p["slug"] == "without-repo")
        .expect("without-repo present");
    assert!(
        without_repo["github_repo_owner"].is_null(),
        "the front filters on null, so the key must be present and null"
    );
}

#[tokio::test]
async fn archived_projects_are_left_out() {
    let app = TestApp::spawn().await;
    let uid = make_user(&app, "jz_archived").await;
    make_project(&app, uid, "live-proj", None).await;
    make_project(&app, uid, "dead-proj", None).await;
    sqlx::query("UPDATE projects SET archived_at = NOW() WHERE slug = 'dead-proj'")
        .execute(&app.db)
        .await
        .unwrap();

    let (_, body) = projects_of(&app, "jz_archived").await;
    let projects = body["data"]["projects"].as_array().unwrap();
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0]["slug"], "live-proj");
}
