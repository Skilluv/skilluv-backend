//! Entitlements, trial periods, and reverse recruitment.

mod common;
use common::TestApp;
use serde_json::{Value, json};
use uuid::Uuid;

async fn an_admin(app: &TestApp, username: &str) {
    app.register_user(username).await;
    sqlx::query("UPDATE users SET role = 'admin' WHERE username = $1")
        .bind(username)
        .execute(&app.db)
        .await
        .unwrap();
    app.login(username).await;
}

async fn an_enterprise(app: &TestApp, company: &str) -> (String, Uuid) {
    app.register_enterprise(company).await;
    let username = company.to_lowercase().replace(' ', "");
    app.login(&username).await;
    app.enable_totp_for(&username).await;
    let id: Uuid = sqlx::query_scalar(
        "SELECT e.id FROM enterprises e JOIN users u ON u.id = e.owner_id
          WHERE u.username = $1",
    )
    .bind(&username)
    .fetch_one(&app.db)
    .await
    .unwrap();
    (username, id)
}

async fn a_talent(app: &TestApp, username: &str, rank: &str) -> Uuid {
    app.register_user(username).await;
    let id: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(username)
        .fetch_one(&app.db)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO user_ranks (user_id, rank) VALUES ($1, $2)
         ON CONFLICT (user_id) DO UPDATE SET rank = EXCLUDED.rank",
    )
    .bind(id)
    .bind(rank)
    .execute(&app.db)
    .await
    .unwrap();
    id
}

/// An engagement to hang entitlements off.
async fn an_engagement(app: &TestApp, enterprise: Uuid, product: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO enterprise_products (enterprise_id, product_type, renews_at)
         VALUES ($1, $2, NOW() + INTERVAL '1 year') RETURNING id",
    )
    .bind(enterprise)
    .bind(product)
    .fetch_one(&app.db)
    .await
    .unwrap()
}

// ═══════════════════════════════════════════════════════════════════
// Entitlements
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn a_quota_has_a_remainder_and_a_ceiling_does_not() {
    let app = TestApp::spawn().await;
    let (owner, enterprise) = an_enterprise(&app, "Quotaco").await;
    let product = an_engagement(&app, enterprise, "enterprise_program_annual").await;
    an_admin(&app, "quota_admin").await;

    for (kind, granted) in [("credits", 200), ("open_positions", 10)] {
        let resp = app
            .post(
                &format!("/api/admin/enterprise-products/{product}/entitlements"),
                &json!({"kind": kind, "granted": granted}),
            )
            .await;
        assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    }

    app.relogin_with_totp(&owner).await;
    let body: Value = app
        .get("/api/enterprise/entitlements")
        .await
        .json()
        .await
        .unwrap();
    let items = body["data"]["entitlements"].as_array().unwrap();

    let credits = items.iter().find(|e| e["kind"] == "credits").unwrap();
    let positions = items
        .iter()
        .find(|e| e["kind"] == "open_positions")
        .unwrap();

    // A quota at zero means spent; a ceiling at zero means none allowed. A
    // dashboard that shows them the same way lies about one of them.
    assert!(!credits["remaining"].is_null());
    assert!(positions["remaining"].is_null());
    assert!(positions["consumed"].is_null());
}

#[tokio::test]
async fn a_flag_carries_no_amount_and_a_quota_must() {
    let app = TestApp::spawn().await;
    let (_, enterprise) = an_enterprise(&app, "Flagco").await;
    let product = an_engagement(&app, enterprise, "enterprise_program_annual").await;
    an_admin(&app, "flag_admin").await;

    let flag_with_number = app
        .post(
            &format!("/api/admin/enterprise-products/{product}/entitlements"),
            &json!({"kind": "priority_talent_access", "granted": 5}),
        )
        .await;
    assert_eq!(flag_with_number.status(), 400);

    // A quota with no figure is an unlimited quota by accident.
    let quota_without_number = app
        .post(
            &format!("/api/admin/enterprise-products/{product}/entitlements"),
            &json!({"kind": "credits"}),
        )
        .await;
    assert_eq!(quota_without_number.status(), 400);

    let flag = app
        .post(
            &format!("/api/admin/enterprise-products/{product}/entitlements"),
            &json!({"kind": "priority_talent_access"}),
        )
        .await;
    assert_eq!(flag.status(), 200, "{}", flag.text().await.unwrap());
}

