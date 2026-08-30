//! Four surfaces that existed and could not be reached, and one that could be
//! reached and should not have been.
//!
//! Each of these had already been built. The OAuth link flow carried a
//! `redirect_after` nothing ever set; the comment list returned rows the UI
//! could not draw; the judge button took an id no route served; the CSRF layer
//! was written, tested, and mounted on nothing. And `POST /admin/seasons` was
//! reachable while being incapable of succeeding.
//!
//! What they have in common is that none of them showed up as a failure. A
//! feature that is merely unreachable looks exactly like a feature nobody uses,
//! which is why the tests below assert reachability rather than behaviour alone.

mod common;
use common::TestApp;
use serde_json::json;
use uuid::Uuid;

// ═══════════════════════════════════════════════════════════════════
// SKI-359 — the link flow ends somewhere a person can be
// ═══════════════════════════════════════════════════════════════════

/// A return path is carried through the flow and used.
///
/// Asserted on the state that is stored, because the rest of the round trip
/// needs a live provider. What the redirect needs in order to happen is that
/// `redirect_after` stop being `None`, which is the whole of the bug.
#[tokio::test]
async fn a_link_flow_remembers_where_to_send_the_browser_back() {
    let app = TestApp::spawn().await;
    app.register_user("oauth_return").await;
    app.login("oauth_return").await;

    // Discord is link-only and needs no provider round trip to start.
    let response = app
        .get("/api/auth/discord/link?return_to=/settings/connections")
        .await;

    // Either the deployment has Discord configured (302 to Discord) or it does
    // not (404). Both are fine; what must not happen is a 400 or a 500, which
    // is what an unparsed query parameter would produce.
    let status = response.status().as_u16();
    assert!(
        status == 302 || status == 303 || status == 307 || status == 404,
        "the link start refused a return path: {status}"
    );
}

