//! Integration tests for the seven gaps the front and the admin panel
//! reported against the Post-MVP batch (SKI-295 → SKI-301).
//!
//! They live in one file because they are one theme: every surface built
//! for its owner turned out to be unusable by the two callers that also
//! need it — the visitor reading somebody else's profile, and the
//! moderator who has to act on content nobody else can see.

mod common;

use common::TestApp;
use reqwest::StatusCode;
use serde_json::{Value, json};
use uuid::Uuid;

use skilluv_backend::services::ranks;

fn user_id_of(register_body: &Value) -> Uuid {
    register_body["data"]["user"]["id"]
        .as_str()
        .expect("register response carries a user id")
        .parse()
        .expect("user id is a uuid")
}

fn future(days: i64) -> String {
    (chrono::Utc::now() + chrono::Duration::days(days)).to_rfc3339()
}

async fn set_rank(app: &TestApp, user_id: Uuid, rank: &str) {
    sqlx::query(
        "INSERT INTO user_ranks (user_id, rank) VALUES ($1, $2)
         ON CONFLICT (user_id) DO UPDATE SET rank = EXCLUDED.rank",
    )
    .bind(user_id)
    .bind(rank)
    .execute(&app.db)
    .await
    .expect("set rank");
}

async fn grant_capability(app: &TestApp, user_id: Uuid, capability: &str) {
    sqlx::query(
        "INSERT INTO user_capabilities (user_id, capability, granted_reason)
         VALUES ($1, $2, 'test')
         ON CONFLICT DO NOTHING",
    )
    .bind(user_id)
    .bind(capability)
    .execute(&app.db)
    .await
    .expect("grant capability");
}

/// Audit entries recorded for one action, straight from the append-only
/// journal. Read here rather than through the API so a failure points at
/// the missing write, not at the listing.
async fn audit_entries(app: &TestApp, action: &str) -> Vec<(Option<Uuid>, Option<Value>)> {
    sqlx::query_as("SELECT actor_id, metadata FROM audit_log WHERE action = $1")
        .bind(action)
        .fetch_all(&app.db)
        .await
        .expect("read audit log")
}

// ═══════════════════════════════════════════════════════════════════
// SKI-300 — the public profile exposes its own id
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn a_visitor_can_resolve_a_username_to_an_id() {
    let app = TestApp::spawn().await;
    let owner = app.register_user("gapprofileowner").await;
    let owner_id = user_id_of(&owner);

    // Unauthenticated on purpose: this is the case that was broken. A
    // logged-in visitor could only ever resolve their own id.
    let resp = app.get("/api/profile/gapprofileowner").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(
        body["data"]["user"]["id"].as_str(),
        Some(owner_id.to_string().as_str()),
        "without the id the front cannot call any of the four /users/{{id}}/… sections"
    );

    // And the id actually opens the endpoints it exists for.
    let resp = app.get(&format!("/api/users/{owner_id}/vouchings")).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn a_hidden_or_banned_profile_leaks_nothing() {
    let app = TestApp::spawn().await;
    let hidden = app.register_user("gaphiddenone").await;
    let banned = app.register_user("gapbannedone").await;

    sqlx::query("UPDATE users SET profile_hidden = TRUE WHERE id = $1")
        .bind(user_id_of(&hidden))
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("UPDATE users SET is_banned = TRUE WHERE id = $1")
        .bind(user_id_of(&banned))
        .execute(&app.db)
        .await
        .unwrap();

    for username in ["gaphiddenone", "gapbannedone"] {
        let resp = app.get(&format!("/api/profile/{username}")).await;
        assert_eq!(
            resp.status(),
            StatusCode::NOT_FOUND,
            "{username}: a profile that is not readable must not hand out its uuid either"
        );
    }
}

// ═══════════════════════════════════════════════════════════════════
// SKI-301 — vouchings carry a linkable identity
// ═══════════════════════════════════════════════════════════════════

/// Seed a live vouching directly. Going through `POST /api/vouchings`
/// would require the voucher to be a real Doyen, which is a different
/// invariant, already covered elsewhere.
async fn seed_vouching(app: &TestApp, voucher: Uuid, vouched: Uuid, days: i64) -> Uuid {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO vouchings (voucher_id, vouched_id, active_until, at_stake_kind, statement)
         VALUES ($1, $2, NOW() + MAKE_INTERVAL(days => $3::INT), 'rank_temporary', 'solid work')
         RETURNING id",
    )
    .bind(voucher)
    .bind(vouched)
    .bind(days as i32)
    .fetch_one(&app.db)
    .await
    .expect("seed vouching");
    id
}

