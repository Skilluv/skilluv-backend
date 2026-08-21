//! Recruitment Skilluv runs: the brief, the consent, the fee, the guarantee.

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
        "title": title,
        "brief_md": "Nous cherchons quelqu'un pour reprendre notre backend.",
        "target_role": "Développeur backend",
        "target_domain": "code",
        "target_orientations": ["web-backend-developer"],
        "success_fee_percent": "10.00",
    })
}

async fn open_campaign(app: &TestApp, body: &Value) -> Uuid {
    let resp = app
        .post("/api/enterprise/recruitment/campaigns", body)
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let created: Value = resp.json().await.unwrap();
    created["data"]["campaign"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

// ═══════════════════════════════════════════════════════════════════
// The brief
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn an_empty_brief_cannot_be_sourced_against() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Emptybriefco").await;

    let mut body = a_brief("Vide");
    body["brief_md"] = json!("   ");
    let resp = app
        .post("/api/enterprise/recruitment/campaigns", &body)
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn a_brief_targeting_a_typo_is_refused() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Typobriefco").await;

    // A brief targeting a trade nobody has sources against nothing, and
    // nobody finds out until the shortlist is empty.
    let mut body = a_brief("Typo");
    body["target_orientations"] = json!(["metier-invente"]);
    let resp = app
        .post("/api/enterprise/recruitment/campaigns", &body)
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn a_renamed_trade_is_still_a_valid_target() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Renamedco").await;

    let mut body = a_brief("Ancien vocabulaire");
    body["target_orientations"] = json!(["dev-backend"]);
    let resp = app
        .post("/api/enterprise/recruitment/campaigns", &body)
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn the_volume_discount_is_applied_not_typed() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Volumeco").await;

    let mut body = a_brief("Cinq personnes");
    body["kind"] = json!("volume");
    body["positions_count"] = json!(10);
    let id = open_campaign(&app, &body).await;

    // 10 % rate, 20 % discount at ten positions, so 8 %. A discount somebody
    // enters by hand eventually disagrees with the scale it came from.
    let (rate, discount): (f64, f64) = sqlx::query_as(
        "SELECT success_fee_percent::FLOAT8, volume_discount_percent::FLOAT8
           FROM recruitment_campaigns WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(discount, 20.0);
    assert_eq!(rate, 8.0);
}

#[tokio::test]
async fn one_position_is_not_a_volume_campaign() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Onepositionco").await;

    let mut body = a_brief("Une seule");
    body["kind"] = json!("volume");
    body["positions_count"] = json!(1);
    let resp = app
        .post("/api/enterprise/recruitment/campaigns", &body)
        .await;
    assert_eq!(
        resp.status(),
        400,
        "one position at a reduced rate is a discount, not a volume campaign"
    );
}

#[tokio::test]
async fn a_retained_pool_charges_no_success_fee() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Poolco").await;

    // The client already pays monthly to keep it warm; charging on success
    // too would be charging twice for one hire.
    let mut body = a_brief("Vivier");
    body["kind"] = json!("private_pool");
    body["monthly_fee"] = json!("800.00");
    body["refresh_cadence_days"] = json!(30);
    let id = open_campaign(&app, &body).await;

    let fee: Option<f64> = sqlx::query_scalar(
        "SELECT success_fee_percent::FLOAT8 FROM recruitment_campaigns WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(fee.is_none());
}

#[tokio::test]
async fn a_pool_without_a_cadence_is_refused() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Nocadenceco").await;

    let mut body = a_brief("Vivier sans entretien");
    body["kind"] = json!("private_pool");
    body["monthly_fee"] = json!("800.00");
    let resp = app
        .post("/api/enterprise/recruitment/campaigns", &body)
        .await;
    assert_eq!(resp.status(), 400);
}

// ═══════════════════════════════════════════════════════════════════
// Consent
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn a_shortlist_entry_must_argue_for_the_person() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Argueco").await;
    let campaign = open_campaign(&app, &a_brief("Avec argument")).await;
    let talent = a_talent(&app, "argue_talent").await;
    an_admin(&app, "argue_admin").await;

    let silent = app
        .post(
            &format!("/api/admin/recruitment/campaigns/{campaign}/shortlist"),
            &json!({"talent_user_id": talent, "match_reason_md": "   "}),
        )
        .await;
    assert_eq!(
        silent.status(),
        400,
        "a list of names with no argument is a search result"
    );

    let argued = app
        .post(
            &format!("/api/admin/recruitment/campaigns/{campaign}/shortlist"),
            &json!({
                "talent_user_id": talent,
                "match_reason_md": "Trois contributions fusionnées sur des projets Rust.",
            }),
        )
        .await;
    assert_eq!(argued.status(), 200, "{}", argued.text().await.unwrap());
}

