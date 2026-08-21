//! Paying a design mission as it is delivered.
//!
//! The claim under test: one budget, released in agreed shares as rounds are
//! accepted, and never more than the client has actually put up.

mod common;
use common::TestApp;
use uuid::Uuid;

async fn user_id(app: &TestApp, username: &str) -> Uuid {
    sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(username)
        .fetch_one(&app.db)
        .await
        .unwrap()
}

/// A published design mission paid by milestone, with one applicant.
async fn a_milestone_mission(
    app: &TestApp,
    slug: &str,
    client: Uuid,
    talent: Uuid,
    charity: bool,
) -> (Uuid, Uuid) {
    let enterprise: Uuid = sqlx::query_scalar(
        "INSERT INTO enterprises (owner_id, company_name, slug, company_size)
         VALUES ($1, 'Coopérative test', $2, '11-50') RETURNING id",
    )
    .bind(client)
    .bind(format!("ent-{slug}"))
    .fetch_one(&app.db)
    .await
    .unwrap();

    // Answering a round is a membership, not an ownership: `waiting_round`
    // reads `enterprise_members`, so an owner with no membership row is
    // refused — correctly, since that is how a colleague answers too.
    sqlx::query(
        "INSERT INTO enterprise_members (enterprise_id, user_id, role, status)
         VALUES ($1, $2, 'owner', 'active')",
    )
    .bind(enterprise)
    .bind(client)
    .execute(&app.db)
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
             milestone_split, charity_brief, commission_percent, status)
         VALUES ($1, $2, $3, 'design', 'Identité coopérative',
                 'Logotype, palette et guidelines.',
                 'Le logotype tient en une couleur.',
                 'brand_package', 'milestone_iteration', 2000,
                 ARRAY[20, 20, 20, 40], $4, 15, 'published')
         RETURNING id",
    )
    .bind(slug)
    .bind(enterprise)
    .bind(mission_type)
    .bind(charity)
    .fetch_one(&app.db)
    .await
    .unwrap();

    let application: Uuid = sqlx::query_scalar(
        "INSERT INTO mission_applications (mission_id, user_id, cover_letter, status)
         VALUES ($1, $2,
                 'Je travaille les identités qui doivent survivre à un tampon encreur.',
                 'submitted')
         RETURNING id",
    )
    .bind(mission)
    .bind(talent)
    .fetch_one(&app.db)
    .await
    .unwrap();

    (mission, application)
}

