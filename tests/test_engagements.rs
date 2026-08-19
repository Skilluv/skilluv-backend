//! Teams Skilluv sells: studios, engagements, milestones, beta programmes.
//!
//! The tests worth having here are the ones about money and consent. A
//! milestone that pays without being reviewed, a set of shares that quietly
//! underpays everybody, a person put on paid work without agreeing — each is
//! silent in production and obvious in a test.

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

fn a_brief(title: &str) -> Value {
    json!({
        "kind": "outsourcing",
        "title": title,
        "brief_md": "Reprendre le backend et le livrer en trois jalons.",
        "orientations_required": ["web-backend-developer"],
        "team_size_max": 3,
        "duration_weeks": 8,
        "pricing_model": "fixed_price",
        "budget": "30000.00",
    })
}

async fn open_engagement(app: &TestApp, body: &Value) -> Uuid {
    let resp = app.post("/api/enterprise/engagements", body).await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let created: Value = resp.json().await.unwrap();
    created["data"]["engagement"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

// ═══════════════════════════════════════════════════════════════════
// The brief
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn a_brief_that_prices_itself_two_ways_is_refused() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Twopricesco").await;

    // A retainer with no monthly figure is a contract nobody can invoice
    // against, and the disagreement only surfaces at the first bill.
    let mut body = a_brief("Contradiction");
    body["pricing_model"] = json!("retainer_monthly");
    let resp = app.post("/api/enterprise/engagements", &body).await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn a_fractional_placement_is_one_person_for_part_of_a_week() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Fractionalco").await;

    let mut body = a_brief("Un CTO deux jours");
    body["kind"] = json!("fractional");
    body["pricing_model"] = json!("day_rate");
    body["day_rate"] = json!("600.00");
    body["budget"] = Value::Null;

    // Three people at two days each is not a fractional placement; it is a
    // team, and pricing it as one hides what the client is buying.
    let resp = app.post("/api/enterprise/engagements", &body).await;
    assert_eq!(resp.status(), 400);

    body["team_size_max"] = json!(1);
    body["days_per_week"] = json!("2.0");
    let resp = app.post("/api/enterprise/engagements", &body).await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn a_discovery_cannot_run_forever() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Discoveryco").await;

    // The timebox is the product: an open-ended exploration becomes an
    // open-ended bill.
    let mut body = a_brief("Cadrage sans fin");
    body["kind"] = json!("discovery");
    body["duration_weeks"] = json!(26);
    let resp = app.post("/api/enterprise/engagements", &body).await;
    assert_eq!(resp.status(), 400);

    body["duration_weeks"] = json!(4);
    let resp = app.post("/api/enterprise/engagements", &body).await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn a_brief_targeting_a_typo_is_refused() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Typoengageco").await;

    let mut body = a_brief("Typo");
    body["orientations_required"] = json!(["metier-invente"]);
    let resp = app.post("/api/enterprise/engagements", &body).await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn an_engagement_registers_as_an_enterprise_product() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Registerco").await;
    let id = open_engagement(&app, &a_brief("Enregistré")).await;

    // Otherwise the company's engagement list shows recruitment and
    // subscriptions but not the work, which is the largest line on it.
    let product: (String, Option<sqlx::types::BigDecimal>) = sqlx::query_as(
        "SELECT product_type, contract_value FROM enterprise_products
          WHERE source_table = 'team_engagements' AND source_id = $1",
    )
    .bind(id)
    .fetch_one(&app.db)
    .await
    .unwrap();

    assert_eq!(product.0, "outsourcing_project");
    assert_eq!(
        product.1.unwrap().to_string(),
        "30000.00",
        "the contract value should follow the pricing model"
    );
}

#[tokio::test]
async fn a_studio_that_is_still_forming_cannot_be_booked() {
    let app = TestApp::spawn().await;
    an_admin(&app, "studioadmin").await;

    let resp = app
        .post(
            "/api/admin/studios",
            &json!({
                "slug": "forge-backend",
                "name": "Forge Backend",
                "specialization": "APIs Rust et Postgres",
                "day_rate": "900.00",
            }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let created: Value = resp.json().await.unwrap();
    let studio_id = created["data"]["studio"]["id"].as_str().unwrap();

    an_enterprise(&app, "Bookearlyco").await;
    let mut body = a_brief("Trop tôt");
    body["studio_id"] = json!(studio_id);

    // Booking a forming studio is booking people who have not been recruited.
    let resp = app.post("/api/enterprise/engagements", &body).await;
    assert_eq!(resp.status(), 400);
}

// ═══════════════════════════════════════════════════════════════════
// Studios
// ═══════════════════════════════════════════════════════════════════

async fn a_studio(app: &TestApp, slug: &str) -> Uuid {
    let resp = app
        .post(
            "/api/admin/studios",
            &json!({
                "slug": slug,
                "name": slug,
                "specialization": "Backend",
                "day_rate": "900.00",
            }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let created: Value = resp.json().await.unwrap();
    created["data"]["studio"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

async fn add_studio_member(app: &TestApp, studio: Uuid, user: Uuid, share: &str) {
    let resp = app
        .post(
            &format!("/api/admin/studios/{studio}/members"),
            &json!({ "user_id": user, "role": "Développeuse", "share_percent": share }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn a_studio_whose_shares_do_not_total_a_hundred_cannot_open() {
    let app = TestApp::spawn().await;
    an_admin(&app, "shareadmin").await;
    let studio = a_studio(&app, "shares-studio").await;

    let a = a_talent(&app, "sharesa").await;
    let b = a_talent(&app, "sharesb").await;
    app.login("shareadmin").await;
    add_studio_member(&app, studio, a, "45.00").await;
    add_studio_member(&app, studio, b, "45.00").await;

    // Ninety per cent does not leave a tenth unallocated — it quietly pays
    // everybody ninety per cent of what they agreed, on every engagement the
    // studio ever takes.
    let resp = app
        .post(
            &format!("/api/admin/studios/{studio}/activate"),
            &json!({ "lead_user_id": a }),
        )
        .await;
    assert_eq!(resp.status(), 400);

    add_studio_member(&app, studio, b, "55.00").await;
    let resp = app
        .post(
            &format!("/api/admin/studios/{studio}/activate"),
            &json!({ "lead_user_id": a }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn one_person_is_a_freelancer_not_a_studio() {
    let app = TestApp::spawn().await;
    an_admin(&app, "soloadmin").await;
    let studio = a_studio(&app, "solo-studio").await;

    let a = a_talent(&app, "soloa").await;
    app.login("soloadmin").await;
    add_studio_member(&app, studio, a, "100.00").await;

    let resp = app
        .post(
            &format!("/api/admin/studios/{studio}/activate"),
            &json!({ "lead_user_id": a }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn only_active_studios_are_listed_publicly() {
    let app = TestApp::spawn().await;
    an_admin(&app, "listadmin").await;
    a_studio(&app, "hidden-studio").await;

    let resp = app.get("/api/studios").await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let listed = body["data"]["studios"].as_array().unwrap();
    assert!(
        !listed.iter().any(|s| s["slug"] == "hidden-studio"),
        "a studio still recruiting must not appear as bookable"
    );
}

#[tokio::test]
async fn a_studio_with_live_work_cannot_be_disbanded() {
    let app = TestApp::spawn().await;
    an_admin(&app, "disbandadmin").await;
    let studio = a_studio(&app, "disband-studio").await;

    let a = a_talent(&app, "disbanda").await;
    let b = a_talent(&app, "disbandb").await;
    app.login("disbandadmin").await;
    add_studio_member(&app, studio, a, "50.00").await;
    add_studio_member(&app, studio, b, "50.00").await;
    app.post(
        &format!("/api/admin/studios/{studio}/activate"),
        &json!({ "lead_user_id": a }),
    )
    .await;

    an_enterprise(&app, "Livework").await;
    let mut body = a_brief("En cours");
    body["studio_id"] = json!(studio);
    let engagement = open_engagement(&app, &body).await;
    sqlx::query("UPDATE team_engagements SET status = 'in_progress' WHERE id = $1")
        .bind(engagement)
        .execute(&app.db)
        .await
        .unwrap();

    app.login("disbandadmin").await;
    // Disbanding now would leave a client with a team that no longer exists.
    let resp = app
        .post(
            &format!("/api/admin/studios/{studio}/disband"),
            &json!({ "reason": "Les gens sont partis" }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn a_studio_disbanded_without_a_reason_is_refused() {
    let app = TestApp::spawn().await;
    an_admin(&app, "reasonadmin").await;
    let studio = a_studio(&app, "reason-studio").await;

    // People built a reputation under this name.
    let resp = app
        .post(
            &format!("/api/admin/studios/{studio}/disband"),
            &json!({ "reason": "   " }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn a_studio_carries_its_shares_onto_the_work() {
    let app = TestApp::spawn().await;
    an_admin(&app, "carryadmin").await;
    let studio = a_studio(&app, "carry-studio").await;

    let a = a_talent(&app, "carrya").await;
    let b = a_talent(&app, "carryb").await;
    app.login("carryadmin").await;
    add_studio_member(&app, studio, a, "60.00").await;
    add_studio_member(&app, studio, b, "40.00").await;
    app.post(
        &format!("/api/admin/studios/{studio}/activate"),
        &json!({ "lead_user_id": a }),
    )
    .await;

    an_enterprise(&app, "Carryco").await;
    let mut body = a_brief("Repris par le studio");
    body["studio_id"] = json!(studio);
    let engagement = open_engagement(&app, &body).await;

    app.login("carryadmin").await;
    let resp = app
        .post(
            &format!("/api/admin/engagements/{engagement}/staff-from-studio"),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    // The point of a standing team: the shares were agreed once, not
    // renegotiated on every piece of work.
    let shares: Vec<(Uuid, sqlx::types::BigDecimal)> = sqlx::query_as(
        "SELECT user_id, share_percent FROM engagement_members
          WHERE engagement_id = $1 ORDER BY share_percent DESC",
    )
    .bind(engagement)
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert_eq!(shares.len(), 2);
    assert_eq!(shares[0].0, a);
    common::assert_decimal(&shares[0].1, "60.00");
}

#[tokio::test]
async fn booking_a_studio_costs_more_than_assembling_people() {
    let app = TestApp::spawn().await;
    an_admin(&app, "marginadmin").await;
    let studio = a_studio(&app, "margin-studio").await;

    let a = a_talent(&app, "margina").await;
    let b = a_talent(&app, "marginb").await;
    app.login("marginadmin").await;
    add_studio_member(&app, studio, a, "50.00").await;
    add_studio_member(&app, studio, b, "50.00").await;
    app.post(
        &format!("/api/admin/studios/{studio}/activate"),
        &json!({ "lead_user_id": a }),
    )
    .await;

    an_enterprise(&app, "Marginco").await;
    let ad_hoc = open_engagement(&app, &a_brief("Assemblé")).await;
    let mut body = a_brief("Studio");
    body["studio_id"] = json!(studio);
    let booked = open_engagement(&app, &body).await;

    let margins: Vec<sqlx::types::BigDecimal> = sqlx::query_scalar(
        "SELECT margin_percent FROM team_engagements WHERE id = ANY($1)
          ORDER BY margin_percent",
    )
    .bind(vec![ad_hoc, booked])
    .fetch_all(&app.db)
    .await
    .unwrap();

    // The client is buying a track record and management, not a list of
    // people who happened to be free.
    assert_eq!(margins.len(), 2);
    assert!(margins[0] < margins[1]);
}

// ═══════════════════════════════════════════════════════════════════
// Consent and the start line
// ═══════════════════════════════════════════════════════════════════

async fn an_engagement_with_two(app: &TestApp, company: &str, prefix: &str) -> (Uuid, Uuid, Uuid) {
    an_admin(app, &format!("{prefix}admin")).await;
    let a = a_talent(app, &format!("{prefix}a")).await;
    let b = a_talent(app, &format!("{prefix}b")).await;

    an_enterprise(app, company).await;
    let engagement = open_engagement(app, &a_brief("Deux personnes")).await;

    app.login(&format!("{prefix}admin")).await;
    for (user, share) in [(a, "60.00"), (b, "40.00")] {
        let resp = app
            .post(
                &format!("/api/admin/engagements/{engagement}/members"),
                &json!({ "user_id": user, "role": "Développeur", "share_percent": share }),
            )
            .await;
        assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    }

    (engagement, a, b)
}

#[tokio::test]
async fn nobody_is_put_on_paid_work_without_saying_yes() {
    let app = TestApp::spawn().await;
    let (engagement, _a, _b) = an_engagement_with_two(&app, "Consentco", "consent").await;

    let resp = app
        .post(
            &format!("/api/admin/engagements/{engagement}/milestones"),
            &json!({
                "title": "Tout",
                "acceptance_criteria": "Le backend tourne en production.",
                "value_percent": "100.00",
            }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    // An admin could otherwise start work on behalf of people who never
    // agreed to the share they are being paid.
    let resp = app
        .post(
            &format!("/api/admin/engagements/{engagement}/start"),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), 400);
    assert!(resp.text().await.unwrap().contains("consenta"));
}

#[tokio::test]
async fn a_changed_share_is_a_changed_offer() {
    let app = TestApp::spawn().await;
    let (engagement, a, _b) = an_engagement_with_two(&app, "Reofferco", "reoffer").await;

    app.login("reoffera").await;
    let resp = app
        .post(
            &format!("/api/engagements/{engagement}/respond"),
            &json!({ "accept": true }),
        )
        .await;
    assert_eq!(resp.status(), 200);

    app.login("reofferadmin").await;
    let resp = app
        .post(
            &format!("/api/admin/engagements/{engagement}/members"),
            &json!({ "user_id": a, "role": "Développeur", "share_percent": "20.00" }),
        )
        .await;
    assert_eq!(resp.status(), 200);

    // The agreement was to sixty per cent. It does not carry over to twenty.
    let accepted: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT accepted_at FROM engagement_members
          WHERE engagement_id = $1 AND user_id = $2",
    )
    .bind(engagement)
    .bind(a)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(accepted.is_none());
}

#[tokio::test]
async fn milestones_that_do_not_cover_the_contract_stop_the_start() {
    let app = TestApp::spawn().await;
    let (engagement, _a, _b) = an_engagement_with_two(&app, "Coverco", "cover").await;

    for name in ["covera", "coverb"] {
        app.login(name).await;
        app.post(
            &format!("/api/engagements/{engagement}/respond"),
            &json!({ "accept": true }),
        )
        .await;
    }

    app.login("coveradmin").await;
    app.post(
        &format!("/api/admin/engagements/{engagement}/milestones"),
        &json!({
            "title": "La moitié",
            "acceptance_criteria": "La moitié du travail est livrée.",
            "value_percent": "50.00",
        }),
    )
    .await;

    // The other half would have nowhere to be paid from.
    let resp = app
        .post(
            &format!("/api/admin/engagements/{engagement}/start"),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), 400);

    app.post(
        &format!("/api/admin/engagements/{engagement}/milestones"),
        &json!({
            "title": "L'autre moitié",
            "acceptance_criteria": "Le reste est livré.",
            "value_percent": "50.00",
        }),
    )
    .await;
    let resp = app
        .post(
            &format!("/api/admin/engagements/{engagement}/start"),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
}

// ═══════════════════════════════════════════════════════════════════
// The cascade
// ═══════════════════════════════════════════════════════════════════

async fn a_running_engagement(
    app: &TestApp,
    company: &str,
    prefix: &str,
) -> (Uuid, Uuid, Uuid, Uuid) {
    let (engagement, a, b) = an_engagement_with_two(app, company, prefix).await;

    for name in [format!("{prefix}a"), format!("{prefix}b")] {
        app.login(&name).await;
        app.post(
            &format!("/api/engagements/{engagement}/respond"),
            &json!({ "accept": true }),
        )
        .await;
    }

    app.login(&format!("{prefix}admin")).await;
    let resp = app
        .post(
            &format!("/api/admin/engagements/{engagement}/milestones"),
            &json!({
                "title": "Livraison",
                "acceptance_criteria": "Le backend tourne en production.",
                "value_percent": "100.00",
            }),
        )
        .await;
    let created: Value = resp.json().await.unwrap();
    let milestone: Uuid = created["data"]["milestone_id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    let resp = app
        .post(
            &format!("/api/admin/engagements/{engagement}/start"),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    (engagement, milestone, a, b)
}

#[tokio::test]
async fn nothing_reaches_the_client_unreviewed() {
    let app = TestApp::spawn().await;
    let (engagement, milestone, _a, _b) = a_running_engagement(&app, "Unreviewedco", "unrev").await;

    app.login("unrevco").await;
    // The guarantee is the product. A milestone that skipped review is one
    // the margin was charged for and not delivered.
    let resp = app
        .post(
            &format!("/api/enterprise/engagements/{engagement}/milestones/{milestone}/accept"),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn a_review_with_no_notes_is_a_signature() {
    let app = TestApp::spawn().await;
    let (_engagement, milestone, _a, _b) = a_running_engagement(&app, "Notesco", "notes").await;

    app.login("notesadmin").await;
    sqlx::query("UPDATE engagement_milestones SET status = 'in_review' WHERE id = $1")
        .bind(milestone)
        .execute(&app.db)
        .await
        .unwrap();

    let resp = app
        .post(
            &format!("/api/admin/milestones/{milestone}/review"),
            &json!({ "passed": true, "notes": "  " }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn an_accepted_milestone_pays_the_team_by_their_shares() {
    let app = TestApp::spawn().await;
    let (engagement, milestone, a, b) = a_running_engagement(&app, "Cascadeco", "cascade").await;

    app.login("cascadeadmin").await;
    sqlx::query("UPDATE engagement_milestones SET status = 'in_review' WHERE id = $1")
        .bind(milestone)
        .execute(&app.db)
        .await
        .unwrap();
    let resp = app
        .post(
            &format!("/api/admin/milestones/{milestone}/review"),
            &json!({ "passed": true, "notes": "Conforme aux critères annoncés." }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    app.login("cascadeco").await;
    let resp = app
        .post(
            &format!("/api/enterprise/engagements/{engagement}/milestones/{milestone}/accept"),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    // 30 000 at 15% margin leaves 25 500 for the team: 60/40 is 15 300 and
    // 10 200.
    let body: Value = resp.json().await.unwrap();
    let paid = body["data"]["paid"].as_array().unwrap();
    assert_eq!(paid.len(), 2);

    let amount_for = |user: Uuid| -> String {
        paid.iter()
            .find(|p| p["user_id"] == user.to_string())
            .unwrap()["amount"]
            .as_str()
            .unwrap()
            .to_string()
    };
    assert_eq!(amount_for(a), "15300.00");
    assert_eq!(amount_for(b), "10200.00");

    // And the margin is booked as its own line, with the rate that produced it.
    let revenue: (sqlx::types::BigDecimal, i32) = sqlx::query_as(
        "SELECT amount_credits, fee_rate_bps FROM platform_revenues
          WHERE source = 'outsourcing_margin' ORDER BY occurred_at DESC LIMIT 1",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    common::assert_decimal(&revenue.0, "4500.00");
    assert_eq!(revenue.1, 1500);
}

#[tokio::test]
async fn a_client_cannot_accept_somebody_elses_milestone() {
    let app = TestApp::spawn().await;
    let (engagement, milestone, _a, _b) = a_running_engagement(&app, "Ownerco", "owner").await;

    an_enterprise(&app, "Intruderco").await;
    // Not found rather than forbidden: confirming an id exists tells a
    // competitor which of their guesses is a real engagement.
    let resp = app
        .post(
            &format!("/api/enterprise/engagements/{engagement}/milestones/{milestone}/accept"),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), 404);
}

// ═══════════════════════════════════════════════════════════════════
// Beta programmes
// ═══════════════════════════════════════════════════════════════════

fn a_program(name: &str) -> Value {
    json!({
        "product_name": name,
        "brief_md": "Tester le tunnel d'inscription et signaler ce qui bloque.",
        "test_type": "usability",
        "testers_wanted": 5,
        "duration_weeks": 2,
        "tester_reward": "25.00",
        "program_fee": "500.00",
    })
}

async fn open_program(app: &TestApp, body: &Value) -> Uuid {
    let resp = app.post("/api/enterprise/beta-programs", body).await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let created: Value = resp.json().await.unwrap();
    created["data"]["program"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

#[tokio::test]
async fn an_unpaid_beta_is_not_brokered_as_work() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Unpaidbetaco").await;

    let mut body = a_program("Gratuit");
    body["tester_reward"] = json!("0.00");
    let resp = app.post("/api/enterprise/beta-programs", &body).await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn the_quote_is_the_maximum_the_client_can_be_billed() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Quoteco").await;

    let resp = app
        .post("/api/enterprise/beta-programs", &a_program("Devis"))
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();

    // Five testers at 25 plus a 500 fee. Quoted at the maximum, because a
    // client who budgets for the average and is billed for the maximum has
    // been misled by arithmetic.
    common::assert_amount(&body["data"]["quoted_maximum"], "625.00");
}

#[tokio::test]
async fn a_programme_stops_taking_testers_when_it_is_full() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Fullbetaco").await;

    let mut body = a_program("Complet");
    body["testers_wanted"] = json!(5);
    let program = open_program(&app, &body).await;

    for i in 0..5 {
        let name = format!("betatester{i}");
        a_talent(&app, &name).await;
        app.login(&name).await;
        let resp = app
            .post(&format!("/api/beta-programs/{program}/join"), &json!({}))
            .await;
        assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    }

    a_talent(&app, "betalate").await;
    app.login("betalate").await;
    let resp = app
        .post(&format!("/api/beta-programs/{program}/join"), &json!({}))
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn feedback_too_short_to_report_on_is_refused() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Shortbetaco").await;
    let program = open_program(&app, &a_program("Court")).await;

    a_talent(&app, "shortbeta").await;
    app.login("shortbeta").await;
    app.post(&format!("/api/beta-programs/{program}/join"), &json!({}))
        .await;

    // Below the floor there is nothing for the report to be built from, and
    // the reward would be paid for nothing.
    let resp = app
        .post(
            &format!("/api/beta-programs/{program}/feedback"),
            &json!({ "feedback_md": "ça marche" }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn refusing_feedback_carries_a_reason_and_accepting_pays() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Verdictco").await;
    let program = open_program(&app, &a_program("Verdict")).await;

    let tester = a_talent(&app, "verdicttester").await;
    app.login("verdicttester").await;
    app.post(&format!("/api/beta-programs/{program}/join"), &json!({}))
        .await;
    let resp = app
        .post(
            &format!("/api/beta-programs/{program}/feedback"),
            &json!({
                "feedback_md": "Le tunnel bloque à l'étape trois : le bouton \
                                de validation reste désactivé sans message.",
            }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    app.login("verdictco").await;
    // Somebody spent hours on this. A refusal without a reason is how a
    // programme loses the testers it took weeks to recruit.
    let resp = app
        .post(
            &format!("/api/enterprise/beta-programs/{program}/testers/{tester}/review"),
            &json!({ "accept": false }),
        )
        .await;
    assert_eq!(resp.status(), 400);

    let resp = app
        .post(
            &format!("/api/enterprise/beta-programs/{program}/testers/{tester}/review"),
            &json!({ "accept": true }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    let paid: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT reward_paid_at FROM beta_testers WHERE program_id = $1 AND user_id = $2",
    )
    .bind(program)
    .bind(tester)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(paid.is_some());
}

#[tokio::test]
async fn a_programme_with_testers_still_waiting_cannot_close() {
    let app = TestApp::spawn().await;
    an_admin(&app, "closeadmin").await;
    an_enterprise(&app, "Closebetaco").await;
    let program = open_program(&app, &a_program("Fermeture")).await;

    a_talent(&app, "closetester").await;
    app.login("closetester").await;
    app.post(&format!("/api/beta-programs/{program}/join"), &json!({}))
        .await;
    app.post(
        &format!("/api/beta-programs/{program}/feedback"),
        &json!({
            "feedback_md": "Rien à signaler sur le tunnel, mais la page de \
                            confirmation met dix secondes à s'afficher.",
        }),
    )
    .await;

    app.login("closeadmin").await;
    // Closing now would leave them unpaid with no way to ask why.
    let resp = app
        .post(
            &format!("/api/admin/beta-programs/{program}/close"),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn the_program_fee_is_booked_at_closing_not_at_opening() {
    let app = TestApp::spawn().await;
    an_admin(&app, "feeadmin").await;
    an_enterprise(&app, "Feebetaco").await;
    let program = open_program(&app, &a_program("Frais")).await;

    // The fee is earned by delivering the report. A programme cancelled in
    // its first week has earned none of it.
    let before: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM platform_revenues WHERE source = 'beta_program_fee'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(before, 0);

    app.login("feeadmin").await;
    let resp = app
        .post(
            &format!("/api/admin/beta-programs/{program}/close"),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    let booked: sqlx::types::BigDecimal = sqlx::query_scalar(
        "SELECT amount_credits FROM platform_revenues WHERE source = 'beta_program_fee'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    common::assert_decimal(&booked, "500.00");
}
