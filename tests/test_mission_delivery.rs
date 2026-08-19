//! Handing a paid mission in, being told to try again, and rating afterwards.
//!
//! Two claims this suite exists to hold:
//!
//!   * the mission status never goes backwards, even though the work does —
//!     the rounds live on the delivery, and the mission reaches `delivered`
//!     only when a round is accepted;
//!   * a rating is written blind. One that the other side can read before
//!     writing their own is a negotiation, not a rating.

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

/// An enterprise with one member, and a mission assigned to a designer.
///
/// Built with SQL rather than through the enterprise endpoints: those carry
/// their own gate (verified email, second factor) which is not what this
/// suite is about.
async fn a_mission_in_progress(
    app: &TestApp,
    client: Uuid,
    talent: Uuid,
    slug: &str,
    included_rounds: Option<i16>,
) -> Uuid {
    let enterprise: Uuid = sqlx::query_scalar(
        "INSERT INTO enterprises (owner_id, company_name, slug, company_size)
         VALUES ($1, 'Coopérative test', $2, '11-50') RETURNING id",
    )
    .bind(client)
    .bind(format!("ent-{}", Uuid::new_v4()))
    .fetch_one(&app.db)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO enterprise_members (enterprise_id, user_id, role, status)
         VALUES ($1, $2, 'owner', 'active')",
    )
    .bind(enterprise)
    .bind(client)
    .execute(&app.db)
    .await
    .unwrap();

    let mission_type: Uuid = sqlx::query_scalar(
        "SELECT id FROM mission_types WHERE skill_domain = 'design' ORDER BY sort_order LIMIT 1",
    )
    .fetch_one(&app.db)
    .await
    .expect("migration 0240 seeds the design mission types");

    sqlx::query_scalar(
        "INSERT INTO missions
            (slug, enterprise_id, mission_type_id, skill_domain, title, description,
             acceptance_criteria, deliverable_format, payment_model, budget_eur,
             commission_percent, status, assigned_user_id, assigned_at, included_rounds)
         VALUES ($1, $2, $3, 'design', 'Identité coopérative',
                 'Logotype, palette et guidelines.',
                 'Le logotype tient en une couleur et reste lisible en favicon.',
                 -- A fixed-price mission has to carry the price: the schema
                 -- refuses a payment model with no figure behind it.
                 'brand_package', 'fixed_price', 2000, 15, 'in_progress', $4, NOW(), $5)
         RETURNING id",
    )
    .bind(slug)
    .bind(enterprise)
    .bind(mission_type)
    .bind(talent)
    .bind(included_rounds)
    .fetch_one(&app.db)
    .await
    .expect("mission")
}

fn a_delivery(round: u32) -> Value {
    json!({
        "artifact_url": format!("https://figma.test/mission/round-{round}"),
        "notes_md": "Le logotype tient maintenant en monochrome, la contreforme est ouverte.",
    })
}

#[tokio::test]
async fn a_design_mission_can_be_handed_over_as_a_brand_package() {
    let app = TestApp::spawn().await;
    app.register_user("mission_client").await;
    app.register_user("mission_talent").await;
    let client = user_id(&app, "mission_client").await;
    let talent = user_id(&app, "mission_talent").await;

    // The four accepted formats used to be code shapes only, so a design
    // mission had to claim `consulting_report` and lie about what it produced.
    let mission = a_mission_in_progress(&app, client, talent, "m-brand-package", Some(3)).await;

    let format: String =
        sqlx::query_scalar("SELECT deliverable_format FROM missions WHERE id = $1")
            .bind(mission)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(format, "brand_package");
}

#[tokio::test]
async fn a_round_of_changes_leaves_the_mission_in_progress() {
    let app = TestApp::spawn().await;
    app.register_user("rounds_client").await;
    app.register_user("rounds_talent").await;
    let client = user_id(&app, "rounds_client").await;
    let talent = user_id(&app, "rounds_talent").await;
    a_mission_in_progress(&app, client, talent, "m-rounds", Some(3)).await;

    app.login("rounds_talent").await;
    let first = app
        .post("/api/missions/m-rounds/deliveries", &a_delivery(1))
        .await;
    assert_eq!(first.status().as_u16(), 201, "{:?}", first.text().await);

    app.login("rounds_client").await;
    let changes = app
        .post(
            "/api/missions/m-rounds/deliveries/request-changes",
            &json!({
                "reason": "La marque ne survit pas au monochrome : les deux contreformes se ferment."
            }),
        )
        .await;
    assert_eq!(changes.status().as_u16(), 200, "{:?}", changes.text().await);

    // Nothing regresses, because nothing had advanced. Two or three rounds is
    // the normal case for design work.
    let status: String = sqlx::query_scalar("SELECT status FROM missions WHERE slug = 'm-rounds'")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(status, "in_progress");

    // And the second round can be handed in.
    app.login("rounds_talent").await;
    let second = app
        .post("/api/missions/m-rounds/deliveries", &a_delivery(2))
        .await;
    assert_eq!(second.status().as_u16(), 201, "{:?}", second.text().await);

    app.login("rounds_client").await;
    let accepted = app
        .post("/api/missions/m-rounds/deliveries/accept", &json!({}))
        .await;
    assert_eq!(
        accepted.status().as_u16(),
        200,
        "{:?}",
        accepted.text().await
    );

    let status: String = sqlx::query_scalar("SELECT status FROM missions WHERE slug = 'm-rounds'")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(status, "delivered", "accepting is what ends the round loop");
}

