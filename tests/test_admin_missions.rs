//! Paid missions, seen from outside the two parties.
//!
//! The claim under test is the narrow one: an admin surface over missions is
//! not a way of running them. The only write is the case the round loop
//! cannot reach — a client who will not accept and a designer who will not
//! hand in again, with the money sitting in escrow between them.

mod common;
use bigdecimal::BigDecimal;
use common::TestApp;
use serde_json::{Value, json};
use skilluv_backend::services::ledger::{self, Currency, State};
use std::str::FromStr;
use uuid::Uuid;

fn eur(value: &str) -> BigDecimal {
    BigDecimal::from_str(value).unwrap()
}

async fn balance(app: &TestApp, user: Uuid, state: State) -> BigDecimal {
    ledger::user_balance(&app.db, user, state, Currency::Eur)
        .await
        .unwrap()
}

/// Money in escrow: an invoice for the whole budget, paid, captured onto the
/// ledger exactly as `mission_billing::capture` would have.
///
/// The fixture the arbitration tests were missing. Without an invoice there is
/// nothing to release and nothing to return, which is why an endpoint whose
/// own documentation says "the money is released" could move none and pass.
async fn escrowed(app: &TestApp, mission: Uuid, talent: Uuid) -> Uuid {
    let invoice: Uuid = sqlx::query_scalar(
        "INSERT INTO mission_invoices
            (mission_id, sequence, label, amount, currency, commission_percent, status)
         VALUES ($1, 1, 'Solde de la mission', 2000, 'EUR', 15, 'issued')
         RETURNING id",
    )
    .bind(mission)
    .fetch_one(&app.db)
    .await
    .unwrap();

    // The payment names the invoice, not the mission: that is the subject the
    // refund looks the charge up by, and getting it wrong is the difference
    // between reversing the card and logging "refund this by hand".
    let payment: Uuid = sqlx::query_scalar(
        "INSERT INTO payments (subject_type, subject_id, provider, method, amount,
                               currency, status, succeeded_at)
         VALUES ('mission_invoice', $1, 'stripe', 'card', 2000, 'EUR', 'succeeded', NOW())
         RETURNING id",
    )
    .bind(invoice)
    .fetch_one(&app.db)
    .await
    .unwrap();

    sqlx::query("UPDATE mission_invoices SET payment_id = $2 WHERE id = $1")
        .bind(invoice)
        .bind(payment)
        .execute(&app.db)
        .await
        .unwrap();

    skilluv_backend::services::mission_billing::capture(&app.db, invoice, payment)
        .await
        .expect("capture");

    // 2000 less the fifteen percent commission.
    assert_eq!(balance(app, talent, State::Pending).await, eur("1700.00"));

    invoice
}

async fn user_id(app: &TestApp, username: &str) -> Uuid {
    sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(username)
        .fetch_one(&app.db)
        .await
        .unwrap()
}

async fn grant(app: &TestApp, user: Uuid, capability: &str) {
    sqlx::query(
        "INSERT INTO user_capabilities (user_id, capability, granted_reason)
         VALUES ($1, $2, 'test_setup') ON CONFLICT DO NOTHING",
    )
    .bind(user)
    .bind(capability)
    .execute(&app.db)
    .await
    .unwrap();
}

