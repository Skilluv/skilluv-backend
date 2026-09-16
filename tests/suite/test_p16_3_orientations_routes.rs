//! Tests P16.3 : routes API orientations (catalogue public + user_orientations).

use crate::common::TestApp;
use serde_json::json;

#[tokio::test]
async fn get_catalog_lists_curated_orientations() {
    let app = TestApp::spawn().await;
    let resp = app.get("/api/orientations?limit=100").await;
    assert!(resp.status().is_success());
    let body: serde_json::Value = resp.json().await.unwrap();
    let list = body["data"]["orientations"].as_array().unwrap();
    assert!(list.len() >= 30, "seed should expose 30+ curated");
    // Vérifie qu'un slug attendu est présent
    assert!(list.iter().any(|o| o["slug"] == "web-frontend-developer"));
}

#[tokio::test]
async fn get_catalog_filters_by_domain() {
    let app = TestApp::spawn().await;
    let resp = app.get("/api/orientations?domain=security&limit=100").await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let list = body["data"]["orientations"].as_array().unwrap();
    assert!(!list.is_empty());
    for o in list {
        assert_eq!(o["primary_domain"], "security");
    }
}

#[tokio::test]
async fn get_orientation_detail_includes_skills() {
    let app = TestApp::spawn().await;

    // Attache 1 skill au track web-frontend-developer pour vérifier le join.
    let track_id: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM orientations WHERE slug = 'web-frontend-developer'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    let skill_id: uuid::Uuid = sqlx::query_scalar(
        "SELECT id FROM skill_nodes WHERE slug = 'component-composition' LIMIT 1",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO orientation_skill_map (orientation_id, skill_id, is_core, weight)
         VALUES ($1, $2, TRUE, 2.5) ON CONFLICT DO NOTHING",
    )
    .bind(track_id)
    .bind(skill_id)
    .execute(&app.db)
    .await
    .unwrap();

    let resp = app.get("/api/orientations/web-frontend-developer").await;
    assert!(resp.status().is_success());
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(
        body["data"]["orientation"]["slug"],
        "web-frontend-developer"
    );
    let skills = body["data"]["skills"].as_array().unwrap();
    assert!(!skills.is_empty());
    assert!(skills.iter().any(|s| s["slug"] == "component-composition"));
}

#[tokio::test]
async fn get_orientation_detail_404_on_unknown() {
    let app = TestApp::spawn().await;
    let resp = app.get("/api/orientations/does-not-exist").await;
    assert_eq!(resp.status().as_u16(), 404);
}

#[tokio::test]
async fn register_orientation_auto_promotes_first_to_primary() {
    let app = TestApp::spawn().await;
    app.register_user("kim16r3").await;
    app.login("kim16r3").await;

    let resp = app
        .post(
            "/api/users/me/orientations",
            &json!({ "slug": "web-frontend-developer", "mode": "learning" }),
        )
        .await;
    assert_eq!(resp.status().as_u16(), 201);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(
        body["data"]["is_primary"], true,
        "1st orientation auto-primary"
    );
}

#[tokio::test]
async fn cannot_register_more_than_three_active_orientations() {
    let app = TestApp::spawn().await;
    app.register_user("kim16r4").await;
    app.login("kim16r4").await;

    for slug in [
        "web-frontend-developer",
        "web-backend-developer",
        "design-web",
    ] {
        let r = app
            .post("/api/users/me/orientations", &json!({ "slug": slug }))
            .await;
        assert_eq!(r.status().as_u16(), 201, "slug {slug} should succeed");
    }
    // A fourth live orientation, so the 400 is the cap and not something else.
    // This used to name `pentester-web`, which migration 0542 archived when the
    // security domain opened - and an archived slug is refused for its own
    // reason, which would have made this assertion pass while testing nothing.
    let over = app
        .post(
            "/api/users/me/orientations",
            &json!({ "slug": "security-red-team" }),
        )
        .await;
    assert_eq!(over.status().as_u16(), 400, "cap 3 enforced");
}