#[tokio::test]
async fn spending_draws_down_the_oldest_engagement_first() {
    let app = TestApp::spawn().await;
    let (_, enterprise) = an_enterprise(&app, "Spendco").await;
    an_admin(&app, "spend_admin").await;

    // An old subscription and a newer pack, both granting credits.
    let old = an_engagement(&app, enterprise, "enterprise_program_annual").await;
    sqlx::query(
        "UPDATE enterprise_products SET started_at = NOW() - INTERVAL '6 months' WHERE id = $1",
    )
    .bind(old)
    .execute(&app.db)
    .await
    .unwrap();
    let new = an_engagement(&app, enterprise, "subscription_pipeline").await;

    for product in [old, new] {
        app.post(
            &format!("/api/admin/enterprise-products/{product}/entitlements"),
            &json!({"kind": "credits", "granted": 100}),
        )
        .await;
    }

    let left = skilluv_backend::services::entitlements::consume(
        &app.db,
        enterprise,
        "credits",
        bigdecimal::BigDecimal::from(150),
    )
    .await
    .unwrap();
    assert_eq!(
        left,
        bigdecimal::BigDecimal::from(0),
        "150 of 200 is covered"
    );

    // Oldest first, so what was about to lapse is used before what was not.
    let old_consumed: f64 = sqlx::query_scalar(
        "SELECT consumed::FLOAT8 FROM enterprise_entitlements
          WHERE product_id = $1 AND kind = 'credits'",
    )
    .bind(old)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(old_consumed, 100.0);
}

#[tokio::test]
async fn spending_more_than_granted_reports_the_shortfall() {
    let app = TestApp::spawn().await;
    let (_, enterprise) = an_enterprise(&app, "Shortco").await;
    an_admin(&app, "short_admin").await;
    let product = an_engagement(&app, enterprise, "credits_pack").await;
    app.post(
        &format!("/api/admin/enterprise-products/{product}/entitlements"),
        &json!({"kind": "credits", "granted": 50}),
    )
    .await;

    // Not an error: what to do about the shortfall is the caller's decision.
    let left = skilluv_backend::services::entitlements::consume(
        &app.db,
        enterprise,
        "credits",
        bigdecimal::BigDecimal::from(80),
    )
    .await
    .unwrap();
    assert_eq!(left, bigdecimal::BigDecimal::from(30));
}

#[tokio::test]
async fn a_lapsed_engagement_stops_granting() {
    let app = TestApp::spawn().await;
    let (_, enterprise) = an_enterprise(&app, "Lapsedco").await;
    an_admin(&app, "lapsed_admin").await;
    let product = an_engagement(&app, enterprise, "subscription_pipeline").await;
    app.post(
        &format!("/api/admin/enterprise-products/{product}/entitlements"),
        &json!({"kind": "credits", "granted": 100}),
    )
    .await;

    sqlx::query("UPDATE enterprise_products SET status = 'lapsed', ended_at = NOW() WHERE id = $1")
        .bind(product)
        .execute(&app.db)
        .await
        .unwrap();

    // Showing it as available would let somebody spend what they no longer
    // have.
    let left = skilluv_backend::services::entitlements::remaining(&app.db, enterprise, "credits")
        .await
        .unwrap();
    assert_eq!(left, Some(bigdecimal::BigDecimal::from(0)));
}

// ═══════════════════════════════════════════════════════════════════
// Trials
// ═══════════════════════════════════════════════════════════════════

