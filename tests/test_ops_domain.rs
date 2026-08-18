//! The ops domain.
//!
//! Three things an ops contributor is judged on, none of which is a pull
//! request: whether a service met what was promised, what happened when it
//! did not, and whether a cost reduction broke anything.
//!
//! Every one of them is recorded as a claim somebody else can dispute, which
//! is what these tests check.

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

async fn a_talent(app: &TestApp, username: &str) -> Uuid {
    app.register_user(username).await;
    sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(username)
        .fetch_one(&app.db)
        .await
        .unwrap()
}

/// A project to hang objectives and cost work off.
async fn a_project(app: &TestApp, owner: Uuid, slug: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO projects (slug, name, description, owner_type, owner_id)
         VALUES ($1, $1, 'Un projet', 'user', $2)
         RETURNING id",
    )
    .bind(slug)
    .bind(owner)
    .fetch_one(&app.db)
    .await
    .unwrap()
}

// ═══════════════════════════════════════════════════════════════════
// The trades
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn ops_has_eight_trades_in_five_review_families() {
    let app = TestApp::spawn().await;

    let resp = app.get("/api/ops/reference").await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    let orientations = body["data"]["orientations"].as_array().unwrap();

    // Three from migration 0088, five added.
    assert_eq!(orientations.len(), 8);
    assert!(
        orientations
            .iter()
            .all(|o| o["reviewer_group"].is_string()),
        "every trade belongs to a review family, or nobody can be given rights over it"
    );

    for slug in [
        "devops-engineer",
        "sre",
        "cloud-architect",
        "platform-engineer",
        "kubernetes-specialist",
        "observability-engineer",
        "incident-commander",
        "database-administrator",
    ] {
        assert!(
            orientations.iter().any(|o| o["slug"] == slug),
            "{slug} is missing"
        );
    }
}

#[tokio::test]
async fn the_new_trades_are_grouped_by_competence_not_by_org_chart() {
    let app = TestApp::spawn().await;

    let groups: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT slug, reviewer_group FROM orientations WHERE primary_domain = 'ops'",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    let group_of = |slug: &str| -> String {
        groups
            .iter()
            .find(|(s, _)| s == slug)
            .and_then(|(_, g)| g.clone())
            .unwrap_or_default()
    };

    // Somebody who reads a Terraform plan reads a Helm chart.
    assert_eq!(group_of("devops-engineer"), group_of("kubernetes-specialist"));
    // Somebody who has run incidents judges a post-mortem.
    assert_eq!(group_of("sre"), group_of("incident-commander"));
    // And has no opinion worth having on an index.
    assert_ne!(group_of("sre"), group_of("database-administrator"));
}

#[tokio::test]
async fn the_ops_reviewer_capabilities_are_grantable() {
    let app = TestApp::spawn().await;
    let user = a_talent(&app, "opsreviewer").await;

    // The CHECK was replaced rather than extended, so this also proves the
    // restated list did not drop anything.
    for capability in [
        "ops_reviewer:infra",
        "ops_reviewer:reliability",
        "ops_reviewer:cloud",
        "ops_reviewer:observability",
        "ops_reviewer:data",
        "ops_reviewer:all",
        // From earlier migrations, restated in this one.
        "code_reviewer:web",
        "ai_reviewer:safety",
        "challenge_validator:ops",
        "mentor",
    ] {
        let granted = sqlx::query(
            "INSERT INTO user_capabilities (user_id, capability) VALUES ($1, $2)",
        )
        .bind(user)
        .bind(capability)
        .execute(&app.db)
        .await;
        assert!(granted.is_ok(), "{capability} became ungrantable");
    }
}

