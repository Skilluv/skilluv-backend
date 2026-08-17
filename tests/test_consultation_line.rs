//! One-off consultation, and audits of a client's own team.
//!
//! The audit half is the one worth testing hardest. The person assessed is
//! not the customer, did not ask for it, and may be managed out of a job on
//! the strength of it — so nothing is written about somebody who has not been
//! told, and nothing reaches the client until everybody assessed has seen
//! what was concluded about them.

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

async fn an_expert(app: &TestApp, username: &str, rank: &str) -> Uuid {
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

fn an_advisory() -> Value {
    json!({
        "kind": "advisory",
        "topic": "Choix de base de données",
        "question_md": "Nous hésitons entre Postgres et une base document pour un \
                        catalogue de 50 millions de références.",
        "skill_domain": "code",
        "duration_minutes": 60,
        "fee": "400.00",
    })
}

fn a_review() -> Value {
    json!({
        "kind": "architecture_review",
        "topic": "Découpage en services",
        "question_md": "Le document propose de découper le monolithe en sept services. \
                        Nous voulons un avis extérieur sur les frontières.",
        "skill_domain": "code",
        "document_url": "https://example.test/rfc.pdf",
        "review_deadline": "2027-03-01T00:00:00Z",
        "reviewers_wanted": 3,
        "fee": "9000.00",
    })
}

// ═══════════════════════════════════════════════════════════════════
// Consultations
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn a_consultation_with_no_stated_question_is_refused() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Vagueco").await;

    // An hour of both people working out what the hour is for.
    let mut body = an_advisory();
    body["question_md"] = json!("Une question.");
    let resp = app.post("/api/enterprise/consultations", &body).await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn an_advisory_says_how_long_the_call_is() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Lengthco").await;

    let mut body = an_advisory();
    body["duration_minutes"] = Value::Null;
    // The expert is pricing their afternoon on it.
    let resp = app.post("/api/enterprise/consultations", &body).await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn a_review_needs_the_document_a_deadline_and_a_count() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Docco").await;

    let mut body = a_review();
    body["document_url"] = Value::Null;
    let resp = app.post("/api/enterprise/consultations", &body).await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn a_review_keeps_more_than_an_advisory() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Rateco").await;

    let resp = app
        .post("/api/enterprise/consultations", &an_advisory())
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let advisory: Value = resp.json().await.unwrap();

    let resp = app.post("/api/enterprise/consultations", &a_review()).await;
    let review: Value = resp.json().await.unwrap();

    // An advisory is an introduction. A review is a panel assembled, a
    // deadline held across several people, and a synthesis written.
    assert_eq!(
        advisory["data"]["consultation"]["commission_percent"],
        "25.00"
    );
    assert_eq!(
        review["data"]["consultation"]["commission_percent"],
        "40.00"
    );
}

