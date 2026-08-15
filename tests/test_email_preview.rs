//! Looking at an email must never send one, and must never be public.

mod common;

use common::TestApp;

#[tokio::test]
async fn the_preview_is_admin_only() {
    let app = TestApp::spawn().await;
    app.register_user("prev_nobody").await;
    app.login("prev_nobody").await;

    // It renders every message the platform sends, including the ones that
    // name amounts and providers. Not a page for a logged-in stranger.
    let resp = app.get("/api/admin/email-preview?kind=payout.sent").await;
    assert_eq!(resp.status().as_u16(), 403);

    let index = app.get("/api/admin/email-preview/index").await;
    assert_eq!(index.status().as_u16(), 403);
}

#[tokio::test]
async fn previewing_sends_nothing() {
    let app = TestApp::spawn().await;
    app.register_admin("prev_admin").await;
    app.login("prev_admin").await;

    let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM email_log")
        .fetch_one(&app.db)
        .await
        .unwrap();

    let resp = app
        .get("/api/admin/email-preview?kind=payout.sent&locale=fr&theme=vesperal")
        .await;
    assert_eq!(resp.status().as_u16(), 200);
    let html = resp.text().await.unwrap();
    assert!(html.contains("<!doctype html>"), "it renders an email");

    let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM email_log")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(before, after, "a preview is not a send");

    let notifications: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM notifications")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(notifications, 0, "nor is it a notification");
}

#[tokio::test]
async fn no_placeholder_survives_a_preview() {
    let app = TestApp::spawn().await;
    app.register_admin("prev_holes").await;
    app.login("prev_holes").await;

    // The point of the endpoint: an unfilled `{title}` is invisible until
    // someone looks, and `deliverable.first_verified` shipped with one for
    // a column `deliverables` does not have.
    let kinds: Vec<String> =
        sqlx::query_scalar("SELECT kind FROM notification_kinds ORDER BY kind")
            .fetch_all(&app.db)
            .await
            .unwrap();

    let mut broken = Vec::new();
    for kind in &kinds {
        for locale in ["fr", "en", "ar"] {
            let html = app
                .get(&format!(
                    "/api/admin/email-preview?kind={kind}&locale={locale}"
                ))
                .await
                .text()
                .await
                .unwrap();
            // Only the body carries interpolation; the frame has none.
            if html.contains('{') && html.contains('}') {
                let opening = html.find('{').unwrap();
                let snippet: String = html[opening..].chars().take(40).collect();
                broken.push(format!("{locale} {kind}: {snippet}"));
            }
        }
    }
    assert!(broken.is_empty(), "unfilled placeholders: {broken:#?}");
}

#[tokio::test]
async fn an_unknown_kind_says_so_instead_of_rendering_its_own_name() {
    let app = TestApp::spawn().await;
    app.register_admin("prev_typo").await;
    app.login("prev_typo").await;

    // Without the check, a typo renders an email whose subject is
    // `notification.payout.snet.title` and reads as a template bug.
    let resp = app.get("/api/admin/email-preview?kind=payout.snet").await;
    assert_eq!(resp.status().as_u16(), 400);
}

#[tokio::test]
async fn the_index_reports_missing_translations() {
    let app = TestApp::spawn().await;
    app.register_admin("prev_index").await;
    app.login("prev_index").await;

    let body: serde_json::Value = app
        .get("/api/admin/email-preview/index")
        .await
        .json()
        .await
        .unwrap();

    let kinds = body["data"]["kinds"].as_array().unwrap();
    assert!(!kinds.is_empty());
    assert_eq!(body["data"]["themes"].as_array().unwrap().len(), 5);

    // Everything shipped is translated. The field exists so an operator
    // sees a gap the day one appears, rather than a subject line reading as
    // its own key in production.
    let gaps: Vec<&serde_json::Value> = kinds
        .iter()
        .filter(|k| !k["untranslated"].as_array().unwrap().is_empty())
        .collect();
    assert!(gaps.is_empty(), "untranslated kinds: {gaps:#?}");
}

#[tokio::test]
async fn a_receipt_offers_no_way_to_opt_out_of_receipts() {
    let app = TestApp::spawn().await;
    app.register_admin("prev_trans").await;
    app.login("prev_trans").await;

    let transactional = app
        .get("/api/admin/email-preview?kind=payout.failed&locale=fr")
        .await
        .text()
        .await
        .unwrap();
    assert!(
        !transactional.contains("unsubscribe"),
        "a payout failure is an obligation, not marketing"
    );

    let declinable = app
        .get("/api/admin/email-preview?kind=digest.weekly&locale=fr")
        .await
        .text()
        .await
        .unwrap();
    assert!(
        declinable.contains("unsubscribe"),
        "a digest without a way out is not acceptable, and for a bulk sender not legal"
    );
}