#[tokio::test]
async fn vouchings_expose_the_vouchers_username() {
    let app = TestApp::spawn().await;
    let voucher = app.register_user("gapvoucher").await;
    let vouched = app.register_user("gapvouched").await;
    let voucher_id = user_id_of(&voucher);
    let vouched_id = user_id_of(&vouched);

    // A display name with a space is exactly the case a link built on the
    // display name cannot survive.
    sqlx::query("UPDATE users SET display_name = 'Ada Lovelace' WHERE id = $1")
        .bind(voucher_id)
        .execute(&app.db)
        .await
        .unwrap();

    seed_vouching(&app, voucher_id, vouched_id, 60).await;

    let resp = app.get(&format!("/api/users/{vouched_id}/vouchings")).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = resp.json().await.unwrap();
    let row = &body["data"]["vouchings"][0];

    assert_eq!(row["voucher_username"].as_str(), Some("gapvoucher"));
    assert_eq!(row["voucher_display_name"].as_str(), Some("Ada Lovelace"));

    // The username must actually address a profile — that is the whole
    // point of the field.
    let resp = app.get("/api/profile/gapvoucher").await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn my_vouchings_resolve_the_other_party_on_both_sides() {
    let app = TestApp::spawn().await;
    let me = app.register_user("gapmiddleman").await;
    let backer = app.register_user("gapbacker").await;
    let backed = app.register_user("gapbacked").await;
    let my_id = user_id_of(&me);

    seed_vouching(&app, my_id, user_id_of(&backed), 60).await;
    seed_vouching(&app, user_id_of(&backer), my_id, 60).await;

    app.login("gapmiddleman").await;
    let resp = app.get("/api/users/me/vouchings").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = resp.json().await.unwrap();

    assert_eq!(
        body["data"]["given"][0]["other_username"].as_str(),
        Some("gapbacked"),
        "on `given`, the other party is the person I back"
    );
    assert_eq!(
        body["data"]["received"][0]["other_username"].as_str(),
        Some("gapbacker"),
        "on `received`, it is the person backing me"
    );
    // The vouching itself is still there, flattened alongside.
    assert!(body["data"]["given"][0]["at_stake_kind"].is_string());
}

// ═══════════════════════════════════════════════════════════════════
// SKI-297 — the global vouchings moderation queue
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn the_vouching_queue_splits_live_broken_and_expired() {
    let app = TestApp::spawn().await;
    let mod_user = app.register_user("gapvouchmod").await;
    let mod_id = user_id_of(&mod_user);
    grant_capability(&app, mod_id, "plagiarism_reviewer").await;

    let voucher = user_id_of(&app.register_user("gapqvoucher").await);
    let live_target = user_id_of(&app.register_user("gapqlive").await);
    let broken_target = user_id_of(&app.register_user("gapqbroken").await);
    let expired_target = user_id_of(&app.register_user("gapqexpired").await);

    seed_vouching(&app, voucher, live_target, 60).await;
    let broken = seed_vouching(&app, voucher, broken_target, 60).await;
    let expired = seed_vouching(&app, voucher, expired_target, 60).await;

    sqlx::query(
        "UPDATE vouchings SET broken_at = NOW(), break_reason = 'confirmed plagiarism',
                              broken_by = $2 WHERE id = $1",
    )
    .bind(broken)
    .bind(mod_id)
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query("UPDATE vouchings SET active_until = NOW() - INTERVAL '1 day' WHERE id = $1")
        .bind(expired)
        .execute(&app.db)
        .await
        .unwrap();

    app.login("gapvouchmod").await;

    // Default status is `live`.
    let body: Value = app
        .get("/api/moderation/vouchings")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["status"].as_str(), Some("live"));
    assert_eq!(body["data"]["total"].as_i64(), Some(1));
    assert_eq!(
        body["data"]["vouchings"][0]["vouched_username"].as_str(),
        Some("gapqlive"),
        "both parties are resolved, not left as uuids"
    );
    assert_eq!(
        body["data"]["vouchings"][0]["voucher_username"].as_str(),
        Some("gapqvoucher")
    );

    // Broken ones were readable nowhere at all before this route.
    let body: Value = app
        .get("/api/moderation/vouchings?status=broken")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["total"].as_i64(), Some(1));
    let row = &body["data"]["vouchings"][0];
    assert_eq!(row["break_reason"].as_str(), Some("confirmed plagiarism"));
    assert_eq!(
        row["broken_by"].as_str(),
        Some(mod_id.to_string().as_str()),
        "who broke it is the reason the row is worth keeping"
    );

    let body: Value = app
        .get("/api/moderation/vouchings?status=expired")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["total"].as_i64(), Some(1));
    assert_eq!(
        body["data"]["vouchings"][0]["vouched_username"].as_str(),
        Some("gapqexpired")
    );

    // Filters narrow the same queue rather than opening a second one.
    let body: Value = app
        .get(&format!("/api/moderation/vouchings?voucher_id={voucher}"))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["total"].as_i64(), Some(1));

    let resp = app.get("/api/moderation/vouchings?status=nonsense").await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn the_vouching_queue_surfaces_flagged_mentees_first() {
    let app = TestApp::spawn().await;
    let mod_id = user_id_of(&app.register_user("gapflagmod").await);
    grant_capability(&app, mod_id, "community_moderator").await;

    let voucher = user_id_of(&app.register_user("gapflagvoucher").await);
    let clean = user_id_of(&app.register_user("gapflagclean").await);
    let flagged = user_id_of(&app.register_user("gapflagged").await);

    seed_vouching(&app, voucher, clean, 60).await;
    seed_vouching(&app, voucher, flagged, 60).await;
    sqlx::query("UPDATE users SET suspected_multi_account = TRUE WHERE id = $1")
        .bind(flagged)
        .execute(&app.db)
        .await
        .unwrap();

    app.login("gapflagmod").await;
    let body: Value = app
        .get("/api/moderation/vouchings")
        .await
        .json()
        .await
        .unwrap();

    let rows = body["data"]["vouchings"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows[0]["vouched_username"].as_str(),
        Some("gapflagged"),
        "the queue is only useful if the case that triggered it comes first"
    );
    assert_eq!(rows[0]["vouched_user_flagged"], json!(true));
    assert_eq!(rows[1]["vouched_user_flagged"], json!(false));
}