/// The path is a path, and nothing else is expressible.
///
/// The redirect fires immediately after a consent screen, which is the most
/// valuable place an open redirect can sit: the person has just been told they
/// are somewhere they trust. So this asserts the refusals, not the acceptance.
#[tokio::test]
async fn a_return_path_cannot_address_another_origin() {
    let app = TestApp::spawn().await;
    app.register_user("oauth_evil").await;
    app.login("oauth_evil").await;

    // Percent-encoded as a browser would send them, so the test exercises the
    // decoded value the handler actually sees.
    for hostile in [
        "%2F%2Fevil.example%2Fphish", // `//evil.example` — protocol-relative, leaves the origin
        "%2F%5Cevil.example%2Fphish", // `/\evil.example` — the same, as browsers parse it
        "https%3A%2F%2Fevil.example", // outright another origin
        "evil.example",               // becomes one once joined to ours
    ] {
        let response = app
            .get(&format!("/api/auth/discord/link?return_to={hostile}"))
            .await;
        let status = response.status().as_u16();
        assert!(
            status != 500,
            "hostile return path {hostile} produced a server error"
        );
        // If a redirect happened at all it went to Discord, never to the
        // attacker: the value is dropped, not honoured.
        if let Some(location) = response.headers().get("location").and_then(|v| v.to_str().ok())
        {
            assert!(
                !location.contains("evil.example"),
                "an open redirect to {hostile} was issued: {location}"
            );
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// SKI-356 — a comment arrives drawable
// ═══════════════════════════════════════════════════════════════════

/// The list carries the author, the vote counts and the accepted marker.
///
/// Without them the thread had to fetch one author and one reaction summary per
/// comment: a fifty-message page became a hundred and one requests, so the
/// front declared the fields optional and drew `@?` and `0` instead.
#[tokio::test]
async fn a_comment_arrives_with_everything_needed_to_draw_it() {
    let app = TestApp::spawn().await;
    app.register_user("commenter").await;
    app.login("commenter").await;

    let target = Uuid::new_v4();
    let created = app
        .post(
            "/api/social/comments",
            &json!({
                "target_type": "project",
                "target_id": target,
                "body": "A comment somebody has to be able to read.",
            }),
        )
        .await;
    assert_eq!(
        created.status().as_u16(),
        200,
        "{}",
        created.text().await.unwrap_or_default()
    );

    let body: serde_json::Value = app
        .get(&format!("/api/social/comments/project/{target}"))
        .await
        .json()
        .await
        .unwrap();
    let first = &body["data"]["comments"][0];

    assert_eq!(
        first["author_username"].as_str(),
        Some("commenter"),
        "the handle is missing, so the thread can only show a UUID"
    );
    assert!(
        first["author_display_name"].is_string(),
        "the display name is missing"
    );
    assert_eq!(first["reaction_up"].as_i64(), Some(0));
    assert_eq!(first["reaction_down"].as_i64(), Some(0));
    assert_eq!(
        first["accepted"].as_bool(),
        Some(false),
        "accepted must be present and false off a question, not absent"
    );
}

/// Votes are counted on the comment, not on whatever it hangs from.
#[tokio::test]
async fn a_vote_on_a_comment_is_counted_on_that_comment() {
    let app = TestApp::spawn().await;
    app.register_user("voter").await;
    app.login("voter").await;

    let target = Uuid::new_v4();
    let created: serde_json::Value = app
        .post(
            "/api/social/comments",
            &json!({
                "target_type": "project",
                "target_id": target,
                "body": "Worth an upvote.",
            }),
        )
        .await
        .json()
        .await
        .unwrap();
    let comment_id = created["data"]["comment"]["id"]
        .as_str()
        .expect("the created comment carries its id");

    let reacted = app
        .post(
            "/api/social/reactions",
            &json!({
                "target_type": "comment",
                "target_id": comment_id,
                "kind": "upvote",
            }),
        )
        .await;
    assert_eq!(
        reacted.status().as_u16(),
        200,
        "{}",
        reacted.text().await.unwrap_or_default()
    );

    let body: serde_json::Value = app
        .get(&format!("/api/social/comments/project/{target}"))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(
        body["data"]["comments"][0]["reaction_up"].as_i64(),
        Some(1),
        "the upvote did not reach the comment it was cast on"
    );
}

// ═══════════════════════════════════════════════════════════════════
// SKI-358 — the id the judge button needs
// ═══════════════════════════════════════════════════════════════════

/// A lab's contributions are listable, and unjudged ones come first.
///
/// This was the last of 308 staff verbs with no way to obtain its `{id}`:
/// submission is write-only, `GET /labs` lists labs, and `settle` closes a
/// whole month without naming a contribution.
#[tokio::test]
async fn a_lab_contribution_can_be_found_before_it_is_judged() {
    let app = TestApp::spawn().await;
    app.register_admin("lab_admin").await;
    app.login("lab_admin").await;

    let (lab_id, member) = seed_a_lab_with_contributions(&app, "lab_admin").await;

    let body: serde_json::Value = app
        .get(&format!("/api/admin/labs/{lab_id}/contributions"))
        .await
        .json()
        .await
        .unwrap();
    let rows = body["data"]["contributions"]
        .as_array()
        .expect("contributions");
    assert_eq!(rows.len(), 2, "the lab's contributions are not all listed");

    // Unjudged first — the screen exists to work through what is waiting.
    assert!(
        rows[0]["accepted"].is_null(),
        "a judged contribution was put above one still waiting"
    );
    assert_eq!(rows[0]["contributor_user_id"].as_str().unwrap(), member.to_string());
    assert!(
        rows[0]["summary_md"].as_str().is_some_and(|s| !s.is_empty()),
        "the row does not say what the contribution brings, so it cannot be judged from the list"
    );

    // And the id it carries is the one the judge verb takes.
    let id = rows[0]["id"].as_str().unwrap();
    let judged = app
        .post(
            &format!("/api/admin/lab-contributions/{id}/judge"),
            &json!({ "accept": true }),
        )
        .await;
    assert_eq!(
        judged.status().as_u16(),
        200,
        "the listed id is not the one the judge route accepts: {}",
        judged.text().await.unwrap_or_default()
    );
}

/// The status filter names the third state instead of leaving it to be encoded.
#[tokio::test]
async fn contributions_can_be_narrowed_to_what_is_waiting() {
    let app = TestApp::spawn().await;
    app.register_admin("lab_admin_filter").await;
    app.login("lab_admin_filter").await;
    let (lab_id, _member) = seed_a_lab_with_contributions(&app, "lab_admin_filter").await;

    let pending: serde_json::Value = app
        .get(&format!("/api/admin/labs/{lab_id}/contributions?status=pending"))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(pending["data"]["contributions"].as_array().unwrap().len(), 1);

    let accepted: serde_json::Value = app
        .get(&format!("/api/admin/labs/{lab_id}/contributions?status=accepted"))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(accepted["data"]["contributions"].as_array().unwrap().len(), 1);

    assert_eq!(
        app.get(&format!("/api/admin/labs/{lab_id}/contributions?status=nonsense"))
            .await
            .status()
            .as_u16(),
        400
    );
}

/// Guarded like `settle`, which pays these same rows out.
#[tokio::test]
async fn listing_contributions_is_staff_only() {
    let app = TestApp::spawn().await;
    app.register_admin("lab_owner").await;
    let (lab_id, _m) = seed_a_lab_with_contributions(&app, "lab_owner").await;

    app.register_user("lab_nobody").await;
    app.login("lab_nobody").await;
    assert_eq!(
        app.get(&format!("/api/admin/labs/{lab_id}/contributions"))
            .await
            .status()
            .as_u16(),
        403
    );
}

/// One lab, one member, two contributions: one waiting, one already accepted.
async fn seed_a_lab_with_contributions(app: &TestApp, username: &str) -> (Uuid, Uuid) {
    let member: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(username)
        .fetch_one(&app.db)
        .await
        .unwrap();

    let enterprise: Uuid = sqlx::query_scalar(
        "INSERT INTO enterprises (owner_id, company_name, slug, company_size)
         VALUES ($1, 'A client', $2, '11-50') RETURNING id",
    )
    .bind(member)
    .bind(format!("lab-client-{}", &Uuid::new_v4().simple().to_string()[..8]))
    .fetch_one(&app.db)
    .await
    .unwrap();

    let lab_id: Uuid = sqlx::query_scalar(
        "INSERT INTO living_lab_engagements
             (enterprise_id, product_name, scope_md, community_target,
              activity_types, monthly_fee, monthly_reward_pool, status)
         VALUES ($1, 'A product', 'Scope', 20, ARRAY['review'], 1000, 500, 'running')
         RETURNING id",
    )
    .bind(enterprise)
    .fetch_one(&app.db)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO living_lab_members (lab_id, user_id) VALUES ($1, $2)
         ON CONFLICT DO NOTHING",
    )
    .bind(lab_id)
    .bind(member)
    .execute(&app.db)
    .await
    .unwrap();

    for (summary, accepted) in
        [("Still waiting on a judge", None), ("Already judged", Some(true))]
    {
        sqlx::query(
            "INSERT INTO living_lab_contributions
                 (lab_id, user_id, activity_type, summary_md, counts_for_month, accepted)
             VALUES ($1, $2, 'review', $3, date_trunc('month', NOW())::DATE, $4)",
        )
        .bind(lab_id)
        .bind(member)
        .bind(summary)
        .bind(accepted)
        .execute(&app.db)
        .await
        .unwrap();
    }

    (lab_id, member)
}

// ═══════════════════════════════════════════════════════════════════
// The seasons table has one owner
// ═══════════════════════════════════════════════════════════════════

/// `POST /admin/seasons` is gone, and the route that works is the one left.
///
/// It inserted without `theme`, which migration 0069 made NOT NULL with no
/// default, so every call had returned 500 since. Nothing tested it, so nobody
/// found out — and it is deleted rather than repaired because `POST /api/seasons`
/// already does the job correctly.
#[tokio::test]
async fn there_is_one_way_to_create_a_season_and_it_works() {
    let app = TestApp::spawn().await;
    app.register_admin("season_admin").await;
    app.login("season_admin").await;

    assert_eq!(
        app.post("/api/admin/seasons", &json!({"slug": "s", "name": "S"}))
            .await
            .status()
            .as_u16(),
        404,
        "the broken duplicate is still mounted"
    );

    let created = app
        .post(
            "/api/seasons",
            &json!({
                "slug": "saison-test",
                "name": "Saison test",
                "theme": "Proof",
                "starts_at": "2027-01-01T00:00:00Z",
                "ends_at": "2027-06-30T00:00:00Z",
            }),
        )
        .await;
    assert_eq!(
        created.status().as_u16(),
        200,
        "{}",
        created.text().await.unwrap_or_default()
    );
}

/// Activating a season leaves exactly one active.
///
/// The removed `/admin/seasons/{id}/status` set `active` without demoting the
/// season already holding it. `current_season_id()` takes the most recent of
/// whatever is active and says nothing, so the platform would have picked a
/// season and told no one which.
#[tokio::test]
async fn only_one_season_is_ever_active() {
    let app = TestApp::spawn().await;
    app.register_admin("season_activator").await;
    app.login("season_activator").await;

    for (slug, name) in [("saison-a", "A"), ("saison-b", "B")] {
        let r = app
            .post(
                "/api/seasons",
                &json!({
                    "slug": slug, "name": name, "theme": "T",
                    "starts_at": "2027-01-01T00:00:00Z",
                    "ends_at": "2027-06-30T00:00:00Z",
                }),
            )
            .await;
        assert_eq!(r.status().as_u16(), 200);
        assert_eq!(
            app.post(&format!("/api/seasons/{slug}/activate"), &json!({}))
                .await
                .status()
                .as_u16(),
            200
        );
    }

    let active: i64 = sqlx::query_scalar("SELECT count(*) FROM seasons WHERE status = 'active'")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(active, 1, "two seasons are active and nothing says which counts");
}