async fn a_trial(app: &TestApp, company: &str, talent_name: &str) -> (String, Uuid, Uuid) {
    let (owner, _) = an_enterprise(app, company).await;
    let talent = a_talent(app, talent_name, "artisan").await;

    let resp = app
        .post(
            "/api/enterprise/trials",
            &json!({
                "talent_user_id": talent,
                "duration_weeks": 3,
                "hourly_rate": "80.00",
                "currency": "EUR",
            }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    let trial: Uuid = body["data"]["trial"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    (owner, talent, trial)
}

#[tokio::test]
async fn an_unpaid_trial_is_refused() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Unpaidco").await;
    let talent = a_talent(&app, "unpaid_talent", "artisan").await;

    // A trial of unpaid exercises is an interview with extra steps.
    let resp = app
        .post(
            "/api/enterprise/trials",
            &json!({"talent_user_id": talent, "duration_weeks": 2, "hourly_rate": "0"}),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn a_trial_longer_than_eight_weeks_is_a_job() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Longco").await;
    let talent = a_talent(&app, "long_talent", "artisan").await;

    let resp = app
        .post(
            "/api/enterprise/trials",
            &json!({"talent_user_id": talent, "duration_weeks": 12, "hourly_rate": "50"}),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn the_maximum_cost_is_shown_before_anybody_starts() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Exposureco").await;
    let talent = a_talent(&app, "exposure_talent", "artisan").await;

    let body: Value = app
        .post(
            "/api/enterprise/trials",
            &json!({"talent_user_id": talent, "duration_weeks": 4, "hourly_rate": "80.00"}),
        )
        .await
        .json()
        .await
        .unwrap();

    // Four weeks at 80, thirty-five hours a week. Seen now rather than on the
    // first invoice.
    assert_eq!(body["data"]["maximum_cost"], "11200.00");
}

#[tokio::test]
async fn hours_outside_the_window_are_refused() {
    let app = TestApp::spawn().await;
    let (_, _, trial) = a_trial(&app, "Windowco", "window_talent").await;

    app.login("window_talent").await;
    let before = app
        .post(
            &format!("/api/trials/{trial}/hours"),
            &json!({
                "worked_on": "2020-01-01",
                "hours": "7.5",
                "summary": "du travail",
            }),
        )
        .await;
    assert_eq!(before.status(), 400);

    let today = chrono::Utc::now().date_naive().to_string();
    let inside = app
        .post(
            &format!("/api/trials/{trial}/hours"),
            &json!({"worked_on": today, "hours": "7.5", "summary": "reprise du module de facturation"}),
        )
        .await;
    assert_eq!(inside.status(), 200, "{}", inside.text().await.unwrap());
}

#[tokio::test]
async fn refusing_somebody_hours_requires_a_reason() {
    let app = TestApp::spawn().await;
    let (owner, _, trial) = a_trial(&app, "Refuseco", "refuse_talent").await;

    app.login("refuse_talent").await;
    let today = chrono::Utc::now().date_naive().to_string();
    app.post(
        &format!("/api/trials/{trial}/hours"),
        &json!({"worked_on": today, "hours": "8", "summary": "du travail fait"}),
    )
    .await;

    let entry: Uuid =
        sqlx::query_scalar("SELECT id FROM recruitment_trial_hours WHERE trial_id = $1")
            .bind(trial)
            .fetch_one(&app.db)
            .await
            .unwrap();

    app.relogin_with_totp(&owner).await;
    // Refusing somebody's hours without saying why is refusing their wages
    // without saying why.
    let silent = app
        .post(
            &format!("/api/trials/hours/{entry}/decision"),
            &json!({"approve": false}),
        )
        .await;
    assert_eq!(silent.status(), 400);

    let spoken = app
        .post(
            &format!("/api/trials/hours/{entry}/decision"),
            &json!({"approve": false, "reason": "cette journée était un jour férié"}),
        )
        .await;
    assert_eq!(spoken.status(), 200);
}

#[tokio::test]
async fn only_approved_hours_are_settled() {
    let app = TestApp::spawn().await;
    let (owner, _, trial) = a_trial(&app, "Settleco", "settle_talent").await;

    app.login("settle_talent").await;
    let today = chrono::Utc::now().date_naive();
    for (offset, hours) in [(0, "8"), (1, "6")] {
        app.post(
            &format!("/api/trials/{trial}/hours"),
            &json!({
                "worked_on": (today + chrono::Duration::days(offset)).to_string(),
                "hours": hours,
                "summary": "du travail",
            }),
        )
        .await;
    }

    // Approve only the first.
    let entry: Uuid = sqlx::query_scalar(
        "SELECT id FROM recruitment_trial_hours WHERE trial_id = $1 ORDER BY worked_on LIMIT 1",
    )
    .bind(trial)
    .fetch_one(&app.db)
    .await
    .unwrap();

    app.relogin_with_totp(&owner).await;
    app.post(
        &format!("/api/trials/hours/{entry}/decision"),
        &json!({"approve": true}),
    )
    .await;

    let body: Value = app
        .post(
            &format!("/api/enterprise/trials/{trial}/conclude"),
            &json!({"outcome": "converted_hire"}),
        )
        .await
        .json()
        .await
        .unwrap();

    // 8 h at 80 = 640 gross. 15% platform = 96, talent 544. The unapproved
    // six hours are not money owed.
    assert_eq!(body["data"]["talent_owed"], "544.00");
    assert_eq!(body["data"]["platform_share"], "96.00");
}

#[tokio::test]
async fn correcting_an_entry_withdraws_its_approval() {
    let app = TestApp::spawn().await;
    let (owner, _, trial) = a_trial(&app, "Correctco", "correct_talent").await;
    let today = chrono::Utc::now().date_naive().to_string();

    app.login("correct_talent").await;
    app.post(
        &format!("/api/trials/{trial}/hours"),
        &json!({"worked_on": today, "hours": "8", "summary": "premier chiffre"}),
    )
    .await;

    let entry: Uuid =
        sqlx::query_scalar("SELECT id FROM recruitment_trial_hours WHERE trial_id = $1")
            .bind(trial)
            .fetch_one(&app.db)
            .await
            .unwrap();

    app.relogin_with_totp(&owner).await;
    app.post(
        &format!("/api/trials/hours/{entry}/decision"),
        &json!({"approve": true}),
    )
    .await;

    // The talent corrects it. An approval belongs to the figure it approved.
    app.login("correct_talent").await;
    app.post(
        &format!("/api/trials/{trial}/hours"),
        &json!({"worked_on": today, "hours": "10", "summary": "chiffre corrigé"}),
    )
    .await;

    let approved: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT approved_at FROM recruitment_trial_hours WHERE id = $1")
            .bind(entry)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(approved.is_none());
}

#[tokio::test]
async fn two_running_trials_with_the_same_company_are_refused() {
    let app = TestApp::spawn().await;
    let (_, talent, _) = a_trial(&app, "Doubleco", "double_talent").await;

    let again = app
        .post(
            "/api/enterprise/trials",
            &json!({"talent_user_id": talent, "duration_weeks": 2, "hourly_rate": "80"}),
        )
        .await;
    assert_eq!(again.status(), 400);
}

// ═══════════════════════════════════════════════════════════════════
// Reverse recruitment
// ═══════════════════════════════════════════════════════════════════

fn a_posting() -> Value {
    json!({
        "title": "Backend Rust, disponible en janvier",
        "desired_role": "Développeur backend",
        "desired_domain": "code",
        "desired_orientations": ["web-backend-developer"],
        "remote_only": true,
        "available_from": "2027-01-15",
    })
}

fn a_pitch() -> Value {
    json!({
        "pitch_md": "Nous reprenons un service de facturation écrit en Rust il y a trois ans, \
                     et nous cherchons quelqu'un pour en être le référent plutôt que pour \
                     exécuter des tickets. L'équipe fait quatre personnes, les revues sont \
                     lentes et argumentées, et le code part en open source dans six mois.",
        "offered_salary": "45000.00",
        "currency": "EUR",
    })
}

#[tokio::test]
async fn posting_needs_the_rank_that_says_the_work_exists() {
    let app = TestApp::spawn().await;
    a_talent(&app, "reverse_beginner", "apprenti").await;
    app.login("reverse_beginner").await;

    // The argument for companies coming to you is that your work speaks for
    // itself, which needs some of it to exist.
    let refused = app
        .post("/api/reverse-recruitment/postings", &a_posting())
        .await;
    assert_eq!(refused.status(), 400);

    a_talent(&app, "reverse_artisan", "artisan").await;
    app.login("reverse_artisan").await;
    let accepted = app
        .post("/api/reverse-recruitment/postings", &a_posting())
        .await;
    assert_eq!(accepted.status(), 200, "{}", accepted.text().await.unwrap());
}

#[tokio::test]
async fn a_two_line_pitch_is_refused() {
    let app = TestApp::spawn().await;
    a_talent(&app, "pitch_target", "artisan").await;
    app.login("pitch_target").await;
    let body: Value = app
        .post("/api/reverse-recruitment/postings", &a_posting())
        .await
        .json()
        .await
        .unwrap();
    let posting = body["data"]["posting"]["id"].as_str().unwrap().to_string();

    an_enterprise(&app, "Lazyco").await;
    // The premise is that the company does the persuading.
    let short = app
        .post(
            &format!("/api/reverse-recruitment/postings/{posting}/pitch"),
            &json!({"pitch_md": "Bonjour, on recrute."}),
        )
        .await;
    assert_eq!(short.status(), 400);

    let real = app
        .post(
            &format!("/api/reverse-recruitment/postings/{posting}/pitch"),
            &a_pitch(),
        )
        .await;
    assert_eq!(real.status(), 200, "{}", real.text().await.unwrap());
}

#[tokio::test]
async fn the_monthly_ceiling_holds() {
    let app = TestApp::spawn().await;
    a_talent(&app, "ceiling_target", "artisan").await;
    app.login("ceiling_target").await;

    let mut posting_body = a_posting();
    posting_body["max_pitches_per_month"] = json!(1);
    let body: Value = app
        .post("/api/reverse-recruitment/postings", &posting_body)
        .await
        .json()
        .await
        .unwrap();
    let posting = body["data"]["posting"]["id"].as_str().unwrap().to_string();

    an_enterprise(&app, "Firstco").await;
    let first = app
        .post(
            &format!("/api/reverse-recruitment/postings/{posting}/pitch"),
            &a_pitch(),
        )
        .await;
    assert_eq!(first.status(), 200);

    // The ceiling is what keeps the inbox readable; without it the feature
    // dies of its own success.
    an_enterprise(&app, "Secondco").await;
    let second = app
        .post(
            &format!("/api/reverse-recruitment/postings/{posting}/pitch"),
            &a_pitch(),
        )
        .await;
    assert_eq!(second.status(), 400);

    // And it disappears from the browse list rather than inviting four
    // hundred words the database will refuse.
    let browsed: Value = app
        .get("/api/reverse-recruitment/postings")
        .await
        .json()
        .await
        .unwrap();
    assert!(browsed["data"]["postings"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn a_company_pitches_once_per_posting() {
    let app = TestApp::spawn().await;
    a_talent(&app, "once_target", "artisan").await;
    app.login("once_target").await;
    let body: Value = app
        .post("/api/reverse-recruitment/postings", &a_posting())
        .await
        .json()
        .await
        .unwrap();
    let posting = body["data"]["posting"]["id"].as_str().unwrap().to_string();

    an_enterprise(&app, "Twiceco").await;
    app.post(
        &format!("/api/reverse-recruitment/postings/{posting}/pitch"),
        &a_pitch(),
    )
    .await;

    // A second one is a follow-up, which belongs in a conversation.
    let again = app
        .post(
            &format!("/api/reverse-recruitment/postings/{posting}/pitch"),
            &a_pitch(),
        )
        .await;
    assert_eq!(again.status(), 400);
}

#[tokio::test]
async fn the_talent_answers_and_the_company_is_told() {
    let app = TestApp::spawn().await;
    a_talent(&app, "answer_target", "artisan").await;
    app.login("answer_target").await;
    let body: Value = app
        .post("/api/reverse-recruitment/postings", &a_posting())
        .await
        .json()
        .await
        .unwrap();
    let posting = body["data"]["posting"]["id"].as_str().unwrap().to_string();

    an_enterprise(&app, "Answerco").await;
    app.post(
        &format!("/api/reverse-recruitment/postings/{posting}/pitch"),
        &a_pitch(),
    )
    .await;

    app.login("answer_target").await;
    let inbox: Value = app.get("/api/users/me/pitches").await.json().await.unwrap();
    let pitches = inbox["data"]["pitches"].as_array().unwrap();
    assert_eq!(pitches.len(), 1);
    assert_eq!(pitches[0]["company_name"], "Answerco");
    let pitch_id = pitches[0]["id"].as_str().unwrap().to_string();

    let answered = app
        .post(
            &format!("/api/pitches/{pitch_id}/respond"),
            &json!({"interested": true}),
        )
        .await;
    assert_eq!(answered.status(), 200);

    let status: String =
        sqlx::query_scalar("SELECT status FROM reverse_recruitment_pitches WHERE id = $1::UUID")
            .bind(&pitch_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(status, "interested");
}

#[tokio::test]
async fn the_postings_are_not_public() {
    let app = TestApp::spawn().await;
    a_talent(&app, "private_target", "artisan").await;
    app.login("private_target").await;
    app.post("/api/reverse-recruitment/postings", &a_posting())
        .await;

    // A public listing of who is looking for work is a listing that reaches
    // their current employer.
    app.register_user("reverse_nosy").await;
    app.login("reverse_nosy").await;
    assert_eq!(
        app.get("/api/reverse-recruitment/postings").await.status(),
        403
    );
}