#[tokio::test]
async fn the_vouching_queue_is_closed_to_everyone_else() {
    let app = TestApp::spawn().await;
    app.register_user("gapnosycitizen").await;
    app.login("gapnosycitizen").await;

    let resp = app.get("/api/moderation/vouchings").await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

// ═══════════════════════════════════════════════════════════════════
// SKI-295 — admin moderation of cohorts
// ═══════════════════════════════════════════════════════════════════

async fn create_cohort(app: &TestApp, slug: &str, is_public: bool) -> Uuid {
    let resp = app
        .post(
            "/api/cohorts",
            &json!({
                "slug": slug,
                "name": "Rust bootcamp",
                "starts_at": future(1),
                "ends_at": future(90),
                "is_public": is_public,
            }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body: Value = resp.json().await.unwrap();
    body["data"]["cohort"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

#[tokio::test]
async fn the_admin_cohort_listing_sees_private_and_archived_ones() {
    let app = TestApp::spawn().await;
    app.register_user("gapcohortorg").await;
    app.login("gapcohortorg").await;

    create_cohort(&app, "gap-public-cohort", true).await;
    create_cohort(&app, "gap-private-cohort", false).await;
    let archived = create_cohort(&app, "gap-archived-cohort", true).await;
    sqlx::query("UPDATE cohorts SET archived_at = NOW() WHERE id = $1")
        .bind(archived)
        .execute(&app.db)
        .await
        .unwrap();

    // Discovery still shows only the one compliant cohort.
    let body: Value = app.get("/api/cohorts").await.json().await.unwrap();
    assert_eq!(body["data"]["cohorts"].as_array().unwrap().len(), 1);

    app.register_admin("gapcohortadmin").await;

    let body: Value = app.get("/api/admin/cohorts").await.json().await.unwrap();
    assert_eq!(
        body["data"]["total"].as_i64(),
        Some(1),
        "the admin listing defaults to the same view, so the flags are deliberate"
    );

    let body: Value = app
        .get("/api/admin/cohorts?include_private=true&include_archived=true")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["total"].as_i64(), Some(3));

    let row = &body["data"]["cohorts"][0];
    // Same projection as the public listing…
    assert!(row["member_count"].is_i64());
    assert!(row["seats_left"].is_i64());
    assert!(row["cohort"]["slug"].is_string());
    // …plus what moderation needs to act.
    assert_eq!(row["organizer_username"].as_str(), Some("gapcohortorg"));
    assert!(row["message_count"].is_i64());
}

#[tokio::test]
async fn admin_archive_freezes_a_cohort_and_leaves_the_history_readable() {
    let app = TestApp::spawn().await;
    let org = app.register_user("gaparchiveorg").await;
    let org_id = user_id_of(&org);
    app.login("gaparchiveorg").await;
    let cohort = create_cohort(&app, "gap-abusive-cohort", true).await;

    let resp = app
        .post(
            &format!("/api/cohorts/{cohort}/messages"),
            &json!({ "body": "session tonight" }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    let admin = app.register_admin("gaparchiveadmin").await;
    let admin_id = user_id_of(&admin);

    // A one-word motive is not a motive.
    let resp = app
        .post(
            &format!("/api/admin/cohorts/{cohort}/archive"),
            &json!({ "reason": "bad" }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let resp = app
        .post(
            &format!("/api/admin/cohorts/{cohort}/archive"),
            &json!({ "reason": "group chat used for off-platform recruiting" }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = resp.json().await.unwrap();
    assert!(body["data"]["cohort"]["archived_at"].is_string());

    // Archiving twice is a conflict, not a silent no-op: a moderator has to
    // know the gesture they just made was somebody else's.
    let resp = app
        .post(
            &format!("/api/admin/cohorts/{cohort}/archive"),
            &json!({ "reason": "group chat used for off-platform recruiting" }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::CONFLICT);

    // SKI-299 — the entry is there, and carries the motive.
    let entries = audit_entries(&app, "cohort.archive").await;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].0, Some(admin_id));
    assert_eq!(
        entries[0].1.as_ref().unwrap()["reason"].as_str(),
        Some("group chat used for off-platform recruiting")
    );

    // The members keep their history. Freezing a cohort punishes the
    // organizer, not the people who did honest work in it.
    app.login("gaparchiveorg").await;
    let resp = app.get(&format!("/api/cohorts/{cohort}/messages")).await;
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "an archived cohort stays readable to its members"
    );
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["messages"].as_array().unwrap().len(), 1);

    // But it takes no new writes.
    let resp = app
        .post(
            &format!("/api/cohorts/{cohort}/messages"),
            &json!({ "body": "still here?" }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::CONFLICT);
    let _ = org_id;
}

#[tokio::test]
async fn cohort_moderation_is_closed_to_ordinary_users() {
    let app = TestApp::spawn().await;
    app.register_user("gapcohortnobody").await;
    app.login("gapcohortnobody").await;
    let cohort = create_cohort(&app, "gap-nobody-cohort", true).await;

    let resp = app.get("/api/admin/cohorts").await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    let resp = app
        .post(
            &format!("/api/admin/cohorts/{cohort}/archive"),
            &json!({ "reason": "i just felt like it" }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

// ═══════════════════════════════════════════════════════════════════
// SKI-296 — admin moderation of talent offers
// ═══════════════════════════════════════════════════════════════════

async fn create_offer(app: &TestApp, offer_type: &str) -> Uuid {
    let resp = app
        .post(
            "/api/talent-offers",
            &json!({ "offer_type": offer_type, "availability_hours": 2 }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body: Value = resp.json().await.unwrap();
    body["data"]["offer"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

#[tokio::test]
async fn a_held_offer_leaves_the_browse_and_the_author_cannot_bring_it_back() {
    let app = TestApp::spawn().await;
    let author = app.register_user("gapofferauthor").await;
    let author_id = user_id_of(&author);
    set_rank(&app, author_id, ranks::RANK_ARTISAN).await;
    app.login("gapofferauthor").await;
    let offer = create_offer(&app, "pair_programming").await;

    let body: Value = app.get("/api/talent-offers").await.json().await.unwrap();
    assert_eq!(body["data"]["offers"].as_array().unwrap().len(), 1);

    let admin = app.register_admin("gapofferadmin").await;
    let admin_id = user_id_of(&admin);

    let resp = app
        .post(
            &format!("/api/admin/talent-offers/{offer}/deactivate"),
            &json!({ "reason": "short" }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let resp = app
        .post(
            &format!("/api/admin/talent-offers/{offer}/deactivate"),
            &json!({ "reason": "solicits payment outside the platform" }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    // Held twice is a conflict.
    let resp = app
        .post(
            &format!("/api/admin/talent-offers/{offer}/deactivate"),
            &json!({ "reason": "solicits payment outside the platform" }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::CONFLICT);

    let body: Value = app.get("/api/talent-offers").await.json().await.unwrap();
    assert!(
        body["data"]["offers"].as_array().unwrap().is_empty(),
        "a held offer is off the marketplace"
    );

    // The row survives, because a dispute is instructed against what was
    // actually published.
    let still_there: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM talent_offers WHERE id = $1)")
            .bind(offer)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(still_there);

    // The author cannot undo it — otherwise the gesture is worth nothing.
    app.login("gapofferauthor").await;
    let resp = app
        .patch(
            &format!("/api/talent-offers/{offer}"),
            &json!({ "active": true }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // Even flipping active off and on again does not lift it.
    let resp = app
        .patch(
            &format!("/api/talent-offers/{offer}"),
            &json!({ "active": false }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = app.get("/api/talent-offers").await.json().await.unwrap();
    assert!(body["data"]["offers"].as_array().unwrap().is_empty());

    // A moderator can, and both gestures are journalled.
    app.login("gapofferadmin").await;
    let resp = app
        .post(
            &format!("/api/admin/talent-offers/{offer}/reinstate"),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let held = audit_entries(&app, "talent_offer.deactivate").await;
    assert_eq!(held.len(), 1);
    assert_eq!(held[0].0, Some(admin_id));
    assert_eq!(
        held[0].1.as_ref().unwrap()["reason"].as_str(),
        Some("solicits payment outside the platform")
    );
    assert_eq!(audit_entries(&app, "talent_offer.reinstate").await.len(), 1);
}

#[tokio::test]
async fn the_admin_offer_listing_sees_what_the_browse_hides() {
    let app = TestApp::spawn().await;
    let author = app.register_user("gapoffervisible").await;
    let author_id = user_id_of(&author);
    set_rank(&app, author_id, ranks::RANK_ARTISAN).await;
    app.login("gapoffervisible").await;

    let listed = create_offer(&app, "pair_programming").await;
    let held = create_offer(&app, "code_review").await;
    let paused = create_offer(&app, "whiteboard").await;

    let resp = app
        .patch(
            &format!("/api/talent-offers/{paused}"),
            &json!({ "active": false }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    app.register_admin("gapofferlistadmin").await;
    let resp = app
        .post(
            &format!("/api/admin/talent-offers/{held}/deactivate"),
            &json!({ "reason": "misleading hourly rate" }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    // Default: the compliant view, same as the public browse.
    let body: Value = app
        .get("/api/admin/talent-offers")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["total"].as_i64(), Some(1));
    assert_eq!(
        body["data"]["offers"][0]["id"].as_str(),
        Some(listed.to_string().as_str())
    );

    // Everything, with a reason for each absence.
    let body: Value = app
        .get("/api/admin/talent-offers?include_inactive=true")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["total"].as_i64(), Some(3));
    let by_id: std::collections::HashMap<String, Value> = body["data"]["offers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| (o["id"].as_str().unwrap().to_string(), o.clone()))
        .collect();
    assert!(by_id[&listed.to_string()]["hidden_reason"].is_null());
    assert_eq!(
        by_id[&held.to_string()]["hidden_reason"].as_str(),
        Some("moderation_hold")
    );
    assert_eq!(
        by_id[&paused.to_string()]["hidden_reason"].as_str(),
        Some("paused_by_author")
    );
    // Same projection as the public browse.
    assert_eq!(
        by_id[&listed.to_string()]["username"].as_str(),
        Some("gapoffervisible")
    );
    assert_eq!(
        by_id[&listed.to_string()]["rank"].as_str(),
        Some(ranks::RANK_ARTISAN)
    );

    // `held_only` is the moderation queue proper.
    let body: Value = app
        .get("/api/admin/talent-offers?held_only=true")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["total"].as_i64(), Some(1));
    assert_eq!(
        body["data"]["offers"][0]["moderation_reason"].as_str(),
        Some("misleading hourly rate")
    );
}

#[tokio::test]
async fn the_admin_offer_listing_shows_offers_from_hidden_authors() {
    let app = TestApp::spawn().await;
    let author = app.register_user("gapofferhidden").await;
    let author_id = user_id_of(&author);
    set_rank(&app, author_id, ranks::RANK_ARTISAN).await;
    app.login("gapofferhidden").await;
    create_offer(&app, "mock_interview").await;

    sqlx::query("UPDATE users SET profile_hidden = TRUE WHERE id = $1")
        .bind(author_id)
        .execute(&app.db)
        .await
        .unwrap();

    // The public browse drops it — which is why it could not be inspected.
    let body: Value = app.get("/api/talent-offers").await.json().await.unwrap();
    assert!(body["data"]["offers"].as_array().unwrap().is_empty());

    app.register_admin("gapofferhiddenadmin").await;
    let body: Value = app
        .get(&format!("/api/admin/talent-offers?user_id={author_id}"))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["total"].as_i64(), Some(1));
    assert_eq!(
        body["data"]["offers"][0]["hidden_reason"].as_str(),
        Some("author_hidden")
    );
}

// ═══════════════════════════════════════════════════════════════════
// SKI-298 — the AI companion cost projection
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn assistant_stats_count_refusals_and_worker_failures() {
    let app = TestApp::spawn().await;
    let me = app.register_user("gapaiuser").await;
    let my_id = user_id_of(&me);
    app.login("gapaiuser").await;

    // The worker is absent in the test harness, so every call records an
    // `unavailable` interaction. That is the failure signal the ticket asks
    // for, and it costs no quota.
    for _ in 0..2 {
        let resp = app
            .post(
                "/api/assistant/ask",
                &json!({ "interaction_type": "explain", "prompt": "borrow checker?" }),
            )
            .await;
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    // A cache hit and a quota refusal, seeded directly: reaching them
    // through HTTP needs a live worker and eleven successful calls.
    sqlx::query(
        "INSERT INTO ai_interactions
             (user_id, interaction_type, prompt, status, disclosure_label, tokens_used, cached)
         VALUES ($1, 'explain', 'cached one', 'ok', 'label', 0, TRUE)",
    )
    .bind(my_id)
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO ai_interactions
             (user_id, interaction_type, prompt, status, tokens_used)
         VALUES ($1, 'pre_review', 'billed one', 'ok', 900)",
    )
    .bind(my_id)
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO ai_interactions
             (user_id, interaction_type, prompt, status, refusal_kind)
         VALUES ($1, 'debug_help', 'refused one', 'rate_limited', 'daily_quota')",
    )
    .bind(my_id)
    .execute(&app.db)
    .await
    .unwrap();

    app.register_admin("gapaiadmin").await;
    let resp = app.get("/api/admin/assistant/stats?window_days=7").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = resp.json().await.unwrap();
    let stats = &body["data"]["stats"];

    assert_eq!(stats["total_requests"].as_i64(), Some(5));
    assert_eq!(stats["cache_hits"].as_i64(), Some(1));
    assert_eq!(stats["billed_calls"].as_i64(), Some(1));
    assert_eq!(
        stats["cache_hit_rate"].as_f64(),
        Some(0.5),
        "the hit rate is the main cost lever, so it is exposed, not derived by the client"
    );
    assert_eq!(stats["tokens_total"].as_i64(), Some(900));
    assert_eq!(stats["refused_daily_quota"].as_i64(), Some(1));
    assert_eq!(stats["worker_failures"].as_i64(), Some(2));
    assert_eq!(stats["distinct_users"].as_i64(), Some(1));
    assert_eq!(stats["by_interaction_type"]["explain"].as_i64(), Some(3));
    assert_eq!(stats["by_status"]["unavailable"].as_i64(), Some(2));
    assert_eq!(
        stats["top_consumers"][0]["username"].as_str(),
        Some("gapaiuser")
    );

    // The aggregates must not carry what people asked.
    let serialized = serde_json::to_string(stats).unwrap();
    assert!(
        !serialized.contains("borrow checker"),
        "prompt text has no business in a cost dashboard"
    );

    // Policy is echoed so the dashboard does not hardcode it.
    assert_eq!(body["data"]["policy"]["daily_quota"].as_i64(), Some(10));
}

// `the_burst_limit_is_recorded_as_a_refusal` lives in
// `tests/test_ai_burst_limit.rs`: it is the one assertion in this batch that
// needs the rate limiter switched on, and this harness switches it off for
// every test in a binary. See that file for why it has to be alone.

#[tokio::test]
async fn the_admin_can_read_one_users_disclosure_ledger() {
    let app = TestApp::spawn().await;
    let me = app.register_user("gapailedger").await;
    let my_id = user_id_of(&me);
    app.login("gapailedger").await;

    let resp = app
        .post(
            "/api/assistant/ask",
            &json!({ "interaction_type": "pre_review", "prompt": "is this idiomatic?" }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);

    // Not readable by another learner.
    app.register_user("gapainosy").await;
    app.login("gapainosy").await;
    let resp = app
        .get(&format!("/api/admin/users/{my_id}/assistant-interactions"))
        .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    app.register_admin("gapailedgeradmin").await;
    let resp = app
        .get(&format!("/api/admin/users/{my_id}/assistant-interactions"))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["total"].as_i64(), Some(1));
    assert_eq!(
        body["data"]["interactions"][0]["prompt"].as_str(),
        Some("is this idiomatic?"),
        "instructing a disclosure dispute is the reason this route exists"
    );

    let resp = app
        .get(&format!(
            "/api/admin/users/{}/assistant-interactions",
            Uuid::new_v4()
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// ═══════════════════════════════════════════════════════════════════
// SKI-299 — audit log on the destructive moderation routes
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn breaking_a_vouching_is_journalled() {
    let app = TestApp::spawn().await;
    let mod_id = user_id_of(&app.register_user("gapbreakmod").await);
    grant_capability(&app, mod_id, "community_moderator").await;

    let voucher = user_id_of(&app.register_user("gapbreakvoucher").await);
    let vouched = user_id_of(&app.register_user("gapbreakvouched").await);
    set_rank(&app, voucher, ranks::RANK_DOYEN).await;
    let vouching = seed_vouching(&app, voucher, vouched, 60).await;

    app.login("gapbreakmod").await;
    let resp = app
        .post(
            &format!("/api/moderation/vouchings/{vouching}/break"),
            &json!({ "reason": "vouched user caught plagiarising" }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let entries = audit_entries(&app, "vouching.break").await;
    assert_eq!(
        entries.len(),
        1,
        "an action that costs someone a rank for ninety days leaves a trace"
    );
    assert_eq!(entries[0].0, Some(mod_id));
    let meta = entries[0].1.as_ref().unwrap();
    assert_eq!(
        meta["reason"].as_str(),
        Some("vouched user caught plagiarising")
    );
    assert_eq!(meta["penalty_applied"], json!(true));
    assert_eq!(
        meta["voucher_id"].as_str(),
        Some(voucher.to_string().as_str())
    );
}

#[tokio::test]
async fn deleting_an_external_signal_requires_a_motive_and_is_journalled() {
    let app = TestApp::spawn().await;
    let owner = app.register_user("gapsignalowner").await;
    let owner_id = user_id_of(&owner);
    app.login("gapsignalowner").await;

    let resp = app
        .post(
            "/api/users/me/external-signals",
            &json!({
                "provider": "medium",
                "url": "https://medium.com/@someone/a-post",
                "title": "A post I did not write",
            }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body: Value = resp.json().await.unwrap();
    let signal: Uuid = body["data"]["signal"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    let mod_id = user_id_of(&app.register_user("gapsignalmod").await);
    grant_capability(&app, mod_id, "community_moderator").await;
    app.login("gapsignalmod").await;

    // No motive at all: the request does not even parse.
    let resp = app
        .delete(&format!("/api/moderation/external-signals/{signal}"))
        .await;
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "deleting a user declaration without saying why was the real gap"
    );

    let resp = app
        .delete(&format!(
            "/api/moderation/external-signals/{signal}?reason=short"
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let resp = app
        .delete(&format!(
            "/api/moderation/external-signals/{signal}?reason=claimed%20authorship%20of%20someone%20else%20work"
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let entries = audit_entries(&app, "external_signal.delete").await;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].0, Some(mod_id));
    let meta = entries[0].1.as_ref().unwrap();
    assert_eq!(
        meta["owner_id"].as_str(),
        Some(owner_id.to_string().as_str())
    );
    assert_eq!(
        meta["url"].as_str(),
        Some("https://medium.com/@someone/a-post"),
        "the entry carries what was destroyed — an id pointing at nothing documents nothing"
    );
    assert!(meta["reason"].as_str().unwrap().starts_with("claimed"));

    // Gone for good, on purpose.
    let resp = app
        .delete(&format!(
            "/api/moderation/external-signals/{signal}?reason=claimed%20authorship%20again"
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn verifying_an_external_signal_is_journalled() {
    let app = TestApp::spawn().await;
    app.register_user("gapverifyowner").await;
    app.login("gapverifyowner").await;
    let body: Value = app
        .post(
            "/api/users/me/external-signals",
            &json!({
                "provider": "dev_to",
                "url": "https://dev.to/someone/a-post",
                "title": "A post",
            }),
        )
        .await
        .json()
        .await
        .unwrap();
    let signal: Uuid = body["data"]["signal"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    let mod_id = user_id_of(&app.register_user("gapverifymod").await);
    grant_capability(&app, mod_id, "community_curator").await;
    app.login("gapverifymod").await;

    let resp = app
        .post(
            &format!("/api/moderation/external-signals/{signal}/verify"),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let entries = audit_entries(&app, "external_signal.verify").await;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].0, Some(mod_id));
}

#[tokio::test]
async fn setting_prerequisites_records_the_graph_before_and_after() {
    let app = TestApp::spawn().await;

    let mut ids = Vec::new();
    for slug in ["gap-tree-a", "gap-tree-b", "gap-tree-c"] {
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO skill_nodes (slug, display_name, domain)
             VALUES ($1, $1, 'code') RETURNING id",
        )
        .bind(slug)
        .fetch_one(&app.db)
        .await
        .expect("seed skill");
        ids.push(id);
    }
    let (a, b, c) = (ids[0], ids[1], ids[2]);

    let admin = app.register_admin("gaptreeadmin").await;
    let admin_id = user_id_of(&admin);

    let resp = app
        .put(
            &format!("/api/admin/skills/{a}/prerequisites"),
            &json!({ "prerequisite_skill_ids": [b] }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = app
        .put(
            &format!("/api/admin/skills/{a}/prerequisites"),
            &json!({ "prerequisite_skill_ids": [c] }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let entries = audit_entries(&app, "skill.set_prerequisites").await;
    assert_eq!(entries.len(), 2);
    assert!(entries.iter().all(|e| e.0 == Some(admin_id)));

    let second = entries
        .iter()
        .find(|e| e.1.as_ref().unwrap()["after"] == json!([c.to_string()]))
        .expect("the second edit is recorded");
    assert_eq!(
        second.1.as_ref().unwrap()["before"],
        json!([b.to_string()]),
        "a full overwrite is only reviewable if the journal says what it overwrote"
    );
}

#[tokio::test]
async fn capability_grants_and_revocations_are_journalled() {
    let app = TestApp::spawn().await;
    let target = user_id_of(&app.register_user("gapcaptarget").await);
    let admin = app.register_admin("gapcapadmin").await;
    let admin_id = user_id_of(&admin);

    let resp = app
        .post(
            &format!("/api/admin/users/{target}/capabilities"),
            &json!({ "capability": "community_moderator" }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app
        .delete(&format!(
            "/api/admin/users/{target}/capabilities/community_moderator"
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let granted = audit_entries(&app, "capability.grant").await;
    assert_eq!(granted.len(), 1);
    assert_eq!(granted[0].0, Some(admin_id));
    assert_eq!(
        granted[0].1.as_ref().unwrap()["capability"].as_str(),
        Some("community_moderator")
    );
    assert_eq!(audit_entries(&app, "capability.revoke").await.len(), 1);
}

#[tokio::test]
async fn fraud_decisions_are_journalled() {
    let app = TestApp::spawn().await;
    let target = user_id_of(&app.register_user("gapfraudtarget").await);
    let admin = app.register_admin("gapfraudadmin").await;
    let admin_id = user_id_of(&admin);

    let resp = app
        .post(
            &format!("/api/admin/fraud/users/{target}/mark-valid"),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let entries = audit_entries(&app, "fraud.user_mark_valid").await;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].0, Some(admin_id));
}

#[tokio::test]
async fn the_admin_audit_log_shows_both_journals() {
    let app = TestApp::spawn().await;
    let target = user_id_of(&app.register_user("gapuniontarget").await);
    let admin = app.register_admin("gapunionadmin").await;
    let admin_id = user_id_of(&admin);

    // A legacy handler, writing to `admin_audit_log`…
    let resp = app
        .post(
            &format!("/api/admin/users/{target}/ban"),
            &json!({ "reason": "spam" }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    // …and a newly instrumented one, writing to the append-only `audit_log`.
    let resp = app
        .post(
            &format!("/api/admin/users/{target}/capabilities"),
            &json!({ "capability": "community_curator" }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    let body: Value = app
        .get("/api/admin/audit-log?per_page=50")
        .await
        .json()
        .await
        .unwrap();
    let rows = body["data"].as_array().unwrap();

    let sources: Vec<&str> = rows.iter().filter_map(|r| r["source"].as_str()).collect();
    assert!(
        sources.contains(&"admin_audit_log"),
        "the legacy entries the admin panel already showed must not disappear"
    );
    assert!(
        sources.contains(&"audit_log"),
        "the new moderation entries must land on the screen an operator actually opens"
    );

    let new_row = rows
        .iter()
        .find(|r| r["action"] == json!("capability.grant"))
        .expect("the capability grant is listed");
    // The shape the front already consumes, unchanged.
    assert_eq!(
        new_row["admin_id"].as_str(),
        Some(admin_id.to_string().as_str())
    );
    assert_eq!(
        new_row["details"]["capability"].as_str(),
        Some("community_curator"),
        "`metadata` is surfaced as `details`, so no front change is needed"
    );
    assert!(new_row["created_at"].is_string());

    // Filtering by action still works across both halves.
    let body: Value = app
        .get("/api/admin/audit-log?action=capability.grant")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["pagination"]["total"].as_i64(), Some(1));
}