/// Select the applicant, which is what raises the schedule.
async fn select_applicant(app: &TestApp, application: Uuid, decider: Uuid) {
    skilluv_backend::services::missions::decide(&app.db, application, decider, "selected", None)
        .await
        .expect("selection");
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
async fn the_whole_schedule_is_raised_when_the_mission_is_assigned() {
    let app = TestApp::spawn().await;
    let (client, talent) = a_cast(&app, "ms_schedule").await;
    let (mission, application) =
        a_milestone_mission(&app, "ms-schedule", client, talent, false).await;
    select_applicant(&app, application, client).await;

    let rows: Vec<(i16, String, f64)> = sqlx::query_as(
        "SELECT sequence, status, amount::FLOAT8 FROM mission_invoices
          WHERE mission_id = $1 ORDER BY sequence",
    )
    .bind(mission)
    .fetch_all(&app.db)
    .await
    .unwrap();

    // A designer starting work sees the whole schedule, not the first
    // instalment with the rest promised.
    assert_eq!(rows.len(), 4, "{rows:?}");
    assert_eq!(rows[0].2, 400.0);
    assert_eq!(rows[3].2, 800.0);
    // Issued, not draft: an invoice nobody has been shown is one nobody pays.
    assert!(rows.iter().all(|r| r.1 == "issued"), "{rows:?}");

    let total: f64 = rows.iter().map(|r| r.2).sum();
    assert_eq!(total, 2000.0, "the shares add back up to the budget");
}

#[tokio::test]
async fn assigning_twice_does_not_double_the_schedule() {
    let app = TestApp::spawn().await;
    let (client, talent) = a_cast(&app, "ms_twice").await;
    let (mission, application) = a_milestone_mission(&app, "ms-twice", client, talent, false).await;
    select_applicant(&app, application, client).await;

    // Selection can be retried. A second schedule would double the money the
    // enterprise is asked for.
    let again =
        skilluv_backend::services::mission_milestones::schedule_on_assignment(&app.db, mission)
            .await
            .unwrap();
    assert_eq!(again, 0);

    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM mission_invoices WHERE mission_id = $1")
            .bind(mission)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(count, 4);
}

#[tokio::test]
async fn a_charity_brief_pays_no_commission_and_the_reason_is_written_down() {
    let app = TestApp::spawn().await;
    let (client, talent) = a_cast(&app, "ms_charity").await;
    let (mission, application) =
        a_milestone_mission(&app, "ms-charity", client, talent, true).await;
    select_applicant(&app, application, client).await;

    let (percent, reason): (f64, Option<String>) = sqlx::query_as(
        "SELECT commission_percent::FLOAT8, commission_reason FROM missions WHERE id = $1",
    )
    .bind(mission)
    .fetch_one(&app.db)
    .await
    .unwrap();

    // Skilluv does not take a cut of work given away, and a rate with nothing
    // to point at is a rate somebody will argue about.
    assert_eq!(percent, 0.0);
    assert_eq!(reason.as_deref(), Some("charity_brief"));

    let on_invoices: f64 = sqlx::query_scalar(
        "SELECT max(commission_percent)::FLOAT8 FROM mission_invoices WHERE mission_id = $1",
    )
    .bind(mission)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(on_invoices, 0.0, "the rate is frozen onto the invoices too");
}

#[tokio::test]
async fn an_ordinary_mission_records_the_standard_rate_and_says_so() {
    let app = TestApp::spawn().await;
    let (client, talent) = a_cast(&app, "ms_standard").await;
    let (mission, application) =
        a_milestone_mission(&app, "ms-standard", client, talent, false).await;
    select_applicant(&app, application, client).await;

    let (percent, reason): (f64, Option<String>) = sqlx::query_as(
        "SELECT commission_percent::FLOAT8, commission_reason FROM missions WHERE id = $1",
    )
    .bind(mission)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(percent, 15.0);
    assert_eq!(reason.as_deref(), Some("standard"));
}

#[tokio::test]
async fn an_accepted_round_releases_only_what_the_client_has_paid() {
    let app = TestApp::spawn().await;
    let (client, talent) = a_cast(&app, "ms_release").await;
    let (mission, application) =
        a_milestone_mission(&app, "ms-release", client, talent, false).await;
    select_applicant(&app, application, client).await;

    // The enterprise funds the first instalment and nothing else. A captured
    // invoice has to name a payment that exists — the foreign key is what
    // stops "paid" from being a word somebody typed.
    let payment: Uuid = sqlx::query_scalar(
        "INSERT INTO payments (subject_type, subject_id, provider, method, amount,
                               currency, status, succeeded_at)
         VALUES ('mission_invoice', $1, 'stripe', 'card', 400, 'EUR', 'succeeded', NOW())
         RETURNING id",
    )
    .bind(mission)
    .fetch_one(&app.db)
    .await
    .unwrap();

    sqlx::query(
        "UPDATE mission_invoices SET status = 'paid', payment_id = $2, captured_at = NOW()
          WHERE mission_id = $1 AND sequence = 1",
    )
    .bind(mission)
    .bind(payment)
    .execute(&app.db)
    .await
    .unwrap();

    app.login("ms_release_talent").await;
    let delivered = app
        .post(
            "/api/missions/ms-release/deliveries",
            &serde_json::json!({
                "artifact_url": "https://figma.test/ms-release/1",
                "notes_md": "Premier jet du logotype, monochrome compris.",
            }),
        )
        .await;
    // Asserted rather than assumed: an unchecked step hides the reason the
    // next one failed.
    assert_eq!(
        delivered.status().as_u16(),
        201,
        "{:?}",
        delivered.text().await
    );

    app.login("ms_release_client").await;
    let accepted = app
        .post(
            "/api/missions/ms-release/deliveries/accept",
            &serde_json::json!({}),
        )
        .await;
    assert_eq!(accepted.status().as_u16(), 200);

    let statuses: Vec<(i16, String)> = sqlx::query_as(
        "SELECT sequence, status FROM mission_invoices WHERE mission_id = $1 ORDER BY sequence",
    )
    .bind(mission)
    .fetch_all(&app.db)
    .await
    .unwrap();

    // The funded round is released; the three nobody has paid for are not.
    // Releasing them would pay the designer out of the platform's pocket.
    assert_eq!(statuses[0].1, "released", "{statuses:?}");
    assert!(
        statuses[1..].iter().all(|s| s.1 == "issued"),
        "{statuses:?}"
    );
}

