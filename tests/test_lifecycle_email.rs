//! One preference system, and the toggles that now mean something.
//!
//! What is being proved: that turning email off in the settings screen also
//! stops the digest and the onboarding sequence — which it did not, because
//! those read a second table — that quiet hours suppress a buzz without
//! losing the message, and that no email is built outside the shared
//! template any more.

mod common;

use common::TestApp;
use serde_json::json;
use uuid::Uuid;

async fn person(app: &TestApp, username: &str) -> Uuid {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "INSERT INTO users (username, email, password_hash, display_name,
                            first_name, last_name, email_verified, profile_active)
         VALUES ('{username}', '{username}@test.dev', 'x', '{username}', 'F', 'L', TRUE, TRUE)
         RETURNING id"
    )))
    .fetch_one(&app.db)
    .await
    .unwrap()
}

// ─── One system ───────────────────────────────────────────────────

#[tokio::test]
async fn the_second_preference_table_is_gone() {
    let app = TestApp::spawn().await;

    // It answered "may we email this person" for three categories while the
    // catalogue answered it for every kind. Two answers, and the one that
    // won was whichever code path ran.
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM information_schema.tables
                        WHERE table_name = 'user_email_preferences')",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(!exists, "user_email_preferences must not come back");
}

#[tokio::test]
async fn every_email_the_platform_sends_has_a_kind_in_the_catalogue() {
    let app = TestApp::spawn().await;

    // The digest and the six sequences used to be invisible to the settings
    // screen, because they were not kinds. Anything absent here is
    // something a person cannot decline.
    for kind in [
        "digest.weekly",
        "streak.reminder",
        "lifecycle.activate",
        "lifecycle.join_guild",
        "lifecycle.silent",
        "lifecycle.last_chance",
        "lifecycle.enterprise_welcome",
        "lifecycle.enterprise_demo",
        "lifecycle.enterprise_value",
    ] {
        let known: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM notification_kinds WHERE kind = $1)")
                .bind(kind)
                .fetch_one(&app.db)
                .await
                .unwrap();
        assert!(known, "{kind} is sent but cannot be declined");
    }
}

