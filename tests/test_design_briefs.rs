//! Where design work comes from.
//!
//! Design has no ingestion source the way code has GitHub issues, so a brief
//! is written by a person and read by a person. What this suite guards is that
//! the two ends actually meet: a brief that is accepted becomes something
//! somebody can claim, and a brief that is refused comes back with a reason.

mod common;
use common::TestApp;
use serde_json::{Value, json};
use uuid::Uuid;

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
         VALUES ($1, $2, 'test') ON CONFLICT DO NOTHING",
    )
    .bind(user)
    .bind(capability)
    .execute(&app.db)
    .await
    .unwrap();
}

fn a_brief() -> Value {
    json!({
        "title": "Identité pour une coopérative de transformation d'anacarde",
        "brief_md": "## Contexte\n\nUne coopérative de deux cents productrices transforme et \
    exporte de l'anacarde. Elle n'a pas d'identité : les sacs portent un tampon fait à la main, \
    différent selon l'atelier.\n\n## Problème\n\nÀ l'export, un acheteur ne distingue pas leurs \
    sacs de ceux d'un intermédiaire, et la valeur ajoutée du travail de tri leur échappe.\n\n\
    ## Contraintes\n\nLe logotype doit tenir en une seule couleur pour la sérigraphie sur toile, \
    et rester lisible en tampon de 3 cm.\n\n## Livrables\n\nLogotype en SVG, palette avec \
    valeurs de contraste, et un document de règles de quatre pages.",
        "orientation_slug": "design-brand-identity",
        "design_subtype": "brand_kit",
        "difficulty": 3,
        "estimated_hours": 20,
        "expected_rounds": 3,
    })
}

#[tokio::test]
async fn a_brief_of_two_lines_is_refused() {
    let app = TestApp::spawn().await;
    app.register_user("brief_short").await;
    app.login("brief_short").await;

    // Below two hundred characters a brief carries no context, no constraint
    // and no deliverable list — after which the reviewer arbitrates on taste,
    // which is the failure the whole grid system exists to prevent.
    let mut short = a_brief();
    short["brief_md"] = json!("Fais-moi un logo sympa.");

    let resp = app.post("/api/design/briefs", &short).await;
    assert_eq!(resp.status().as_u16(), 400);
}

#[tokio::test]
async fn a_brief_for_a_trade_that_does_not_exist_is_refused() {
    let app = TestApp::spawn().await;
    app.register_user("brief_bad_trade").await;
    app.login("brief_bad_trade").await;

    // A brief nobody can be routed to fails at the one thing a brief has to
    // do.
    let mut wrong = a_brief();
    wrong["orientation_slug"] = json!("design-tapisserie");

    let resp = app.post("/api/design/briefs", &wrong).await;
    assert_eq!(resp.status().as_u16(), 400);
}

#[tokio::test]
async fn anybody_may_propose_without_holding_a_capability_first() {
    let app = TestApp::spawn().await;
    app.register_user("brief_newcomer").await;
    app.login("brief_newcomer").await;

    // A capability earned *by* proposing cannot also be required *to*
    // propose, or nobody ever earns it.
    let resp = app.post("/api/design/briefs", &a_brief()).await;
    assert_eq!(resp.status().as_u16(), 201, "{:?}", resp.text().await);
}

