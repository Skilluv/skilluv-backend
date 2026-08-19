//! The Discord queue, and the producer it never had.
//!
//! `discord_notifications_queue` has existed since migration 0135 with a
//! consumer polling it every fifteen seconds and nothing at all writing to
//! it. These tests cover the writing: that the four events reach the queue,
//! and that each one resolves to the right room.

mod common;
use common::TestApp;
use uuid::Uuid;

/// The Monday a featuring is awarded for. `featured_talents` refuses any
/// other day: a week is named by its Monday, and half the rows landing on a
/// Thursday would make "this week" ambiguous.
fn monday_of_this_week() -> chrono::NaiveDate {
    use chrono::Datelike;
    let today = chrono::Utc::now().date_naive();
    today - chrono::Duration::days(today.weekday().num_days_from_monday() as i64)
}

/// A configured room.
async fn a_channel(app: &TestApp, purpose: &str, domain: &str, id: &str) {
    sqlx::query(
        "INSERT INTO discord_channels (purpose, skill_domain, channel_id, label)
         VALUES ($1, $2, $3, 'test')
         ON CONFLICT (purpose, skill_domain) DO UPDATE SET channel_id = EXCLUDED.channel_id",
    )
    .bind(purpose)
    .bind(domain)
    .bind(id)
    .execute(&app.db)
    .await
    .unwrap();
}

/// The single queued row of a given type, or none.
async fn queued(app: &TestApp, event_type: &str) -> Option<(Option<String>, serde_json::Value)> {
    sqlx::query_as(
        "SELECT target_channel_id, payload_json FROM discord_notifications_queue
          WHERE event_type = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(event_type)
    .fetch_optional(&app.db)
    .await
    .unwrap()
}

#[tokio::test]
async fn a_domain_room_beats_the_shared_one() {
    let app = TestApp::spawn().await;
    // A server that has grown a per-domain room keeps its shared one for the
    // domains that have not. Both configured, the specific one wins.
    a_channel(&app, "contests", "", "111111111").await;
    a_channel(&app, "contests", "design", "222222222").await;

    let design = skilluv_backend::services::discord_announce::resolve_channel(
        &app.db,
        skilluv_backend::services::discord_announce::Purpose::Contests,
        Some("design"),
    )
    .await;
    assert_eq!(design.as_deref(), Some("222222222"));

    let code = skilluv_backend::services::discord_announce::resolve_channel(
        &app.db,
        skilluv_backend::services::discord_announce::Purpose::Contests,
        Some("code"),
    )
    .await;
    assert_eq!(
        code.as_deref(),
        Some("111111111"),
        "falls back to the shared room"
    );
}

#[tokio::test]
async fn an_unconfigured_purpose_still_enqueues() {
    let app = TestApp::spawn().await;
    // No room configured at all. The announcement is queued with a null
    // channel and the consumer posts it in its default — a message in the
    // wrong room is recoverable, a message nobody sent is not.
    skilluv_backend::services::discord_announce::mission_posted(
        &app.db,
        "design",
        "une-mission",
        "Identité pour une coopérative",
    )
    .await;

    let row = queued(&app, "mission_posted").await.expect("queued");
    assert!(row.0.is_none(), "no channel resolved");
    assert_eq!(row.1["slug"], "une-mission");
}

#[tokio::test]
async fn a_created_contest_reaches_the_room() {
    let app = TestApp::spawn().await;
    a_channel(&app, "contests", "design", "333333333").await;
    let creator: Uuid = {
        app.register_user("disc_creator").await;
        sqlx::query_scalar("SELECT id FROM users WHERE username = 'disc_creator'")
            .fetch_one(&app.db)
            .await
            .unwrap()
    };

    skilluv_backend::services::tournament::create_tournament(
        &app.db,
        creator,
        skilluv_backend::services::tournament::CreateTournamentInput {
            season_id: None,
            slug: "concours-identite".into(),
            name: "Concours d'identité".into(),
            description: None,
            kind: "individual".into(),
            format: None,
            prize_pool_fragments: None,
            prize_pool_gp: None,
            sponsor_enterprise_id: None,
            sponsor_logo_url: None,
            sponsor_blurb: None,
            registration_opens_at: None,
            starts_at: chrono::Utc::now() + chrono::Duration::days(1),
            ends_at: chrono::Utc::now() + chrono::Duration::days(30),
            skill_domain: Some("design".into()),
            rules: None,
        },
    )
    .await
    .expect("contest created");

    let row = queued(&app, "contest_opened").await.expect("queued");
    assert_eq!(
        row.0.as_deref(),
        Some("333333333"),
        "routed to the design room"
    );
    assert_eq!(row.1["title"], "Concours d'identité");
    // No prize was funded, so none is announced. "0 €" reads as a mistake.
    assert!(row.1["prize"].is_null(), "{}", row.1);
}

#[tokio::test]
async fn a_featuring_reaches_the_room_with_the_username() {
    let app = TestApp::spawn().await;
    a_channel(&app, "general", "design", "444444444").await;

    // The announcement, not the featuring: whether a featuring may be awarded
    // at all is `test_featured_talents.rs`'s subject, and it needs a verified
    // deliverable to be awarded against. What is under test here is where the
    // announcement lands and what it carries.
    skilluv_backend::services::discord_announce::talent_featured(
        &app.db,
        "design",
        "disc_featured",
        monday_of_this_week(),
    )
    .await;

    let row = queued(&app, "talent_featured").await.expect("queued");
    assert_eq!(row.0.as_deref(), Some("444444444"));
    // The username, not the id: a chat message with a UUID in it helps nobody.
    assert_eq!(row.1["username"], "disc_featured");
}

#[tokio::test]
async fn a_broken_queue_is_swallowed_rather_than_raised() {
    let app = TestApp::spawn().await;

    // Make every insert fail. A Discord post is the least important thing
    // happening at any of these call sites — a contest is still concluded, a
    // featuring still awarded, a mission still published. Trading a real
    // outcome for a chat message is the failure mode this guards against, and
    // the producer returns `()` precisely so a caller cannot make that trade
    // by accident.
    sqlx::query("ALTER TABLE discord_notifications_queue ADD CONSTRAINT never CHECK (FALSE)")
        .execute(&app.db)
        .await
        .unwrap();

    skilluv_backend::services::discord_announce::mission_posted(
        &app.db,
        "design",
        "une-mission",
        "Identité pour une coopérative",
    )
    .await;

    assert!(
        queued(&app, "mission_posted").await.is_none(),
        "the insert was refused, as arranged"
    );
}

#[tokio::test]
async fn an_event_type_nobody_defined_is_refused_by_the_database() {
    let app = TestApp::spawn().await;
    // The allowlist is the contract between the producer and the consumer.
    // A typo that reached the queue would sit there being rendered as
    // "Événement Skilluv : contst_opened" until somebody noticed.
    let bad = sqlx::query(
        "INSERT INTO discord_notifications_queue (event_type, payload_json)
         VALUES ('contst_opened', '{}'::jsonb)",
    )
    .execute(&app.db)
    .await;
    assert!(bad.is_err());
}
