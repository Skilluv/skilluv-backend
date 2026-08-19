//! The ecosystem line.
//!
//! Two things are worth testing here more than the rest. That a badge cannot
//! be bought — because the person a bought badge misleads is the contributor
//! who took the job, not the company that paid. And that a marketplace sale
//! always adds back to what the buyer paid, because the alternative is money
//! that went somewhere nobody can name.

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

async fn an_enterprise(app: &TestApp, owner: Uuid, name: &str) -> Uuid {
    sqlx::query_scalar(
        // `company_size` has been NOT NULL since migration 0006. Omitting it
        // fails the insert rather than defaulting, which is the point: an
        // enterprise nobody sized cannot be quoted for.
        "INSERT INTO enterprises (owner_id, company_name, slug, company_size)
         VALUES ($1, $2, $3, '11-50') RETURNING id",
    )
    .bind(owner)
    .bind(name)
    .bind(name.to_lowercase().replace(' ', "-"))
    .fetch_one(&app.db)
    .await
    .unwrap()
}

// ═══════════════════════════════════════════════════════════════════
// Certifications
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn the_pass_mark_is_published_with_the_price() {
    let app = TestApp::spawn().await;
    let resp = app.get("/api/certifications/programs").await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let programs = body["data"]["programs"].as_array().unwrap();

    // A certification whose bar is private is one nobody can judge the worth
    // of.
    assert!(!programs.is_empty());
    for program in programs {
        assert!(program["pass_mark"].is_string() || program["pass_mark"].is_number());
        assert!(program["annual_fee"].is_string() || program["annual_fee"].is_number());
    }

    // Gold asks the most, because it is the level that commits Skilluv's
    // credibility furthest.
    let gold = programs
        .iter()
        .find(|p| p["slug"] == "enterprise_partner_gold")
        .unwrap();
    let bronze = programs
        .iter()
        .find(|p| p["slug"] == "enterprise_partner_bronze")
        .unwrap();
    assert!(gold["pass_mark"].as_str().unwrap() > bronze["pass_mark"].as_str().unwrap());
}

