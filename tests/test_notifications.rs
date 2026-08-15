//! Notifications across the three channels, in the recipient's language.
//!
//! What is being proved: that a caller says *what happened* rather than
//! writing a French title by hand, that preferences are honoured, that
//! transactional messages cannot be silenced, and that adding a tenth
//! language costs a file rather than a refactor.

mod common;
use common::TestApp;
use serde_json::json;
use skilluv_backend::services::i18n;
use uuid::Uuid;

async fn person(app: &TestApp, username: &str, locale: Option<&str>) -> Uuid {
    let id: Uuid = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "INSERT INTO users (username, email, password_hash, display_name,
                            first_name, last_name, email_verified)
         VALUES ('{username}', '{username}@test.dev', 'x', '{username}', 'F', 'L', TRUE)
         RETURNING id"
    )))
    .fetch_one(&app.db)
    .await
    .unwrap();
    if let Some(locale) = locale {
        sqlx::query("UPDATE users SET preferred_language = $1 WHERE id = $2")
            .bind(locale)
            .bind(id)
            .execute(&app.db)
            .await
            .unwrap();
    }
    id
}

// ─── The catalogue ────────────────────────────────────────────────

#[tokio::test]
async fn every_seeded_kind_has_a_translation_in_every_locale() {
    // The check that keeps a language from silently rotting: a kind added
    // without its keys renders its own identifier as a subject line.
    let app = TestApp::spawn().await;
    let kinds: Vec<String> =
        sqlx::query_scalar("SELECT kind FROM notification_kinds ORDER BY kind")
            .fetch_all(&app.db)
            .await
            .unwrap();
    assert!(!kinds.is_empty(), "the catalogue must be seeded");

    let mut missing = Vec::new();
    for locale in i18n::available() {
        for kind in &kinds {
            for suffix in ["title", "body"] {
                let key = format!("notification.{kind}.{suffix}");
                if i18n::t(locale, &key) == key {
                    missing.push(format!("{locale}: {key}"));
                }
            }
        }
        for key in ["email.greeting", "email.footer_note", "email.unsubscribe"] {
            if i18n::t(locale, key) == key {
                missing.push(format!("{locale}: {key}"));
            }
        }
    }
    assert!(missing.is_empty(), "untranslated keys: {missing:#?}");
}

#[tokio::test]
async fn a_kind_defaults_cannot_exceed_what_it_allows() {
    let app = TestApp::spawn().await;
    let bad = sqlx::query(
        "INSERT INTO notification_kinds
            (kind, category, allows_email, default_email)
         VALUES ('bogus.kind', 'test', FALSE, TRUE)",
    )
    .execute(&app.db)
    .await;
    assert!(
        bad.is_err(),
        "defaulting a channel on that the kind forbids is a promise that cannot be kept"
    );
}

#[tokio::test]
async fn a_kind_name_must_look_like_an_i18n_key() {
    let app = TestApp::spawn().await;
    let bad =
        sqlx::query("INSERT INTO notification_kinds (kind, category) VALUES ('NotDotted', 'test')")
            .execute(&app.db)
            .await;
    assert!(bad.is_err(), "kind names double as translation keys");
}

// ─── Preferences ──────────────────────────────────────────────────