#[tokio::test]
async fn a_client_never_sees_somebody_who_has_not_answered() {
    let app = TestApp::spawn().await;
    let owner = an_enterprise(&app, "Consentco").await;
    let campaign = open_campaign(&app, &a_brief("Consentement")).await;

    let silent = a_talent(&app, "consent_silent").await;
    let willing = a_talent(&app, "consent_willing").await;
    an_admin(&app, "consent_admin").await;

    for talent in [silent, willing] {
        app.post(
            &format!("/api/admin/recruitment/campaigns/{campaign}/shortlist"),
            &json!({"talent_user_id": talent, "match_reason_md": "un bon profil"}),
        )
        .await;
    }

    app.login("consent_willing").await;
    let answered = app
        .post(
            &format!("/api/recruitment/campaigns/{campaign}/respond"),
            &json!({"interested": true}),
        )
        .await;
    assert_eq!(answered.status(), 200);

    app.relogin_with_totp(&owner).await;
    let body: Value = app
        .get(&format!(
            "/api/enterprise/recruitment/campaigns/{campaign}/shortlist"
        ))
        .await
        .json()
        .await
        .unwrap();
    let shortlist = body["data"]["shortlist"].as_array().unwrap();

    // Presenting somebody who has not agreed is how a platform burns the
    // trust it runs on.
    assert_eq!(shortlist.len(), 1);
    assert_eq!(shortlist[0]["username"], "consent_willing");
}

#[tokio::test]
async fn nobody_can_be_advanced_without_having_answered() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Advanceco").await;
    let campaign = open_campaign(&app, &a_brief("Sans réponse")).await;
    let talent = a_talent(&app, "advance_talent").await;
    an_admin(&app, "advance_admin").await;

    app.post(
        &format!("/api/admin/recruitment/campaigns/{campaign}/shortlist"),
        &json!({"talent_user_id": talent, "match_reason_md": "un bon profil"}),
    )
    .await;

    // Straight to the database: even an admin with SQL access is stopped,
    // because the rule is a trigger rather than a service check.
    let forced = sqlx::query(
        "UPDATE recruitment_shortlist SET status = 'interviewed'
          WHERE campaign_id = $1 AND talent_user_id = $2",
    )
    .bind(campaign)
    .bind(talent)
    .execute(&app.db)
    .await;
    assert!(forced.is_err());
}

#[tokio::test]
async fn an_invitation_says_who_is_asking_and_for_what() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Askingco").await;
    let campaign = open_campaign(&app, &a_brief("Poste précis")).await;
    let talent = a_talent(&app, "invited_talent").await;
    an_admin(&app, "invite_admin").await;

    app.post(
        &format!("/api/admin/recruitment/campaigns/{campaign}/shortlist"),
        &json!({"talent_user_id": talent, "match_reason_md": "profil correspondant"}),
    )
    .await;

    app.login("invited_talent").await;
    let body: Value = app
        .get("/api/users/me/recruitment-invitations")
        .await
        .json()
        .await
        .unwrap();
    let invitations = body["data"]["invitations"].as_array().unwrap();
    assert_eq!(invitations.len(), 1);

    // Asking somebody to agree to "an opportunity" is how people stop
    // answering.
    assert_eq!(invitations[0]["company_name"], "Askingco");
    assert!(!invitations[0]["brief_md"].as_str().unwrap().is_empty());
    assert_eq!(invitations[0]["my_status"], "proposed");
}

// ═══════════════════════════════════════════════════════════════════
// The fee and the guarantee
// ═══════════════════════════════════════════════════════════════════