#[tokio::test]
async fn a_published_brief_becomes_something_somebody_can_claim() {
    let app = TestApp::spawn().await;
    app.register_user("brief_author").await;
    app.register_user("brief_curator").await;
    let author = user_id(&app, "brief_author").await;
    let curator = user_id(&app, "brief_curator").await;
    grant(&app, curator, "community_curator").await;

    app.login("brief_author").await;
    let created: Value = app
        .post("/api/design/briefs", &a_brief())
        .await
        .json()
        .await
        .unwrap();
    let id = created["data"]["brief"]["id"].as_str().unwrap().to_string();

    let fragments_before: i32 =
        sqlx::query_scalar("SELECT total_fragments FROM users WHERE id = $1")
            .bind(author)
            .fetch_one(&app.db)
            .await
            .unwrap();

    app.login("brief_curator").await;
    let published: Value = app
        .post(
            &format!("/api/admin/design/briefs/{id}/publish"),
            &json!({}),
        )
        .await
        .json()
        .await
        .unwrap();

    let slice_id = published["data"]["brief"]["published_slice_id"]
        .as_str()
        .expect("the brief says what it became");

    // A slice of the right shape, open, in the trade the brief named — which
    // is what the review loop can actually run on.
    let (slice_type, status, subtype, rounds): (String, String, Option<String>, Option<i16>) =
        sqlx::query_as(
            "SELECT slice_type, status, design_subtype, design_expected_rounds
               FROM project_slices WHERE id = $1::uuid",
        )
        .bind(slice_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(slice_type, "design_artifact");
    assert_eq!(status, "open");
    assert_eq!(subtype.as_deref(), Some("brand_kit"));
    assert_eq!(rounds, Some(3));

    // And the author is acknowledged: setting work leaves no deliverable and
    // earns no craft score, so without this it is invisible.
    let fragments_after: i32 =
        sqlx::query_scalar("SELECT total_fragments FROM users WHERE id = $1")
            .bind(author)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(fragments_after - fragments_before, 20);
}

#[tokio::test]
async fn a_refusal_has_to_say_why() {
    let app = TestApp::spawn().await;
    app.register_user("brief_author2").await;
    app.register_user("brief_curator2").await;
    let curator = user_id(&app, "brief_curator2").await;
    grant(&app, curator, "community_curator").await;

    app.login("brief_author2").await;
    let created: Value = app
        .post("/api/design/briefs", &a_brief())
        .await
        .json()
        .await
        .unwrap();
    let id = created["data"]["brief"]["id"].as_str().unwrap().to_string();

    app.login("brief_curator2").await;
    // A refusal with no reason is a refusal that comes back next week as the
    // same brief.
    let empty = app
        .post(
            &format!("/api/admin/design/briefs/{id}/reject"),
            &json!({ "feedback": "non" }),
        )
        .await;
    assert_eq!(empty.status().as_u16(), 400);

    let proper = app
        .post(
            &format!("/api/admin/design/briefs/{id}/reject"),
            &json!({
                "feedback": "Le brief ne dit pas sur quels supports la marque apparaîtra, \
                             donc les propositions ne seront pas comparables."
            }),
        )
        .await;
    assert_eq!(proper.status().as_u16(), 200, "{:?}", proper.text().await);

    // The author reads the reason on their own list.
    app.login("brief_author2").await;
    let mine: Value = app
        .get("/api/design/briefs/mine")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(mine["data"]["briefs"][0]["status"], "rejected");
    assert!(
        mine["data"]["briefs"][0]["review_feedback"]
            .as_str()
            .unwrap()
            .contains("supports")
    );
}

#[tokio::test]
async fn a_brief_is_decided_once() {
    let app = TestApp::spawn().await;
    app.register_user("brief_author3").await;
    app.register_user("brief_curator3").await;
    let curator = user_id(&app, "brief_curator3").await;
    grant(&app, curator, "community_curator").await;

    app.login("brief_author3").await;
    let created: Value = app
        .post("/api/design/briefs", &a_brief())
        .await
        .json()
        .await
        .unwrap();
    let id = created["data"]["brief"]["id"].as_str().unwrap().to_string();

    app.login("brief_curator3").await;
    let first = app
        .post(
            &format!("/api/admin/design/briefs/{id}/publish"),
            &json!({}),
        )
        .await;
    assert_eq!(first.status().as_u16(), 200);

    // Twice would mean two slices from one brief, and a second payment.
    let second = app
        .post(
            &format!("/api/admin/design/briefs/{id}/publish"),
            &json!({}),
        )
        .await;
    assert_eq!(second.status().as_u16(), 409);
}

#[tokio::test]
async fn deciding_what_becomes_work_needs_the_curation_capability() {
    let app = TestApp::spawn().await;
    app.register_user("brief_author4").await;
    app.register_user("brief_nobody").await;

    app.login("brief_author4").await;
    let created: Value = app
        .post("/api/design/briefs", &a_brief())
        .await
        .json()
        .await
        .unwrap();
    let id = created["data"]["brief"]["id"].as_str().unwrap().to_string();

    app.login("brief_nobody").await;
    let resp = app
        .post(
            &format!("/api/admin/design/briefs/{id}/publish"),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status().as_u16(), 403);

    let queue = app.get("/api/admin/design/briefs").await;
    assert_eq!(queue.status().as_u16(), 403);
}

#[tokio::test]
async fn a_brief_nobody_has_read_can_be_taken_back() {
    let app = TestApp::spawn().await;
    app.register_user("brief_author5").await;
    app.register_user("brief_curator5").await;
    let curator = user_id(&app, "brief_curator5").await;
    grant(&app, curator, "community_curator").await;

    app.login("brief_author5").await;
    let created: Value = app
        .post("/api/design/briefs", &a_brief())
        .await
        .json()
        .await
        .unwrap();
    let id = created["data"]["brief"]["id"].as_str().unwrap().to_string();

    let withdrawn = app
        .post(&format!("/api/design/briefs/{id}/withdraw"), &json!({}))
        .await;
    assert_eq!(withdrawn.status().as_u16(), 200);

    // And it leaves the queue, rather than sitting there for somebody to
    // publish work its author no longer stands behind.
    app.login("brief_curator5").await;
    let queue: Value = app
        .get("/api/admin/design/briefs")
        .await
        .json()
        .await
        .unwrap();
    let ids: Vec<&str> = queue["data"]["briefs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["id"].as_str().unwrap())
        .collect();
    assert!(!ids.contains(&id.as_str()), "{queue}");
}