#[tokio::test]
async fn asking_for_changes_without_saying_what_is_wrong_is_refused() {
    let app = TestApp::spawn().await;
    app.register_user("vague_client").await;
    app.register_user("vague_talent").await;
    let client = user_id(&app, "vague_client").await;
    let talent = user_id(&app, "vague_talent").await;
    a_mission_in_progress(&app, client, talent, "m-vague", Some(3)).await;

    app.login("vague_talent").await;
    app.post("/api/missions/m-vague/deliveries", &a_delivery(1))
        .await;

    // "Not quite" costs a round and teaches nothing.
    app.login("vague_client").await;
    let resp = app
        .post(
            "/api/missions/m-vague/deliveries/request-changes",
            &json!({ "reason": "bof" }),
        )
        .await;
    assert_eq!(resp.status().as_u16(), 400);
}

#[tokio::test]
async fn a_round_past_what_was_agreed_is_marked_not_refused() {
    let app = TestApp::spawn().await;
    app.register_user("scope_client").await;
    app.register_user("scope_talent").await;
    let client = user_id(&app, "scope_client").await;
    let talent = user_id(&app, "scope_talent").await;
    a_mission_in_progress(&app, client, talent, "m-scope", Some(1)).await;

    let reason = "La direction ne convient pas, reprenons depuis la typographie.";

    app.login("scope_talent").await;
    app.post("/api/missions/m-scope/deliveries", &a_delivery(1))
        .await;
    app.login("scope_client").await;
    app.post(
        "/api/missions/m-scope/deliveries/request-changes",
        &json!({ "reason": reason }),
    )
    .await;

    // The brief included one round. The second is still allowed — the
    // platform is not party to the contract — but it is on the record.
    app.login("scope_talent").await;
    let second = app
        .post("/api/missions/m-scope/deliveries", &a_delivery(2))
        .await;
    assert_eq!(second.status().as_u16(), 201, "{:?}", second.text().await);

    let body: Value = app
        .get("/api/missions/m-scope/deliveries")
        .await
        .json()
        .await
        .unwrap();
    let rounds = body["data"]["rounds"].as_array().unwrap();
    assert_eq!(rounds[0]["beyond_agreed_rounds"], json!(false));
    assert_eq!(rounds[1]["beyond_agreed_rounds"], json!(true));
}

#[tokio::test]
async fn a_second_round_cannot_bury_an_unanswered_one() {
    let app = TestApp::spawn().await;
    app.register_user("bury_client").await;
    app.register_user("bury_talent").await;
    let client = user_id(&app, "bury_client").await;
    let talent = user_id(&app, "bury_talent").await;
    a_mission_in_progress(&app, client, talent, "m-bury", Some(3)).await;

    app.login("bury_talent").await;
    app.post("/api/missions/m-bury/deliveries", &a_delivery(1))
        .await;

    // Otherwise a request for changes could be answered by pretending it was
    // never made.
    let second = app
        .post("/api/missions/m-bury/deliveries", &a_delivery(2))
        .await;
    assert_eq!(second.status().as_u16(), 409);
}

#[tokio::test]
async fn only_the_assigned_person_delivers_and_only_the_client_answers() {
    let app = TestApp::spawn().await;
    app.register_user("side_client").await;
    app.register_user("side_talent").await;
    app.register_user("side_stranger").await;
    let client = user_id(&app, "side_client").await;
    let talent = user_id(&app, "side_talent").await;
    a_mission_in_progress(&app, client, talent, "m-sides", Some(3)).await;

    app.login("side_stranger").await;
    let intruder = app
        .post("/api/missions/m-sides/deliveries", &a_delivery(1))
        .await;
    assert_eq!(intruder.status().as_u16(), 403);

    app.login("side_talent").await;
    app.post("/api/missions/m-sides/deliveries", &a_delivery(1))
        .await;

    // The designer cannot accept their own delivery.
    let self_accept = app
        .post("/api/missions/m-sides/deliveries/accept", &json!({}))
        .await;
    assert_eq!(self_accept.status().as_u16(), 403);
}