/// A design mission in progress, with one round handed in and unanswered.
async fn a_stuck_mission(app: &TestApp, slug: &str, client: Uuid, talent: Uuid) -> Uuid {
    let enterprise: Uuid = sqlx::query_scalar(
        "INSERT INTO enterprises (owner_id, company_name, slug, company_size)
         VALUES ($1, 'Coopérative test', $2, '11-50') RETURNING id",
    )
    .bind(client)
    .bind(format!("ent-{slug}"))
    .fetch_one(&app.db)
    .await
    .unwrap();

    let mission_type: Uuid = sqlx::query_scalar(
        "SELECT id FROM mission_types WHERE skill_domain = 'design' ORDER BY sort_order LIMIT 1",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    let mission: Uuid = sqlx::query_scalar(
        "INSERT INTO missions
            (slug, enterprise_id, mission_type_id, skill_domain, title, description,
             acceptance_criteria, deliverable_format, payment_model, budget_eur,
             commission_percent, status, assigned_user_id, assigned_at, included_rounds)
         VALUES ($1, $2, $3, 'design', 'Identité coopérative',
                 'Logotype, palette et guidelines.',
                 'Le logotype tient en une couleur et reste lisible en favicon.',
                 'brand_package', 'fixed_price', 2000, 15, 'in_progress', $4, NOW(), 2)
         RETURNING id",
    )
    .bind(slug)
    .bind(enterprise)
    .bind(mission_type)
    .bind(talent)
    .fetch_one(&app.db)
    .await
    .unwrap();

    // Handed in a month ago and never answered. That is what a dispute looks
    // like from outside: not a slow mission, an unanswered one.
    sqlx::query(
        "INSERT INTO mission_deliveries
            (mission_id, round, delivered_by, artifact_url, notes_md, delivered_at)
         VALUES ($1, 1, $2, 'https://figma.test/mission/round-1',
                 'Le logotype tient en monochrome.', NOW() - INTERVAL '30 days')",
    )
    .bind(mission)
    .bind(talent)
    .execute(&app.db)
    .await
    .unwrap();

    mission
}

async fn a_cast(app: &TestApp, prefix: &str) -> (Uuid, Uuid) {
    app.register_user(&format!("{prefix}_client")).await;
    app.register_user(&format!("{prefix}_talent")).await;
    (
        user_id(app, &format!("{prefix}_client")).await,
        user_id(app, &format!("{prefix}_talent")).await,
    )
}

#[tokio::test]
async fn a_curator_reads_their_domain_and_must_name_it() {
    let app = TestApp::spawn().await;
    let (client, talent) = a_cast(&app, "am_curator").await;
    a_stuck_mission(&app, "am-curator", client, talent).await;

    app.register_user("am_the_curator").await;
    let curator = user_id(&app, "am_the_curator").await;
    grant(&app, curator, "domain_curator:design").await;
    app.login("am_the_curator").await;

    let mine = app.get("/api/admin/missions?skill_domain=design").await;
    assert_eq!(mine.status(), 200, "{}", mine.text().await.unwrap());

    assert_eq!(
        app.get("/api/admin/missions?skill_domain=security")
            .await
            .status(),
        403
    );

    // Leaving the filter blank asks for every domain, which is an admin's
    // question. Answering it would hand a curator the domains they were not
    // given.
    assert_eq!(app.get("/api/admin/missions").await.status(), 403);
}

#[tokio::test]
async fn the_stuck_queue_is_the_one_an_arbiter_works() {
    let app = TestApp::spawn().await;
    let (client, talent) = a_cast(&app, "am_stuck").await;
    a_stuck_mission(&app, "am-stuck", client, talent).await;

    // A second mission whose round was answered the day it arrived.
    let (client2, talent2) = a_cast(&app, "am_moving").await;
    let moving = a_stuck_mission(&app, "am-moving", client2, talent2).await;
    sqlx::query(
        "UPDATE mission_deliveries
            SET decision = 'accepted', decided_by = $2, decided_at = NOW()
          WHERE mission_id = $1",
    )
    .bind(moving)
    .bind(client2)
    .execute(&app.db)
    .await
    .unwrap();

    app.register_admin("am_stuck_admin").await;
    app.login("am_stuck_admin").await;

    let body: Value = app
        .get("/api/admin/missions?stuck_only=true")
        .await
        .json()
        .await
        .unwrap();
    let slugs: Vec<&str> = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["slug"].as_str().unwrap())
        .collect();

    assert!(slugs.contains(&"am-stuck"), "{body}");
    assert!(
        !slugs.contains(&"am-moving"),
        "a mission whose round was answered is not stuck: {body}"
    );
}