#[tokio::test]
async fn an_unfunded_round_is_accepted_without_releasing_anything() {
    let app = TestApp::spawn().await;
    let (client, talent) = a_cast(&app, "ms_unfunded").await;
    let (mission, application) =
        a_milestone_mission(&app, "ms-unfunded", client, talent, false).await;
    select_applicant(&app, application, client).await;

    app.login("ms_unfunded_talent").await;
    let delivered = app
        .post(
            "/api/missions/ms-unfunded/deliveries",
            &serde_json::json!({
                "artifact_url": "https://figma.test/ms-unfunded/1",
                "notes_md": "Premier jet du logotype, monochrome compris.",
            }),
        )
        .await;
    assert_eq!(
        delivered.status().as_u16(),
        201,
        "{:?}",
        delivered.text().await
    );

    app.login("ms_unfunded_client").await;
    let accepted = app
        .post(
            "/api/missions/ms-unfunded/deliveries/accept",
            &serde_json::json!({}),
        )
        .await;

    // The round happened. An unfunded schedule is the enterprise's problem to
    // fix, not a reason to refuse work the designer has already done.
    assert_eq!(accepted.status().as_u16(), 200);
    let released: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM mission_invoices WHERE mission_id = $1 AND status = 'released'",
    )
    .bind(mission)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(released, 0);
}

/// An enterprise and a design mission type, so a refusal test refuses
/// something rather than inserting nothing.
///
/// The first version of these two selected from an empty `enterprises`, which
/// inserts zero rows and succeeds — a test that proved the opposite of what it
/// claimed.
async fn a_client(app: &TestApp, prefix: &str) -> (Uuid, Uuid) {
    app.register_user(prefix).await;
    let owner = user_id(app, prefix).await;
    let enterprise: Uuid = sqlx::query_scalar(
        "INSERT INTO enterprises (owner_id, company_name, slug, company_size)
         VALUES ($1, 'Coopérative test', $2, '11-50') RETURNING id",
    )
    .bind(owner)
    .bind(format!("ent-{prefix}"))
    .fetch_one(&app.db)
    .await
    .unwrap();
    let mission_type: Uuid = sqlx::query_scalar(
        "SELECT id FROM mission_types WHERE skill_domain = 'design' ORDER BY sort_order LIMIT 1",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    (enterprise, mission_type)
}

#[tokio::test]
async fn a_split_that_does_not_add_up_is_refused_by_the_database() {
    let app = TestApp::spawn().await;
    let (enterprise, mission_type) = a_client(&app, "ms_bad").await;

    // Ninety percent would leave a tenth of the budget in escrow with nothing
    // to release it, and nobody would notice until somebody reconciled the
    // year.
    let bad = sqlx::query(
        "INSERT INTO missions
            (slug, enterprise_id, mission_type_id, skill_domain, title, description,
             acceptance_criteria, deliverable_format, payment_model, budget_eur,
             milestone_split, commission_percent, status)
         VALUES ('ms-bad', $1, $2, 'design', 'Identité', 'Description', 'Critère',
                 'brand_package', 'milestone_iteration', 1000,
                 ARRAY[20, 30, 40], 15, 'draft')",
    )
    .bind(enterprise)
    .bind(mission_type)
    .execute(&app.db)
    .await;
    assert!(bad.is_err(), "a split summing to ninety was accepted");
}

#[tokio::test]
async fn a_milestone_mission_without_a_split_is_refused() {
    let app = TestApp::spawn().await;
    let (enterprise, mission_type) = a_client(&app, "ms_nosplit").await;

    // The model would be unimplementable at the moment it matters.
    let bad = sqlx::query(
        "INSERT INTO missions
            (slug, enterprise_id, mission_type_id, skill_domain, title, description,
             acceptance_criteria, deliverable_format, payment_model, budget_eur,
             commission_percent, status)
         VALUES ('ms-nosplit', $1, $2, 'design', 'Identité', 'Description', 'Critère',
                 'brand_package', 'milestone_iteration', 1000, 15, 'draft')",
    )
    .bind(enterprise)
    .bind(mission_type)
    .execute(&app.db)
    .await;
    assert!(
        bad.is_err(),
        "a milestone mission with no split was accepted"
    );
}