#[tokio::test]
async fn advising_for_money_under_our_name_has_a_rank_floor() {
    let app = TestApp::spawn().await;
    an_admin(&app, "expertadmin").await;
    an_enterprise(&app, "Floorco").await;
    let resp = app
        .post("/api/enterprise/consultations", &an_advisory())
        .await;
    let created: Value = resp.json().await.unwrap();
    let id = created["data"]["consultation"]["id"].as_str().unwrap();

    let junior = an_expert(&app, "juniorexpert", "artisan").await;
    app.login("expertadmin").await;
    // The client is buying our judgement about who to put in the room.
    let resp = app
        .post(
            &format!("/api/admin/consultations/{id}/invite"),
            &json!({ "expert_user_id": junior }),
        )
        .await;
    assert_eq!(resp.status(), 400);

    let senior = an_expert(&app, "seniorexpert", "maitre").await;
    let resp = app
        .post(
            &format!("/api/admin/consultations/{id}/invite"),
            &json!({ "expert_user_id": senior }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn nobody_is_paid_for_a_slot_they_did_not_fill() {
    let app = TestApp::spawn().await;
    an_admin(&app, "silentadmin").await;
    an_enterprise(&app, "Silentco").await;
    let resp = app.post("/api/enterprise/consultations", &a_review()).await;
    let created: Value = resp.json().await.unwrap();
    let id = created["data"]["consultation"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let expert = an_expert(&app, "silentexpert", "maitre").await;
    app.login("silentadmin").await;
    app.post(
        &format!("/api/admin/consultations/{id}/invite"),
        &json!({ "expert_user_id": expert }),
    )
    .await;

    app.login("silentexpert").await;
    app.post(
        &format!("/api/consultations/{id}/respond"),
        &json!({ "accept": true }),
    )
    .await;

    // Accepted but wrote nothing. The fee buys the opinion, not the
    // availability.
    app.login("silentadmin").await;
    let resp = app
        .post(
            &format!("/api/admin/consultations/{id}/deliver"),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn a_delivered_review_divides_the_fee_between_the_people_who_wrote() {
    let app = TestApp::spawn().await;
    an_admin(&app, "divideadmin").await;
    an_enterprise(&app, "Divideco").await;
    let resp = app.post("/api/enterprise/consultations", &a_review()).await;
    let created: Value = resp.json().await.unwrap();
    let id = created["data"]["consultation"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Three invited, two write.
    let mut experts = Vec::new();
    for i in 0..3 {
        let name = format!("divideexpert{i}");
        experts.push((name.clone(), an_expert(&app, &name, "maitre").await));
    }

    app.login("divideadmin").await;
    for (_, expert) in &experts {
        app.post(
            &format!("/api/admin/consultations/{id}/invite"),
            &json!({ "expert_user_id": expert }),
        )
        .await;
    }

    for (name, _) in experts.iter().take(2) {
        app.login(name).await;
        app.post(
            &format!("/api/consultations/{id}/respond"),
            &json!({ "accept": true }),
        )
        .await;
        let resp = app
            .post(
                &format!("/api/consultations/{id}/opinion"),
                &json!({
                    "comment_md": "Les frontières proposées suivent l'organigramme \
                                   plutôt que les données, ce qui produira des appels \
                                   croisés partout.",
                    "verdict": "concerns",
                }),
            )
            .await;
        assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    }

    app.login("divideadmin").await;
    let resp = app
        .post(
            &format!("/api/admin/consultations/{id}/deliver"),
            &json!({ "synthesis_md": "Deux avis sur trois pointent le même risque." }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();

    // 9000 at 40%: 3600 kept, 5400 between the two who wrote.
    assert_eq!(body["data"]["commission"].as_str().unwrap(), "3600.00");
    assert_eq!(body["data"]["experts_paid"], 2);

    let shares: Vec<sqlx::types::BigDecimal> = sqlx::query_scalar(
        "SELECT share FROM consultation_experts
          WHERE consultation_id = $1::uuid AND share IS NOT NULL",
    )
    .bind(&id)
    .fetch_all(&app.db)
    .await
    .unwrap();
    assert_eq!(shares.len(), 2);
    let total: sqlx::types::BigDecimal = shares.into_iter().sum();
    assert_eq!(total.to_string(), "5400.00");
}

#[tokio::test]
async fn a_review_is_not_delivered_without_its_synthesis() {
    let app = TestApp::spawn().await;
    an_admin(&app, "synthadmin").await;
    an_enterprise(&app, "Synthco").await;
    let resp = app.post("/api/enterprise/consultations", &a_review()).await;
    let created: Value = resp.json().await.unwrap();
    let id = created["data"]["consultation"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let expert = an_expert(&app, "synthexpert", "maitre").await;
    app.login("synthadmin").await;
    app.post(
        &format!("/api/admin/consultations/{id}/invite"),
        &json!({ "expert_user_id": expert }),
    )
    .await;

    app.login("synthexpert").await;
    app.post(
        &format!("/api/consultations/{id}/respond"),
        &json!({ "accept": true }),
    )
    .await;
    app.post(
        &format!("/api/consultations/{id}/opinion"),
        &json!({
            "comment_md": "Le découpage est raisonnable mais la couche de données \
                           reste partagée, ce qui annule le bénéfice.",
        }),
    )
    .await;

    // The comments are the working; the synthesis is what the client bought.
    app.login("synthadmin").await;
    let resp = app
        .post(
            &format!("/api/admin/consultations/{id}/deliver"),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

// ═══════════════════════════════════════════════════════════════════
// Skill audits
// ═══════════════════════════════════════════════════════════════════

fn an_audit() -> Value {
    json!({
        "scope": "Équipe backend",
        "stated_purpose": "Identifier les besoins de formation avant la refonte de \
                           l'année prochaine.",
        "employees_count": 6,
        "domains_assessed": ["code"],
        "orientations_assessed": ["web-backend-developer"],
        "fee": "12000.00",
    })
}

async fn an_open_audit(app: &TestApp) -> String {
    let resp = app.post("/api/enterprise/skill-audits", &an_audit()).await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let created: Value = resp.json().await.unwrap();
    created["data"]["audit"]["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn an_audit_says_what_it_is_for() {
    let app = TestApp::spawn().await;
    an_enterprise(&app, "Purposeco").await;

    let mut body = an_audit();
    body["stated_purpose"] = json!("Audit.");
    // It is the difference between a development plan and a redundancy list,
    // and the people assessed are shown it.
    let resp = app.post("/api/enterprise/skill-audits", &body).await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn nothing_is_written_about_somebody_who_has_not_been_told() {
    let app = TestApp::spawn().await;
    an_admin(&app, "toldadmin").await;
    an_enterprise(&app, "Toldco").await;
    let audit = an_open_audit(&app).await;

    // An assessment row that skipped the informing step, forced in directly.
    let assessment: Uuid = sqlx::query_scalar(
        "INSERT INTO enterprise_employee_assessments
            (audit_id, employee_email, orientation_slug)
         VALUES ($1::uuid, 'personne@client.test', 'web-backend-developer')
         RETURNING id",
    )
    .bind(&audit)
    .fetch_one(&app.db)
    .await
    .unwrap();

    app.login("toldadmin").await;
    let resp = app
        .post(
            &format!("/api/admin/assessments/{assessment}"),
            &json!({ "assessed_level": "mid" }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn an_audit_is_not_delivered_until_everybody_has_seen_their_own() {
    let app = TestApp::spawn().await;
    an_admin(&app, "seenadmin").await;
    an_enterprise(&app, "Seenco").await;
    let audit = an_open_audit(&app).await;

    app.login("seenadmin").await;
    let resp = app
        .post(
            &format!("/api/admin/skill-audits/{audit}/inform"),
            &json!({
                "employee_email": "salarie@client.test",
                "orientation_slug": "web-backend-developer",
            }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let created: Value = resp.json().await.unwrap();
    let assessment = created["data"]["assessment_id"]
        .as_str()
        .unwrap()
        .to_string();

    app.post(
        &format!("/api/admin/assessments/{assessment}"),
        &json!({
            "assessed_level": "mid",
            "strengths": ["tests"],
            "gaps": ["observabilité"],
        }),
    )
    .await;

    // The commercial pressure is to deliver on the client's date. The gate is
    // in the database.
    let resp = app
        .post(
            &format!("/api/admin/skill-audits/{audit}/deliver"),
            &json!({ "matrix_url": "https://example.test/matrix.pdf" }),
        )
        .await;
    assert_eq!(resp.status(), 400);

    app.post(
        &format!("/api/admin/assessments/{assessment}/share"),
        &json!({}),
    )
    .await;

    let resp = app
        .post(
            &format!("/api/admin/skill-audits/{audit}/deliver"),
            &json!({ "matrix_url": "https://example.test/matrix.pdf" }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    let booked: sqlx::types::BigDecimal = sqlx::query_scalar(
        "SELECT amount_credits FROM platform_revenues WHERE source = 'consulting_fee'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(booked.to_string(), "12000.00");
}

#[tokio::test]
async fn the_client_can_see_how_far_the_audit_is_from_deliverable() {
    let app = TestApp::spawn().await;
    an_admin(&app, "readyadmin").await;
    let company = an_enterprise(&app, "Readyco").await;
    let audit = an_open_audit(&app).await;

    app.login("readyadmin").await;
    let resp = app
        .post(
            &format!("/api/admin/skill-audits/{audit}/inform"),
            &json!({
                "employee_email": "a@client.test",
                "orientation_slug": "web-backend-developer",
            }),
        )
        .await;
    let created: Value = resp.json().await.unwrap();
    let assessment = created["data"]["assessment_id"]
        .as_str()
        .unwrap()
        .to_string();
    app.post(
        &format!("/api/admin/assessments/{assessment}"),
        &json!({ "assessed_level": "senior" }),
    )
    .await;

    app.login(&company).await;
    let resp = app
        .get(&format!("/api/enterprise/skill-audits/{audit}/readiness"))
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["informed"], 1);
    assert_eq!(body["data"]["assessed"], 1);
    assert_eq!(body["data"]["shared_with_the_person"], 0);
    assert_eq!(body["data"]["deliverable"], false);
}

#[tokio::test]
async fn the_person_assessed_reads_it_through_their_own_door_and_can_reply() {
    let app = TestApp::spawn().await;
    an_admin(&app, "replyadmin").await;
    an_enterprise(&app, "Replyco").await;
    let audit = an_open_audit(&app).await;

    // The employee happens to have a Skilluv account, so the assessment can
    // reach them without going through the employer who commissioned it.
    app.register_user("assessedperson").await;
    let email: String = sqlx::query_scalar("SELECT email FROM users WHERE username = $1")
        .bind("assessedperson")
        .fetch_one(&app.db)
        .await
        .unwrap();

    app.login("replyadmin").await;
    let resp = app
        .post(
            &format!("/api/admin/skill-audits/{audit}/inform"),
            &json!({
                "employee_email": email,
                "orientation_slug": "web-backend-developer",
            }),
        )
        .await;
    let created: Value = resp.json().await.unwrap();
    let assessment = created["data"]["assessment_id"]
        .as_str()
        .unwrap()
        .to_string();

    app.post(
        &format!("/api/admin/assessments/{assessment}"),
        &json!({
            "assessed_level": "junior",
            "gaps": ["concurrence"],
        }),
    )
    .await;

    // Before sharing: nothing to see. After: theirs to read.
    app.login("assessedperson").await;
    let resp = app.get("/api/users/me/assessments").await;
    let body: Value = resp.json().await.unwrap();
    assert!(body["data"]["assessments"].as_array().unwrap().is_empty());

    app.login("replyadmin").await;
    app.post(
        &format!("/api/admin/assessments/{assessment}/share"),
        &json!({}),
    )
    .await;

    app.login("assessedperson").await;
    let resp = app.get("/api/users/me/assessments").await;
    let body: Value = resp.json().await.unwrap();
    let mine = body["data"]["assessments"].as_array().unwrap();
    assert_eq!(mine.len(), 1);
    assert_eq!(mine[0]["assessed_level"], "junior");
    // They see what the audit was said to be for.
    assert!(
        mine[0]["stated_purpose"]
            .as_str()
            .unwrap()
            .contains("formation")
    );

    // A conclusion with no right of reply is a verdict.
    let resp = app
        .post(
            &format!("/api/assessments/{assessment}/response"),
            &json!({ "response_md": "Le point sur la concurrence porte sur un projet \
                                     que je n'ai pas écrit." }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    let stored: Option<String> = sqlx::query_scalar(
        "SELECT employee_response_md FROM enterprise_employee_assessments
          WHERE id = $1::uuid",
    )
    .bind(&assessment)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(stored.unwrap().contains("je n'ai pas écrit"));
}

#[tokio::test]
async fn somebody_elses_assessment_is_not_readable() {
    let app = TestApp::spawn().await;
    an_admin(&app, "nosyadmin").await;
    an_enterprise(&app, "Nosyco").await;
    let audit = an_open_audit(&app).await;

    app.register_user("subjectperson").await;
    app.register_user("nosyperson").await;
    let email: String = sqlx::query_scalar("SELECT email FROM users WHERE username = $1")
        .bind("subjectperson")
        .fetch_one(&app.db)
        .await
        .unwrap();

    app.login("nosyadmin").await;
    let resp = app
        .post(
            &format!("/api/admin/skill-audits/{audit}/inform"),
            &json!({
                "employee_email": email,
                "orientation_slug": "web-backend-developer",
            }),
        )
        .await;
    let created: Value = resp.json().await.unwrap();
    let assessment = created["data"]["assessment_id"]
        .as_str()
        .unwrap()
        .to_string();
    app.post(
        &format!("/api/admin/assessments/{assessment}"),
        &json!({ "assessed_level": "mid" }),
    )
    .await;
    app.post(
        &format!("/api/admin/assessments/{assessment}/share"),
        &json!({}),
    )
    .await;

    app.login("nosyperson").await;
    let resp = app.get("/api/users/me/assessments").await;
    let body: Value = resp.json().await.unwrap();
    assert!(body["data"]["assessments"].as_array().unwrap().is_empty());

    let resp = app
        .post(
            &format!("/api/assessments/{assessment}/response"),
            &json!({ "response_md": "Pas moi." }),
        )
        .await;
    assert_eq!(resp.status(), 404);
}
