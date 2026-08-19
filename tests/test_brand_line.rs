//! The brand line: events, sponsors, campaigns, ambassadors, the audience.
//!
//! Everything here sells access to a community that did not sign up to be
//! sold to, so the tests worth having are the ones about the restraints:
//! consent before a name reaches a sponsor, Skilluv's gate before a company
//! judges a piece, the person's own answer before their name is lent out.

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

/// An event created straight in the database: the admin route is covered by
/// its own test file, and every test here needs one to hang things off.
async fn an_event(app: &TestApp, slug: &str, status: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO events (slug, name, description, starts_at, event_type, status)
         VALUES ($1, $1, '', NOW() + INTERVAL '7 days', 'hackathon', $2)
         RETURNING id",
    )
    .bind(slug)
    .bind(status)
    .fetch_one(&app.db)
    .await
    .unwrap()
}

// ═══════════════════════════════════════════════════════════════════
// Events
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn a_draft_event_is_on_no_public_list() {
    let app = TestApp::spawn().await;
    an_event(&app, "secret-jam", "draft").await;

    let resp = app.get("/api/events").await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert!(
        !body["data"]["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["slug"] == "secret-jam")
    );

    // And not by its own URL either, or the draft is public to anyone who
    // guesses the name.
    let resp = app.get("/api/events/secret-jam").await;
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn the_active_flag_follows_the_status_and_cannot_be_set_against_it() {
    let app = TestApp::spawn().await;
    let id = an_event(&app, "derived-flag", "published").await;

    let active: bool = sqlx::query_scalar("SELECT is_active FROM events WHERE id = $1")
        .bind(id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert!(active);

    sqlx::query("UPDATE events SET status = 'finished' WHERE id = $1")
        .bind(id)
        .execute(&app.db)
        .await
        .unwrap();

    let active: bool = sqlx::query_scalar("SELECT is_active FROM events WHERE id = $1")
        .bind(id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert!(!active, "a finished event is not active");

    // Writing it directly is refused by the database: it is derived, so the
    // two can no longer disagree.
    let direct = sqlx::query("UPDATE events SET is_active = TRUE WHERE id = $1")
        .bind(id)
        .execute(&app.db)
        .await;
    assert!(direct.is_err());
}

#[tokio::test]
async fn a_jury_seat_cannot_be_claimed_only_given() {
    let app = TestApp::spawn().await;
    an_event(&app, "jury-event", "published").await;
    a_talent(&app, "wouldbejuror").await;
    app.login("wouldbejuror").await;

    // A jury somebody can join is a jury whose verdict means nothing.
    let resp = app
        .post("/api/events/jury-event/join", &json!({ "role": "jury" }))
        .await;
    assert_eq!(resp.status(), 400);

    let resp = app.post("/api/events/jury-event/join", &json!({})).await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn an_event_stops_taking_people_when_it_is_full() {
    let app = TestApp::spawn().await;
    let id = an_event(&app, "small-event", "published").await;
    sqlx::query("UPDATE events SET max_participants = 2 WHERE id = $1")
        .bind(id)
        .execute(&app.db)
        .await
        .unwrap();

    for i in 0..2 {
        let name = format!("seat{i}");
        a_talent(&app, &name).await;
        app.login(&name).await;
        let resp = app.post("/api/events/small-event/join", &json!({})).await;
        assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    }

    a_talent(&app, "seatlate").await;
    app.login("seatlate").await;
    let resp = app.post("/api/events/small-event/join", &json!({})).await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn a_jury_appointment_does_not_take_a_participant_seat() {
    let app = TestApp::spawn().await;
    let id = an_event(&app, "jury-seats", "published").await;
    sqlx::query("UPDATE events SET max_participants = 1 WHERE id = $1")
        .bind(id)
        .execute(&app.db)
        .await
        .unwrap();

    let juror = a_talent(&app, "realjuror").await;
    an_admin(&app, "juryadmin").await;
    let resp = app
        .post(
            &format!("/api/admin/events/{id}/appoint"),
            &json!({ "user_id": juror, "role": "jury" }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    // A jury is invited, not admitted. Counting them would close
    // registration early.
    a_talent(&app, "juryseatuser").await;
    app.login("juryseatuser").await;
    let resp = app.post("/api/events/jury-seats/join", &json!({})).await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
}

// ═══════════════════════════════════════════════════════════════════
// Sponsorship
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn the_grid_is_published_and_ordered() {
    let app = TestApp::spawn().await;
    let resp = app.get("/api/sponsorship/packages").await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let tiers: Vec<&str> = body["data"]["packages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["tier"].as_str().unwrap())
        .collect();
    assert_eq!(
        tiers,
        vec!["bronze", "silver", "gold", "platinum", "custom"]
    );
}

async fn a_sponsorship(app: &TestApp, event_id: Uuid, tier: &str) -> Uuid {
    let resp = app
        .post(
            "/api/enterprise/sponsorships",
            &json!({ "event_id": event_id, "package_tier": tier }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let created: Value = resp.json().await.unwrap();
    created["data"]["sponsorship"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

#[tokio::test]
async fn a_sponsorship_takes_the_published_price_unless_told_otherwise() {
    let app = TestApp::spawn().await;
    let event = an_event(&app, "priced-event", "published").await;
    an_enterprise(&app, "Pricedco").await;
    let id = a_sponsorship(&app, event, "gold").await;

    let fee: sqlx::types::BigDecimal =
        sqlx::query_scalar("SELECT agreed_fee FROM event_sponsorships WHERE id = $1")
            .bind(id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    common::assert_decimal(&fee, "3800.00");
}

#[tokio::test]
async fn a_negotiated_price_does_not_rewrite_the_grid() {
    let app = TestApp::spawn().await;
    let event = an_event(&app, "discount-event", "published").await;
    an_enterprise(&app, "Discountco").await;

    let resp = app
        .post(
            "/api/enterprise/sponsorships",
            &json!({
                "event_id": event, "package_tier": "gold",
                "agreed_fee": "2500.00",
            }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    // Discounting by editing the grid would rewrite history for every other
    // sponsor at the same tier.
    let list: sqlx::types::BigDecimal =
        sqlx::query_scalar("SELECT list_fee FROM event_sponsorship_packages WHERE tier = 'gold'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    common::assert_decimal(&list, "3800.00");
}

#[tokio::test]
async fn a_custom_package_has_to_say_what_it_costs() {
    let app = TestApp::spawn().await;
    let event = an_event(&app, "custom-event", "published").await;
    an_enterprise(&app, "Customco").await;

    let resp = app
        .post(
            "/api/enterprise/sponsorships",
            &json!({ "event_id": event, "package_tier": "custom" }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn a_tier_cannot_claim_a_benefit_it_does_not_include() {
    let app = TestApp::spawn().await;
    let event = an_event(&app, "benefit-event", "published").await;
    an_enterprise(&app, "Benefitco").await;

    // Bronze does not come with a named challenge, and accepting it here
    // would mean a promise nobody checks against what was paid.
    let resp = app
        .post(
            "/api/enterprise/sponsorships",
            &json!({
                "event_id": event, "package_tier": "bronze",
                "named_challenge_slug": "bronze-branded",
            }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn one_company_sponsors_an_event_once() {
    let app = TestApp::spawn().await;
    let event = an_event(&app, "double-event", "published").await;
    an_enterprise(&app, "Doubleco").await;
    a_sponsorship(&app, event, "bronze").await;

    let resp = app
        .post(
            "/api/enterprise/sponsorships",
            &json!({ "event_id": event, "package_tier": "silver" }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn signing_grants_the_credits_the_tier_promised() {
    let app = TestApp::spawn().await;
    let event = an_event(&app, "credits-event", "published").await;
    an_admin(&app, "creditsadmin").await;
    an_enterprise(&app, "Creditsco").await;
    let id = a_sponsorship(&app, event, "silver").await;

    app.login("creditsadmin").await;
    let resp = app
        .post(&format!("/api/admin/sponsorships/{id}/sign"), &json!({}))
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    // Through the entitlement machinery, not a counter: there is already one
    // place that answers what a company has the right to do.
    let granted: sqlx::types::BigDecimal = sqlx::query_scalar(
        "SELECT e.granted FROM enterprise_entitlements e
           JOIN enterprise_products p ON p.id = e.product_id
          WHERE p.source_table = 'event_sponsorships' AND p.source_id = $1
            AND e.kind = 'credits'",
    )
    .bind(id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    common::assert_decimal(&granted, "20.00");
}

#[tokio::test]
async fn revenue_is_booked_when_the_event_happened_not_when_it_was_signed() {
    let app = TestApp::spawn().await;
    let event = an_event(&app, "booked-event", "published").await;
    an_admin(&app, "bookedadmin").await;
    an_enterprise(&app, "Bookedco").await;
    let id = a_sponsorship(&app, event, "bronze").await;

    app.login("bookedadmin").await;
    app.post(&format!("/api/admin/sponsorships/{id}/sign"), &json!({}))
        .await;

    // A sponsorship signed for an event later cancelled has earned nothing.
    let before: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM platform_revenues WHERE source = 'event_sponsorship'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(before, 0);

    let resp = app
        .post(&format!("/api/admin/sponsorships/{id}/honour"), &json!({}))
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    let booked: sqlx::types::BigDecimal = sqlx::query_scalar(
        "SELECT amount_credits FROM platform_revenues WHERE source = 'event_sponsorship'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    common::assert_decimal(&booked, "460.00");
}

#[tokio::test]
async fn a_contract_cannot_cover_more_events_than_it_bought() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Annualco").await;

    let resp = app
        .post(
            "/api/enterprise/annual-sponsorships",
            &json!({
                "year": 2026, "total_fee": "20000.00", "max_events": 2,
                "volume_discount_percent": "15.00",
                "contract_url": "https://example.test/contract.pdf",
            }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let created: Value = resp.json().await.unwrap();
    let contract = created["data"]["contract"]["id"].as_str().unwrap();

    for i in 0..2 {
        let event = an_event(&app, &format!("annual-{i}"), "published").await;
        let resp = app
            .post(
                "/api/enterprise/sponsorships",
                &json!({
                    "event_id": event, "package_tier": "bronze",
                    "annual_contract_id": contract,
                }),
            )
            .await;
        assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    }

    let event = an_event(&app, "annual-third", "published").await;
    let resp = app
        .post(
            "/api/enterprise/sponsorships",
            &json!({
                "event_id": event, "package_tier": "bronze",
                "annual_contract_id": contract,
            }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn an_unsigned_contract_gets_no_discount() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Unsignedco").await;

    // The discount is what the company is paid for committing, so it is only
    // real once the commitment is.
    let resp = app
        .post(
            "/api/enterprise/annual-sponsorships",
            &json!({
                "year": 2026, "total_fee": "20000.00", "max_events": 3,
                "volume_discount_percent": "20.00",
            }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

// ═══════════════════════════════════════════════════════════════════
// Leads
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn a_sponsor_sees_the_visitors_who_consented_and_a_count_of_the_rest() {
    let app = TestApp::spawn().await;
    let event = an_event(&app, "stand-event", "published").await;
    an_enterprise(&app, "Standco").await;
    let sponsorship = a_sponsorship(&app, event, "gold").await;

    a_talent(&app, "willingvisitor").await;
    app.login("willingvisitor").await;
    let resp = app
        .post(
            &format!("/api/events/{event}/stands/{sponsorship}"),
            &json!({ "interaction": "demo_booked", "contact_consent": true }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    a_talent(&app, "shyvisitor").await;
    app.login("shyvisitor").await;
    app.post(
        &format!("/api/events/{event}/stands/{sponsorship}"),
        &json!({ "interaction": "stand_visit", "contact_consent": false }),
    )
    .await;

    app.login("standco").await;
    let resp = app
        .get(&format!("/api/enterprise/sponsorships/{sponsorship}/leads"))
        .await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();

    let leads = body["data"]["leads"].as_array().unwrap();
    assert_eq!(leads.len(), 1);
    assert_eq!(leads[0]["username"], "willingvisitor");
    // The stand worked, and the sponsor learns that without learning who.
    assert_eq!(body["data"]["visitors_without_consent"], 1);
}

#[tokio::test]
async fn withdrawing_consent_takes_the_name_back() {
    let app = TestApp::spawn().await;
    let event = an_event(&app, "withdraw-event", "published").await;
    an_enterprise(&app, "Withdrawco").await;
    let sponsorship = a_sponsorship(&app, event, "gold").await;

    a_talent(&app, "changedmind").await;
    app.login("changedmind").await;
    app.post(
        &format!("/api/events/{event}/stands/{sponsorship}"),
        &json!({ "interaction": "cv_shared", "contact_consent": true }),
    )
    .await;
    // Consent can be withdrawn as easily as it was given, and the later
    // answer is the one that counts.
    app.post(
        &format!("/api/events/{event}/stands/{sponsorship}"),
        &json!({ "interaction": "cv_shared", "contact_consent": false }),
    )
    .await;

    app.login("withdrawco").await;
    let resp = app
        .get(&format!("/api/enterprise/sponsorships/{sponsorship}/leads"))
        .await;
    let body: Value = resp.json().await.unwrap();
    assert!(body["data"]["leads"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn nothing_without_consent_can_be_marked_exported() {
    let app = TestApp::spawn().await;
    let event = an_event(&app, "export-event", "published").await;
    an_enterprise(&app, "Exportco").await;
    let sponsorship = a_sponsorship(&app, event, "gold").await;

    let user = a_talent(&app, "noconsent").await;
    sqlx::query(
        "INSERT INTO sponsorship_leads (sponsorship_id, user_id, interaction, contact_consent)
         VALUES ($1, $2, 'stand_visit', FALSE)",
    )
    .bind(sponsorship)
    .bind(user)
    .execute(&app.db)
    .await
    .unwrap();

    // The database refuses it outright — this is the constraint the whole
    // lead model rests on.
    let forced =
        sqlx::query("UPDATE sponsorship_leads SET exported_at = NOW() WHERE sponsorship_id = $1")
            .bind(sponsorship)
            .execute(&app.db)
            .await;
    assert!(forced.is_err());
}

// ═══════════════════════════════════════════════════════════════════
// Launch campaigns
// ═══════════════════════════════════════════════════════════════════

fn a_campaign_body() -> Value {
    json!({
        "product_name": "Widget 3",
        "brief_md": "Nous sortons Widget 3. Écrivez ce que vous en pensez vraiment.",
        "product_launch_date": "2026-09-01",
        "starts_at": "2026-08-01T00:00:00Z",
        "ends_at": "2026-09-30T00:00:00Z",
        "content_types_wanted": ["blog_post", "video"],
        "reward_pool": "600.00",
        "reward_per_piece": "200.00",
        "campaign_fee": "3000.00",
    })
}

async fn an_open_campaign(app: &TestApp, prefix: &str) -> Uuid {
    let resp = app
        .post("/api/enterprise/launch-campaigns", &a_campaign_body())
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let created: Value = resp.json().await.unwrap();
    let id: Uuid = created["data"]["campaign"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    app.login(&format!("{prefix}admin")).await;
    let resp = app
        .post(
            &format!("/api/admin/launch-campaigns/{id}/open"),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    id
}

#[tokio::test]
async fn a_pot_too_small_for_one_piece_is_refused() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Smallpotco").await;

    let mut body = a_campaign_body();
    body["reward_pool"] = json!("100.00");
    body["reward_per_piece"] = json!("200.00");
    // The person who finds out otherwise is the one who already wrote the
    // article.
    let resp = app.post("/api/enterprise/launch-campaigns", &body).await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn a_writer_is_told_the_pot_is_empty_before_writing_not_after() {
    let app = TestApp::spawn().await;
    an_admin(&app, "potadmin").await;
    an_enterprise(&app, "Potco").await;
    let campaign = an_open_campaign(&app, "pot").await;

    // Three pieces at 200 out of a 600 pot: after three accepted, nothing is
    // payable.
    let author = a_talent(&app, "potwriter").await;
    for i in 0..3 {
        sqlx::query(
            "INSERT INTO launch_campaign_pieces
                (campaign_id, author_user_id, content_type, title, url, status,
                 quality_reviewed_at)
             VALUES ($1, $2, 'blog_post', 'x', $3, 'accepted', NOW())",
        )
        .bind(campaign)
        .bind(author)
        .bind(format!("https://example.test/{i}"))
        .execute(&app.db)
        .await
        .unwrap();
    }

    a_talent(&app, "latewriter").await;
    app.login("latewriter").await;
    let resp = app
        .post(
            &format!("/api/launch-campaigns/{campaign}/pieces"),
            &json!({
                "content_type": "blog_post", "title": "Trop tard",
                "url": "https://example.test/late",
            }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn the_sponsor_sees_nothing_skilluv_has_not_checked() {
    let app = TestApp::spawn().await;
    an_admin(&app, "gateadmin").await;
    an_enterprise(&app, "Gateco").await;
    let campaign = an_open_campaign(&app, "gate").await;

    a_talent(&app, "gatewriter").await;
    app.login("gatewriter").await;
    let resp = app
        .post(
            &format!("/api/launch-campaigns/{campaign}/pieces"),
            &json!({
                "content_type": "blog_post", "title": "Un avis",
                "url": "https://example.test/avis",
            }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let created: Value = resp.json().await.unwrap();
    let piece: Uuid = created["data"]["piece_id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    app.login("gateco").await;
    let resp = app
        .get(&format!(
            "/api/enterprise/launch-campaigns/{campaign}/pieces"
        ))
        .await;
    let body: Value = resp.json().await.unwrap();
    assert!(
        body["data"]["pieces"].as_array().unwrap().is_empty(),
        "an unchecked piece must not reach the sponsor"
    );

    // And the sponsor cannot decide on it either — without the first gate, a
    // company could reject honest criticism as poor quality.
    let resp = app
        .post(
            &format!("/api/enterprise/launch-pieces/{piece}/decide"),
            &json!({ "accept": false, "reason": "pas assez élogieux" }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn a_refusal_after_the_quality_gate_owes_the_author_a_reason() {
    let app = TestApp::spawn().await;
    an_admin(&app, "reasonbrandadmin").await;
    an_enterprise(&app, "Reasonbrandco").await;
    let campaign = an_open_campaign(&app, "reasonbrand").await;

    a_talent(&app, "reasonwriter").await;
    app.login("reasonwriter").await;
    let resp = app
        .post(
            &format!("/api/launch-campaigns/{campaign}/pieces"),
            &json!({
                "content_type": "video", "title": "Une vidéo",
                "url": "https://example.test/video",
            }),
        )
        .await;
    let created: Value = resp.json().await.unwrap();
    let piece: Uuid = created["data"]["piece_id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    app.login("reasonbrandadmin").await;
    let resp = app
        .post(
            &format!("/api/admin/launch-pieces/{piece}/quality"),
            &json!({ "passed": true, "notes": "Travail réel, sourcé, publié." }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    app.login("reasonbrandco").await;
    let resp = app
        .post(
            &format!("/api/enterprise/launch-pieces/{piece}/decide"),
            &json!({ "accept": false }),
        )
        .await;
    assert_eq!(resp.status(), 400);

    let resp = app
        .post(
            &format!("/api/enterprise/launch-pieces/{piece}/decide"),
            &json!({ "accept": true }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    common::assert_amount(&body["data"]["reward_paid"], "200.00");
}

#[tokio::test]
async fn a_campaign_with_pieces_still_waiting_cannot_close() {
    let app = TestApp::spawn().await;
    an_admin(&app, "closebrandadmin").await;
    an_enterprise(&app, "Closebrandco").await;
    let campaign = an_open_campaign(&app, "closebrand").await;

    a_talent(&app, "waitingwriter").await;
    app.login("waitingwriter").await;
    app.post(
        &format!("/api/launch-campaigns/{campaign}/pieces"),
        &json!({
            "content_type": "blog_post", "title": "En attente",
            "url": "https://example.test/attente",
        }),
    )
    .await;

    app.login("closebrandadmin").await;
    // Closing would leave the author unpaid with nothing to appeal against.
    let resp = app
        .post(
            &format!("/api/admin/launch-campaigns/{campaign}/close"),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

// ═══════════════════════════════════════════════════════════════════
// Sponsored content
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn sponsored_content_always_says_it_is_sponsored() {
    let app = TestApp::spawn().await;
    an_admin(&app, "contentadmin").await;
    an_enterprise(&app, "Contentco").await;
    let sponsor: Uuid =
        sqlx::query_scalar("SELECT id FROM enterprises WHERE slug LIKE 'contentco%'")
            .fetch_one(&app.db)
            .await
            .unwrap();

    app.login("contentadmin").await;
    let resp = app
        .post(
            "/api/admin/sponsored-content",
            &json!({
                "sponsor_enterprise_id": sponsor,
                "content_type": "blog_post",
                "title": "Retour sur le hackathon",
                "fee": "1200.00",
            }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let created: Value = resp.json().await.unwrap();
    let id: Uuid = created["data"]["content_id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    // Not asked for, generated: the disclosure is the part a hurried editor
    // drops.
    let disclosure: String =
        sqlx::query_scalar("SELECT disclosure_text FROM event_sponsored_content WHERE id = $1")
            .bind(id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(disclosure.len() >= 10);
    assert!(disclosure.to_lowercase().contains("contentco"));

    let resp = app
        .post(
            &format!("/api/admin/sponsored-content/{id}/publish"),
            &json!({ "url": "https://example.test/recap" }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    let booked: sqlx::types::BigDecimal = sqlx::query_scalar(
        "SELECT amount_credits FROM platform_revenues WHERE source = 'media_sponsor_content'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    common::assert_decimal(&booked, "1200.00");
}

// ═══════════════════════════════════════════════════════════════════
// Ambassadors
// ═══════════════════════════════════════════════════════════════════

fn an_ambassador_program() -> Value {
    json!({
        "name": "Programme Widget",
        "brief_md": "Parler de Widget, honnêtement, une fois par mois.",
        "target_count": 2,
        "monthly_stipend": "300.00",
        "expected_deliverables_per_month": 3,
        "duration_months": 6,
        "activation_fee": "5000.00",
        "management_monthly_fee": "1000.00",
    })
}

async fn a_program(app: &TestApp) -> Uuid {
    let resp = app
        .post(
            "/api/enterprise/ambassador-programs",
            &an_ambassador_program(),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let created: Value = resp.json().await.unwrap();
    created["data"]["program"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

async fn rank(app: &TestApp, user: Uuid, rank: &str) {
    sqlx::query(
        "INSERT INTO user_ranks (user_id, rank) VALUES ($1, $2)
         ON CONFLICT (user_id) DO UPDATE SET rank = EXCLUDED.rank",
    )
    .bind(user)
    .bind(rank)
    .execute(&app.db)
    .await
    .unwrap();
}

#[tokio::test]
async fn an_unpaid_ambassadorship_is_not_brokered() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Unpaidambco").await;

    let mut body = an_ambassador_program();
    body["monthly_stipend"] = json!("0.00");
    let resp = app.post("/api/enterprise/ambassador-programs", &body).await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn somebody_below_the_rank_floor_is_not_invited() {
    let app = TestApp::spawn().await;
    an_admin(&app, "rankadmin").await;
    an_enterprise(&app, "Rankco").await;
    let program = a_program(&app).await;

    let junior = a_talent(&app, "juniorname").await;
    rank(&app, junior, "ranger").await;

    app.login("rankadmin").await;
    // The company is buying a name that means something to the community.
    let resp = app
        .post(
            &format!("/api/admin/ambassador-programs/{program}/invite"),
            &json!({ "user_id": junior }),
        )
        .await;
    assert_eq!(resp.status(), 400);

    rank(&app, junior, "artisan").await;
    let resp = app
        .post(
            &format!("/api/admin/ambassador-programs/{program}/invite"),
            &json!({ "user_id": junior }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn nobody_becomes_an_ambassador_without_saying_yes() {
    let app = TestApp::spawn().await;
    an_admin(&app, "consentambadmin").await;
    an_enterprise(&app, "Consentambco").await;
    let program = a_program(&app).await;

    let person = a_talent(&app, "invitedperson").await;
    rank(&app, person, "maitre").await;
    app.login("consentambadmin").await;
    app.post(
        &format!("/api/admin/ambassador-programs/{program}/invite"),
        &json!({ "user_id": person }),
    )
    .await;

    // Activating with only invitations bills the company for a cohort that
    // does not exist.
    let resp = app
        .post(
            &format!("/api/admin/ambassador-programs/{program}/activate"),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), 400);

    app.login("invitedperson").await;
    let resp = app
        .post(
            &format!("/api/ambassador-programs/{program}/respond"),
            &json!({ "accept": true }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    app.login("consentambadmin").await;
    let resp = app
        .post(
            &format!("/api/admin/ambassador-programs/{program}/activate"),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    let booked: sqlx::types::BigDecimal = sqlx::query_scalar(
        "SELECT amount_credits FROM platform_revenues
          WHERE source = 'ambassador_program_fee'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    common::assert_decimal(&booked, "5000.00");
}

#[tokio::test]
async fn a_short_month_is_pro_rated_and_paid_once() {
    let app = TestApp::spawn().await;
    an_admin(&app, "stipendadmin").await;
    an_enterprise(&app, "Stipendco").await;
    let program = a_program(&app).await;

    let person = a_talent(&app, "stipendperson").await;
    rank(&app, person, "artisan").await;
    app.login("stipendadmin").await;
    app.post(
        &format!("/api/admin/ambassador-programs/{program}/invite"),
        &json!({ "user_id": person }),
    )
    .await;
    app.login("stipendperson").await;
    app.post(
        &format!("/api/ambassador-programs/{program}/respond"),
        &json!({ "accept": true }),
    )
    .await;

    // Two of the three expected pieces.
    for i in 0..2 {
        let resp = app
            .post(
                &format!("/api/ambassador-programs/{program}/deliverables"),
                &json!({
                    "kind": "blog_post",
                    "url": format!("https://example.test/amb{i}"),
                    "counts_for_month": "2026-08-01",
                }),
            )
            .await;
        assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    }

    app.login("stipendadmin").await;
    let resp = app
        .post(
            &format!("/api/admin/ambassador-programs/{program}/pay"),
            &json!({ "user_id": person, "month": "2026-08-15" }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    common::assert_amount(&body["data"]["paid"], "200.00");

    // A retry that paid twice would be found by an accountant months later,
    // if at all.
    let resp = app
        .post(
            &format!("/api/admin/ambassador-programs/{program}/pay"),
            &json!({ "user_id": person, "month": "2026-08-01" }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn a_month_with_nothing_delivered_pays_nothing() {
    let app = TestApp::spawn().await;
    an_admin(&app, "emptymonthadmin").await;
    an_enterprise(&app, "Emptymonthco").await;
    let program = a_program(&app).await;

    let person = a_talent(&app, "idleambassador").await;
    rank(&app, person, "artisan").await;
    app.login("emptymonthadmin").await;
    app.post(
        &format!("/api/admin/ambassador-programs/{program}/invite"),
        &json!({ "user_id": person }),
    )
    .await;
    app.login("idleambassador").await;
    app.post(
        &format!("/api/ambassador-programs/{program}/respond"),
        &json!({ "accept": true }),
    )
    .await;

    app.login("emptymonthadmin").await;
    // A stipend paid regardless is how a programme quietly becomes a
    // subscription the company cannot cancel.
    let resp = app
        .post(
            &format!("/api/admin/ambassador-programs/{program}/pay"),
            &json!({ "user_id": person, "month": "2026-08-01" }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

// ═══════════════════════════════════════════════════════════════════
// The audience
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn the_only_thing_an_individual_can_buy_is_a_replay() {
    let app = TestApp::spawn().await;
    let resp = app.get("/api/audience/plans").await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let plans = body["data"]["plans"].as_array().unwrap();

    // Talents do not pay to be seen. Nothing here sells visibility, ranking
    // or access to work.
    //
    // An allowlist rather than a count. The count said one, 0365 added the
    // paid newsletter — an individual paying monthly for something that is
    // not visibility, which is allowed — and a count would have been edited
    // to two without anybody restating why. Adding a plan should require
    // saying here that it does not sell attention.
    const AN_INDIVIDUAL_MAY_BUY: &[&str] = &[
        // Watching a talk again.
        "event_replays_annual",
        // Reading. Sells no place in any listing.
        "newsletter_premium",
    ];
    for plan in plans {
        let slug = plan["slug"].as_str().unwrap();
        assert!(
            AN_INDIVIDUAL_MAY_BUY.contains(&slug),
            "'{slug}' is sold to individuals and nobody has said what it sells:              if it is attention, ranking or access to work, it does not belong here"
        );
    }
    assert!(
        plans.iter().any(|p| p["slug"] == "event_replays_annual"),
        "the replay plan is the one this endpoint exists for"
    );
}

#[tokio::test]
async fn renewing_extends_the_subscription_rather_than_adding_a_second() {
    let app = TestApp::spawn().await;
    a_talent(&app, "subscriber").await;
    app.login("subscriber").await;

    let first = app
        .post(
            "/api/audience/subscribe",
            &json!({ "plan": "event_replays_annual" }),
        )
        .await;
    assert_eq!(first.status(), 200, "{}", first.text().await.unwrap());
    let first: Value = first.json().await.unwrap();
    let first_expiry = first["data"]["expires_at"].as_str().unwrap().to_string();

    let second = app
        .post(
            "/api/audience/subscribe",
            &json!({ "plan": "event_replays_annual" }),
        )
        .await;
    assert_eq!(second.status(), 200);
    let second: Value = second.json().await.unwrap();
    let second_expiry = second["data"]["expires_at"].as_str().unwrap().to_string();

    // Renewing early must not throw away the time already paid for.
    assert!(second_expiry > first_expiry);

    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM audience_subscriptions")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(rows, 1, "two rows would mean two charges for one access");
}

#[tokio::test]
async fn cancelling_stops_the_renewal_and_keeps_what_was_paid_for() {
    let app = TestApp::spawn().await;
    a_talent(&app, "canceller").await;
    app.login("canceller").await;
    app.post(
        "/api/audience/subscribe",
        &json!({ "plan": "event_replays_annual" }),
    )
    .await;

    let resp = app
        .post(
            "/api/audience/cancel",
            &json!({ "plan": "event_replays_annual" }),
        )
        .await;
    assert_eq!(resp.status(), 200);

    // Ending access on the day somebody cancels is how a refund request
    // starts.
    let resp = app.get("/api/users/me/audience").await;
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["premium"], true);

    let renews: bool = sqlx::query_scalar("SELECT auto_renew FROM audience_subscriptions")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert!(!renews);
}