#[tokio::test]
async fn a_recent_hand_in_is_not_a_dispute() {
    let app = TestApp::spawn().await;
    let (client, talent) = a_cast(&app, "am_recent").await;
    let mission = a_stuck_mission(&app, "am-recent", client, talent).await;
    sqlx::query("UPDATE mission_deliveries SET delivered_at = NOW() WHERE mission_id = $1")
        .bind(mission)
        .execute(&app.db)
        .await
        .unwrap();

    app.register_admin("am_recent_admin").await;
    app.login("am_recent_admin").await;

    // Twenty-one days by default: long enough that a fortnight's holiday does
    // not read as a refusal.
    let body: Value = app
        .get("/api/admin/missions?stuck_only=true")
        .await
        .json()
        .await
        .unwrap();
    assert!(body["data"].as_array().unwrap().is_empty(), "{body}");
}

#[tokio::test]
async fn the_detail_shows_the_rounds_and_the_terms() {
    let app = TestApp::spawn().await;
    let (client, talent) = a_cast(&app, "am_detail").await;
    a_stuck_mission(&app, "am-detail", client, talent).await;

    app.register_admin("am_detail_admin").await;
    app.login("am_detail_admin").await;

    let body: Value = app
        .get("/api/admin/missions/am-detail")
        .await
        .json()
        .await
        .unwrap();
    let data = &body["data"];

    assert_eq!(data["mission"]["slug"], "am-detail");
    assert_eq!(data["mission"]["rounds"], 1);
    assert_eq!(data["mission"]["awaiting_decision"], true);
    assert_eq!(data["mission"]["arbitrated"], false);
    // The terms are on the page an arbitration is decided from. Nobody reads
    // a contract they have to go and find.
    assert!(data["ip_terms"].is_string(), "{body}");
    assert_eq!(data["rounds"][0]["round"], 1);
    assert!(data["arbitration"].is_null());
}

#[tokio::test]
async fn a_mission_nobody_may_read_answers_not_found() {
    let app = TestApp::spawn().await;
    let (client, talent) = a_cast(&app, "am_hidden").await;
    a_stuck_mission(&app, "am-hidden", client, talent).await;

    app.register_user("am_wrong_curator").await;
    let curator = user_id(&app, "am_wrong_curator").await;
    grant(&app, curator, "domain_curator:security").await;
    app.login("am_wrong_curator").await;

    // 403, because the mission exists and this person may not read it. The
    // 404 is reserved for a mission that is not there — which is what
    // `am-nope` below is.
    assert_eq!(app.get("/api/admin/missions/am-hidden").await.status(), 403);
    assert_eq!(app.get("/api/admin/missions/am-nope").await.status(), 404);
}

#[tokio::test]
async fn arbitration_needs_more_than_a_verdict() {
    let app = TestApp::spawn().await;
    let (client, talent) = a_cast(&app, "am_short").await;
    a_stuck_mission(&app, "am-short", client, talent).await;

    app.register_user("am_arbiter").await;
    let arbiter = user_id(&app, "am_arbiter").await;
    grant(&app, arbiter, "mission_arbiter").await;
    app.login("am_arbiter").await;

    // Both sides read this and one of them has just lost. "Refusé" teaches
    // nobody anything and cannot be argued with.
    let short = app
        .post(
            "/api/admin/missions/am-short/arbitrate",
            &json!({"outcome": "cancelled", "reason_md": "Refusé."}),
        )
        .await;
    assert_eq!(short.status(), 400);

    let unknown = app
        .post(
            "/api/admin/missions/am-short/arbitrate",
            &json!({
                "outcome": "split_the_difference",
                "reason_md": "Une raison bien assez longue pour passer le plancher de quatre-vingts \
                              caractères imposé par la contrainte.",
            }),
        )
        .await;
    assert_eq!(unknown.status(), 400);
}