#[tokio::test]
async fn a_rating_stays_hidden_until_the_other_side_writes() {
    let app = TestApp::spawn().await;
    app.register_user("rate_client").await;
    app.register_user("rate_talent").await;
    let client = user_id(&app, "rate_client").await;
    let talent = user_id(&app, "rate_talent").await;
    a_mission_in_progress(&app, client, talent, "m-rating", Some(3)).await;

    app.login("rate_talent").await;
    app.post("/api/missions/m-rating/deliveries", &a_delivery(1))
        .await;
    app.login("rate_client").await;
    app.post("/api/missions/m-rating/deliveries/accept", &json!({}))
        .await;

    let first = app
        .post(
            "/api/missions/m-rating/ratings",
            &json!({ "rating": 5, "comment_md": "Brief clair, paiement rapide." }),
        )
        .await;
    assert_eq!(first.status().as_u16(), 201, "{:?}", first.text().await);

    // A designer who could see five stars would write five back. That is a
    // negotiation, not a rating.
    let hidden: Value = app
        .get("/api/missions/m-rating/ratings")
        .await
        .json()
        .await
        .unwrap();
    assert!(
        hidden["data"]["ratings"].as_array().unwrap().is_empty(),
        "{hidden}"
    );

    app.login("rate_talent").await;
    let second = app
        .post("/api/missions/m-rating/ratings", &json!({ "rating": 4 }))
        .await;
    assert_eq!(second.status().as_u16(), 201, "{:?}", second.text().await);

    let revealed: Value = app
        .get("/api/missions/m-rating/ratings")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(revealed["data"]["ratings"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn a_silent_client_cannot_suppress_a_rating_for_ever() {
    let app = TestApp::spawn().await;
    app.register_user("silent_client").await;
    app.register_user("silent_talent").await;
    let client = user_id(&app, "silent_client").await;
    let talent = user_id(&app, "silent_talent").await;
    a_mission_in_progress(&app, client, talent, "m-silent", Some(3)).await;

    app.login("silent_talent").await;
    app.post("/api/missions/m-silent/deliveries", &a_delivery(1))
        .await;
    app.login("silent_client").await;
    app.post("/api/missions/m-silent/deliveries/accept", &json!({}))
        .await;

    app.login("silent_talent").await;
    app.post("/api/missions/m-silent/ratings", &json!({ "rating": 2 }))
        .await;

    // The client never answers. After the window, the designer's rating is
    // readable anyway — otherwise silence is a veto.
    sqlx::query(
        "UPDATE mission_ratings SET created_at = NOW() - INTERVAL '15 days'
          WHERE mission_id = (SELECT id FROM missions WHERE slug = 'm-silent')",
    )
    .execute(&app.db)
    .await
    .unwrap();

    let body: Value = app
        .get("/api/missions/m-silent/ratings")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["ratings"].as_array().unwrap().len(), 1);

    // And it counts towards the client's standing.
    let standing: Value = app
        .get("/api/users/silent_client/mission-standing")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(standing["data"]["standing"]["received"], 1);
    assert_eq!(standing["data"]["standing"]["average"], 2.0);
}

#[tokio::test]
async fn an_unrated_person_is_not_a_badly_rated_one() {
    let app = TestApp::spawn().await;
    app.register_user("unrated").await;

    // A zero on a profile would say the opposite of what is true.
    let body: Value = app
        .get("/api/users/unrated/mission-standing")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["standing"]["received"], 0);
    assert!(body["data"]["standing"]["average"].is_null(), "{body}");
}

#[tokio::test]
async fn work_still_being_argued_about_is_not_rated() {
    let app = TestApp::spawn().await;
    app.register_user("early_client").await;
    app.register_user("early_talent").await;
    let client = user_id(&app, "early_client").await;
    let talent = user_id(&app, "early_talent").await;
    a_mission_in_progress(&app, client, talent, "m-early", Some(3)).await;

    // Rating a mission mid-flight is a lever, not an opinion.
    app.login("early_client").await;
    let resp = app
        .post("/api/missions/m-early/ratings", &json!({ "rating": 1 }))
        .await;
    assert_eq!(resp.status().as_u16(), 409);
}
