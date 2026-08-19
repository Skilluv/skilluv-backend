//! The data line.
//!
//! Every product here describes people who are not the customer, so almost
//! every test below is a test that somebody's answer was respected: no by
//! default, per purpose, revocable, and read fresh at the moment it matters
//! rather than copied into a list that goes stale.

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

/// Consent granted straight in the database, for the tests that need a
/// cohort rather than a session.
async fn consenting(app: &TestApp, count: i64, purpose: &str) -> Vec<Uuid> {
    let mut people = Vec::new();
    for i in 0..count {
        let id = a_talent(app, &format!("{purpose}{i}")).await;
        sqlx::query(
            "INSERT INTO talent_data_consent (user_id, purpose, wording_agreed)
             SELECT $1, $2, description FROM data_purposes WHERE slug = $2",
        )
        .bind(id)
        .bind(purpose)
        .execute(&app.db)
        .await
        .unwrap();
        people.push(id);
    }
    people
}

// ═══════════════════════════════════════════════════════════════════
// Consent
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn the_answer_is_no_until_somebody_says_otherwise() {
    let app = TestApp::spawn().await;
    a_talent(&app, "silentperson").await;
    app.login("silentperson").await;

    let resp = app.get("/api/users/me/data-consent").await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert!(body["data"]["consent"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn agreeing_to_one_purpose_does_not_agree_to_another() {
    let app = TestApp::spawn().await;
    let person = a_talent(&app, "oneperson").await;
    app.login("oneperson").await;

    let resp = app
        .post(
            "/api/users/me/data-consent/public_score_api",
            &json!({ "agree": true }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    // Happy to appear in a score API is not happy to be sold to a bank.
    let commercial: bool =
        sqlx::query_scalar("SELECT has_data_consent($1, 'commercial_licensing')")
            .bind(person)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(!commercial);

    let public: bool = sqlx::query_scalar("SELECT has_data_consent($1, 'public_score_api')")
        .bind(person)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert!(public);
}

#[tokio::test]
async fn the_wording_agreed_to_is_kept_with_the_answer() {
    let app = TestApp::spawn().await;
    let person = a_talent(&app, "wordingperson").await;
    app.login("wordingperson").await;
    app.post(
        "/api/users/me/data-consent/research_licensing",
        &json!({ "agree": true }),
    )
    .await;

    // The purpose description will be improved. Consent to the old wording
    // was not consent to the new one.
    let stored: String = sqlx::query_scalar(
        "SELECT wording_agreed FROM talent_data_consent
          WHERE user_id = $1 AND purpose = 'research_licensing'",
    )
    .bind(person)
    .fetch_one(&app.db)
    .await
    .unwrap();

    sqlx::query("UPDATE data_purposes SET description = 'texte revu' WHERE slug = $1")
        .bind("research_licensing")
        .execute(&app.db)
        .await
        .unwrap();

    let after: String = sqlx::query_scalar(
        "SELECT wording_agreed FROM talent_data_consent
          WHERE user_id = $1 AND purpose = 'research_licensing'",
    )
    .bind(person)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(stored, after);
    assert_ne!(after, "texte revu");
}

#[tokio::test]
async fn withdrawing_keeps_the_row_rather_than_deleting_it() {
    let app = TestApp::spawn().await;
    let person = a_talent(&app, "withdrawperson").await;
    app.login("withdrawperson").await;
    app.post(
        "/api/users/me/data-consent/public_score_api",
        &json!({ "agree": true }),
    )
    .await;
    let resp = app
        .post(
            "/api/users/me/data-consent/public_score_api",
            &json!({ "agree": false }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    // The row proves consent existed for the period a dataset was built in,
    // which is exactly the audit where it matters.
    let row: (Option<chrono::DateTime<chrono::Utc>>,) = sqlx::query_as(
        "SELECT revoked_at FROM talent_data_consent
          WHERE user_id = $1 AND purpose = 'public_score_api'",
    )
    .bind(person)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(row.0.is_some());

    let live: bool = sqlx::query_scalar("SELECT has_data_consent($1, 'public_score_api')")
        .bind(person)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert!(!live);
}

#[tokio::test]
async fn withdrawing_something_never_agreed_to_says_so() {
    let app = TestApp::spawn().await;
    a_talent(&app, "nothingperson").await;
    app.login("nothingperson").await;

    let resp = app
        .post(
            "/api/users/me/data-consent/public_score_api",
            &json!({ "agree": false }),
        )
        .await;
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn a_purpose_nobody_is_asked_about_is_refused() {
    let app = TestApp::spawn().await;
    a_talent(&app, "invperson").await;
    app.login("invperson").await;

    let resp = app
        .post(
            "/api/users/me/data-consent/sell-everything",
            &json!({ "agree": true }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

// ═══════════════════════════════════════════════════════════════════
// The metered API
// ═══════════════════════════════════════════════════════════════════

/// A key on a named plan, and the raw string to present.
async fn an_api_key(app: &TestApp, owner: Uuid, plan: &str) -> String {
    // The developer route mints keys for people; this makes one directly so
    // the plan can be set without a billing flow.
    let raw = format!("sk_test_{}", Uuid::new_v4().simple());
    let prefix: String = raw.chars().take(12).collect();
    let hash = argon2_hash(&raw);

    sqlx::query(
        "INSERT INTO api_keys (user_id, name, key_prefix, key_hash, plan)
         VALUES ($1, 'test', $2, $3, $4)",
    )
    .bind(owner)
    .bind(&prefix)
    .bind(&hash)
    .bind(plan)
    .execute(&app.db)
    .await
    .unwrap();

    raw
}

fn argon2_hash(raw: &str) -> String {
    use argon2::password_hash::{PasswordHasher, SaltString, rand_core::OsRng};
    let salt = SaltString::generate(&mut OsRng);
    argon2::Argon2::default()
        .hash_password(raw.as_bytes(), &salt)
        .unwrap()
        .to_string()
}

#[tokio::test]
async fn the_api_says_nothing_about_somebody_who_did_not_opt_in() {
    let app = TestApp::spawn().await;
    let caller = a_talent(&app, "apicaller").await;
    a_talent(&app, "quietsubject").await;
    let key = an_api_key(&app, caller, "free").await;

    // Not found rather than "private": a directory built from refusals is a
    // directory of everybody who declined.
    let resp = app
        .get_with_header(
            "/api/public/v1/talent-score/quietsubject",
            "x-api-key",
            &key,
        )
        .await;
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn the_api_says_nothing_without_a_key() {
    let app = TestApp::spawn().await;
    a_talent(&app, "keylesssubject").await;

    let resp = app.get("/api/public/v1/talent-score/keylesssubject").await;
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn an_opted_in_profile_is_readable_and_the_free_tier_owes_attribution() {
    let app = TestApp::spawn().await;
    let caller = a_talent(&app, "attribcaller").await;
    let subject = a_talent(&app, "opensubject").await;
    let key = an_api_key(&app, caller, "free").await;

    sqlx::query(
        "INSERT INTO talent_data_consent (user_id, purpose, wording_agreed)
         SELECT $1, 'public_score_api', description FROM data_purposes
          WHERE slug = 'public_score_api'",
    )
    .bind(subject)
    .execute(&app.db)
    .await
    .unwrap();

    let resp = app
        .get_with_header("/api/public/v1/talent-score/opensubject", "x-api-key", &key)
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["username"], "opensubject");
    assert_eq!(body["meta"]["attribution_required"], true);
}

#[tokio::test]
async fn withdrawing_closes_the_api_immediately() {
    let app = TestApp::spawn().await;
    let caller = a_talent(&app, "revcaller").await;
    a_talent(&app, "revsubject").await;
    let key = an_api_key(&app, caller, "free").await;

    app.login("revsubject").await;
    app.post(
        "/api/users/me/data-consent/public_score_api",
        &json!({ "agree": true }),
    )
    .await;

    let resp = app
        .get_with_header("/api/public/v1/talent-score/revsubject", "x-api-key", &key)
        .await;
    assert_eq!(resp.status(), 200);

    app.post(
        "/api/users/me/data-consent/public_score_api",
        &json!({ "agree": false }),
    )
    .await;

    // Read fresh at the moment it matters, not from a cached list.
    let resp = app
        .get_with_header("/api/public/v1/talent-score/revsubject", "x-api-key", &key)
        .await;
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn a_key_over_its_daily_ceiling_is_refused_with_a_wait() {
    let app = TestApp::spawn().await;
    let caller = a_talent(&app, "throttlecaller").await;
    a_talent(&app, "throttlesubject").await;
    let key = an_api_key(&app, caller, "free").await;

    // The free ceiling is a hundred a day. Spend it without making a hundred
    // calls: the ceiling is the thing under test, not the loop.
    sqlx::query(
        "INSERT INTO api_usage_daily (api_key_id, used_on, requests)
         SELECT id, CURRENT_DATE, 100 FROM api_keys WHERE user_id = $1",
    )
    .bind(caller)
    .execute(&app.db)
    .await
    .unwrap();

    let resp = app
        .get_with_header(
            "/api/public/v1/talent-score/throttlesubject",
            "x-api-key",
            &key,
        )
        .await;
    assert_eq!(resp.status(), 429);

    // The refusal is counted separately, so a client asking why it stopped
    // working gets an answer rather than a shrug.
    let throttled: i32 = sqlx::query_scalar(
        "SELECT throttled FROM api_usage_daily u
           JOIN api_keys k ON k.id = u.api_key_id
          WHERE k.user_id = $1 AND u.used_on = CURRENT_DATE",
    )
    .bind(caller)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(throttled, 1);
}

// ═══════════════════════════════════════════════════════════════════
// Reports and licences
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn a_report_resting_on_too_few_people_is_not_delivered() {
    let app = TestApp::spawn().await;
    an_admin(&app, "reportadmin").await;
    consenting(&app, 5, "research_licensing").await;
    app.login("reportadmin").await;

    let resp = app
        .post(
            "/api/admin/data/reports",
            &json!({
                "client_type": "development_bank",
                "client_org": "Banque de développement",
                "title": "État du talent tech",
                "scope_md": "Répartition par métier et par pays.",
                "fee": "30000.00",
            }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let created: Value = resp.json().await.unwrap();
    let id = created["data"]["report"]["id"].as_str().unwrap();

    // Five people is a chart that names those five, whatever the header says.
    let resp = app
        .post(
            &format!("/api/admin/data/reports/{id}/deliver"),
            &json!({
                "document_url": "https://example.test/report.pdf",
                "purpose": "research_licensing",
            }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn a_report_over_the_floor_is_delivered_and_booked() {
    let app = TestApp::spawn().await;
    an_admin(&app, "bigreportadmin").await;
    consenting(&app, 30, "research_licensing").await;
    app.login("bigreportadmin").await;

    let resp = app
        .post(
            "/api/admin/data/reports",
            &json!({
                "client_type": "government",
                "client_org": "Ministère du numérique",
                "title": "Écart de compétences 2027",
                "scope_md": "Métiers en tension et volumes formés.",
                "fee": "15000.00",
            }),
        )
        .await;
    let created: Value = resp.json().await.unwrap();
    let id = created["data"]["report"]["id"].as_str().unwrap();

    let resp = app
        .post(
            &format!("/api/admin/data/reports/{id}/deliver"),
            &json!({
                "document_url": "https://example.test/gap.pdf",
                "purpose": "research_licensing",
            }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    let booked: sqlx::types::BigDecimal = sqlx::query_scalar(
        "SELECT amount_credits FROM platform_revenues WHERE source = 'intelligence_report'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    common::assert_decimal(&booked, "15000.00");
}

#[tokio::test]
async fn a_licence_over_an_empty_cohort_is_refused() {
    let app = TestApp::spawn().await;
    an_admin(&app, "emptylicenceadmin").await;

    // There is nothing here that can honestly be licensed.
    let resp = app
        .post(
            "/api/admin/data/licences",
            &json!({
                "licensee_org": "Laboratoire X",
                "licensee_type": "research_lab",
                "purpose": "research_licensing",
                "contract_purpose_md": "Étude sur les parcours de reconversion.",
                "starts_on": "2027-01-01",
                "total_fee": "20000.00",
                "talents_share_percent": "0.00",
            }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn a_commercial_licence_has_to_pay_the_people_in_it() {
    let app = TestApp::spawn().await;
    an_admin(&app, "commlicenceadmin").await;
    consenting(&app, 30, "commercial_licensing").await;
    app.login("commlicenceadmin").await;

    // Zero is defensible for a public research dataset. It is not for a sale.
    let resp = app
        .post(
            "/api/admin/data/licences",
            &json!({
                "licensee_org": "Recruteur SA",
                "licensee_type": "enterprise",
                "purpose": "commercial_licensing",
                "contract_purpose_md": "Ciblage de campagnes de recrutement.",
                "starts_on": "2027-01-01",
                "total_fee": "50000.00",
                "talents_share_percent": "0.00",
                "contract_url": "https://example.test/licence.pdf",
            }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn settling_pays_everybody_currently_consenting_and_nobody_who_withdrew() {
    let app = TestApp::spawn().await;
    an_admin(&app, "settleadmin").await;
    let people = consenting(&app, 30, "commercial_licensing").await;
    app.login("settleadmin").await;

    let resp = app
        .post(
            "/api/admin/data/licences",
            &json!({
                "licensee_org": "Recruteur SA",
                "licensee_type": "enterprise",
                "purpose": "commercial_licensing",
                "contract_purpose_md": "Ciblage de campagnes de recrutement.",
                "starts_on": "2027-01-01",
                "total_fee": "30000.00",
                "talents_share_percent": "1.00",
                "contract_url": "https://example.test/licence.pdf",
            }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let created: Value = resp.json().await.unwrap();
    let licence = created["data"]["licence"]["id"].as_str().unwrap();

    // One of them changes their mind before the settlement.
    sqlx::query(
        "UPDATE talent_data_consent SET revoked_at = NOW()
          WHERE user_id = $1 AND purpose = 'commercial_licensing'",
    )
    .bind(people[0])
    .execute(&app.db)
    .await
    .unwrap();

    let resp = app
        .post(
            &format!("/api/admin/data/licences/{licence}/settle"),
            &json!({ "period_start": "2027-01-01", "period_end": "2027-04-01" }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();

    // Twenty-nine, not thirty: the cohort is read at settlement, never from
    // a stored list that would still be paying somebody who withdrew.
    assert_eq!(body["data"]["people_paid"], 29);

    let paid_to_leaver: i64 =
        sqlx::query_scalar("SELECT count(*) FROM talent_data_royalties WHERE user_id = $1")
            .bind(people[0])
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(paid_to_leaver, 0);
}

#[tokio::test]
async fn settling_the_same_period_twice_does_not_pay_twice() {
    let app = TestApp::spawn().await;
    an_admin(&app, "twiceadmin").await;
    consenting(&app, 30, "commercial_licensing").await;
    app.login("twiceadmin").await;

    let resp = app
        .post(
            "/api/admin/data/licences",
            &json!({
                "licensee_org": "Recruteur SA",
                "licensee_type": "enterprise",
                "purpose": "commercial_licensing",
                "contract_purpose_md": "Ciblage.",
                "starts_on": "2027-01-01",
                "total_fee": "30000.00",
                "talents_share_percent": "1.00",
                "contract_url": "https://example.test/l.pdf",
            }),
        )
        .await;
    let created: Value = resp.json().await.unwrap();
    let licence = created["data"]["licence"]["id"].as_str().unwrap();

    let period = json!({ "period_start": "2027-01-01", "period_end": "2027-04-01" });
    app.post(
        &format!("/api/admin/data/licences/{licence}/settle"),
        &period,
    )
    .await;
    let resp = app
        .post(
            &format!("/api/admin/data/licences/{licence}/settle"),
            &period,
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["people_paid"], 0, "the second run pays nobody");

    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM talent_data_royalties")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(rows, 30);
}

// ═══════════════════════════════════════════════════════════════════
// White-label
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn only_a_government_partner_can_recognise_anything_officially() {
    let app = TestApp::spawn().await;
    an_admin(&app, "wladmin").await;

    // A bootcamp saying so is a claim, not a recognition.
    let resp = app
        .post(
            "/api/admin/data/deployments",
            &json!({
                "partner_org": "Bootcamp X",
                "partner_type": "bootcamp",
                "deployment_host": "learn.bootcamp-x.test",
                "official_recognition_scope": ["digital_skills_certification"],
                "contract_url": "https://example.test/c.pdf",
            }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn official_recognition_rests_on_a_signed_contract() {
    let app = TestApp::spawn().await;
    an_admin(&app, "govadmin").await;

    // Without one it is a claim, and the people carrying the attestation are
    // the ones who find out it was worthless.
    let mut body = json!({
        "partner_org": "Ministère du numérique",
        "partner_type": "government",
        "country": "BJ",
        "deployment_host": "competences.gouv.test",
        "official_recognition_scope": ["digital_skills_certification"],
        "annual_fee": "150000.00",
    });
    let resp = app.post("/api/admin/data/deployments", &body).await;
    assert_eq!(resp.status(), 400);

    body["contract_url"] = json!("https://example.test/convention.pdf");
    let resp = app.post("/api/admin/data/deployments", &body).await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn a_deployment_goes_live_on_a_contract_not_on_an_intention() {
    let app = TestApp::spawn().await;
    an_admin(&app, "liveadmin").await;

    let resp = app
        .post(
            "/api/admin/data/deployments",
            &json!({
                "partner_org": "Université Y",
                "partner_type": "university",
                "deployment_host": "skills.univ-y.test",
                "setup_fee": "20000.00",
                "monthly_fee": "2000.00",
            }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let created: Value = resp.json().await.unwrap();
    let id = created["data"]["deployment"]["id"].as_str().unwrap();

    let resp = app
        .post(
            &format!("/api/admin/data/deployments/{id}/go-live"),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

// ═══════════════════════════════════════════════════════════════════
// The unified profile
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn a_partner_cannot_be_named_before_the_profile_is_agreed_to() {
    let app = TestApp::spawn().await;
    a_talent(&app, "partnerperson").await;
    app.login("partnerperson").await;

    // Naming a partner for something you have not allowed is not a decision
    // anybody could act on.
    let resp = app
        .post(
            "/api/users/me/identity-partners",
            &json!({ "partner_slug": "banque-x", "allow": true }),
        )
        .await;
    assert_eq!(resp.status(), 400);

    app.post(
        "/api/users/me/data-consent/identity_aggregation",
        &json!({ "agree": true }),
    )
    .await;

    let resp = app
        .post(
            "/api/users/me/identity-partners",
            &json!({ "partner_slug": "banque-x", "allow": true }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["partners"], json!(["banque-x"]));
}

#[tokio::test]
async fn a_unified_score_names_the_sources_it_rests_on() {
    let app = TestApp::spawn().await;
    let person = a_talent(&app, "unifiedperson").await;

    sqlx::query(
        "INSERT INTO craft_scores (user_id, skill_domain, score) VALUES ($1, 'code', 1200)",
    )
    .bind(person)
    .execute(&app.db)
    .await
    .unwrap();

    app.login("unifiedperson").await;
    let resp = app.get("/api/users/me/unified-profile").await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();

    // A score built from one platform and one built from six are different
    // claims, and a reader should be able to tell them apart.
    assert_eq!(body["data"]["profile"]["aggregate_score"], 1200);
    assert_eq!(
        body["data"]["profile"]["platforms_covered"],
        json!(["skilluv"])
    );
    assert_eq!(body["data"]["profile"]["breakdown"]["craft_score"], 1200);
}

#[tokio::test]
async fn the_cohort_report_says_which_purposes_can_be_published_at_all() {
    let app = TestApp::spawn().await;
    an_admin(&app, "cohortadmin").await;
    consenting(&app, 30, "research_licensing").await;
    app.login("cohortadmin").await;

    let resp = app.get("/api/admin/data/cohorts").await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let cohorts = body["data"]["cohorts"].as_array().unwrap();

    let research = cohorts
        .iter()
        .find(|c| c["purpose"] == "research_licensing")
        .unwrap();
    assert_eq!(research["people"], 30);
    assert_eq!(research["publishable"], true);

    let commercial = cohorts
        .iter()
        .find(|c| c["purpose"] == "commercial_licensing")
        .unwrap();
    assert_eq!(commercial["people"], 0);
    assert_eq!(commercial["publishable"], false);
}
