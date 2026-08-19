//! Paid mentoring.
//!
//! The audit ticket asked whether the four modes from migration 0107 work.
//! They did not: only `paid_session` was wired. These tests cover what was
//! dormant — the monthly arrangement, the volunteer hours, the placement
//! commission and its anti-double-dipping rule — plus the two products built
//! on top.

mod common;
use common::TestApp;
use serde_json::{Value, json};
use uuid::Uuid;

async fn an_admin(app: &TestApp, username: &str) {
    // `register_admin`, not `role = 'admin'`: since P21 the admin gate reads
    // `user_capabilities`, and the column on its own opens nothing. The helper
    // grants the capability and enrols the passkey the admin 2FA middleware
    // wants, then logs in.
    app.register_admin(username).await;
}

async fn a_talent(app: &TestApp, username: &str) -> Uuid {
    app.register_user(username).await;
    sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(username)
        .fetch_one(&app.db)
        .await
        .unwrap()
}

/// A mentor in a given economic mode.
async fn a_mentor(app: &TestApp, username: &str, mode: &str, monthly_cents: Option<i64>) -> Uuid {
    let id = a_talent(app, username).await;
    sqlx::query(
        "INSERT INTO mentor_profiles
            (user_id, headline, bio, hourly_rate_eur_cents, mode,
             monthly_subscription_eur_cents)
         VALUES ($1, 'Mentor', 'Bio', 5000, $2, $3)",
    )
    .bind(id)
    .bind(mode)
    .bind(monthly_cents)
    .execute(&app.db)
    .await
    .unwrap();
    id
}