#[tokio::test]
async fn a_certification_pointed_at_the_wrong_kind_of_subject_is_refused() {
    let app = TestApp::spawn().await;
    let person = a_talent(&app, "certperson").await;
    app.login("certperson").await;

    // The studio programme certifies an outside organisation. Pointed at a
    // person it certifies nothing anybody asked for.
    let resp = app
        .post(
            "/api/certifications/request",
            &json!({ "program": "external_studio", "subject_user_id": person }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn paying_does_not_certify() {
    let app = TestApp::spawn().await;
    an_admin(&app, "certadmin").await;
    let owner = a_talent(&app, "certowner").await;
    let enterprise = an_enterprise(&app, owner, "Payeur SA").await;

    app.login("certowner").await;
    let resp = app
        .post(
            "/api/certifications/request",
            &json!({
                "program": "enterprise_partner_gold",
                "subject_enterprise_id": enterprise,
            }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let created: Value = resp.json().await.unwrap();
    let id = created["data"]["certification"]["id"].as_str().unwrap();

    // Nothing is issued and nothing is booked until an audit says so.
    let booked: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM platform_revenues WHERE source = 'certification_program'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(booked, 0);

    // And the database refuses an issue with no audit behind it.
    let forced = sqlx::query(
        "UPDATE program_certifications SET status = 'issued', issued_at = NOW(),
                expires_at = NOW() + INTERVAL '1 year'
          WHERE id = $1::uuid",
    )
    .bind(id)
    .execute(&app.db)
    .await;
    assert!(forced.is_err());
}

#[tokio::test]
async fn an_audit_without_evidence_is_an_opinion_with_a_number_on_it() {
    let app = TestApp::spawn().await;
    an_admin(&app, "evidenceadmin").await;
    let owner = a_talent(&app, "evidenceowner").await;
    let enterprise = an_enterprise(&app, owner, "Preuve SA").await;

    app.login("evidenceowner").await;
    let resp = app
        .post(
            "/api/certifications/request",
            &json!({
                "program": "enterprise_partner_bronze",
                "subject_enterprise_id": enterprise,
            }),
        )
        .await;
    let created: Value = resp.json().await.unwrap();
    let id = created["data"]["certification"]["id"].as_str().unwrap();

    app.login("evidenceadmin").await;
    let resp = app
        .post(
            &format!("/api/admin/certifications/{id}/audit"),
            &json!({
                "findings": [
                    { "criterion": "fair_pay", "score": "90", "evidence": "   " }
                ]
            }),
        )
        .await;
    assert_eq!(resp.status(), 400);

    let resp = app
        .post(
            &format!("/api/admin/certifications/{id}/audit"),
            &json!({ "findings": [] }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn failing_the_audit_costs_the_fee_and_gets_no_badge() {
    let app = TestApp::spawn().await;
    an_admin(&app, "failadmin").await;
    let owner = a_talent(&app, "failowner").await;
    let enterprise = an_enterprise(&app, owner, "Recale SA").await;

    app.login("failowner").await;
    let resp = app
        .post(
            "/api/certifications/request",
            &json!({
                "program": "enterprise_partner_gold",
                "subject_enterprise_id": enterprise,
            }),
        )
        .await;
    let created: Value = resp.json().await.unwrap();
    let id = created["data"]["certification"]["id"].as_str().unwrap();

    // Gold needs 90. A weighted mean of 70 does not reach it.
    app.login("failadmin").await;
    let resp = app
        .post(
            &format!("/api/admin/certifications/{id}/audit"),
            &json!({
                "findings": [
                    { "criterion": "fair_pay", "score": "70",
                      "evidence": "Trois paiements en retard sur douze." },
                    { "criterion": "delivery", "score": "70",
                      "evidence": "Deux missions livrées hors délai." }
                ]
            }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["certification"]["status"], "failed");

    // Nothing booked: the revenue follows the badge, not the order.
    let booked: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM platform_revenues WHERE source = 'certification_program'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(booked, 0);

    // And it is not on the live list.
    let resp = app.get("/api/certifications/live").await;
    let body: Value = resp.json().await.unwrap();
    assert!(
        body["data"]["certifications"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn passing_the_audit_issues_a_dated_badge_and_books_the_fee() {
    let app = TestApp::spawn().await;
    an_admin(&app, "passadmin").await;
    let owner = a_talent(&app, "passowner").await;
    let enterprise = an_enterprise(&app, owner, "Recu SA").await;

    app.login("passowner").await;
    let resp = app
        .post(
            "/api/certifications/request",
            &json!({
                "program": "enterprise_partner_bronze",
                "subject_enterprise_id": enterprise,
            }),
        )
        .await;
    let created: Value = resp.json().await.unwrap();
    let id = created["data"]["certification"]["id"].as_str().unwrap();

    app.login("passadmin").await;
    let resp = app
        .post(
            &format!("/api/admin/certifications/{id}/audit"),
            &json!({
                "findings": [
                    { "criterion": "fair_pay", "score": "95", "weight": "2",
                      "evidence": "Douze paiements sur douze dans les délais." },
                    { "criterion": "delivery", "score": "80",
                      "evidence": "Une mission livrée avec deux jours de retard." }
                ],
                "notes": "Bon dossier."
            }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["certification"]["status"], "issued");
    // (95*2 + 80) / 3 = 90
    assert_eq!(body["data"]["certification"]["audit_score"], "90.00");
    assert!(body["data"]["certification"]["expires_at"].is_string());

    let booked: sqlx::types::BigDecimal = sqlx::query_scalar(
        "SELECT amount_credits FROM platform_revenues
          WHERE source = 'certification_program'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(booked.to_string(), "5000.00");

    // The findings are kept, so the score can be argued with rather than
    // only believed.
    let findings: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM certification_audit_findings WHERE certification_id = $1::uuid",
    )
    .bind(id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(findings, 2);
}

#[tokio::test]
async fn one_live_certification_per_subject_per_programme() {
    let app = TestApp::spawn().await;
    let owner = a_talent(&app, "dupowner").await;
    let enterprise = an_enterprise(&app, owner, "Double SA").await;

    app.login("dupowner").await;
    let body = json!({
        "program": "enterprise_partner_bronze",
        "subject_enterprise_id": enterprise,
    });
    app.post("/api/certifications/request", &body).await;

    // Two would let them show whichever suits.
    let resp = app.post("/api/certifications/request", &body).await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn a_lapsed_certification_stops_being_live() {
    let app = TestApp::spawn().await;
    an_admin(&app, "lapseadmin").await;
    let owner = a_talent(&app, "lapseowner").await;
    let enterprise = an_enterprise(&app, owner, "Perime SA").await;

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO program_certifications
            (program, subject_enterprise_id, fee, audit_score, audited_at,
             status, issued_at, expires_at)
         VALUES ('enterprise_partner_bronze', $1, 5000.00, 80.00, NOW() - INTERVAL '2 years',
                 'issued', NOW() - INTERVAL '2 years', NOW() - INTERVAL '1 year')
         RETURNING id",
    )
    .bind(enterprise)
    .fetch_one(&app.db)
    .await
    .unwrap();

    // Not on the live list even before the sweep runs.
    let resp = app.get("/api/certifications/live").await;
    let body: Value = resp.json().await.unwrap();
    assert!(
        body["data"]["certifications"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    app.login("lapseadmin").await;
    let resp = app
        .post("/api/admin/certifications/expire-lapsed", &json!({}))
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    // A status that lags reality shows up in every export, and somebody
    // eventually trusts one of those.
    let status: String =
        sqlx::query_scalar("SELECT status FROM program_certifications WHERE id = $1")
            .bind(id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(status, "expired");
}

// ═══════════════════════════════════════════════════════════════════
// The marketplace
// ═══════════════════════════════════════════════════════════════════

fn an_item(slug: &str, price: &str) -> Value {
    json!({
        "slug": slug,
        "item_type": "code_boilerplate",
        "skill_domain": "code",
        "title": "Squelette API Rust",
        "description_md": "Un projet axum prêt à démarrer, avec ses tests.",
        "thumbnail_url": "https://example.test/thumb.png",
        "file_keys": ["items/skeleton.zip"],
        "license_type": "commercial",
        "license_summary": "Utilisable dans vos projets clients, sans revente en l'état.",
        "price": price,
    })
}

async fn a_published_item(app: &TestApp, creator: &str, slug: &str, price: &str) -> Uuid {
    app.login(creator).await;
    let resp = app
        .post("/api/marketplace/items", &an_item(slug, price))
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let created: Value = resp.json().await.unwrap();
    let id: Uuid = created["data"]["item"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    let resp = app
        .post(&format!("/api/marketplace/items/{id}/publish"), &json!({}))
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    id
}

#[tokio::test]
async fn a_licence_nobody_can_read_is_refused() {
    let app = TestApp::spawn().await;
    a_talent(&app, "licencecreator").await;
    app.login("licencecreator").await;

    let mut body = an_item("bad-licence", "30.00");
    body["license_summary"] = json!("MIT");
    let resp = app.post("/api/marketplace/items", &body).await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn the_creator_sees_their_take_before_listing() {
    let app = TestApp::spawn().await;
    a_talent(&app, "takecreator").await;
    let id = a_published_item(&app, "takecreator", "take-item", "30.00").await;

    let resp = app.get(&format!("/api/marketplace/items/{id}")).await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();

    // 15% above twenty euros.
    assert_eq!(
        body["data"]["platform_commission"].as_str().unwrap(),
        "4.50"
    );
    assert_eq!(body["data"]["creator_receives"].as_str().unwrap(), "25.50");
}

#[tokio::test]
async fn a_small_item_carries_the_higher_commission() {
    let app = TestApp::spawn().await;
    a_talent(&app, "microcreator").await;
    let id = a_published_item(&app, "microcreator", "micro-item", "3.00").await;

    let resp = app.get(&format!("/api/marketplace/items/{id}")).await;
    let body: Value = resp.json().await.unwrap();
    // 20% below twenty euros: the cost of handling a sale barely moves with
    // its size.
    assert_eq!(
        body["data"]["platform_commission"].as_str().unwrap(),
        "0.60"
    );
    assert_eq!(body["data"]["creator_receives"].as_str().unwrap(), "2.40");
}

#[tokio::test]
async fn a_creator_cannot_buy_their_own_item() {
    let app = TestApp::spawn().await;
    a_talent(&app, "selfcreator").await;
    let id = a_published_item(&app, "selfcreator", "self-item", "30.00").await;

    // It would inflate the two numbers a buyer actually reads.
    let resp = app
        .post(&format!("/api/marketplace/items/{id}/purchase"), &json!({}))
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn a_sale_splits_exactly_and_pays_the_creator() {
    let app = TestApp::spawn().await;
    let creator = a_talent(&app, "salecreator").await;
    let id = a_published_item(&app, "salecreator", "sale-item", "99.99").await;

    a_talent(&app, "salebuyer").await;
    app.login("salebuyer").await;
    let resp = app
        .post(&format!("/api/marketplace/items/{id}/purchase"), &json!({}))
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    let split: (
        sqlx::types::BigDecimal,
        sqlx::types::BigDecimal,
        sqlx::types::BigDecimal,
    ) = sqlx::query_as(
        "SELECT amount_paid, commission_amount, creator_payout
           FROM marketplace_purchases WHERE item_id = $1",
    )
    .bind(id)
    .fetch_one(&app.db)
    .await
    .unwrap();

    // 15% of 99.99 is 14.9985, rounded down to 14.99; the creator takes the
    // rest, and the two add back exactly.
    common::assert_decimal(&split.0, "99.99");
    common::assert_decimal(&split.1, "14.99");
    common::assert_decimal(&split.2, "85.00");

    let booked: sqlx::types::BigDecimal = sqlx::query_scalar(
        "SELECT amount_credits FROM platform_revenues
          WHERE source = 'marketplace_creators_commission' AND related_talent_id = $1",
    )
    .bind(creator)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(booked.to_string(), "14.99");
}

#[tokio::test]
async fn a_download_link_expires() {
    let app = TestApp::spawn().await;
    a_talent(&app, "dlcreator").await;
    let id = a_published_item(&app, "dlcreator", "dl-item", "30.00").await;

    a_talent(&app, "dlbuyer").await;
    app.login("dlbuyer").await;
    let resp = app
        .post(&format!("/api/marketplace/items/{id}/purchase"), &json!({}))
        .await;
    let body: Value = resp.json().await.unwrap();
    let url = body["data"]["download_url"].as_str().unwrap().to_string();

    let resp = app.get(&url).await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let files: Value = resp.json().await.unwrap();
    assert_eq!(files["data"]["files"], json!(["items/skeleton.zip"]));

    // A permanent link posted once is the whole catalogue given away.
    sqlx::query("UPDATE marketplace_purchases SET token_expires_at = NOW() - INTERVAL '1 hour'")
        .execute(&app.db)
        .await
        .unwrap();

    let resp = app.get(&url).await;
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn only_a_buyer_can_rate_and_the_average_follows() {
    let app = TestApp::spawn().await;
    a_talent(&app, "ratecreator").await;
    let id = a_published_item(&app, "ratecreator", "rate-item", "30.00").await;

    a_talent(&app, "ratebuyer").await;
    app.login("ratebuyer").await;
    let resp = app
        .post(&format!("/api/marketplace/items/{id}/purchase"), &json!({}))
        .await;
    let body: Value = resp.json().await.unwrap();
    let purchase = body["data"]["purchase_id"].as_str().unwrap().to_string();

    // Somebody who bought nothing cannot rate — a rating anybody can leave
    // is a rating a competitor can leave.
    a_talent(&app, "ratestranger").await;
    app.login("ratestranger").await;
    let resp = app
        .post(
            &format!("/api/marketplace/purchases/{purchase}/rate"),
            &json!({ "rating": 1 }),
        )
        .await;
    assert_eq!(resp.status(), 404);

    app.login("ratebuyer").await;
    let resp = app
        .post(
            &format!("/api/marketplace/purchases/{purchase}/rate"),
            &json!({ "rating": 4, "review": "Fait le travail." }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    // The average is generated from the parts, so they cannot disagree.
    let item: (Option<sqlx::types::BigDecimal>, i32) =
        sqlx::query_as("SELECT rating_avg, rating_count FROM marketplace_items WHERE id = $1")
            .bind(id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(item.0.unwrap().to_string(), "4.00");
    assert_eq!(item.1, 1);

    // Changing the rating moves the average rather than adding a second one.
    app.post(
        &format!("/api/marketplace/purchases/{purchase}/rate"),
        &json!({ "rating": 2 }),
    )
    .await;

    let item: (Option<sqlx::types::BigDecimal>, i32) =
        sqlx::query_as("SELECT rating_avg, rating_count FROM marketplace_items WHERE id = $1")
            .bind(id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(item.0.unwrap().to_string(), "2.00");
    assert_eq!(item.1, 1);
}

// ═══════════════════════════════════════════════════════════════════
// Academy cohorts
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn nobody_is_interviewed_out_of_a_cohort_before_they_finish_it() {
    let app = TestApp::spawn().await;
    let owner = a_talent(&app, "academyowner").await;
    let trainee = a_talent(&app, "academytrainee").await;
    let enterprise = an_enterprise(&app, owner, "Academie SA").await;

    let cohort: Uuid = sqlx::query_scalar(
        "INSERT INTO academy_cohorts
            (sponsoring_enterprise_id, name, brief_md, skill_domain, cohort_size,
             duration_weeks, sponsorship_fee)
         VALUES ($1, 'Promotion 1', 'Douze semaines.', 'code', 20, 12, 80000.00)
         RETURNING id",
    )
    .bind(enterprise)
    .fetch_one(&app.db)
    .await
    .unwrap();

    sqlx::query("INSERT INTO academy_cohort_members (cohort_id, user_id) VALUES ($1, $2)")
        .bind(cohort)
        .bind(trainee)
        .execute(&app.db)
        .await
        .unwrap();

    // The sponsor bought a trained cohort, not first refusal on people
    // halfway through the course.
    let early = sqlx::query(
        "UPDATE academy_cohort_members SET interviewed_at = NOW()
          WHERE cohort_id = $1 AND user_id = $2",
    )
    .bind(cohort)
    .bind(trainee)
    .execute(&app.db)
    .await;
    assert!(early.is_err());

    sqlx::query(
        "UPDATE academy_cohort_members SET graduated_at = NOW(), status = 'graduated'
          WHERE cohort_id = $1 AND user_id = $2",
    )
    .bind(cohort)
    .bind(trainee)
    .execute(&app.db)
    .await
    .unwrap();

    let now_ok = sqlx::query(
        "UPDATE academy_cohort_members SET interviewed_at = NOW()
          WHERE cohort_id = $1 AND user_id = $2",
    )
    .bind(cohort)
    .bind(trainee)
    .execute(&app.db)
    .await;
    assert!(now_ok.is_ok());
}

#[tokio::test]
async fn a_success_fee_cannot_be_charged_without_a_hire() {
    let app = TestApp::spawn().await;
    let owner = a_talent(&app, "feeowner").await;
    let trainee = a_talent(&app, "feetrainee").await;
    let enterprise = an_enterprise(&app, owner, "Frais SA").await;

    let cohort: Uuid = sqlx::query_scalar(
        "INSERT INTO academy_cohorts
            (sponsoring_enterprise_id, name, brief_md, skill_domain, cohort_size,
             duration_weeks, sponsorship_fee, success_fee_per_hire)
         VALUES ($1, 'Promotion 2', 'Brief.', 'code', 10, 8, 40000.00, 5000.00)
         RETURNING id",
    )
    .bind(enterprise)
    .fetch_one(&app.db)
    .await
    .unwrap();

    sqlx::query("INSERT INTO academy_cohort_members (cohort_id, user_id) VALUES ($1, $2)")
        .bind(cohort)
        .bind(trainee)
        .execute(&app.db)
        .await
        .unwrap();

    let forced = sqlx::query(
        "UPDATE academy_cohort_members SET success_fee_charged = 5000.00
          WHERE cohort_id = $1 AND user_id = $2",
    )
    .bind(cohort)
    .bind(trainee)
    .execute(&app.db)
    .await;
    assert!(forced.is_err());
}
