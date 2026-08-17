//! Long placements, corporate learning seats, open calls for proposals.
//!
//! Four of section 12's seven products needed no test file of their own
//! because they needed no new machinery: the newsletter is an audience plan,
//! rank-as-a-service is a scope on the metered API, consulting is a third
//! kind of consultation, and media sponsorship is sponsored content without
//! an event. The tests here cover the three that are genuinely new.

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

async fn an_enterprise(app: &TestApp, company: &str) -> String {
    app.register_enterprise(company).await;
    let username = company.to_lowercase().replace(' ', "");
    app.login(&username).await;
    app.enable_totp_for(&username).await;
    username
}

async fn a_talent(app: &TestApp, username: &str) -> Uuid {
    app.register_user(username).await;
    sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(username)
        .fetch_one(&app.db)
        .await
        .unwrap()
}

// ═══════════════════════════════════════════════════════════════════
// Long placements
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn a_monitoring_fee_needs_somebody_doing_the_monitoring() {
    let app = TestApp::spawn().await;
    let junior = a_talent(&app, "monitoredjunior").await;
    an_enterprise(&app, "Monitorco").await;

    // Otherwise it is a charge for nothing.
    let resp = app
        .post(
            "/api/enterprise/placements",
            &json!({
                "junior_user_id": junior,
                "annual_salary_declared": "18000.00",
                "upfront_fee": "3000.00",
                "monthly_monitoring_fee": "200.00",
            }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn a_placement_starts_and_bills_only_once_the_person_agrees() {
    let app = TestApp::spawn().await;
    let junior = a_talent(&app, "placedjunior").await;
    an_enterprise(&app, "Placeco").await;

    let resp = app
        .post(
            "/api/enterprise/placements",
            &json!({
                "junior_user_id": junior,
                "annual_salary_declared": "18000.00",
                "upfront_fee": "3000.00",
            }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let created: Value = resp.json().await.unwrap();
    let id = created["data"]["placement"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let booked: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM platform_revenues WHERE source = 'long_term_placement'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(booked, 0);

    app.login("placedjunior").await;
    let resp = app
        .post(
            &format!("/api/placements/{id}/respond"),
            &json!({ "accept": true }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    let booked: sqlx::types::BigDecimal = sqlx::query_scalar(
        "SELECT amount_credits FROM platform_revenues WHERE source = 'long_term_placement'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(booked.to_string(), "3000.00");
}

#[tokio::test]
async fn a_month_is_billed_once_and_only_while_active() {
    let app = TestApp::spawn().await;
    an_admin(&app, "billadmin").await;
    let junior = a_talent(&app, "billedjunior").await;
    let mentor = a_talent(&app, "billmentor").await;
    an_enterprise(&app, "Billco").await;

    let resp = app
        .post(
            "/api/enterprise/placements",
            &json!({
                "junior_user_id": junior,
                "mentor_user_id": mentor,
                "annual_salary_declared": "18000.00",
                "upfront_fee": "3000.00",
                "monthly_monitoring_fee": "200.00",
            }),
        )
        .await;
    let created: Value = resp.json().await.unwrap();
    let id = created["data"]["placement"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Not active yet: a month that was not monitored is not billed.
    app.login("billadmin").await;
    let resp = app
        .post(
            &format!("/api/admin/placements/{id}/bill-month"),
            &json!({ "month": "2027-02-01" }),
        )
        .await;
    assert_eq!(resp.status(), 400);

    app.login("billedjunior").await;
    app.post(
        &format!("/api/placements/{id}/respond"),
        &json!({ "accept": true }),
    )
    .await;

    app.login("billadmin").await;
    let resp = app
        .post(
            &format!("/api/admin/placements/{id}/bill-month"),
            &json!({ "month": "2027-02-15" }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    // Any day in the month bills the same month, once.
    let resp = app
        .post(
            &format!("/api/admin/placements/{id}/bill-month"),
            &json!({ "month": "2027-02-01" }),
        )
        .await;
    assert_eq!(resp.status(), 400);

    let months: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM placement_monitoring_months WHERE placement_id = $1::uuid",
    )
    .bind(&id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(months, 1);
}

#[tokio::test]
async fn the_guarantee_covers_a_departure_and_not_a_restructuring() {
    let app = TestApp::spawn().await;
    an_admin(&app, "guaranteeadmin").await;

    for (username, reason, expected) in [
        ("leaverjunior", "person_left", true),
        ("restructjunior", "company_ended", false),
    ] {
        let junior = a_talent(&app, username).await;
        an_enterprise(&app, &format!("{username}co")).await;

        let resp = app
            .post(
                "/api/enterprise/placements",
                &json!({
                    "junior_user_id": junior,
                    "annual_salary_declared": "18000.00",
                    "upfront_fee": "3000.00",
                }),
            )
            .await;
        let created: Value = resp.json().await.unwrap();
        let id = created["data"]["placement"]["id"]
            .as_str()
            .unwrap()
            .to_string();

        app.login(username).await;
        app.post(
            &format!("/api/placements/{id}/respond"),
            &json!({ "accept": true }),
        )
        .await;

        app.login("guaranteeadmin").await;
        let resp = app
            .post(
                &format!("/api/admin/placements/{id}/end"),
                &json!({ "reason": reason }),
            )
            .await;
        assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
        let body: Value = resp.json().await.unwrap();

        // A company that restructured has not been let down; charging Skilluv
        // for that would make the guarantee a refund clause for anything.
        assert_eq!(
            body["data"]["guarantee_applies"], expected,
            "{reason} was judged wrongly"
        );
    }
}

// ═══════════════════════════════════════════════════════════════════
// Corporate learning
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn the_plans_are_published_with_what_they_include() {
    let app = TestApp::spawn().await;
    let resp = app.get("/api/learning/plans").await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let plans = body["data"]["plans"].as_array().unwrap();
    assert_eq!(plans.len(), 3);

    let essentials = plans.iter().find(|p| p["slug"] == "essentials").unwrap();
    let enterprise = plans.iter().find(|p| p["slug"] == "enterprise").unwrap();
    assert!(
        enterprise["features"].as_array().unwrap().len()
            > essentials["features"].as_array().unwrap().len()
    );
}

#[tokio::test]
async fn a_subscription_cannot_hand_out_more_seats_than_it_bought() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Seatco").await;

    let resp = app
        .post(
            "/api/enterprise/learning",
            &json!({ "plan": "essentials", "seats": 2 }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let created: Value = resp.json().await.unwrap();
    let id = created["data"]["subscription"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    for i in 0..2 {
        let employee = a_talent(&app, &format!("seatemployee{i}")).await;
        app.login("seatco").await;
        let resp = app
            .post(
                &format!("/api/enterprise/learning/{id}/seats"),
                &json!({ "employee_user_id": employee }),
            )
            .await;
        assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    }

    let extra = a_talent(&app, "seatextra").await;
    app.login("seatco").await;
    let resp = app
        .post(
            &format!("/api/enterprise/learning/{id}/seats"),
            &json!({ "employee_user_id": extra }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn a_seat_assigned_and_never_taken_is_not_a_user() {
    let app = TestApp::spawn().await;
    let company = an_enterprise(&app, "Usageco").await;

    let resp = app
        .post(
            "/api/enterprise/learning",
            &json!({ "plan": "professional", "seats": 3 }),
        )
        .await;
    let created: Value = resp.json().await.unwrap();
    let id = created["data"]["subscription"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let taker = a_talent(&app, "seattaker").await;
    let ignorer = a_talent(&app, "seatignorer").await;
    app.login(&company).await;
    for employee in [taker, ignorer] {
        app.post(
            &format!("/api/enterprise/learning/{id}/seats"),
            &json!({ "employee_user_id": employee }),
        )
        .await;
    }

    app.login("seattaker").await;
    let resp = app
        .post(&format!("/api/learning/{id}/activate"), &json!({}))
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    app.login(&company).await;
    let resp = app
        .get(&format!("/api/enterprise/learning/{id}/usage"))
        .await;
    let body: Value = resp.json().await.unwrap();

    // Reporting the assigned count as engagement would let a company believe
    // it bought something it did not.
    assert_eq!(body["data"]["seats_bought"], 3);
    assert_eq!(body["data"]["seats_assigned"], 2);
    assert_eq!(body["data"]["seats_taken_up"], 1);
}

// ═══════════════════════════════════════════════════════════════════
// Open calls
// ═══════════════════════════════════════════════════════════════════

fn an_rfp(slug: &str) -> Value {
    json!({
        "slug": slug,
        "title": "Refonte de notre back-office",
        "context_md": "Notre back-office a douze ans, tourne sur une version de PHP qui \
                       n'est plus maintenue, et trois personnes savent encore le \
                       déployer. Nous voulons savoir par où commencer.",
        "desired_outcome_md": "Une trajectoire de reprise chiffrée et un premier lot \
                               livrable en trois mois.",
        "budget_min": "20000.00",
        "budget_max": "60000.00",
        "proposal_deadline": "2027-04-01T00:00:00Z",
        "selection_deadline": "2027-04-30T00:00:00Z",
        "facilitation_fee": "2000.00",
    })
}

#[tokio::test]
async fn a_call_publishes_its_budget() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Nobudgetco").await;

    let mut body = an_rfp("no-budget");
    body["budget_min"] = json!("0.00");
    // A call with none wastes the time of everybody whose answer would have
    // been "not for that".
    let resp = app.post("/api/enterprise/rfps", &body).await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn a_call_says_when_it_will_have_chosen() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Nodeadlineco").await;

    let mut body = an_rfp("no-deadline");
    body["selection_deadline"] = json!("2027-03-01T00:00:00Z");
    // Before the proposal deadline: a pile of unpaid proposals nobody ever
    // answers.
    let resp = app.post("/api/enterprise/rfps", &body).await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn nobody_proposes_twice_on_the_same_call() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Twiceco").await;
    let resp = app
        .post("/api/enterprise/rfps", &an_rfp("twice-call"))
        .await;
    let created: Value = resp.json().await.unwrap();
    let id = created["data"]["rfp"]["id"].as_str().unwrap().to_string();

    a_talent(&app, "twiceproposer").await;
    app.login("twiceproposer").await;
    let proposal = json!({
        "pitch_md": "Nous avons repris deux back-offices de cet âge, dont un dans la \
                     logistique, et nous savons à quoi ressemble le premier mois.",
        "approach_md": "Un audit d'une semaine pour cartographier les dépendances, puis \
                        un premier lot sur le module de facturation, isolé derrière une \
                        façade.",
        "estimated_price": "45000.00",
        "estimated_weeks": 12,
    });
    let resp = app
        .post(&format!("/api/rfps/{id}/proposals"), &proposal)
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    let resp = app
        .post(&format!("/api/rfps/{id}/proposals"), &proposal)
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn every_proposal_is_answered_before_the_call_is_awarded() {
    let app = TestApp::spawn().await;
    let company = an_enterprise(&app, "Answerco").await;
    let resp = app
        .post("/api/enterprise/rfps", &an_rfp("answer-call"))
        .await;
    let created: Value = resp.json().await.unwrap();
    let id = created["data"]["rfp"]["id"].as_str().unwrap().to_string();

    let mut proposals = Vec::new();
    for i in 0..2 {
        let name = format!("answerproposer{i}");
        a_talent(&app, &name).await;
        app.login(&name).await;
        let resp = app
            .post(
                &format!("/api/rfps/{id}/proposals"),
                &json!({
                    "pitch_md": "Nous avons déjà fait exactement cela pour une \
                                 entreprise de taille comparable, et nous savons où \
                                 sont les surprises.",
                    "approach_md": "Cartographie d'abord, puis un lot isolé derrière \
                                    une façade, puis le reste par ordre de risque \
                                    décroissant.",
                    "estimated_price": "40000.00",
                    "estimated_weeks": 10,
                }),
            )
            .await;
        let created: Value = resp.json().await.unwrap();
        proposals.push(created["data"]["proposal_id"].as_str().unwrap().to_string());
    }

    app.login(&company).await;

    // A refusal with no reason: silence is the one thing not owed to them.
    let resp = app
        .post(
            &format!("/api/enterprise/rfp-proposals/{}/decide", proposals[1]),
            &json!({ "selected": false }),
        )
        .await;
    assert_eq!(resp.status(), 400);

    // Awarding while one is unanswered: the company has what it wants and the
    // other is the one left waiting.
    let resp = app
        .post(
            &format!("/api/enterprise/rfps/{id}/award"),
            &json!({ "winner_proposal_id": proposals[0] }),
        )
        .await;
    assert_eq!(resp.status(), 400);

    app.post(
        &format!("/api/enterprise/rfp-proposals/{}/decide", proposals[0]),
        &json!({ "selected": true }),
    )
    .await;
    app.post(
        &format!("/api/enterprise/rfp-proposals/{}/decide", proposals[1]),
        &json!({ "selected": false, "note": "Approche plus risquée sur la reprise." }),
    )
    .await;

    let resp = app
        .post(
            &format!("/api/enterprise/rfps/{id}/award"),
            &json!({ "winner_proposal_id": proposals[0] }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    assert_eq!(
        body["data"]["facilitation_fee"].as_str().unwrap(),
        "2000.00"
    );
}

#[tokio::test]
async fn a_rival_cannot_read_the_proposals_on_somebody_elses_call() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Ownercallco").await;
    let resp = app
        .post("/api/enterprise/rfps", &an_rfp("owner-call"))
        .await;
    let created: Value = resp.json().await.unwrap();
    let id = created["data"]["rfp"]["id"].as_str().unwrap().to_string();

    an_enterprise(&app, "Rivalco").await;
    let resp = app.get(&format!("/api/rfps/{id}/proposals")).await;
    assert_eq!(resp.status(), 404);
}

// ═══════════════════════════════════════════════════════════════════
// The four that reused existing machinery
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn the_paid_newsletter_is_an_audience_plan() {
    let app = TestApp::spawn().await;
    a_talent(&app, "newsreader").await;
    app.login("newsreader").await;

    // No second subscription mechanism: the one built for replays takes it.
    let resp = app
        .post(
            "/api/audience/subscribe",
            &json!({ "plan": "newsletter_premium" }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    let plan: String = sqlx::query_scalar(
        "SELECT plan FROM audience_subscriptions WHERE plan = 'newsletter_premium'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(plan, "newsletter_premium");
}

#[tokio::test]
async fn consulting_is_a_third_kind_of_consultation() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Implementco").await;

    // Same table, same experts, same commission machinery — a longer clock.
    let resp = app
        .post(
            "/api/enterprise/consultations",
            &json!({
                "kind": "implementation",
                "topic": "Mettre en place un compagnonnage interne",
                "question_md": "Nous voulons structurer l'apprentissage de nos juniors \
                                sur le modèle du compagnonnage plutôt que par des \
                                formations ponctuelles.",
                "skill_domain": "code",
                "implementation_type": "compagnonnage_setup",
                "duration_weeks": 12,
                "fee": "60000.00",
            }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn an_implementation_says_what_it_is_and_how_long() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Vagueimplco").await;

    let resp = app
        .post(
            "/api/enterprise/consultations",
            &json!({
                "kind": "implementation",
                "topic": "Quelque chose",
                "question_md": "Nous voulons faire mieux sur la formation interne, sans \
                                savoir exactement quoi pour l'instant.",
                "skill_domain": "code",
                "fee": "60000.00",
            }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}
