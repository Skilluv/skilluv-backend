//! Onboarding as a service, living labs, and proposals that start with the
//! team.
//!
//! The recurring shape: somebody who is not the customer has to agree before
//! anything happens to them, and the money follows the agreement.

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

// ═══════════════════════════════════════════════════════════════════
// Onboarding
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn an_onboarding_mentor_has_a_floor() {
    let app = TestApp::spawn().await;
    let junior = a_talent(&app, "newhire", "apprenti").await;
    let weak = a_talent(&app, "weakmentor", "ranger").await;
    an_enterprise(&app, "Floorhireco").await;

    // This is somebody's first three months in a job.
    let resp = app
        .post(
            "/api/enterprise/onboardings",
            &json!({
                "junior_user_id": junior,
                "mentor_user_id": weak,
                "fee": "6000.00",
            }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn nothing_starts_and_nothing_is_paid_until_the_person_agrees() {
    let app = TestApp::spawn().await;
    let junior = a_talent(&app, "consentjunior", "apprenti").await;
    let mentor = a_talent(&app, "consentmentor", "maitre").await;
    an_enterprise(&app, "Consenthireco").await;

    let resp = app
        .post(
            "/api/enterprise/onboardings",
            &json!({
                "junior_user_id": junior,
                "mentor_user_id": mentor,
                "fee": "6000.00",
            }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let created: Value = resp.json().await.unwrap();
    let id = created["data"]["onboarding"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(created["data"]["onboarding"]["status"], "proposed");

    let booked: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM platform_revenues WHERE source = 'onboarding_service'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(booked, 0);

    // Their employer bought it; that does not make it consented to.
    app.login("consentjunior").await;
    let resp = app
        .post(
            &format!("/api/onboardings/{id}/respond"),
            &json!({ "accept": true }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["onboarding"]["status"], "active");

    // 6000 at 60% to the mentor: 2400 kept.
    let booked: sqlx::types::BigDecimal = sqlx::query_scalar(
        "SELECT amount_credits FROM platform_revenues WHERE source = 'onboarding_service'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    common::assert_decimal(&booked, "2400.00");
}

#[tokio::test]
async fn declining_costs_nothing() {
    let app = TestApp::spawn().await;
    let junior = a_talent(&app, "declinejunior", "apprenti").await;
    let mentor = a_talent(&app, "declinementor", "maitre").await;
    an_enterprise(&app, "Declinehireco").await;

    let resp = app
        .post(
            "/api/enterprise/onboardings",
            &json!({
                "junior_user_id": junior,
                "mentor_user_id": mentor,
                "fee": "6000.00",
            }),
        )
        .await;
    let created: Value = resp.json().await.unwrap();
    let id = created["data"]["onboarding"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    app.login("declinejunior").await;
    let resp = app
        .post(
            &format!("/api/onboardings/{id}/respond"),
            &json!({ "accept": false }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    let booked: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM platform_revenues WHERE source = 'onboarding_service'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(booked, 0);
}

#[tokio::test]
async fn nothing_is_recorded_about_somebody_who_never_agreed() {
    let app = TestApp::spawn().await;
    an_admin(&app, "retentionadmin").await;
    let junior = a_talent(&app, "unaskedjunior", "apprenti").await;
    let mentor = a_talent(&app, "unaskedmentor", "maitre").await;
    an_enterprise(&app, "Unaskedco").await;

    let resp = app
        .post(
            "/api/enterprise/onboardings",
            &json!({
                "junior_user_id": junior,
                "mentor_user_id": mentor,
                "fee": "6000.00",
            }),
        )
        .await;
    let created: Value = resp.json().await.unwrap();
    let id = created["data"]["onboarding"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Whether somebody is still in a job is a fact about them their employer
    // benefits from. Not for an engagement they never agreed to.
    app.login("retentionadmin").await;
    let resp = app
        .post(
            &format!("/api/admin/onboardings/{id}/retention"),
            &json!({ "months": 3, "still_there": true }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn both_sides_can_write_a_check_in() {
    let app = TestApp::spawn().await;
    let junior = a_talent(&app, "checkjunior", "apprenti").await;
    let mentor = a_talent(&app, "checkmentor", "maitre").await;
    an_enterprise(&app, "Checkinco").await;

    let resp = app
        .post(
            "/api/enterprise/onboardings",
            &json!({
                "junior_user_id": junior,
                "mentor_user_id": mentor,
                "fee": "6000.00",
            }),
        )
        .await;
    let created: Value = resp.json().await.unwrap();
    let id = created["data"]["onboarding"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    app.login("checkjunior").await;
    app.post(
        &format!("/api/onboardings/{id}/respond"),
        &json!({ "accept": true }),
    )
    .await;

    // An onboarding assessed only by the person paid to deliver it assesses
    // itself.
    let resp = app
        .post(
            &format!("/api/onboardings/{id}/check-in"),
            &json!({ "month_number": 1, "notes_md": "Bon accueil, peu de contexte métier." }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    app.login("checkmentor").await;
    let resp = app
        .post(
            &format!("/api/onboardings/{id}/check-in"),
            &json!({
                "month_number": 1,
                "notes_md": "Prend le code en main vite ; manque de repères sur le domaine.",
                "going_well": true,
            }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    let both: (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT junior_notes_md, mentor_notes_md FROM onboarding_check_ins
          WHERE engagement_id = $1::uuid AND month_number = 1",
    )
    .bind(&id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(both.0.is_some());
    assert!(both.1.is_some());

    // And a stranger cannot.
    a_talent(&app, "checkstranger", "maitre").await;
    app.login("checkstranger").await;
    let resp = app
        .post(
            &format!("/api/onboardings/{id}/check-in"),
            &json!({ "month_number": 2, "notes_md": "Je passais par là." }),
        )
        .await;
    assert_eq!(resp.status(), 404);
}

// ═══════════════════════════════════════════════════════════════════
// Living labs
// ═══════════════════════════════════════════════════════════════════

fn a_lab() -> Value {
    json!({
        "product_name": "Console v2",
        "scope_md": "Tester le nouveau tableau de bord et remonter ce qui bloque.",
        "community_target": 30,
        "activity_types": ["user_testing", "feedback_sessions"],
        "monthly_fee": "5000.00",
        "monthly_reward_pool": "1000.00",
    })
}

#[tokio::test]
async fn a_lab_with_no_reward_pool_is_refused() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Freelabco").await;

    // Otherwise it is a company asking a hundred people to work on its
    // product for the pleasure of it, with Skilluv charging for arranging it.
    let mut body = a_lab();
    body["monthly_reward_pool"] = json!("0.00");
    let resp = app.post("/api/enterprise/labs", &body).await;
    assert_eq!(resp.status(), 400);
}

async fn an_open_lab(app: &TestApp, company: &str) -> String {
    an_enterprise(app, company).await;
    let resp = app.post("/api/enterprise/labs", &a_lab()).await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let created: Value = resp.json().await.unwrap();
    let id = created["data"]["lab"]["id"].as_str().unwrap().to_string();
    sqlx::query("UPDATE living_lab_engagements SET status = 'recruiting' WHERE id = $1::uuid")
        .bind(&id)
        .execute(&app.db)
        .await
        .unwrap();
    id
}

#[tokio::test]
async fn a_contribution_has_to_be_an_activity_the_lab_asked_for() {
    let app = TestApp::spawn().await;
    let lab = an_open_lab(&app, "Activityco").await;

    a_talent(&app, "labtester", "ranger").await;
    app.login("labtester").await;
    app.post(&format!("/api/labs/{lab}/join"), &json!({})).await;

    let resp = app
        .post(
            &format!("/api/labs/{lab}/contributions"),
            &json!({
                "activity_type": "code_review",
                "summary_md": "J'ai relu le code, ce n'est pas ce qui était demandé.",
            }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn a_quiet_month_pays_the_people_who_showed_up_more() {
    let app = TestApp::spawn().await;
    an_admin(&app, "labadmin").await;
    let lab = an_open_lab(&app, "Poolco").await;

    // Two people contribute out of a pool of 1000.
    for i in 0..2 {
        let name = format!("labperson{i}");
        a_talent(&app, &name, "ranger").await;
        app.login(&name).await;
        app.post(&format!("/api/labs/{lab}/join"), &json!({})).await;
        let resp = app
            .post(
                &format!("/api/labs/{lab}/contributions"),
                &json!({
                    "activity_type": "user_testing",
                    "summary_md": "Le tableau de bord met huit secondes à charger sur \
                                   une connexion mobile.",
                }),
            )
            .await;
        assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    }

    let contributions: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM living_lab_contributions WHERE lab_id = $1::uuid")
            .bind(&lab)
            .fetch_all(&app.db)
            .await
            .unwrap();

    app.login("labadmin").await;
    for id in &contributions {
        app.post(
            &format!("/api/admin/lab-contributions/{id}/judge"),
            &json!({ "accept": true }),
        )
        .await;
    }

    let today = chrono::Utc::now().date_naive().to_string();
    let resp = app
        .post(
            &format!("/api/admin/labs/{lab}/settle"),
            &json!({ "month": today }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();

    // The point of a pool rather than a per-item rate.
    assert_eq!(body["data"]["contributions_paid"], 2);
    common::assert_amount(&body["data"]["each"], "500.00");
}

#[tokio::test]
async fn a_refused_contribution_carries_a_reason_and_is_not_paid() {
    let app = TestApp::spawn().await;
    an_admin(&app, "refuselabadmin").await;
    let lab = an_open_lab(&app, "Refuselabco").await;

    a_talent(&app, "refusedtester", "ranger").await;
    app.login("refusedtester").await;
    app.post(&format!("/api/labs/{lab}/join"), &json!({})).await;
    let resp = app
        .post(
            &format!("/api/labs/{lab}/contributions"),
            &json!({
                "activity_type": "user_testing",
                "summary_md": "Rien à signaler, tout fonctionne parfaitement partout.",
            }),
        )
        .await;
    let created: Value = resp.json().await.unwrap();
    let contribution = created["data"]["contribution_id"]
        .as_str()
        .unwrap()
        .to_string();

    app.login("refuselabadmin").await;
    // Somebody spent an evening on this and the pool is what they were
    // promised for it.
    let resp = app
        .post(
            &format!("/api/admin/lab-contributions/{contribution}/judge"),
            &json!({ "accept": false }),
        )
        .await;
    assert_eq!(resp.status(), 400);

    let resp = app
        .post(
            &format!("/api/admin/lab-contributions/{contribution}/judge"),
            &json!({ "accept": false, "reason": "Aucun élément vérifiable." }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    let forced =
        sqlx::query("UPDATE living_lab_contributions SET paid_at = NOW() WHERE id = $1::uuid")
            .bind(&contribution)
            .execute(&app.db)
            .await;
    assert!(forced.is_err());
}

// ═══════════════════════════════════════════════════════════════════
// Team proposals
// ═══════════════════════════════════════════════════════════════════

fn a_proposal(slug: &str) -> Value {
    json!({
        "slug": slug,
        "title": "Reprendre la facturation",
        "problem_md": "Les entreprises de logistique de la sous-région facturent encore \
                       à la main, ce qui produit des écarts de trésorerie de plusieurs \
                       semaines et des litiges clients qu'aucun système ne trace. Nous \
                       l'avons vu chez trois d'entre elles.",
        "approach_md": "Nous proposons un module de facturation adossé aux ordres de \
                        transport existants, livré en trois jalons, avec reprise des \
                        données et formation des équipes administratives sur place.",
        "budget_estimate": "40000.00",
    })
}

#[tokio::test]
async fn a_proposal_that_opens_with_the_solution_is_refused() {
    let app = TestApp::spawn().await;
    a_talent(&app, "solutionfirst", "artisan").await;
    app.login("solutionfirst").await;

    let mut body = a_proposal("solution-first");
    body["problem_md"] = json!("Ils ont besoin de notre outil.");
    // A proposal that opens with the solution is a team describing what it
    // wants to build.
    let resp = app.post("/api/proposals", &body).await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn a_proposal_is_not_published_while_somebody_named_has_not_agreed() {
    let app = TestApp::spawn().await;
    a_talent(&app, "proposer", "artisan").await;
    let colleague = a_talent(&app, "namedcolleague", "artisan").await;

    app.login("proposer").await;
    let resp = app.post("/api/proposals", &a_proposal("named-team")).await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let created: Value = resp.json().await.unwrap();
    let id = created["data"]["proposal"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    app.post(
        &format!("/api/proposals/{id}/members"),
        &json!({ "user_id": colleague, "role": "Développeuse" }),
    )
    .await;

    // A team assembled on paper, and the client finds out at the kickoff.
    let resp = app
        .post(&format!("/api/proposals/{id}/publish"), &json!({}))
        .await;
    assert_eq!(resp.status(), 400);
    assert!(resp.text().await.unwrap().contains("namedcolleague"));

    app.login("namedcolleague").await;
    app.post(
        &format!("/api/proposals/{id}/respond"),
        &json!({ "accept": true }),
    )
    .await;

    app.login("proposer").await;
    let resp = app
        .post(&format!("/api/proposals/{id}/publish"), &json!({}))
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn a_proposal_aimed_at_named_companies_is_invisible_to_the_others() {
    let app = TestApp::spawn().await;
    let owner = a_talent(&app, "targetowner", "artisan").await;
    let target: Uuid = sqlx::query_scalar(
        "INSERT INTO enterprises (owner_id, company_name, slug, company_size)
         VALUES ($1, 'Cible SA', 'cible-sa', '11-50') RETURNING id",
    )
    .bind(owner)
    .fetch_one(&app.db)
    .await
    .unwrap();

    a_talent(&app, "targetproposer", "artisan").await;
    app.login("targetproposer").await;
    let mut body = a_proposal("targeted");
    body["target_enterprise_ids"] = json!([target]);
    let resp = app.post("/api/proposals", &body).await;
    let created: Value = resp.json().await.unwrap();
    let id = created["data"]["proposal"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    app.post(&format!("/api/proposals/{id}/publish"), &json!({}))
        .await;

    // A company it is not aimed at sees nothing.
    an_enterprise(&app, "Outsiderco").await;
    let resp = app.get("/api/proposals").await;
    let body: Value = resp.json().await.unwrap();
    assert!(body["data"]["proposals"].as_array().unwrap().is_empty());

    let resp = app
        .post(&format!("/api/proposals/{id}/interest"), &json!({}))
        .await;
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn a_signed_proposal_books_the_facilitation_share() {
    let app = TestApp::spawn().await;
    an_admin(&app, "signadmin").await;
    let proposer = a_talent(&app, "signproposer", "artisan").await;

    app.login("signproposer").await;
    let resp = app.post("/api/proposals", &a_proposal("signed-one")).await;
    let created: Value = resp.json().await.unwrap();
    let id = created["data"]["proposal"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    app.post(&format!("/api/proposals/{id}/publish"), &json!({}))
        .await;

    an_enterprise(&app, "Signerco").await;
    let resp = app
        .post(
            &format!("/api/proposals/{id}/interest"),
            &json!({ "note_md": "Nous avons exactement ce problème." }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    let enterprise: Uuid =
        sqlx::query_scalar("SELECT id FROM enterprises WHERE slug LIKE 'signerco%'")
            .fetch_one(&app.db)
            .await
            .unwrap();

    app.login("signadmin").await;
    let resp = app
        .post(
            &format!("/api/admin/proposals/{id}/signed"),
            &json!({ "enterprise_id": enterprise, "contract_value": "40000.00" }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();

    // The team found the problem and convinced the client; Skilluv held the
    // meeting. Ten per cent.
    common::assert_amount(&body["data"]["facilitation_fee"], "4000.00");

    let booked: Uuid = sqlx::query_scalar(
        "SELECT related_talent_id FROM platform_revenues
          WHERE source = 'proposal_facilitation'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(booked, proposer);
}

#[tokio::test]
async fn a_signature_from_a_company_that_never_asked_is_refused() {
    let app = TestApp::spawn().await;
    an_admin(&app, "ghostadmin").await;
    a_talent(&app, "ghostproposer", "artisan").await;
    let owner = a_talent(&app, "ghostowner", "artisan").await;
    let stranger: Uuid = sqlx::query_scalar(
        "INSERT INTO enterprises (owner_id, company_name, slug, company_size)
         VALUES ($1, 'Fantome SA', 'fantome-sa', '11-50') RETURNING id",
    )
    .bind(owner)
    .fetch_one(&app.db)
    .await
    .unwrap();

    app.login("ghostproposer").await;
    let resp = app.post("/api/proposals", &a_proposal("ghost-one")).await;
    let created: Value = resp.json().await.unwrap();
    let id = created["data"]["proposal"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    app.login("ghostadmin").await;
    let resp = app
        .post(
            &format!("/api/admin/proposals/{id}/signed"),
            &json!({ "enterprise_id": stranger, "contract_value": "10000.00" }),
        )
        .await;
    assert_eq!(resp.status(), 404);
}