#[tokio::test]
async fn only_an_arbiter_decides_and_a_curator_does_not() {
    let app = TestApp::spawn().await;
    let (client, talent) = a_cast(&app, "am_reader").await;
    a_stuck_mission(&app, "am-reader", client, talent).await;

    app.register_user("am_reader_curator").await;
    let curator = user_id(&app, "am_reader_curator").await;
    grant(&app, curator, "domain_curator:design").await;
    app.login("am_reader_curator").await;

    // Reading a domain and deciding a contract in it are different
    // permissions, and the curator has only the first.
    assert_eq!(app.get("/api/admin/missions/am-reader").await.status(), 200);
    let refused = app
        .post(
            "/api/admin/missions/am-reader/arbitrate",
            &json!({
                "outcome": "cancelled",
                "reason_md": "Une raison bien assez longue pour passer le plancher de quatre-vingts \
                              caractères imposé par la contrainte.",
            }),
        )
        .await;
    assert_eq!(refused.status(), 403);
}

#[tokio::test]
async fn an_arbitration_ends_the_round_and_says_it_was_decided() {
    let app = TestApp::spawn().await;
    let (client, talent) = a_cast(&app, "am_decide").await;
    let mission = a_stuck_mission(&app, "am-decide", client, talent).await;

    app.register_user("am_decider").await;
    let arbiter = user_id(&app, "am_decider").await;
    grant(&app, arbiter, "mission_arbiter").await;
    app.login("am_decider").await;

    let reason = "Le livrable répond au critère écrit dans la mission : le logotype tient en une \
                  couleur et reste lisible en favicon. Le client n'a formulé aucun grief en trente \
                  jours.";

    let resp = app
        .post(
            "/api/admin/missions/am-decide/arbitrate",
            &json!({"outcome": "accepted", "reason_md": reason}),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    let status: String = sqlx::query_scalar("SELECT status FROM missions WHERE id = $1")
        .bind(mission)
        .fetch_one(&app.db)
        .await
        .unwrap();
    // `closed`, not `delivered`. Closing is the client accepting delivery, and
    // arbitration exists because the client will not: stopping at `delivered`
    // would leave the mission waiting on the one act that was refused, with
    // the escrow waiting behind it.
    assert_eq!(status, "closed");

    // The waiting round is answered, so the loop cannot be resumed behind the
    // decision.
    let (decision, decided_by): (Option<String>, Option<Uuid>) =
        sqlx::query_as("SELECT decision, decided_by FROM mission_deliveries WHERE mission_id = $1")
            .bind(mission)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(decision.as_deref(), Some("accepted"));
    assert_eq!(decided_by, Some(arbiter));

    // And the record that it was decided rather than agreed. Without it, this
    // mission and one a happy client accepted read the same.
    let body: Value = app
        .get("/api/admin/missions/am-decide")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["arbitration"]["outcome"], "accepted");
    assert_eq!(body["data"]["arbitration"]["arbiter"], "am_decider");
    assert_eq!(body["data"]["mission"]["arbitrated"], true);
}

#[tokio::test]
async fn a_mission_is_arbitrated_once() {
    let app = TestApp::spawn().await;
    let (client, talent) = a_cast(&app, "am_twice").await;
    a_stuck_mission(&app, "am-twice", client, talent).await;

    app.register_user("am_twice_arbiter").await;
    let arbiter = user_id(&app, "am_twice_arbiter").await;
    grant(&app, arbiter, "mission_arbiter").await;
    app.login("am_twice_arbiter").await;

    let body = json!({
        "outcome": "cancelled",
        "reason_md": "Le livrable ne répond pas au critère écrit dans la mission, et le prestataire \
                      n'a pas repris la main en trente jours malgré la demande.",
    });

    assert_eq!(
        app.post("/api/admin/missions/am-twice/arbitrate", &body)
            .await
            .status(),
        200
    );

    // A second decision would re-open one that has already moved money.
    // Re-opening it is a new mission, not a new row.
    assert_eq!(
        app.post("/api/admin/missions/am-twice/arbitrate", &body)
            .await
            .status(),
        409
    );
}

