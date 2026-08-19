//! The finance line.
//!
//! Two things are being tested more than the arithmetic: that an advance
//! stays an advance rather than drifting into a loan, and that no
//! introduction to a bank or an insurer can happen without the paperwork that
//! permits it.

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

/// An issued invoice on a mission, which is the only thing an advance can
/// point at.
async fn an_issued_invoice(app: &TestApp, owner: Uuid, amount: &str) -> Uuid {
    let enterprise: Uuid = sqlx::query_scalar(
        "INSERT INTO enterprises (owner_id, company_name, slug, company_size)
         VALUES ($1, 'Facture SA',
                 'facture-sa-' || substr(gen_random_uuid()::text, 1, 8), '11-50')
         RETURNING id",
    )
    .bind(owner)
    .fetch_one(&app.db)
    .await
    .unwrap();

    let mission: Uuid = sqlx::query_scalar(
        "INSERT INTO missions
            (enterprise_id, mission_type_id, skill_domain, slug, title, description,
             acceptance_criteria, deliverable_format, payment_model, budget_eur,
             created_by)
         VALUES ($1, (SELECT id FROM mission_types LIMIT 1), 'code',
                 'mission-' || substr(gen_random_uuid()::text, 1, 8),
                 'Mission', 'Brief', 'Livré et relu.', 'github_pr',
                 'fixed_price', 5000.00, $2)
         RETURNING id",
    )
    .bind(enterprise)
    .bind(owner)
    .fetch_one(&app.db)
    .await
    .unwrap();

    sqlx::query_scalar(
        "INSERT INTO mission_invoices
            (mission_id, sequence, label, amount, commission_percent)
         VALUES ($1, 1, 'Livraison', $2::NUMERIC, 15.00)
         RETURNING id",
    )
    .bind(mission)
    .bind(amount)
    .fetch_one(&app.db)
    .await
    .unwrap()
}

// ═══════════════════════════════════════════════════════════════════
// Partnerships
// ═══════════════════════════════════════════════════════════════════

fn a_partnership() -> Value {
    json!({
        "partner_org": "Banque partenaire",
        "kind": "loan",
        "countries": ["BJ", "CI"],
        "commission_percent": "3.00",
        "min_rank": "artisan",
    })
}

#[tokio::test]
async fn a_partnership_cannot_go_live_without_the_paperwork() {
    let app = TestApp::spawn().await;
    an_admin(&app, "partneradmin").await;

    let resp = app
        .post("/api/admin/finance/partnerships", &a_partnership())
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let created: Value = resp.json().await.unwrap();
    let id = created["data"]["partnership"]["id"].as_str().unwrap();

    // Introducing somebody to a lender is a regulated act. The code is
    // complete and the switch is a document.
    let resp = app
        .post(
            &format!("/api/admin/finance/partnerships/{id}/activate"),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), 400);

    sqlx::query(
        "UPDATE financial_partnerships
            SET regulatory_basis = 'ORIAS 25000000',
                contract_url = 'https://example.test/convention.pdf'
          WHERE id = $1::uuid",
    )
    .bind(id)
    .execute(&app.db)
    .await
    .unwrap();

    let resp = app
        .post(
            &format!("/api/admin/finance/partnerships/{id}/activate"),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn a_draft_partnership_is_invisible_rather_than_coming_soon() {
    let app = TestApp::spawn().await;
    an_admin(&app, "draftpartneradmin").await;
    app.post("/api/admin/finance/partnerships", &a_partnership())
        .await;

    // An introduction we cannot lawfully make should not be advertised.
    let resp = app.get("/api/finance/partners").await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert!(body["data"]["partners"].as_array().unwrap().is_empty());
}

/// An active partnership, ready to take introductions.
async fn a_live_partnership(app: &TestApp, kind: &str, commission: &str) -> Uuid {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO financial_partnerships
            (partner_org, kind, countries, commission_percent, regulatory_basis,
             contract_url, min_rank, status)
         VALUES ('Partenaire', $1, ARRAY['BJ']::CHAR(2)[], $2::NUMERIC,
                 'ORIAS 25000000', 'https://example.test/c.pdf', 'artisan', 'active')
         RETURNING id",
    )
    .bind(kind)
    .bind(commission)
    .fetch_one(&app.db)
    .await
    .unwrap();
    id
}