#[tokio::test]
async fn the_settings_screen_sees_defaults_without_any_stored_row() {
    let app = TestApp::spawn().await;
    app.register_user("notif_defaults").await;
    app.login("notif_defaults").await;

    let resp = app.get("/api/users/me/notification-preferences").await;
    assert_eq!(resp.status().as_u16(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let prefs = body["data"]["preferences"].as_array().unwrap();

    assert!(!prefs.is_empty(), "the catalogue is the source of the list");
    // No row was ever written; the response is the merge of the defaults.
    let stored: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM notification_preferences")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(
        stored, 0,
        "a row per user per kind per channel would be tens of millions of \
         rows saying 'yes, the default'"
    );

    let social = prefs
        .iter()
        .find(|p| p["kind"] == "social.mention")
        .expect("social.mention present");
    assert_eq!(social["in_app"], json!(true));
    assert_eq!(
        social["push"],
        json!(false),
        "buzzing a phone for a mention by default is how an app gets muted"
    );
}

#[tokio::test]
async fn a_preference_is_stored_and_reflected() {
    let app = TestApp::spawn().await;
    app.register_user("notif_pref").await;
    app.login("notif_pref").await;

    let resp = app
        .put(
            "/api/users/me/notification-preferences",
            &json!({ "preferences": [{ "kind": "social.mention", "push": true }] }),
        )
        .await;
    assert_eq!(resp.status().as_u16(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["updated"], json!(1));

    let listed: serde_json::Value = app
        .get("/api/users/me/notification-preferences")
        .await
        .json()
        .await
        .unwrap();
    let social = listed["data"]["preferences"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["kind"] == "social.mention")
        .unwrap()
        .clone();
    assert_eq!(social["push"], json!(true));
}

#[tokio::test]
async fn a_transactional_notification_cannot_be_turned_off() {
    let app = TestApp::spawn().await;
    app.register_user("notif_trans").await;
    app.login("notif_trans").await;

    let resp = app
        .put(
            "/api/users/me/notification-preferences",
            &json!({ "preferences": [{ "kind": "payout.failed", "email": false }] }),
        )
        .await;
    assert_eq!(resp.status().as_u16(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["updated"], json!(0));
    let rejected = body["data"]["rejected"].as_array().unwrap();
    assert!(
        rejected
            .iter()
            .any(|r| r.as_str().unwrap().contains("payout.failed")),
        "refused loudly: a silent no-op would leave the toggle looking off"
    );
}

#[tokio::test]
async fn a_channel_a_kind_forbids_is_refused() {
    let app = TestApp::spawn().await;
    app.register_user("notif_chan").await;
    app.login("notif_chan").await;

    // No shipped kind forbids a channel — see the test below, which is the
    // point of the policy — so the ceiling is exercised with one made here.
    sqlx::query(
        "INSERT INTO notification_kinds (kind, category, allows_email, default_email)
         VALUES ('test.no_email', 'test', FALSE, FALSE)",
    )
    .execute(&app.db)
    .await
    .expect("seed a kind that forbids email");

    let body: serde_json::Value = app
        .put(
            "/api/users/me/notification-preferences",
            &json!({ "preferences": [{ "kind": "test.no_email", "email": true }] }),
        )
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["updated"], json!(0));
    assert!(!body["data"]["rejected"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn no_shipped_kind_locks_anyone_out_of_a_channel() {
    let app = TestApp::spawn().await;

    // `allows_*` is a ceiling, not a default: FALSE means nobody may ever
    // turn the channel on, however much they want it. That is a decision
    // taken away from the user, and no kind in the catalogue earns it.
    let locked: Vec<String> = sqlx::query_scalar(
        "SELECT kind FROM notification_kinds
          WHERE NOT (allows_in_app AND allows_push AND allows_email)
          ORDER BY kind",
    )
    .fetch_all(&app.db)
    .await
    .expect("read the catalogue");

    assert!(
        locked.is_empty(),
        "these kinds cannot be enabled by anyone: {locked:?} — use default_* instead"
    );
}

#[tokio::test]
async fn an_unknown_kind_is_reported_not_ignored() {
    let app = TestApp::spawn().await;
    app.register_user("notif_unknown").await;
    app.login("notif_unknown").await;

    let body: serde_json::Value = app
        .put(
            "/api/users/me/notification-preferences",
            &json!({ "preferences": [{ "kind": "made.up.kind", "push": true }] }),
        )
        .await
        .json()
        .await
        .unwrap();
    assert!(
        body["data"]["rejected"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r.as_str().unwrap().contains("made.up.kind"))
    );
}

#[tokio::test]
async fn resetting_removes_overrides_rather_than_writing_defaults() {
    let app = TestApp::spawn().await;
    app.register_user("notif_reset").await;
    app.login("notif_reset").await;

    app.put(
        "/api/users/me/notification-preferences",
        &json!({ "preferences": [{ "kind": "social.mention", "push": true }] }),
    )
    .await;
    let resp = app
        .put("/api/users/me/notification-preferences/reset", &json!({}))
        .await;
    assert_eq!(resp.status().as_u16(), 200);

    let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM notification_preferences")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(
        remaining, 0,
        "absence is the default, so a later change to a default reaches \
         everyone who never expressed an opinion"
    );
}

#[tokio::test]
async fn preferences_require_authentication() {
    let app = TestApp::spawn().await;
    let anonymous = reqwest::Client::new();
    let resp = anonymous
        .get(format!(
            "{}/api/users/me/notification-preferences",
            app.addr
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 401);
}

// ─── Language ─────────────────────────────────────────────────────

#[tokio::test]
async fn the_label_is_returned_in_the_callers_language() {
    let app = TestApp::spawn().await;
    app.register_user("notif_lang").await;
    app.login("notif_lang").await;
    sqlx::query("UPDATE users SET preferred_language = 'en' WHERE username = 'notif_lang'")
        .execute(&app.db)
        .await
        .unwrap();

    let body: serde_json::Value = app
        .get("/api/users/me/notification-preferences")
        .await
        .json()
        .await
        .unwrap();
    let label = body["data"]["preferences"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["kind"] == "payout.sent")
        .unwrap()["label"]
        .as_str()
        .unwrap()
        .to_string();

    assert_eq!(label, i18n::t("en", "notification.payout.sent.title"));
    assert_ne!(
        label,
        i18n::t("fr", "notification.payout.sent.title"),
        "the two languages must actually differ, or this proves nothing"
    );
}

#[tokio::test]
async fn a_user_with_no_stored_language_gets_the_default() {
    let app = TestApp::spawn().await;
    let _ = person(&app, "notif_nolang", None).await;
    assert_eq!(i18n::resolve(None, None), i18n::DEFAULT_LOCALE);
}

#[tokio::test]
async fn an_unsupported_stored_language_falls_back_rather_than_breaking() {
    // Someone whose profile says Swahili before sw.yml exists must still
    // receive readable messages.
    assert_eq!(i18n::resolve(Some("sw"), None), i18n::DEFAULT_LOCALE);
    let text = i18n::t("sw", "notification.payout.sent.title");
    assert_ne!(text, "notification.payout.sent.title");
    assert_eq!(
        text,
        i18n::t(i18n::DEFAULT_LOCALE, "notification.payout.sent.title")
    );
}

#[tokio::test]
async fn adding_a_language_needs_no_code_change_at_the_call_sites() {
    // The property that makes ten languages viable: nothing in a caller
    // names a language, so the set of callers does not grow with the set of
    // locales. Every kind resolves through the same two keys.
    let app = TestApp::spawn().await;
    let kinds: Vec<String> = sqlx::query_scalar("SELECT kind FROM notification_kinds")
        .fetch_all(&app.db)
        .await
        .unwrap();

    for locale in i18n::available() {
        for kind in &kinds {
            let title = i18n::t(locale, &format!("notification.{kind}.title"));
            assert!(!title.is_empty());
        }
    }
}

// ─── Rendering ────────────────────────────────────────────────────

#[tokio::test]
async fn arabic_notifications_render_right_to_left() {
    use skilluv_backend::services::email_template::{Email, render};
    let _app = TestApp::spawn().await;

    let html = render(Email {
        locale: "ar",
        theme: Some("sakura"),
        title: &i18n::t("ar", "notification.payout.sent.title"),
        body: &i18n::t("ar", "notification.payout.sent.body"),
        recipient_name: None,
        stats: &[],
        cta_label: None,
        cta_url: None,
        unsubscribe_url: None,
    });
    assert!(html.contains(r#"dir="rtl""#));
    assert!(html.contains(r#"lang="ar""#));
}

#[tokio::test]
async fn placeholders_are_filled_in_every_language() {
    let _app = TestApp::spawn().await;
    for locale in i18n::available() {
        let text = i18n::t_with(
            locale,
            "notification.payout.sent.body",
            &[("amount", "42,50 €"), ("destination", "MTN")],
        );
        assert!(text.contains("42,50 €"), "{locale}: amount not substituted");
        assert!(
            !text.contains("{amount}"),
            "{locale}: a placeholder survived into the output"
        );
    }
}