#[tokio::test]
async fn marketing_is_off_until_someone_says_yes() {
    let app = TestApp::spawn().await;

    // Consent is given, never inferred. Every lifecycle kind defaults off.
    let opt_in_by_default: Vec<String> = sqlx::query_scalar(
        "SELECT kind FROM notification_kinds
          WHERE category = 'lifecycle' AND default_email = TRUE",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();
    assert!(
        opt_in_by_default.is_empty(),
        "these would mail people who never agreed: {opt_in_by_default:?}"
    );
}

#[tokio::test]
async fn turning_marketing_on_covers_every_sequence() {
    let app = TestApp::spawn().await;
    app.register_user("life_optin").await;
    app.login("life_optin").await;

    let resp = app
        .put(
            "/api/users/me/email-preferences",
            &json!({ "digest_weekly": true, "streak_reminder": true, "marketing": true }),
        )
        .await;
    assert_eq!(resp.status().as_u16(), 200);

    // One box, six sequences. Read from the catalogue rather than a list in
    // the code, so a sequence added later is covered by the same consent
    // instead of being sent to everybody.
    let enabled: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM notification_preferences p
           JOIN notification_kinds k ON k.kind = p.kind
          WHERE k.category = 'lifecycle' AND p.channel = 'email' AND p.enabled = TRUE",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    let total: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM notification_kinds WHERE category = 'lifecycle'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(enabled, total, "consent must cover every sequence");
}

#[tokio::test]
async fn the_three_words_and_the_per_kind_screen_agree() {
    let app = TestApp::spawn().await;
    app.register_user("life_agree").await;
    app.login("life_agree").await;

    // Turn the digest off through the per-kind screen…
    app.put(
        "/api/users/me/notification-preferences",
        &json!({ "preferences": [{ "kind": "digest.weekly", "email": false }] }),
    )
    .await;

    // …and the three-word view must say so. Before, these were two tables
    // and this is exactly where they disagreed.
    let body: serde_json::Value = app
        .get("/api/users/me/email-preferences")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(
        body["data"]["digest_weekly"],
        json!(false),
        "one storage, two views — they cannot disagree"
    );
}

// ─── Quiet hours ──────────────────────────────────────────────────

#[tokio::test]
async fn a_quiet_window_needs_a_zone_and_both_bounds() {
    let app = TestApp::spawn().await;
    app.register_user("life_quiet").await;
    app.login("life_quiet").await;

    // Half a window is a window nobody can interpret.
    let half = app
        .put(
            "/api/users/me/quiet-hours",
            &json!({ "start": 22, "end": null, "timezone": "Africa/Porto-Novo" }),
        )
        .await;
    assert_eq!(half.status().as_u16(), 400);

    // An hour with no zone cannot be placed in time.
    let zoneless = app
        .put(
            "/api/users/me/quiet-hours",
            &json!({ "start": 22, "end": 7, "timezone": null }),
        )
        .await;
    assert_eq!(zoneless.status().as_u16(), 400);

    // A zone we cannot parse would make the window silently not apply,
    // which looks exactly like the feature being broken.
    let nonsense = app
        .put(
            "/api/users/me/quiet-hours",
            &json!({ "start": 22, "end": 7, "timezone": "Mars/Olympus" }),
        )
        .await;
    assert_eq!(nonsense.status().as_u16(), 400);

    let good = app
        .put(
            "/api/users/me/quiet-hours",
            &json!({ "start": 22, "end": 7, "timezone": "Africa/Porto-Novo" }),
        )
        .await;
    assert_eq!(good.status().as_u16(), 200);
}

#[tokio::test]
async fn clearing_the_window_is_possible() {
    let app = TestApp::spawn().await;
    app.register_user("life_clear").await;
    app.login("life_clear").await;

    app.put(
        "/api/users/me/quiet-hours",
        &json!({ "start": 22, "end": 7, "timezone": "Africa/Porto-Novo" }),
    )
    .await;

    let cleared: serde_json::Value = app
        .put(
            "/api/users/me/quiet-hours",
            &json!({ "start": null, "end": null, "timezone": null }),
        )
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(cleared["data"]["start"], json!(null));
    assert_eq!(cleared["data"]["end"], json!(null));
    assert_eq!(
        cleared["data"]["timezone"], "Africa/Porto-Novo",
        "the zone is theirs and outlives the window"
    );
}

#[tokio::test]
async fn a_quiet_window_never_costs_the_record() {
    let app = TestApp::spawn().await;
    let user = person(&app, "life_night").await;

    // A window covering every hour of the day, so the test does not depend
    // on when it runs.
    sqlx::query(
        "UPDATE users SET quiet_hours_start = 0, quiet_hours_end = 23,
                          timezone = 'Africa/Porto-Novo'
          WHERE id = $1",
    )
    .bind(user)
    .execute(&app.db)
    .await
    .unwrap();

    let delivery = skilluv_backend::services::notify::send(
        skilluv_backend::services::notify::Ctx::db_only(&app.db),
        skilluv_backend::services::notify::Recipient::User(user),
        "social.mention",
    )
    .arg("author", "Ada")
    .arg("excerpt", "regarde ça")
    .execute()
    .await
    .expect("delivery");

    // The buzz is suppressed; the record is not. Quiet hours are about
    // interruption, not about hiding what happened.
    assert_eq!(delivery.push, 0, "no phone buzzes inside the window");
    assert_eq!(delivery.in_app, 1, "the record is always written");
}

// ─── The template ─────────────────────────────────────────────────

#[tokio::test]
async fn the_digest_figures_are_translated_and_themed() {
    use skilluv_backend::services::email_template::{Email, render};
    use skilluv_backend::services::i18n;

    let _app = TestApp::spawn().await;

    let stats = vec![
        (i18n::t("fr", "digest.stat.challenges"), "4".to_string()),
        (i18n::t("fr", "digest.stat.streak"), "12".to_string()),
    ];
    let html = render(Email {
        locale: "fr",
        theme: Some("vesperal"),
        title: &i18n::t("fr", "notification.digest.weekly.title"),
        body: &i18n::t("fr", "notification.digest.weekly.body"),
        recipient_name: Some("Ada"),
        stats: &stats,
        cta_label: None,
        cta_url: None,
        unsubscribe_url: Some("https://skill-uv.com/unsub"),
    });

    assert!(html.contains("contributions"), "the labels are translated");
    assert!(html.contains(">4<"), "the figures are rendered");
    // Laid out as a table: Outlook renders neither flex nor grid, and a
    // digest that collapses into orphaned numbers is worse than none.
    assert!(html.contains("<table"), "tables, not flexbox");
    assert!(
        !html.contains("#6c5ce7"),
        "the accent comes from the theme, not from a colour typed into the caller"
    );
}

#[tokio::test]
async fn no_email_points_at_a_domain_we_do_not_own() {
    use skilluv_backend::services::email_template::{Email, render};

    let _app = TestApp::spawn().await;

    let html = render(Email {
        locale: "fr",
        theme: None,
        title: "t",
        body: "b",
        recipient_name: None,
        stats: &[],
        cta_label: Some("Ouvrir"),
        cta_url: Some("https://skill-uv.com/dashboard"),
        unsubscribe_url: None,
    });
    // Twenty-two places hardcoded `skilluv.com`, which is not the domain
    // that was bought. Anyone could register it and receive the traffic.
    assert!(
        !html.contains("https://skilluv.com"),
        "the domain is skill-uv.com"
    );
}

// ─── Background senders reach the inbox too ───────────────────────

#[tokio::test]
async fn a_notification_from_a_pool_only_caller_can_still_email() {
    let app = TestApp::spawn().await;
    let user = person(&app, "life_ambient").await;

    // `rank.promoted` has email on by default, and the proof engine that
    // emits it holds a `PgPool` and nothing else. That combination used to
    // log "email channel requested but this context carries no email
    // service" and send nothing — every rank promotion, every first
    // verified contribution, and the queue of payouts an operator has to
    // unblock.
    let delivery = skilluv_backend::services::notify::send(
        skilluv_backend::services::notify::Ctx::db_only(&app.db),
        skilluv_backend::services::notify::Recipient::User(user),
        "rank.promoted",
    )
    .arg("rank", "Ranger")
    .execute()
    .await
    .expect("delivery");

    assert_eq!(
        delivery.email, 1,
        "a background sender must reach the inbox: {:?}",
        delivery.failures
    );
    assert!(
        delivery.failures.is_empty(),
        "no channel was meant to fire and failed: {:?}",
        delivery.failures
    );

    // And it was recorded, so a bounce or a complaint can be traced back.
    let logged: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM email_log WHERE user_id = $1 AND kind = $2")
            .bind(user)
            .bind("rank.promoted")
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(logged, 1);
}

#[tokio::test]
async fn a_transactional_kind_from_a_background_sweep_reaches_someone() {
    let app = TestApp::spawn().await;

    // `admin.payout_needs_replay` is transactional and emitted by the
    // reconciliation sweep, which has no request behind it. A transactional
    // notification reaching nobody is a lost obligation, not a missed nudge.
    let admin = person(&app, "life_admin").await;
    sqlx::query("INSERT INTO user_capabilities (user_id, capability) VALUES ($1, 'admin')")
        .bind(admin)
        .execute(&app.db)
        .await
        .unwrap();

    let delivery = skilluv_backend::services::notify::send(
        skilluv_backend::services::notify::Ctx::db_only(&app.db),
        skilluv_backend::services::notify::Recipient::Capability("admin"),
        "admin.payout_needs_replay",
    )
    .arg("count", "1")
    .execute()
    .await
    .expect("delivery");

    assert!(
        delivery.email > 0,
        "the payout queue must reach an inbox: {:?}",
        delivery.failures
    );
}