// ═══════════════════════════════════════════════════════════════════
// Service objectives
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn an_objective_belongs_to_something() {
    let app = TestApp::spawn().await;
    a_talent(&app, "floatingops").await;
    app.login("floatingops").await;

    // A target floating on its own is a promise about nothing.
    let resp = app
        .post(
            "/api/ops/objectives",
            &json!({
                "service_name": "api",
                "target_percent": "99.9",
                "window_days": 30,
            }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn closing_a_window_needs_the_figure_and_its_source() {
    let app = TestApp::spawn().await;
    let owner = a_talent(&app, "sloowner").await;
    let project = a_project(&app, owner, "slo-project").await;

    app.login("sloowner").await;
    let resp = app
        .post(
            "/api/ops/objectives",
            &json!({
                "service_name": "api",
                "target_percent": "99.9",
                "window_days": 30,
                "project_id": project,
            }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let created: Value = resp.json().await.unwrap();
    let id = created["data"]["objective"]["id"].as_str().unwrap().to_string();

    // A number with no source is a claim.
    let resp = app
        .post(
            &format!("/api/ops/objectives/{id}/close"),
            &json!({ "achieved_percent": "99.95", "evidence_url": "grafana" }),
        )
        .await;
    assert_eq!(resp.status(), 400);

    let resp = app
        .post(
            &format!("/api/ops/objectives/{id}/close"),
            &json!({
                "achieved_percent": "99.95",
                "evidence_url": "https://example.test/grafana/uptime",
            }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["met"], true);
    // Half the error budget spent. Uptime alone would hide how close it was.
    let consumed = body["data"]["error_budget_consumed_percent"].as_f64().unwrap();
    assert!((consumed - 50.0).abs() < 0.01);
}

#[tokio::test]
async fn a_near_miss_earns_nothing() {
    let app = TestApp::spawn().await;
    an_admin(&app, "sloadmin").await;
    let owner = a_talent(&app, "missowner").await;
    let project = a_project(&app, owner, "miss-project").await;

    app.login("missowner").await;
    let resp = app
        .post(
            "/api/ops/objectives",
            &json!({
                "service_name": "api",
                "target_percent": "99.95",
                "window_days": 30,
                "project_id": project,
            }),
        )
        .await;
    let created: Value = resp.json().await.unwrap();
    let id = created["data"]["objective"]["id"].as_str().unwrap().to_string();

    // 99.94 against 99.95. Rounding would have made this a pass.
    let resp = app
        .post(
            &format!("/api/ops/objectives/{id}/close"),
            &json!({
                "achieved_percent": "99.94",
                "evidence_url": "https://example.test/grafana",
            }),
        )
        .await;
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["met"], false);

    app.login("sloadmin").await;
    let resp = app
        .post(&format!("/api/admin/ops/objectives/{id}/verify"), &json!({}))
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["attestation_issued"], false);

    let attestations: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM attestations WHERE basis = 'ops_uptime_achievement'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(attestations, 0);
}

#[tokio::test]
async fn a_met_objective_earns_its_attestation() {
    let app = TestApp::spawn().await;
    an_admin(&app, "metadmin").await;
    let owner = a_talent(&app, "metowner").await;
    let project = a_project(&app, owner, "met-project").await;

    app.login("metowner").await;
    let resp = app
        .post(
            "/api/ops/objectives",
            &json!({
                "service_name": "passerelle de paiement",
                "target_percent": "99.9",
                "window_days": 90,
                "project_id": project,
            }),
        )
        .await;
    let created: Value = resp.json().await.unwrap();
    let id = created["data"]["objective"]["id"].as_str().unwrap().to_string();

    app.post(
        &format!("/api/ops/objectives/{id}/close"),
        &json!({
            "achieved_percent": "99.98",
            "evidence_url": "https://example.test/grafana",
        }),
    )
    .await;

    app.login("metadmin").await;
    let resp = app
        .post(&format!("/api/admin/ops/objectives/{id}/verify"), &json!({}))
        .await;
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["attestation_issued"], true);

    let basis: String = sqlx::query_scalar(
        "SELECT basis FROM attestations WHERE user_id = $1",
    )
    .bind(owner)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(basis, "ops_uptime_achievement");
}

// ═══════════════════════════════════════════════════════════════════
// Incidents
// ═══════════════════════════════════════════════════════════════════

async fn a_resolved_incident(app: &TestApp, username: &str) -> String {
    app.login(username).await;
    let resp = app
        .post(
            "/api/ops/incidents",
            &json!({
                "title": "Coupure de la passerelle de paiement",
                "severity": "sev1",
                "started_at": "2027-03-01T02:00:00Z",
            }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let created: Value = resp.json().await.unwrap();
    let id = created["data"]["incident"]["id"].as_str().unwrap().to_string();

    let resp = app
        .post(
            &format!("/api/ops/incidents/{id}/resolve"),
            &json!({ "time_to_detect_minutes": 12, "time_to_resolve_minutes": 95 }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    id
}

const A_REAL_POSTMORTEM: &str = "La passerelle a cessé de répondre après un \
déploiement qui a réduit la taille du pool de connexions sans que personne ne \
s'en aperçoive, parce que la métrique de saturation du pool n'était pas dans le \
tableau de bord d'astreinte. Le système a permis de déployer un changement de \
capacité sans alerte associée, et c'est ce point-là qui est corrigé.";

#[tokio::test]
async fn a_post_mortem_shorter_than_a_heading_is_refused() {
    let app = TestApp::spawn().await;
    a_talent(&app, "shortpm").await;
    let id = a_resolved_incident(&app, "shortpm").await;

    app.post(
        &format!("/api/ops/incidents/{id}/actions"),
        &json!({ "description": "Ajouter la métrique au tableau d'astreinte." }),
    )
    .await;

    // The second occurrence of the same incident is what a heading costs.
    let resp = app
        .post(
            &format!("/api/ops/incidents/{id}/postmortem"),
            &json!({ "postmortem_md": "Le service est tombé. C'est réparé." }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn a_post_mortem_with_no_action_items_is_refused() {
    let app = TestApp::spawn().await;
    a_talent(&app, "noactionpm").await;
    let id = a_resolved_incident(&app, "noactionpm").await;

    // Either it found a system that cannot fail again, or it has not looked.
    let resp = app
        .post(
            &format!("/api/ops/incidents/{id}/postmortem"),
            &json!({ "postmortem_md": A_REAL_POSTMORTEM }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn a_published_post_mortem_earns_its_attestation() {
    let app = TestApp::spawn().await;
    let commander = a_talent(&app, "goodpm").await;
    let id = a_resolved_incident(&app, "goodpm").await;

    app.post(
        &format!("/api/ops/incidents/{id}/actions"),
        &json!({
            "description": "Alerte sur la saturation du pool de connexions.",
            "due_on": "2027-03-15",
        }),
    )
    .await;

    let resp = app
        .post(
            &format!("/api/ops/incidents/{id}/postmortem"),
            &json!({ "postmortem_md": A_REAL_POSTMORTEM }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());

    let basis: String =
        sqlx::query_scalar("SELECT basis FROM attestations WHERE user_id = $1")
            .bind(commander)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(basis, "ops_incident_led");
}

#[tokio::test]
async fn a_promised_action_that_is_late_surfaces() {
    let app = TestApp::spawn().await;
    an_admin(&app, "actionadmin").await;
    a_talent(&app, "latecommander").await;
    let id = a_resolved_incident(&app, "latecommander").await;

    app.post(
        &format!("/api/ops/incidents/{id}/actions"),
        &json!({
            "description": "Revoir le seuil d'alerte.",
            "due_on": "2020-01-01",
        }),
    )
    .await;

    app.login("actionadmin").await;
    let resp = app.get("/api/admin/ops/overdue-actions").await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    let overdue = body["data"]["overdue"].as_array().unwrap();

    // The difference between a post-mortem practice and a post-mortem
    // archive.
    assert_eq!(overdue.len(), 1);
    assert_eq!(overdue[0]["severity"], "sev1");
}

#[tokio::test]
async fn an_incident_has_nowhere_to_name_who_caused_it() {
    let app = TestApp::spawn().await;

    // Blameless as a constraint rather than a value statement: the column
    // does not exist, so no interface can offer it.
    let columns: Vec<String> = sqlx::query_scalar(
        "SELECT column_name FROM information_schema.columns
          WHERE table_name = 'ops_incidents'",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    for forbidden in ["caused_by", "responsible_user_id", "at_fault", "blamed_user_id"] {
        assert!(
            !columns.iter().any(|c| c == forbidden),
            "{forbidden} exists — a post-mortem naming a person is one nobody writes \
             honestly the second time"
        );
    }
}

// ═══════════════════════════════════════════════════════════════════
// Cost work
// ═══════════════════════════════════════════════════════════════════

const A_REAL_CHANGE: &str = "Les instances de calcul étaient dimensionnées pour \
le pic de janvier et tournaient à 12 % le reste de l'année. Passage à un groupe \
d'auto-scaling avec un plancher à deux instances et un plafond au pic constaté, \
plus le passage des sauvegardes en stockage froid au-delà de trente jours.";

#[tokio::test]
async fn a_saving_with_no_explanation_is_refused() {
    let app = TestApp::spawn().await;
    let owner = a_talent(&app, "vagueops").await;
    let project = a_project(&app, owner, "vague-project").await;

    app.login("vagueops").await;
    // A saving somebody made by turning off something that was needed.
    let resp = app
        .post(
            "/api/ops/cost-work",
            &json!({
                "scope": "compute",
                "monthly_before": "4000.00",
                "monthly_after": "1500.00",
                "change_md": "Optimisé.",
                "project_id": project,
            }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn a_reduction_that_does_not_reduce_is_refused() {
    let app = TestApp::spawn().await;
    let owner = a_talent(&app, "noreduceops").await;
    let project = a_project(&app, owner, "noreduce-project").await;

    app.login("noreduceops").await;
    let resp = app
        .post(
            "/api/ops/cost-work",
            &json!({
                "scope": "compute",
                "monthly_before": "1000.00",
                "monthly_after": "1200.00",
                "change_md": A_REAL_CHANGE,
                "project_id": project,
            }),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn a_cost_reduction_is_attested_only_if_the_service_still_works() {
    let app = TestApp::spawn().await;
    an_admin(&app, "costadmin").await;
    let owner = a_talent(&app, "costowner").await;
    let project = a_project(&app, owner, "cost-project").await;

    app.login("costowner").await;
    let resp = app
        .post(
            "/api/ops/cost-work",
            &json!({
                "scope": "calcul et sauvegardes",
                "monthly_before": "4000.00",
                "monthly_after": "1500.00",
                "change_md": A_REAL_CHANGE,
                "project_id": project,
            }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let created: Value = resp.json().await.unwrap();
    let id = created["data"]["cost_work"]["id"].as_str().unwrap().to_string();

    // 2500 a month is 30 000 a year — the figure the decision was made
    // against.
    assert_eq!(created["data"]["annual_saving"].as_str().unwrap(), "30000.00");
    assert!((created["data"]["reduction_percent"].as_f64().unwrap() - 62.5).abs() < 0.01);

    app.login("costadmin").await;
    // Verified as a saving that broke the service: an outage with a
    // spreadsheet, and no attestation.
    let resp = app
        .post(
            &format!("/api/admin/ops/cost-work/{id}/verify"),
            &json!({ "service_still_meets_slo": false }),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["attestation_issued"], false);

    let attestations: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM attestations WHERE basis = 'ops_cost_optimization'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(attestations, 0);
}

// ═══════════════════════════════════════════════════════════════════
// Artefacts
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn an_ops_artefact_says_what_it_is() {
    let app = TestApp::spawn().await;
    let owner = a_talent(&app, "artefactops").await;
    let project = a_project(&app, owner, "artefact-project").await;

    // An ops artefact with no subtype says nothing about what was built.
    let bare = sqlx::query(
        "INSERT INTO project_slices
            (project_id, slice_type, title, description, primary_domain, difficulty)
         VALUES ($1, 'ops_artifact', 'Module', 'Un module', 'ops', 3)",
    )
    .bind(project)
    .execute(&app.db)
    .await;
    assert!(bare.is_err());

    // And a subtype on something that is not an ops artefact is meaningless.
    let misplaced = sqlx::query(
        "INSERT INTO project_slices
            (project_id, slice_type, title, description, primary_domain, difficulty,
             ops_subtype)
         VALUES ($1, 'documentation', 'Doc', 'Une doc', 'ops', 2, 'iac_terraform')",
    )
    .bind(project)
    .execute(&app.db)
    .await;
    assert!(misplaced.is_err());

    let good = sqlx::query(
        "INSERT INTO project_slices
            (project_id, slice_type, title, description, primary_domain, difficulty,
             ops_subtype, ops_target_platforms, ops_tooling)
         VALUES ($1, 'ops_artifact', 'Module réseau', 'VPC multi-région', 'ops', 4,
                 'iac_terraform', ARRAY['aws','on-prem'], ARRAY['terraform'])",
    )
    .bind(project)
    .execute(&app.db)
    .await;
    assert!(good.is_ok(), "{good:?}");
}