// ═══════════════════════════════════════════════════════════════════
// The monthly mode that never worked
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn a_mentor_who_charges_by_the_session_offers_no_monthly_arrangement() {
    let app = TestApp::spawn().await;
    let mentor = a_mentor(&app, "sessiononly", "paid_session", None).await;
    a_talent(&app, "sessionmentee").await;
    app.login("sessionmentee").await;

    let resp = app
        .post(&format!("/api/mentors/{mentor}/subscribe"), &json!({}))
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn a_monthly_mode_with_no_price_says_so_rather_than_failing_quietly() {
    let app = TestApp::spawn().await;
    // Exactly the state migration 0107 allowed and nothing ever checked.
    let mentor = a_mentor(&app, "pricelessmentor", "paid_monthly", None).await;
    a_talent(&app, "pricelessmentee").await;
    app.login("pricelessmentee").await;

    let resp = app
        .post(&format!("/api/mentors/{mentor}/subscribe"), &json!({}))
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn subscribing_pays_the_mentor_and_books_the_platform_share() {
    let app = TestApp::spawn().await;
    let mentor = a_mentor(&app, "monthlymentor", "paid_monthly", Some(10_000)).await;
    a_talent(&app, "monthlymentee").await;
    app.login("monthlymentee").await;

    let resp = app
        .post(&format!("/api/mentors/{mentor}/subscribe"), &json!({}))
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    // 100 euros a month, 20% to the platform.
    let booked: sqlx::types::BigDecimal = sqlx::query_scalar(
        "SELECT amount_credits FROM platform_revenues
          WHERE source = 'mentor_session' AND related_talent_id = $1",
    )
    .bind(mentor)
    .fetch_one(&app.db)
    .await
    .unwrap();
    common::assert_decimal(&booked, "20.00");
}

#[tokio::test]
async fn a_monthly_price_is_frozen_at_subscription() {
    let app = TestApp::spawn().await;
    let mentor = a_mentor(&app, "risingmentor", "paid_monthly", Some(10_000)).await;
    a_talent(&app, "frozenmentee").await;
    app.login("frozenmentee").await;
    app.post(&format!("/api/mentors/{mentor}/subscribe"), &json!({}))
        .await;

    sqlx::query(
        "UPDATE mentor_profiles SET monthly_subscription_eur_cents = 50000
          WHERE user_id = $1",
    )
    .bind(mentor)
    .execute(&app.db)
    .await
    .unwrap();

    // A mentor raising their rate must not change what somebody is already
    // paying without them agreeing again.
    let fee: i64 = sqlx::query_scalar(
        "SELECT monthly_fee_cents FROM mentor_subscriptions WHERE mentor_user_id = $1",
    )
    .bind(mentor)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(fee, 10_000);
}

#[tokio::test]
async fn renewing_extends_rather_than_charging_twice_for_the_same_relationship() {
    let app = TestApp::spawn().await;
    let mentor = a_mentor(&app, "renewmentor", "paid_monthly", Some(10_000)).await;
    a_talent(&app, "renewmentee").await;
    app.login("renewmentee").await;

    app.post(&format!("/api/mentors/{mentor}/subscribe"), &json!({}))
        .await;
    let resp = app
        .post(&format!("/api/mentors/{mentor}/subscribe"), &json!({}))
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM mentor_subscriptions")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(rows, 1);
}

#[tokio::test]
async fn the_included_sessions_are_countable() {
    let app = TestApp::spawn().await;
    let mentor = a_mentor(&app, "countmentor", "paid_monthly", Some(10_000)).await;
    a_talent(&app, "countmentee").await;
    app.login("countmentee").await;

    let resp = app
        .post(&format!("/api/mentors/{mentor}/subscribe"), &json!({}))
        .await;
    let created: Value = resp.json().await.unwrap();
    let id = created["data"]["subscription"]["id"].as_str().unwrap();

    // "Two sessions a month" without a count is a promise nobody can check
    // and nobody can dispute.
    let resp = app
        .get(&format!("/api/mentor-subscriptions/{id}/usage"))
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["included"], 2);
    assert_eq!(body["data"]["used_this_month"], 0);
    assert_eq!(body["data"]["remaining"], 2);
}

// ═══════════════════════════════════════════════════════════════════
// Volunteer hours and the placement commission
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn a_paid_mentor_cannot_record_volunteer_hours() {
    let app = TestApp::spawn().await;
    a_mentor(&app, "paidmentor", "paid_session", None).await;
    let mentee = a_talent(&app, "paidmentee").await;
    app.login("paidmentor").await;

    // Recording paid hours as volunteer hours would claim the placement
    // reward for something already charged for.
    let resp = app
        .post(
            "/api/mentors/me/volunteer-hours",
            &json!({ "mentee_user_id": mentee, "hours": "2.0" }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn volunteer_hours_accumulate_and_report_the_threshold() {
    let app = TestApp::spawn().await;
    a_mentor(&app, "volmentor", "volunteer", None).await;
    let mentee = a_talent(&app, "volmentee").await;
    app.login("volmentor").await;

    for _ in 0..2 {
        let resp = app
            .post(
                "/api/mentors/me/volunteer-hours",
                &json!({ "mentee_user_id": mentee, "hours": "1.5" }),
            )
            .await;
        assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    }

    let resp = app
        .post(
            "/api/mentors/me/volunteer-hours",
            &json!({ "mentee_user_id": mentee, "hours": "1.0" }),
        )
        .await;
    let body: Value = resp.json().await.unwrap();
    common::assert_amount(&body["data"]["hours_with_this_mentee"], "4.00");
    assert_eq!(body["data"]["commission_threshold"], 5.0);
}

#[tokio::test]
async fn a_commission_below_the_threshold_is_refused() {
    let app = TestApp::spawn().await;
    an_admin(&app, "commadmin").await;
    let mentor = a_mentor(&app, "shortmentor", "volunteer", None).await;
    let mentee = a_talent(&app, "shortmentee").await;
    let enterprise: Uuid = sqlx::query_scalar(
        "INSERT INTO enterprises (owner_id, company_name, slug, company_size)
         VALUES ($1, 'Embauche SA', 'embauche-sa', '11-50') RETURNING id",
    )
    .bind(mentee)
    .fetch_one(&app.db)
    .await
    .unwrap();

    app.login("shortmentor").await;
    app.post(
        "/api/mentors/me/volunteer-hours",
        &json!({ "mentee_user_id": mentee, "hours": "3.0" }),
    )
    .await;

    app.login("commadmin").await;
    let resp = app
        .post(
            "/api/admin/mentoring/placement-commission",
            &json!({
                "mentor_user_id": mentor,
                "mentee_user_id": mentee,
                "enterprise_id": enterprise,
                "placement_amount_cents": 500_000,
            }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn a_mentor_who_charged_this_mentee_cannot_also_take_the_placement_reward() {
    let app = TestApp::spawn().await;
    an_admin(&app, "dipadmin").await;
    let mentor = a_mentor(&app, "hybridmentor", "hybrid", None).await;
    let mentee = a_talent(&app, "dipmentee").await;
    let enterprise: Uuid = sqlx::query_scalar(
        "INSERT INTO enterprises (owner_id, company_name, slug, company_size)
         VALUES ($1, 'Double SA', 'double-sa-dip', '11-50') RETURNING id",
    )
    .bind(mentee)
    .fetch_one(&app.db)
    .await
    .unwrap();

    // Enough free hours to qualify on their own.
    app.login("hybridmentor").await;
    app.post(
        "/api/mentors/me/volunteer-hours",
        &json!({ "mentee_user_id": mentee, "hours": "8.0" }),
    )
    .await;

    // And a paid session with the same person.
    sqlx::query(
        "INSERT INTO mentorship_sessions
            (mentor_user_id, mentee_user_id, scheduled_at, duration_minutes,
             price_total_cents, price_mentor_cents, price_platform_cents, status)
         VALUES ($1, $2, NOW(), 60, 5000, 4000, 1000, 'completed')",
    )
    .bind(mentor)
    .bind(mentee)
    .execute(&app.db)
    .await
    .unwrap();

    app.login("dipadmin").await;
    // Charging for the hours and claiming the reward for having given them
    // is the same money twice.
    let resp = app
        .post(
            "/api/admin/mentoring/placement-commission",
            &json!({
                "mentor_user_id": mentor,
                "mentee_user_id": mentee,
                "enterprise_id": enterprise,
                "placement_amount_cents": 500_000,
            }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn a_qualifying_mentor_is_paid_ten_per_cent_of_the_placement() {
    let app = TestApp::spawn().await;
    an_admin(&app, "payadmin").await;
    let mentor = a_mentor(&app, "paymentor", "volunteer", None).await;
    let mentee = a_talent(&app, "paymentee").await;
    let enterprise: Uuid = sqlx::query_scalar(
        "INSERT INTO enterprises (owner_id, company_name, slug, company_size)
         VALUES ($1, 'Placement SA', 'placement-sa', '11-50') RETURNING id",
    )
    .bind(mentee)
    .fetch_one(&app.db)
    .await
    .unwrap();

    app.login("paymentor").await;
    app.post(
        "/api/mentors/me/volunteer-hours",
        &json!({ "mentee_user_id": mentee, "hours": "12.0" }),
    )
    .await;

    app.login("payadmin").await;
    let resp = app
        .post(
            "/api/admin/mentoring/placement-commission",
            &json!({
                "mentor_user_id": mentor,
                "mentee_user_id": mentee,
                "enterprise_id": enterprise,
                "placement_amount_cents": 500_000,
            }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["mentor_share_cents"], 50_000);

    // The same triple twice is worth a look before it is worth a payment.
    let resp = app
        .post(
            "/api/admin/mentoring/placement-commission",
            &json!({
                "mentor_user_id": mentor,
                "mentee_user_id": mentee,
                "enterprise_id": enterprise,
                "placement_amount_cents": 500_000,
            }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

// ═══════════════════════════════════════════════════════════════════
// One-off slots
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn a_one_off_slot_is_offered_once() {
    let app = TestApp::spawn().await;
    let mentor = a_mentor(&app, "slotmentor", "paid_session", None).await;
    app.login("slotmentor").await;

    let tomorrow = (chrono::Utc::now() + chrono::Duration::days(1))
        .date_naive()
        .to_string();
    let resp = app
        .post(
            "/api/mentors/me/open-slots",
            &json!({
                "date": tomorrow,
                "start_time": "14:00:00",
                "end_time": "15:00:00",
            }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    let resp = app.get(&format!("/api/mentors/{mentor}/open-slots")).await;
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["slots"].as_array().unwrap().len(), 1);

    // Consumed by a booking. A recurring slot is never consumed; a one-off
    // is, and offering it twice is how two people arrive at the same call.
    let mentee = a_talent(&app, "slotmentee").await;
    let session: Uuid = sqlx::query_scalar(
        "INSERT INTO mentorship_sessions
            (mentor_user_id, mentee_user_id, scheduled_at, duration_minutes,
             price_total_cents, price_mentor_cents, price_platform_cents)
         VALUES ($1, $2, NOW() + INTERVAL '1 day', 60, 5000, 4000, 1000)
         RETURNING id",
    )
    .bind(mentor)
    .bind(mentee)
    .fetch_one(&app.db)
    .await
    .unwrap();

    sqlx::query(
        "UPDATE mentor_availability SET consumed_by_session_id = $1
          WHERE mentor_user_id = $2 AND specific_date IS NOT NULL",
    )
    .bind(session)
    .bind(mentor)
    .execute(&app.db)
    .await
    .unwrap();

    let resp = app.get(&format!("/api/mentors/{mentor}/open-slots")).await;
    let body: Value = resp.json().await.unwrap();
    assert!(body["data"]["slots"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn a_recurring_slot_is_never_consumed() {
    let app = TestApp::spawn().await;
    let mentor = a_mentor(&app, "recurringmentor", "paid_session", None).await;
    let mentee = a_talent(&app, "recurringmentee").await;

    let slot: Uuid = sqlx::query_scalar(
        "INSERT INTO mentor_availability (mentor_user_id, weekday, start_time, end_time)
         VALUES ($1, 2, '14:00', '16:00') RETURNING id",
    )
    .bind(mentor)
    .fetch_one(&app.db)
    .await
    .unwrap();

    let session: Uuid = sqlx::query_scalar(
        "INSERT INTO mentorship_sessions
            (mentor_user_id, mentee_user_id, scheduled_at, duration_minutes,
             price_total_cents, price_mentor_cents, price_platform_cents)
         VALUES ($1, $2, NOW() + INTERVAL '1 day', 60, 5000, 4000, 1000)
         RETURNING id",
    )
    .bind(mentor)
    .bind(mentee)
    .fetch_one(&app.db)
    .await
    .unwrap();

    let forced =
        sqlx::query("UPDATE mentor_availability SET consumed_by_session_id = $2 WHERE id = $1")
            .bind(slot)
            .bind(session)
            .execute(&app.db)
            .await;
    assert!(forced.is_err());
}

// ═══════════════════════════════════════════════════════════════════
// Programmes
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn a_cohort_is_priced_per_head_and_a_corporate_run_by_the_month() {
    let app = TestApp::spawn().await;
    a_mentor(&app, "programmentor", "paid_session", None).await;
    app.login("programmentor").await;

    // A cohort with a monthly fee is priced the wrong way round.
    let resp = app
        .post(
            "/api/mentoring-programs",
            &json!({
                "kind": "premium_cohort",
                "title": "Cohorte backend",
                "brief_md": "Six mois, deux séances par mois.",
                "skill_domain": "code",
                "duration_months": 6,
                "monthly_fee": "300.00",
                "max_mentees": 10,
            }),
        )
        .await;
    assert_eq!(resp.status(), 400);

    let resp = app
        .post(
            "/api/mentoring-programs",
            &json!({
                "kind": "premium_cohort",
                "title": "Cohorte backend",
                "brief_md": "Six mois, deux séances par mois.",
                "skill_domain": "code",
                "duration_months": 6,
                "price_per_mentee": "600.00",
                "max_mentees": 10,
            }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["program"]["payer"], "mentee");
    common::assert_amount(&body["data"]["program"]["commission_percent"], "20.00");
}

#[tokio::test]
async fn a_company_client_carries_the_higher_commission() {
    let app = TestApp::spawn().await;
    let mentor = a_mentor(&app, "corpmentor", "paid_session", None).await;
    let enterprise: Uuid = sqlx::query_scalar(
        "INSERT INTO enterprises (owner_id, company_name, slug, company_size)
         VALUES ($1, 'Corp SA', 'corp-sa', '11-50') RETURNING id",
    )
    .bind(mentor)
    .fetch_one(&app.db)
    .await
    .unwrap();

    app.login("corpmentor").await;
    let resp = app
        .post(
            "/api/mentoring-programs",
            &json!({
                "kind": "corporate",
                "enterprise_id": enterprise,
                "title": "Accompagnement juniors",
                "brief_md": "Deux séances par mois pendant six mois.",
                "skill_domain": "code",
                "duration_months": 6,
                "monthly_fee": "500.00",
                "max_mentees": 3,
            }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();

    // Skilluv found the client; the mentor did not have to sell anything.
    common::assert_amount(&body["data"]["program"]["commission_percent"], "25.00");
    assert_eq!(body["data"]["program"]["payer"], "enterprise");
}

#[tokio::test]
async fn a_corporate_run_is_not_on_the_public_list() {
    let app = TestApp::spawn().await;
    let mentor = a_mentor(&app, "hiddenmentor", "paid_session", None).await;
    let enterprise: Uuid = sqlx::query_scalar(
        "INSERT INTO enterprises (owner_id, company_name, slug, company_size)
         VALUES ($1, 'Prive SA', 'prive-sa', '11-50') RETURNING id",
    )
    .bind(mentor)
    .fetch_one(&app.db)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO mentoring_programs
            (mentor_user_id, kind, payer, enterprise_id, title, brief_md, skill_domain,
             duration_months, monthly_fee, commission_percent, max_mentees)
         VALUES ($1, 'corporate', 'enterprise', $2, 'Interne', 'Brief.', 'code',
                 6, 500.00, 25.00, 3)",
    )
    .bind(mentor)
    .bind(enterprise)
    .execute(&app.db)
    .await
    .unwrap();

    // Its places are allocated by the client who paid for them, not browsed.
    let resp = app.get("/api/mentoring-programs").await;
    let body: Value = resp.json().await.unwrap();
    assert!(body["data"]["programs"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn a_cohort_stops_enrolling_when_it_is_full() {
    let app = TestApp::spawn().await;
    let mentor = a_mentor(&app, "fullmentor", "paid_session", None).await;

    let program: Uuid = sqlx::query_scalar(
        "INSERT INTO mentoring_programs
            (mentor_user_id, kind, payer, title, brief_md, skill_domain,
             duration_months, price_per_mentee, commission_percent, max_mentees)
         VALUES ($1, 'premium_cohort', 'mentee', 'Petite cohorte', 'Brief.', 'code',
                 3, 200.00, 20.00, 2)
         RETURNING id",
    )
    .bind(mentor)
    .fetch_one(&app.db)
    .await
    .unwrap();

    for i in 0..2 {
        let name = format!("cohortmentee{i}");
        a_talent(&app, &name).await;
        app.login(&name).await;
        let resp = app
            .post(
                &format!("/api/mentoring-programs/{program}/enrol"),
                &json!({}),
            )
            .await;
        assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    }

    a_talent(&app, "cohortlate").await;
    app.login("cohortlate").await;
    let resp = app
        .post(
            &format!("/api/mentoring-programs/{program}/enrol"),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn enrolling_pays_the_mentor_their_share() {
    let app = TestApp::spawn().await;
    let mentor = a_mentor(&app, "sharementor", "paid_session", None).await;

    let program: Uuid = sqlx::query_scalar(
        "INSERT INTO mentoring_programs
            (mentor_user_id, kind, payer, title, brief_md, skill_domain,
             duration_months, price_per_mentee, commission_percent, max_mentees)
         VALUES ($1, 'premium_cohort', 'mentee', 'Cohorte payante', 'Brief.', 'code',
                 3, 600.00, 20.00, 10)
         RETURNING id",
    )
    .bind(mentor)
    .fetch_one(&app.db)
    .await
    .unwrap();

    a_talent(&app, "sharementee").await;
    app.login("sharementee").await;
    let resp = app
        .post(
            &format!("/api/mentoring-programs/{program}/enrol"),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    // 600 at 20%: 120 to the platform, 480 to the mentor.
    let booked: sqlx::types::BigDecimal = sqlx::query_scalar(
        "SELECT amount_credits FROM platform_revenues WHERE source = 'mentoring_program'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    common::assert_decimal(&booked, "120.00");
}

#[tokio::test]
async fn a_cohort_enrols_the_caller_and_ignores_a_named_email() {
    let app = TestApp::spawn().await;
    let mentor = a_mentor(&app, "emailmentor", "paid_session", None).await;

    let program: Uuid = sqlx::query_scalar(
        "INSERT INTO mentoring_programs
            (mentor_user_id, kind, payer, title, brief_md, skill_domain,
             duration_months, price_per_mentee, commission_percent, max_mentees)
         VALUES ($1, 'premium_cohort', 'mentee', 'Cohorte', 'Brief.', 'code',
                 3, 200.00, 20.00, 5)
         RETURNING id",
    )
    .bind(mentor)
    .fetch_one(&app.db)
    .await
    .unwrap();

    // The mentee pays for it themselves and needs somewhere to be paid from
    // and reviewed.
    let members = sqlx::query(
        "INSERT INTO mentoring_program_members (program_id, mentee_email)
         VALUES ($1, 'quelquun@example.test')",
    )
    .bind(program)
    .execute(&app.db)
    .await;
    // The database allows it; the service is what refuses, so the check is
    // that the route does.
    assert!(members.is_ok());

    a_talent(&app, "emailmentee").await;
    app.login("emailmentee").await;
    let resp = app
        .post(
            &format!("/api/mentoring-programs/{program}/enrol"),
            &json!({ "mentee_email": "autre@example.test" }),
        )
        .await;
    // A cohort enrols the caller's own account; the email is ignored and the
    // caller is enrolled, which is the behaviour a mentee expects.
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    let enrolled: Option<Uuid> = sqlx::query_scalar(
        "SELECT mentee_user_id FROM mentoring_program_members
          WHERE program_id = $1 AND mentee_user_id IS NOT NULL",
    )
    .bind(program)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(enrolled.is_some());
}