async fn a_hire(app: &TestApp, company: &str, talent_name: &str) -> (Uuid, Uuid) {
    let owner = an_enterprise(app, company).await;
    let campaign = open_campaign(app, &a_brief("Recrutement")).await;
    let talent = a_talent(app, talent_name).await;
    an_admin(app, &format!("{talent_name}_admin")).await;

    app.post(
        &format!("/api/admin/recruitment/campaigns/{campaign}/shortlist"),
        &json!({"talent_user_id": talent, "match_reason_md": "profil correspondant"}),
    )
    .await;

    app.login(talent_name).await;
    app.post(
        &format!("/api/recruitment/campaigns/{campaign}/respond"),
        &json!({"interested": true}),
    )
    .await;

    app.relogin_with_totp(&owner).await;
    let resp = app
        .post(
            &format!("/api/enterprise/recruitment/campaigns/{campaign}/hired"),
            &json!({
                "talent_user_id": talent,
                "annual_salary": "40000.00",
                "currency": "EUR",
            }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    let fee_id = body["data"]["success_fee_id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    (campaign, fee_id)
}

#[tokio::test]
async fn a_hire_books_its_fee_in_the_revenue_ledger() {
    let app = TestApp::spawn().await;
    let (_, fee_id) = a_hire(&app, "Hireco", "hired_talent").await;

    // 10 % of 40 000.
    let amount: f64 = sqlx::query_scalar(
        "SELECT success_fee_amount::FLOAT8 FROM recruitment_success_fees WHERE id = $1",
    )
    .bind(fee_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(amount, 4000.0);

    // Booked when charged, not when collected: the ledger's job is to say
    // what was earned.
    let booked: f64 = sqlx::query_scalar(
        "SELECT COALESCE(sum(amount_credits), 0)::FLOAT8 FROM platform_revenues
          WHERE source = 'recruitment_success_fee'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(booked, 4000.0);
}

#[tokio::test]
async fn leaving_early_refunds_the_whole_fee() {
    let app = TestApp::spawn().await;
    let (_, fee_id) = a_hire(&app, "Earlyco", "early_leaver").await;
    an_admin(&app, "early_refund_admin").await;

    // Two weeks of a six-month guarantee: the placement did not happen in
    // any meaningful sense.
    let left_at = chrono::Utc::now() + chrono::Duration::days(14);
    let resp = app
        .post(
            &format!("/api/admin/recruitment/fees/{fee_id}/departure"),
            &json!({
                "left_at": left_at.to_rfc3339(),
                "reason": "la personne est partie au bout de deux semaines",
            }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    common::assert_amount(&body["data"]["refund_amount"], "4000.00");
}

#[tokio::test]
async fn leaving_after_the_guarantee_refunds_nothing() {
    let app = TestApp::spawn().await;
    let (_, fee_id) = a_hire(&app, "Lateco", "late_leaver").await;
    an_admin(&app, "late_refund_admin").await;

    let left_at = chrono::Utc::now() + chrono::Duration::days(300);
    let resp = app
        .post(
            &format!("/api/admin/recruitment/fees/{fee_id}/departure"),
            &json!({
                "left_at": left_at.to_rfc3339(),
                "reason": "départ après la période de garantie",
            }),
        )
        .await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    common::assert_amount(&body["data"]["refund_amount"], "0.00");
}

#[tokio::test]
async fn a_refund_needs_a_reason_the_client_can_read() {
    let app = TestApp::spawn().await;
    let (_, fee_id) = a_hire(&app, "Reasonco", "reason_leaver").await;
    an_admin(&app, "reason_refund_admin").await;

    let resp = app
        .post(
            &format!("/api/admin/recruitment/fees/{fee_id}/departure"),
            &json!({"left_at": chrono::Utc::now().to_rfc3339(), "reason": "  "}),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn a_fee_is_refunded_once() {
    let app = TestApp::spawn().await;
    let (_, fee_id) = a_hire(&app, "Onceco", "once_leaver").await;
    an_admin(&app, "once_refund_admin").await;

    let left_at = (chrono::Utc::now() + chrono::Duration::days(20)).to_rfc3339();
    let first = app
        .post(
            &format!("/api/admin/recruitment/fees/{fee_id}/departure"),
            &json!({"left_at": left_at, "reason": "départ"}),
        )
        .await;
    assert_eq!(first.status(), 200);

    let again = app
        .post(
            &format!("/api/admin/recruitment/fees/{fee_id}/departure"),
            &json!({"left_at": left_at, "reason": "départ"}),
        )
        .await;
    assert_eq!(again.status(), 400);
}

#[tokio::test]
async fn a_campaign_belongs_to_the_client_who_briefed_it() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Ownerbriefco").await;
    let campaign = open_campaign(&app, &a_brief("Privé")).await;

    an_enterprise(&app, "Nosybriefco").await;
    assert_eq!(
        app.get(&format!(
            "/api/enterprise/recruitment/campaigns/{campaign}/shortlist"
        ))
        .await
        .status(),
        403
    );
}

#[tokio::test]
async fn an_unassigned_campaign_sorts_first_for_the_people_who_do_them() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Queueco").await;
    open_campaign(&app, &a_brief("En attente")).await;
    an_admin(&app, "queue_admin").await;

    let body: Value = app
        .get("/api/admin/recruitment/campaigns")
        .await
        .json()
        .await
        .unwrap();
    let campaigns = body["data"]["campaigns"].as_array().unwrap();
    assert_eq!(campaigns.len(), 1);
    // A campaign with nobody on it is a campaign nobody is doing.
    assert_eq!(campaigns[0]["unassigned"], true);
}
