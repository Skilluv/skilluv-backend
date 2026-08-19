//! Contests a company pays for.
//!
//! The tests worth having are the ones about what the entrants were promised:
//! a recruiting contest pays in interviews and not in cash, a private search
//! does not leak, the whole judged field gets a rank, the shortlist gets a
//! proof whether or not anybody is hired, and a hire needs an interview that
//! actually happened.

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

fn a_recruiting_brief(slug: &str) -> Value {
    json!({
        "slug": slug,
        "kind": "recruiting",
        "title": "Reprendre notre API",
        "brief_md": "Livrez un service qui expose trois endpoints et ses tests.",
        "orientation_target": "web-backend-developer",
        "submissions_deadline": "2027-01-31T23:59:00Z",
        "shortlist_size": 2,
        "mode": "self_serve",
        "setup_fee": "500.00",
        "per_candidate_contact_fee": "20.00",
        "success_fee_percent": "10.00",
        "orchestration_fee": "500.00",
    })
}

async fn a_contest(app: &TestApp, body: &Value) -> Uuid {
    let resp = app.post("/api/enterprise/contests", body).await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let created: Value = resp.json().await.unwrap();
    created["data"]["contest"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

async fn open_for_entries(app: &TestApp, id: Uuid) {
    let resp = app
        .post(
            &format!("/api/enterprise/contests/{id}/status"),
            &json!({ "status": "submissions_open" }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
}

// ═══════════════════════════════════════════════════════════════════
// The brief
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn a_recruiting_contest_cannot_bolt_on_a_cash_prize() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Prizeco").await;

    // The prize is an interview, and the entrants were told so. A cash prize
    // would make it a different product wearing the same name.
    let mut body = a_recruiting_brief("prize-contest");
    body["prize_first"] = json!("5000.00");
    body["prize_pool_total"] = json!("10000.00");
    let resp = app.post("/api/enterprise/contests", &body).await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn a_recruiting_contest_says_how_it_is_billed() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Modeco").await;

    let mut body = a_recruiting_brief("mode-contest");
    body["mode"] = Value::Null;
    // Self-serve and managed are billed differently; neither cannot be
    // invoiced at all.
    let resp = app.post("/api/enterprise/contests", &body).await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn an_award_without_a_prize_is_a_call_for_free_work() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Awardco").await;

    let resp = app
        .post(
            "/api/enterprise/contests",
            &json!({
                "slug": "award-no-prize",
                "kind": "award",
                "title": "Grand défi",
                "brief_md": "Résolvez ceci.",
                "submissions_deadline": "2027-06-30T23:59:00Z",
                "orchestration_fee": "50000.00",
            }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn a_migration_contest_names_both_stacks() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Migrationco").await;

    let mut body = json!({
        "slug": "migration-contest",
        "kind": "migration",
        "title": "Monolithe vers services",
        "brief_md": "Proposez une approche et un PoC.",
        "submissions_deadline": "2027-03-31T23:59:00Z",
        "orchestration_fee": "2000.00",
    });
    // An approach cannot be proposed against a blank.
    let resp = app.post("/api/enterprise/contests", &body).await;
    assert_eq!(resp.status(), 400);

    body["current_stack_md"] = json!("Rails 5, MySQL 5.7, un seul déploiement.");
    body["target_stack_md"] = json!("Services Rust, Postgres 16, déploiements séparés.");
    let resp = app.post("/api/enterprise/contests", &body).await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn a_corporate_hackathon_without_outsiders_is_the_company_alone() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Internalco").await;

    let resp = app
        .post(
            "/api/enterprise/contests",
            &json!({
                "slug": "internal-only",
                "kind": "corporate_internal",
                "title": "Hackathon interne",
                "brief_md": "Deux jours sur nos propres outils.",
                "submissions_deadline": "2027-02-28T23:59:00Z",
                "internal_employees_count": 20,
                "orchestration_fee": "4000.00",
            }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn a_contest_aimed_at_a_typo_is_refused() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Typocontestco").await;

    let mut body = a_recruiting_brief("typo-contest");
    body["orientation_target"] = json!("metier-invente");
    let resp = app.post("/api/enterprise/contests", &body).await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn a_deadline_in_the_past_is_refused() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Pastco").await;

    let mut body = a_recruiting_brief("past-contest");
    body["submissions_deadline"] = json!("2020-01-01T00:00:00Z");
    let resp = app.post("/api/enterprise/contests", &body).await;
    assert_eq!(resp.status(), 400);
}

// ═══════════════════════════════════════════════════════════════════
// Privacy
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn a_private_contest_does_not_confirm_its_own_existence() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Quietco").await;
    let mut body = a_recruiting_brief("quiet-search");
    body["visibility"] = json!("invitation_only");
    let id = a_contest(&app, &body).await;
    open_for_entries(&app, id).await;

    // Not on the open list.
    let resp = app.get("/api/contests/open").await;
    let listed: Value = resp.json().await.unwrap();
    assert!(
        !listed["data"]["contests"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["slug"] == "quiet-search")
    );

    // And 404 to somebody who guessed the slug.
    a_talent(&app, "outsider").await;
    app.login("outsider").await;
    let resp = app.get("/api/contests/quiet-search").await;
    assert_eq!(resp.status(), 404);

    let resp = app
        .post(
            &format!("/api/contests/{id}/submit"),
            &json!({ "deliverable_url": "https://example.test/x" }),
        )
        .await;
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn an_invited_person_can_see_and_enter_a_private_contest() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Invitedco").await;
    let mut body = a_recruiting_brief("invited-search");
    body["visibility"] = json!("invitation_only");
    let id = a_contest(&app, &body).await;
    open_for_entries(&app, id).await;

    let person = a_talent(&app, "invitedentrant").await;
    app.login("invitedco").await;
    let resp = app
        .post(
            &format!("/api/enterprise/contests/{id}/invite"),
            &json!({ "user_id": person }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    app.login("invitedentrant").await;
    let resp = app.get("/api/contests/invited-search").await;
    assert_eq!(resp.status(), 200);

    let resp = app
        .post(
            &format!("/api/contests/{id}/submit"),
            &json!({ "deliverable_url": "https://example.test/entry" }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
}

// ═══════════════════════════════════════════════════════════════════
// Entering and judging
// ═══════════════════════════════════════════════════════════════════

/// A public contest with three entries in.
async fn a_contest_with_entries(app: &TestApp, prefix: &str) -> (Uuid, Vec<Uuid>) {
    an_admin(app, &format!("{prefix}admin")).await;
    an_enterprise(app, &format!("{prefix}co")).await;
    let id = a_contest(app, &a_recruiting_brief(&format!("{prefix}-contest"))).await;
    open_for_entries(app, id).await;

    let mut people = Vec::new();
    for i in 0..3 {
        let name = format!("{prefix}entrant{i}");
        people.push(a_talent(app, &name).await);
        app.login(&name).await;
        let resp = app
            .post(
                &format!("/api/contests/{id}/submit"),
                &json!({
                    "deliverable_url": format!("https://example.test/{prefix}/{i}"),
                }),
            )
            .await;
        assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    }

    (id, people)
}

#[tokio::test]
async fn an_entry_has_to_be_reachable() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Linkco").await;
    let id = a_contest(&app, &a_recruiting_brief("link-contest")).await;
    open_for_entries(&app, id).await;

    a_talent(&app, "linkentrant").await;
    app.login("linkentrant").await;
    // A judge cannot open a link that is not there.
    let resp = app
        .post(
            &format!("/api/contests/{id}/submit"),
            &json!({ "deliverable_url": "mon-projet.zip" }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn entries_close_when_the_contest_does() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Closedco").await;
    let id = a_contest(&app, &a_recruiting_brief("closed-contest")).await;

    // Still in draft: nothing is open yet.
    a_talent(&app, "earlyentrant").await;
    app.login("earlyentrant").await;
    let resp = app
        .post(
            &format!("/api/contests/{id}/submit"),
            &json!({ "deliverable_url": "https://example.test/early" }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn two_entries_cannot_share_a_rank() {
    let app = TestApp::spawn().await;
    let (id, _) = a_contest_with_entries(&app, "tie").await;

    let subs = submission_ids(&app, id).await;
    app.login("tieco").await;
    // A shortlist drawn from a tie is arbitrary, and the person left out has
    // no way to see why.
    let resp = app
        .post(
            &format!("/api/enterprise/contests/{id}/judge"),
            &json!({
                "verdicts": [
                    { "submission_id": subs[0], "final_rank": 1 },
                    { "submission_id": subs[1], "final_rank": 1 },
                ]
            }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

async fn submission_ids(app: &TestApp, contest_id: Uuid) -> Vec<Uuid> {
    sqlx::query_scalar(
        "SELECT id FROM contest_submissions WHERE contest_id = $1 ORDER BY submitted_at",
    )
    .bind(contest_id)
    .fetch_all(&app.db)
    .await
    .unwrap()
}

#[tokio::test]
async fn everybody_judged_gets_a_rank_not_only_the_shortlist() {
    let app = TestApp::spawn().await;
    let (id, _) = a_contest_with_entries(&app, "rank").await;
    let subs = submission_ids(&app, id).await;

    app.login("rankco").await;
    let resp = app
        .post(
            &format!("/api/enterprise/contests/{id}/judge"),
            &json!({
                "verdicts": [
                    { "submission_id": subs[0], "final_rank": 1 },
                    { "submission_id": subs[1], "final_rank": 2 },
                    { "submission_id": subs[2], "final_rank": 3 },
                ]
            }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    // Third out of three is a fact somebody is owed. Without it they are
    // indistinguishable from people who never entered.
    let ranks: Vec<Option<i32>> = sqlx::query_scalar(
        "SELECT final_rank FROM contest_submissions WHERE contest_id = $1
          ORDER BY final_rank",
    )
    .bind(id)
    .fetch_all(&app.db)
    .await
    .unwrap();
    assert_eq!(ranks, vec![Some(1), Some(2), Some(3)]);

    // The shortlist is the agreed size, derived rather than ticked by hand.
    let shortlisted: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM contest_submissions WHERE contest_id = $1 AND shortlisted",
    )
    .bind(id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(shortlisted, 2);
}

#[tokio::test]
async fn the_shortlist_earns_a_proof_even_when_nobody_is_hired() {
    let app = TestApp::spawn().await;
    let (id, people) = a_contest_with_entries(&app, "proof").await;
    let subs = submission_ids(&app, id).await;

    app.login("proofco").await;
    app.post(
        &format!("/api/enterprise/contests/{id}/judge"),
        &json!({
            "verdicts": [
                { "submission_id": subs[0], "final_rank": 1 },
                { "submission_id": subs[1], "final_rank": 2 },
                { "submission_id": subs[2], "final_rank": 3 },
            ]
        }),
    )
    .await;

    // A company with a real vacancy put this person in its last two. That is
    // harder to claim than a certificate, and it survives a "no".
    let attested: Vec<Uuid> = sqlx::query_scalar(
        "SELECT user_id FROM attestations
          WHERE contest_id = $1 AND basis = 'contest_finalist'",
    )
    .bind(id)
    .fetch_all(&app.db)
    .await
    .unwrap();
    assert_eq!(attested.len(), 2);
    assert!(attested.contains(&people[0]));
    assert!(!attested.contains(&people[2]));

    // Each carries its own verification code: a shared one would let either
    // finalist verify as the other.
    let codes: i64 = sqlx::query_scalar(
        "SELECT count(DISTINCT verification_code) FROM attestations WHERE contest_id = $1",
    )
    .bind(id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(codes, 2);
}

#[tokio::test]
async fn judging_twice_does_not_hand_out_two_proofs() {
    let app = TestApp::spawn().await;
    let (id, _) = a_contest_with_entries(&app, "rejudge").await;
    let subs = submission_ids(&app, id).await;

    let verdicts = json!({
        "verdicts": [
            { "submission_id": subs[0], "final_rank": 1 },
            { "submission_id": subs[1], "final_rank": 2 },
            { "submission_id": subs[2], "final_rank": 3 },
        ]
    });

    app.login("rejudgeco").await;
    app.post(&format!("/api/enterprise/contests/{id}/judge"), &verdicts)
        .await;
    let resp = app
        .post(&format!("/api/enterprise/contests/{id}/judge"), &verdicts)
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    // Two identical proofs would double every count that reads them.
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM attestations WHERE contest_id = $1 AND basis = 'contest_finalist'",
    )
    .bind(id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(count, 2);
}

// ═══════════════════════════════════════════════════════════════════
// Interviews and the hire
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn nobody_is_hired_off_an_interview_that_never_happened() {
    let app = TestApp::spawn().await;
    let (id, people) = a_contest_with_entries(&app, "hire").await;
    let subs = submission_ids(&app, id).await;

    app.login("hireco").await;
    app.post(
        &format!("/api/enterprise/contests/{id}/judge"),
        &json!({
            "verdicts": [
                { "submission_id": subs[0], "final_rank": 1 },
                { "submission_id": subs[1], "final_rank": 2 },
                { "submission_id": subs[2], "final_rank": 3 },
            ]
        }),
    )
    .await;

    // The prize was an interview. A hire recorded without one rests on
    // nothing anybody can point at.
    let resp = app
        .post(
            &format!("/api/enterprise/contests/{id}/hire"),
            &json!({ "talent_user_id": people[0], "annual_salary": "30000.00" }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn a_time_that_was_not_offered_cannot_be_booked() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Slotco").await;
    let id = a_contest(&app, &a_recruiting_brief("slot-contest")).await;
    let person = a_talent(&app, "slotperson").await;

    app.login("slotco").await;
    let resp = app
        .post(
            "/api/enterprise/interviews",
            &json!({
                "source_type": "enterprise_contest",
                "source_id": id,
                "talent_user_id": person,
                "platform": "meet",
                "meeting_url": "https://meet.example.test/abc",
                "slots": [
                    { "start": "2027-02-01T09:00:00Z", "end": "2027-02-01T10:00:00Z" },
                    { "start": "2027-02-02T14:00:00Z", "end": "2027-02-02T15:00:00Z" },
                ],
            }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let created: Value = resp.json().await.unwrap();
    let interview = created["data"]["interview"]["id"].as_str().unwrap();

    app.login("slotperson").await;
    // Same start, longer end: accepting this would book an hour where an
    // hour was offered elsewhere.
    let resp = app
        .post(
            &format!("/api/interviews/{interview}/confirm"),
            &json!({
                "slot": { "start": "2027-02-01T09:00:00Z", "end": "2027-02-01T11:00:00Z" }
            }),
        )
        .await;
    assert_eq!(resp.status(), 400);

    let resp = app
        .post(
            &format!("/api/interviews/{interview}/confirm"),
            &json!({
                "slot": { "start": "2027-02-01T09:00:00Z", "end": "2027-02-01T10:00:00Z" }
            }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn an_interview_offering_no_slot_is_refused() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Noslotco").await;
    let id = a_contest(&app, &a_recruiting_brief("noslot-contest")).await;
    let person = a_talent(&app, "noslotperson").await;

    app.login("noslotco").await;
    let resp = app
        .post(
            "/api/enterprise/interviews",
            &json!({
                "source_type": "enterprise_contest",
                "source_id": id,
                "talent_user_id": person,
                "slots": [],
            }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn a_hire_after_a_real_interview_books_the_success_fee() {
    let app = TestApp::spawn().await;
    let (id, people) = a_contest_with_entries(&app, "fullhire").await;
    let subs = submission_ids(&app, id).await;

    app.login("fullhireco").await;
    app.post(
        &format!("/api/enterprise/contests/{id}/judge"),
        &json!({
            "verdicts": [
                { "submission_id": subs[0], "final_rank": 1 },
                { "submission_id": subs[1], "final_rank": 2 },
                { "submission_id": subs[2], "final_rank": 3 },
            ]
        }),
    )
    .await;

    let resp = app
        .post(
            "/api/enterprise/interviews",
            &json!({
                "source_type": "enterprise_contest",
                "source_id": id,
                "talent_user_id": people[0],
                "platform": "phone",
                "slots": [
                    { "start": "2027-02-01T09:00:00Z", "end": "2027-02-01T10:00:00Z" },
                ],
            }),
        )
        .await;
    let created: Value = resp.json().await.unwrap();
    let interview = created["data"]["interview"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    app.login("fullhireentrant0").await;
    app.post(
        &format!("/api/interviews/{interview}/confirm"),
        &json!({
            "slot": { "start": "2027-02-01T09:00:00Z", "end": "2027-02-01T10:00:00Z" }
        }),
    )
    .await;

    app.login("fullhireco").await;
    let resp = app
        .post(
            &format!("/api/enterprise/interviews/{interview}/complete"),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    let resp = app
        .post(
            &format!("/api/enterprise/contests/{id}/hire"),
            &json!({ "talent_user_id": people[0], "annual_salary": "30000.00" }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    // The fee reuses the campaign machinery, pointing at the contest instead.
    let fee: (sqlx::types::BigDecimal, Option<Uuid>) =
        sqlx::query_as("SELECT success_fee_amount, contest_id FROM recruitment_success_fees")
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(fee.0.to_string(), "3000.00");
    assert_eq!(fee.1, Some(id));

    let hired: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM attestations WHERE contest_id = $1 AND basis = 'contest_hired'",
    )
    .bind(id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(hired, 1);
}

// ═══════════════════════════════════════════════════════════════════
// Concluding
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn a_contest_with_unjudged_entries_cannot_conclude() {
    let app = TestApp::spawn().await;
    let (id, _) = a_contest_with_entries(&app, "unjudged").await;

    app.login("unjudgedadmin").await;
    // People spent days on this. Concluding without a verdict leaves them
    // nothing to show for it — not even a rank.
    let resp = app
        .post(&format!("/api/admin/contests/{id}/conclude"), &json!({}))
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn concluding_books_what_skilluv_charged_to_run_it() {
    let app = TestApp::spawn().await;
    let (id, _) = a_contest_with_entries(&app, "conclude").await;
    let subs = submission_ids(&app, id).await;

    app.login("concludeco").await;
    app.post(
        &format!("/api/enterprise/contests/{id}/judge"),
        &json!({
            "verdicts": [
                { "submission_id": subs[0], "final_rank": 1 },
                { "submission_id": subs[1], "final_rank": 2 },
                { "submission_id": subs[2], "final_rank": 3 },
            ]
        }),
    )
    .await;

    app.login("concludeadmin").await;
    let resp = app
        .post(&format!("/api/admin/contests/{id}/conclude"), &json!({}))
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    let booked: sqlx::types::BigDecimal = sqlx::query_scalar(
        "SELECT amount_credits FROM platform_revenues
          WHERE source = 'recruiting_contest_fee'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(booked.to_string(), "500.00");
}

#[tokio::test]
async fn a_recruiting_contest_does_not_end_in_an_engagement() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Outcomeco").await;
    let id = a_contest(&app, &a_recruiting_brief("outcome-contest")).await;

    // It ends in a hire. Pointing it at paid work would mean the winner was
    // engaged rather than employed, which is a different arrangement with
    // different obligations.
    let resp = app
        .post(
            &format!("/api/enterprise/contests/{id}/outcome"),
            &json!({ "engagement_id": Uuid::new_v4() }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}