/// At three trades, you can still change one of your own.
///
/// The ceiling counted the row the upsert was about to update, so re-posting
/// one of your own three looked like a fourth and was refused - with "end one
/// first", about a trade you already had. At three trades that made the whole
/// `DO UPDATE` branch unreachable: no mode change, no moving `is_primary`, no
/// un-ending.
///
/// Onboarding is where it surfaced. The frontend replays the trades picked
/// before the account existed; every replay was refused, so the step concluded
/// nothing had been declared and asked again, five times in a row.
#[tokio::test]
async fn at_the_ceiling_you_can_still_change_a_trade_you_already_hold() {
    let app = TestApp::spawn().await;
    app.register_user("kim16r_reupsert").await;
    app.login("kim16r_reupsert").await;

    for slug in [
        "web-frontend-developer",
        "web-backend-developer",
        "design-web",
    ] {
        let r = app
            .post("/api/users/me/orientations", &json!({ "slug": slug }))
            .await;
        assert_eq!(r.status().as_u16(), 201, "slug {slug} should succeed");
    }

    // The same trade again, with a different mode: the update branch.
    let again = app
        .post(
            "/api/users/me/orientations",
            &json!({ "slug": "design-web", "mode": "active" }),
        )
        .await;
    let status = again.status().as_u16();
    let body: serde_json::Value = again.json().await.unwrap();
    assert_eq!(
        status, 201,
        "re-posting one of your own three is an update, not a fourth: {body}"
    );
    assert_eq!(body["data"]["mode"], "active", "the update has to land");

    // And it is still three, not four.
    let live: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM user_orientations uo
           JOIN users u ON u.id = uo.user_id
          WHERE u.username = 'kim16r_reupsert' AND uo.ended_at IS NULL",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(live, 3, "an update must not add a row");
}

/// Un-ending one of your own, at the ceiling, is the same upsert.
///
/// Somebody who ends a trade and changes their mind has three live again, and
/// the path back is `POST` with the slug they left - which the old count read
/// as a fourth.
#[tokio::test]
async fn a_trade_you_ended_can_be_taken_up_again_at_the_ceiling() {
    let app = TestApp::spawn().await;
    app.register_user("kim16r_unend").await;
    app.login("kim16r_unend").await;

    for slug in [
        "web-frontend-developer",
        "web-backend-developer",
        "design-web",
    ] {
        app.post("/api/users/me/orientations", &json!({ "slug": slug }))
            .await;
    }
    let gone = app.delete("/api/users/me/orientations/design-web").await;
    assert!(gone.status().is_success(), "ending a trade should work");

    let back = app
        .post(
            "/api/users/me/orientations",
            &json!({ "slug": "design-web" }),
        )
        .await;
    assert_eq!(
        back.status().as_u16(),
        201,
        "taking a trade back up is an update of the ended row"
    );
}

#[tokio::test]
async fn delete_orientation_historises_but_keeps_row() {
    let app = TestApp::spawn().await;
    app.register_user("kim16r5").await;
    app.login("kim16r5").await;

    app.post(
        "/api/users/me/orientations",
        &json!({ "slug": "web-backend-developer" }),
    )
    .await;
    let del = app
        .delete("/api/users/me/orientations/web-backend-developer")
        .await;
    assert_eq!(del.status().as_u16(), 200);

    // La ligne existe encore avec ended_at, invisible dans le "actives" mais
    // présente en base - historisation.
    let cnt: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_orientations uo
         JOIN orientations o ON o.id = uo.orientation_id
         WHERE o.slug = 'web-backend-developer' AND uo.ended_at IS NOT NULL",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(cnt, 1);

    // Peut ré-inscrire après end
    let re = app
        .post(
            "/api/users/me/orientations",
            &json!({ "slug": "web-backend-developer" }),
        )
        .await;
    assert_eq!(re.status().as_u16(), 201, "re-registering after end works");
}

#[tokio::test]
async fn patch_switches_primary_flag_atomically() {
    let app = TestApp::spawn().await;
    app.register_user("kim16r6").await;
    app.login("kim16r6").await;

    app.post(
        "/api/users/me/orientations",
        &json!({ "slug": "web-frontend-developer" }),
    )
    .await; // auto-primary
    app.post(
        "/api/users/me/orientations",
        &json!({ "slug": "design-web" }),
    )
    .await;

    let patch = app
        .put(
            "/api/users/me/orientations/design-web",
            &json!({ "is_primary": true }),
        )
        .await;
    // Note: TestApp::put uses PUT, but our route is PATCH. Use the raw client.
    // We'll skip this test path if PATCH isn't in TestApp - swap with client.
    let _ = patch;
    let resp = app
        .client
        .patch(format!("{}/api/users/me/orientations/design-web", app.addr))
        .json(&json!({ "is_primary": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 200);

    // La seule primary active est maintenant design-web.
    let (front_primary, design_primary): (bool, bool) = sqlx::query_as(
        "SELECT
            COALESCE(BOOL_OR(o.slug='web-frontend-developer' AND uo.is_primary), FALSE),
            COALESCE(BOOL_OR(o.slug='design-web' AND uo.is_primary), FALSE)
         FROM user_orientations uo JOIN orientations o ON o.id = uo.orientation_id
         WHERE uo.ended_at IS NULL",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(!front_primary);
    assert!(design_primary);
}