#[tokio::test]
async fn a_mission_that_already_ended_has_nothing_to_arbitrate() {
    let app = TestApp::spawn().await;
    let (client, talent) = a_cast(&app, "am_done").await;
    let mission = a_stuck_mission(&app, "am-done", client, talent).await;
    sqlx::query("UPDATE missions SET status = 'closed' WHERE id = $1")
        .bind(mission)
        .execute(&app.db)
        .await
        .unwrap();

    app.register_user("am_done_arbiter").await;
    let arbiter = user_id(&app, "am_done_arbiter").await;
    grant(&app, arbiter, "mission_arbiter").await;
    app.login("am_done_arbiter").await;

    // Saying so beats writing a decision that changes nothing.
    let resp = app
        .post(
            "/api/admin/missions/am-done/arbitrate",
            &json!({
                "outcome": "accepted",
                "reason_md": "Une raison bien assez longue pour passer le plancher de quatre-vingts \
                              caractères imposé par la contrainte.",
            }),
        )
        .await;
    assert_eq!(resp.status(), 409);
}

/// Deciding for the delivery releases the escrow.
///
/// The endpoint's own documentation said "the delivery stands and the money is
/// released", and it released nothing: the status was written with a raw
/// UPDATE, and both the release and the refund live in `missions::set_status`.
/// So the arbitration read as settled and the funds sat exactly where they
/// were, which is the failure mode the whole payment layer was built to end.
///
/// Nothing caught it because no arbitration test had an invoice at all.
#[tokio::test]
async fn deciding_for_the_delivery_releases_the_escrow() {
    let app = TestApp::spawn().await;
    let (client, talent) = a_cast(&app, "am_pay_ok").await;
    let mission = a_stuck_mission(&app, "am-pay-ok", client, talent).await;
    let invoice = escrowed(&app, mission, talent).await;

    app.register_user("am_pay_ok_arb").await;
    let arbiter = user_id(&app, "am_pay_ok_arb").await;
    grant(&app, arbiter, "mission_arbiter").await;
    app.login("am_pay_ok_arb").await;

    let reason = "Le livrable répond au critère écrit dans la mission : le logotype tient en une \
                  couleur et reste lisible en favicon. Le client n'a formulé aucun grief en trente \
                  jours.";
    let resp = app
        .post(
            "/api/admin/missions/am-pay-ok/arbitrate",
            &json!({"outcome": "accepted", "reason_md": reason}),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    // Pending became available: the talent can withdraw it. The commission
    // never was in their pending balance, so the figure does not change here.
    assert_eq!(balance(&app, talent, State::Pending).await, eur("0"));
    assert_eq!(
        balance(&app, talent, State::Available).await,
        eur("1700.00")
    );

    let status: String = sqlx::query_scalar("SELECT status FROM mission_invoices WHERE id = $1")
        .bind(invoice)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(status, "released");
}

/// Deciding against it returns the escrow.
///
/// The other half of the same silence: `cancelled` set the word and left the
/// captured money in the talent's pending balance forever, with no path that
/// could ever move it.
#[tokio::test]
async fn deciding_against_the_delivery_returns_the_escrow() {
    let app = TestApp::spawn().await;
    let (client, talent) = a_cast(&app, "am_pay_no").await;
    let mission = a_stuck_mission(&app, "am-pay-no", client, talent).await;
    let invoice = escrowed(&app, mission, talent).await;

    app.register_user("am_pay_no_arb").await;
    let arbiter = user_id(&app, "am_pay_no_arb").await;
    grant(&app, arbiter, "mission_arbiter").await;
    app.login("am_pay_no_arb").await;

    let reason = "Le livrable ne répond pas au critère écrit : le logotype devient illisible en \
                  favicon et aucune version monochrome n'a été fournie malgré deux demandes \
                  écrites du client.";
    let resp = app
        .post(
            "/api/admin/missions/am-pay-no/arbitrate",
            &json!({"outcome": "cancelled", "reason_md": reason}),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    // Nothing is owed to anybody: the claim on the talent's side is cancelled
    // and the money is back at the provider.
    assert_eq!(balance(&app, talent, State::Pending).await, eur("0"));
    assert_eq!(balance(&app, talent, State::Available).await, eur("0"));

    let (status, reason_written): (String, Option<String>) =
        sqlx::query_as("SELECT status, cancellation_reason FROM mission_invoices WHERE id = $1")
            .bind(invoice)
            .fetch_one(&app.db)
            .await
            .unwrap();
    // `refunded`, not `cancelled`: a cancelled invoice is one nobody ever
    // paid, and an accountant has to be able to tell those apart.
    assert_eq!(status, "refunded");
    assert!(reason_written.is_some_and(|r| !r.trim().is_empty()));

    // The commission came off with it. Keeping a fee on a service the arbiter
    // has just ruled undelivered would be indefensible, and it would also
    // leave marketplace revenue counting money we gave back.
    let revenue: Option<BigDecimal> = sqlx::query_scalar(
        "SELECT SUM(amount_credits) FROM platform_revenues
          WHERE source = 'mission_marketplace' AND related_talent_id = $1",
    )
    .bind(talent)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(revenue.unwrap_or_else(|| eur("0")), eur("0"));
}

/// Cancelling a mission returns the escrow, arbitrated or not.
///
/// The bug was never only arbitration's. `release_all` was called from
/// `set_status` on `closed` and nothing was called on `cancelled`, so any
/// mission cancelled from `in_progress` with a paid invoice stranded the money
/// -- no arbiter required. Fixing it at the status transition rather than at
/// the endpoint is what makes this test possible to write.
#[tokio::test]
async fn cancelling_a_mission_returns_the_escrow_without_an_arbiter() {
    let app = TestApp::spawn().await;
    let (client, talent) = a_cast(&app, "am_plain_cancel").await;
    let mission = a_stuck_mission(&app, "am-plain-cancel", client, talent).await;
    let invoice = escrowed(&app, mission, talent).await;

    skilluv_backend::services::missions::set_status(
        &app.db,
        mission,
        "cancelled",
        Some("Le client a mis fin à la mission avant la livraison finale."),
    )
    .await
    .expect("cancel");

    assert_eq!(balance(&app, talent, State::Pending).await, eur("0"));

    let status: String = sqlx::query_scalar("SELECT status FROM mission_invoices WHERE id = $1")
        .bind(invoice)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(status, "refunded");
}

/// Money already released stays released.
///
/// On a milestone mission the accepted rounds have already paid out, and
/// cancelling the rest must not reach back for them: once released the amount
/// is the talent's to withdraw and may already be gone. That is what the
/// dispute machinery is for, with its own burden of proof.
#[tokio::test]
async fn a_cancellation_does_not_claw_back_what_was_already_released() {
    let app = TestApp::spawn().await;
    let (client, talent) = a_cast(&app, "am_released").await;
    let mission = a_stuck_mission(&app, "am-released", client, talent).await;
    let invoice = escrowed(&app, mission, talent).await;

    skilluv_backend::services::mission_billing::release_one(&app.db, invoice)
        .await
        .expect("release");
    assert_eq!(
        balance(&app, talent, State::Available).await,
        eur("1700.00")
    );

    skilluv_backend::services::missions::set_status(
        &app.db,
        mission,
        "cancelled",
        Some("Annulée après le versement du premier jalon, pour les rounds restants."),
    )
    .await
    .expect("cancel");

    assert_eq!(
        balance(&app, talent, State::Available).await,
        eur("1700.00")
    );
    let status: String = sqlx::query_scalar("SELECT status FROM mission_invoices WHERE id = $1")
        .bind(invoice)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(status, "released");
}