#[tokio::test]
async fn somebody_below_the_partners_floor_is_not_introduced() {
    let app = TestApp::spawn().await;
    let partnership = a_live_partnership(&app, "loan", "3.00").await;
    a_talent(&app, "juniorborrower", "ranger").await;
    app.login("juniorborrower").await;

    // The partner prices on our assessment, and an assessment with no
    // history behind it is not an assessment.
    let resp = app
        .post(
            "/api/finance/referrals",
            &json!({
                "partnership_id": partnership,
                "purpose": "Acheter un ordinateur portable.",
                "amount_requested": "800.00",
            }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn a_referral_shows_the_person_what_was_said_about_them() {
    let app = TestApp::spawn().await;
    let partnership = a_live_partnership(&app, "loan", "3.00").await;
    a_talent(&app, "borrower", "artisan").await;
    app.login("borrower").await;

    let resp = app
        .post(
            "/api/finance/referrals",
            &json!({
                "partnership_id": partnership,
                "purpose": "Acheter un ordinateur portable.",
                "amount_requested": "800.00",
            }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();

    // They are entitled to see it without asking, and it is what the partner
    // priced on.
    assert_eq!(body["data"]["shared_with_partner"]["rank"], "artisan");
    assert!(body["data"]["shared_with_partner"]["attestations"].is_number());
}

#[tokio::test]
async fn nothing_is_earned_on_a_refusal() {
    let app = TestApp::spawn().await;
    an_admin(&app, "refusaladmin").await;
    let partnership = a_live_partnership(&app, "loan", "3.00").await;
    a_talent(&app, "refusedborrower", "artisan").await;
    app.login("refusedborrower").await;

    let resp = app
        .post(
            "/api/finance/referrals",
            &json!({
                "partnership_id": partnership,
                "purpose": "Un prêt.",
                "amount_requested": "5000.00",
            }),
        )
        .await;
    let created: Value = resp.json().await.unwrap();
    let referral = created["data"]["referral_id"].as_str().unwrap();

    app.login("refusaladmin").await;
    let resp = app
        .post(
            &format!("/api/admin/finance/referrals/{referral}/decision"),
            &json!({ "approved": false, "note": "Dossier incomplet." }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    // Booking a commission on a rejection is how an introduction business
    // starts referring people it knows will be turned down.
    let commissions: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM platform_revenues WHERE source = 'factoring_take'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(commissions, 0);
}

#[tokio::test]
async fn an_approved_loan_books_its_origination_commission() {
    let app = TestApp::spawn().await;
    an_admin(&app, "approvaladmin").await;
    let partnership = a_live_partnership(&app, "loan", "3.00").await;
    a_talent(&app, "approvedborrower", "maitre").await;
    app.login("approvedborrower").await;

    let resp = app
        .post(
            "/api/finance/referrals",
            &json!({
                "partnership_id": partnership,
                "purpose": "Un prêt.",
                "amount_requested": "10000.00",
            }),
        )
        .await;
    let created: Value = resp.json().await.unwrap();
    let referral = created["data"]["referral_id"].as_str().unwrap();

    app.login("approvaladmin").await;
    let resp = app
        .post(
            &format!("/api/admin/finance/referrals/{referral}/decision"),
            &json!({ "approved": true, "approved_amount": "8000.00" }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["commission"].as_str().unwrap(), "240.00");
}

// ═══════════════════════════════════════════════════════════════════
// Advances
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn an_advance_needs_an_issued_invoice_to_point_at() {
    let app = TestApp::spawn().await;
    let person = a_talent(&app, "advanceperson", "artisan").await;
    let invoice = an_issued_invoice(&app, person, "1000.00").await;

    sqlx::query("UPDATE mission_invoices SET status = 'paid' WHERE id = $1")
        .bind(invoice)
        .execute(&app.db)
        .await
        .unwrap();

    app.login("advanceperson").await;
    // Against anything but an issued invoice it would be a loan, which is
    // not what this is.
    let resp = app
        .post(
            "/api/users/me/advances",
            &json!({ "invoice_id": invoice, "advance_percent": "50.00" }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn the_net_amount_is_stated_before_anybody_agrees() {
    let app = TestApp::spawn().await;
    let person = a_talent(&app, "netperson", "artisan").await;
    let invoice = an_issued_invoice(&app, person, "1000.00").await;

    app.login("netperson").await;
    let resp = app
        .post(
            "/api/users/me/advances",
            &json!({ "invoice_id": invoice, "advance_percent": "50.00" }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();

    // 500 advanced, 4% fee, 480 received. The number they care about.
    assert_eq!(body["data"]["advance"]["advance_amount"], "500.00");
    assert_eq!(body["data"]["you_would_receive"], "480.00");
}

#[tokio::test]
async fn one_invoice_carries_one_advance() {
    let app = TestApp::spawn().await;
    let person = a_talent(&app, "doubleperson", "artisan").await;
    let invoice = an_issued_invoice(&app, person, "1000.00").await;

    app.login("doubleperson").await;
    let body = json!({ "invoice_id": invoice, "advance_percent": "50.00" });
    app.post("/api/users/me/advances", &body).await;

    // A second would advance more than the invoice is worth, with nothing to
    // repay it from.
    let resp = app.post("/api/users/me/advances", &body).await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn an_advance_outside_thirty_to_ninety_percent_is_refused() {
    let app = TestApp::spawn().await;
    let person = a_talent(&app, "bandperson", "artisan").await;
    let invoice = an_issued_invoice(&app, person, "1000.00").await;

    app.login("bandperson").await;
    let resp = app
        .post(
            "/api/users/me/advances",
            &json!({ "invoice_id": invoice, "advance_percent": "100.00" }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn the_rank_floor_holds_on_advances_too() {
    let app = TestApp::spawn().await;
    let person = a_talent(&app, "juniorperson", "ranger").await;
    let invoice = an_issued_invoice(&app, person, "1000.00").await;

    app.login("juniorperson").await;
    let resp = app
        .post(
            "/api/users/me/advances",
            &json!({ "invoice_id": invoice, "advance_percent": "50.00" }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn a_written_off_advance_stops_the_next_one() {
    let app = TestApp::spawn().await;
    an_admin(&app, "writeoffadmin").await;
    let person = a_talent(&app, "writeoffperson", "artisan").await;
    let first = an_issued_invoice(&app, person, "1000.00").await;

    app.login("writeoffperson").await;
    let resp = app
        .post(
            "/api/users/me/advances",
            &json!({ "invoice_id": first, "advance_percent": "50.00" }),
        )
        .await;
    let created: Value = resp.json().await.unwrap();
    let advance = created["data"]["advance"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    app.login("writeoffadmin").await;
    app.post(
        &format!("/api/admin/finance/advances/{advance}/disburse"),
        &json!({}),
    )
    .await;
    let resp = app
        .post(
            &format!("/api/admin/finance/advances/{advance}/write-off"),
            &json!({ "reason": "Le client a disparu." }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    // Advancing again into the same situation helps nobody.
    let second = an_issued_invoice(&app, person, "2000.00").await;
    app.login("writeoffperson").await;
    let resp = app
        .post(
            "/api/users/me/advances",
            &json!({ "invoice_id": second, "advance_percent": "50.00" }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn disbursing_pays_the_net_and_books_the_fee() {
    let app = TestApp::spawn().await;
    an_admin(&app, "disburseadmin").await;
    let person = a_talent(&app, "disburseperson", "artisan").await;
    let invoice = an_issued_invoice(&app, person, "1000.00").await;

    app.login("disburseperson").await;
    let resp = app
        .post(
            "/api/users/me/advances",
            &json!({ "invoice_id": invoice, "advance_percent": "60.00" }),
        )
        .await;
    let created: Value = resp.json().await.unwrap();
    let advance = created["data"]["advance"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    app.login("disburseadmin").await;
    let resp = app
        .post(
            &format!("/api/admin/finance/advances/{advance}/disburse"),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    // 600 advanced, 24 fee, 576 out.
    assert_eq!(body["data"]["paid_out"].as_str().unwrap(), "576.00");

    let booked: sqlx::types::BigDecimal = sqlx::query_scalar(
        "SELECT amount_credits FROM platform_revenues WHERE source = 'factoring_take'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(booked.to_string(), "24.00");
}

// ═══════════════════════════════════════════════════════════════════
// The payment guarantee
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn a_claim_without_a_live_guarantee_is_refused() {
    let app = TestApp::spawn().await;
    an_admin(&app, "claimadmin").await;
    let person = a_talent(&app, "uncoveredperson", "artisan").await;

    let resp = app
        .post(
            "/api/admin/finance/guarantee-claims",
            &json!({
                "user_id": person,
                "amount": "300.00",
                "reason": "Le client conteste après validation.",
            }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn a_claim_is_capped_per_mission_and_per_year() {
    let app = TestApp::spawn().await;
    an_admin(&app, "capadmin").await;
    let person = a_talent(&app, "coveredperson", "artisan").await;

    app.login("coveredperson").await;
    let resp = app
        .post("/api/finance/guarantee", &json!({ "tier": "basic" }))
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    app.login("capadmin").await;
    // Basic covers 500 a mission and 1500 a year.
    let resp = app
        .post(
            "/api/admin/finance/guarantee-claims",
            &json!({
                "user_id": person,
                "amount": "900.00",
                "reason": "Litige sur une livraison validée.",
            }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["paid"].as_str().unwrap(), "500.00");

    // Two more of the same exhaust the year.
    for _ in 0..2 {
        app.post(
            "/api/admin/finance/guarantee-claims",
            &json!({
                "user_id": person,
                "amount": "500.00",
                "reason": "Autre litige.",
            }),
        )
        .await;
    }

    // Paying past the cap would be paying out of the next person's premium.
    let resp = app
        .post(
            "/api/admin/finance/guarantee-claims",
            &json!({
                "user_id": person,
                "amount": "500.00",
                "reason": "Encore un litige.",
            }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn renewing_the_guarantee_extends_rather_than_duplicating() {
    let app = TestApp::spawn().await;
    a_talent(&app, "renewperson", "artisan").await;
    app.login("renewperson").await;

    let first = app
        .post("/api/finance/guarantee", &json!({ "tier": "basic" }))
        .await;
    let first: Value = first.json().await.unwrap();
    let first_expiry = first["data"]["expires_at"].as_str().unwrap().to_string();

    let second = app
        .post("/api/finance/guarantee", &json!({ "tier": "premium" }))
        .await;
    assert_eq!(second.status(), 200);
    let second: Value = second.json().await.unwrap();
    assert!(second["data"]["expires_at"].as_str().unwrap() > first_expiry.as_str());

    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM payment_guarantee_subscriptions")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(rows, 1);
}

// ═══════════════════════════════════════════════════════════════════
// Growth financing
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn a_funded_trainee_can_never_be_made_to_owe_anything() {
    let app = TestApp::spawn().await;
    let owner = a_talent(&app, "growthowner", "artisan").await;
    let enterprise: Uuid = sqlx::query_scalar(
        "INSERT INTO enterprises (owner_id, company_name, slug, company_size)
         VALUES ($1, 'Financeur SA', 'financeur-sa', '11-50') RETURNING id",
    )
    .bind(owner)
    .fetch_one(&app.db)
    .await
    .unwrap();

    let program: Uuid = sqlx::query_scalar(
        "INSERT INTO growth_financing_programs
            (enterprise_id, name, brief_md, cohort_size, duration_months,
             total_investment, hires_expected_min)
         VALUES ($1, 'Promotion 2027', 'Vingt juniors, six mois.', 20, 6,
                 50000.00, 5)
         RETURNING id",
    )
    .bind(enterprise)
    .fetch_one(&app.db)
    .await
    .unwrap();

    // An income share agreement a trainee can owe is a debt taken on by
    // somebody with no income. The column exists so the answer is in the
    // data, and the check makes the other value impossible.
    let forced = sqlx::query(
        "UPDATE growth_financing_programs SET unplaced_owe_nothing = FALSE WHERE id = $1",
    )
    .bind(program)
    .execute(&app.db)
    .await;
    assert!(forced.is_err());
}

#[tokio::test]
async fn declining_the_job_at_the_end_is_a_normal_outcome() {
    let app = TestApp::spawn().await;
    let owner = a_talent(&app, "declineowner", "artisan").await;
    let trainee = a_talent(&app, "declinetrainee", "apprenti").await;
    let enterprise: Uuid = sqlx::query_scalar(
        "INSERT INTO enterprises (owner_id, company_name, slug, company_size)
         VALUES ($1, 'Financeur B', 'financeur-b', '11-50') RETURNING id",
    )
    .bind(owner)
    .fetch_one(&app.db)
    .await
    .unwrap();

    let program: Uuid = sqlx::query_scalar(
        "INSERT INTO growth_financing_programs
            (enterprise_id, name, brief_md, cohort_size, duration_months,
             total_investment, hires_expected_min)
         VALUES ($1, 'Promotion B', 'Brief.', 10, 6, 30000.00, 3)
         RETURNING id",
    )
    .bind(enterprise)
    .fetch_one(&app.db)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO growth_financing_trainees (program_id, user_id, declined_at, status)
         VALUES ($1, $2, NOW(), 'completed')",
    )
    .bind(program)
    .bind(trainee)
    .execute(&app.db)
    .await
    .unwrap();

    // The company funded the training; it did not buy the person.
    let forced = sqlx::query(
        "UPDATE growth_financing_trainees SET status = 'hired'
          WHERE program_id = $1 AND user_id = $2",
    )
    .bind(program)
    .bind(trainee)
    .execute(&app.db)
    .await;
    assert!(forced.is_err());
}
