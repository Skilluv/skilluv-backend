//! Integration tests for SKI-45 (reverse marketplace) and SKI-46
//! (reputation staking).
//!
//! They share a file because they share the mechanism that ties them
//! together: a broken vouching drops the voucher's *effective* rank, which
//! must immediately cost them the right to publish offers — without ever
//! rewriting the derived rank the proof engine owns.

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

async fn verify_wallet(app: &TestApp, user_id: Uuid) {
    sqlx::query(
        "INSERT INTO talent_wallets (user_id, stripe_account_id, stripe_kyc_status)
         VALUES ($1, 'acct_test', 'verified')
         ON CONFLICT (user_id) DO UPDATE SET stripe_kyc_status = 'verified'",
    )
    .bind(user_id)
    .execute(&app.db)
    .await
    .expect("verify wallet");
}

async fn grant_capability(app: &TestApp, user_id: Uuid, capability: &str) {
    sqlx::query(
        "INSERT INTO user_capabilities (user_id, capability, granted_reason)
         VALUES ($1, $2, 'test')",
    )
    .bind(user_id)
    .bind(capability)
    .execute(&app.db)
    .await
    .expect("grant capability");
}

// ═══════════════════════════════════════════════════════════════════
// SKI-45 — talent offers
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn publishing_requires_artisan() {
    let app = TestApp::spawn().await;
    let me = app.register_user("offerjunior").await;
    let my_id = user_id_of(&me);
    app.login("offerjunior").await;

    let payload = json!({ "offer_type": "pair_programming", "availability_hours": 2 });

    let resp = app.post("/api/talent-offers", &payload).await;
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "an Apprenti has no track record to offer"
    );

    set_rank(&app, my_id, ranks::RANK_RANGER).await;
    let resp = app.post("/api/talent-offers", &payload).await;
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "Ranger is still below the bar"
    );

    set_rank(&app, my_id, ranks::RANK_ARTISAN).await;
    let resp = app.post("/api/talent-offers", &payload).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn paid_offers_require_a_verified_payout_account() {
    let app = TestApp::spawn().await;
    let me = app.register_user("offerpaid").await;
    let my_id = user_id_of(&me);
    set_rank(&app, my_id, ranks::RANK_ARTISAN).await;
    app.login("offerpaid").await;

    let resp = app
        .post(
            "/api/talent-offers",
            &json!({ "offer_type": "code_review", "price_cents_per_hour": 5000 }),
        )
        .await;
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "advertising a price we could not pay out is a promise we cannot keep"
    );

    // The free version is fine.
    let resp = app
        .post(
            "/api/talent-offers",
            &json!({ "offer_type": "code_review" }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let created: Value = resp.json().await.unwrap();
    let offer_id = created["data"]["offer"]["id"].as_str().unwrap().to_string();

    // Switching it to paid is refused for the same reason...
    let resp = app
        .client
        .patch(format!("{}/api/talent-offers/{offer_id}", app.addr))
        .json(&json!({ "price_cents_per_hour": 5000 }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // ...until the wallet is verified.
    verify_wallet(&app, my_id).await;
    let resp = app
        .client
        .patch(format!("{}/api/talent-offers/{offer_id}", app.addr))
        .json(&json!({ "price_cents_per_hour": 5000 }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["offer"]["price_cents_per_hour"], 5000);

    // Explicit null makes it free again.
    let resp = app
        .client
        .patch(format!("{}/api/talent-offers/{offer_id}", app.addr))
        .json(&json!({ "price_cents_per_hour": null }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = resp.json().await.unwrap();
    assert!(body["data"]["offer"]["price_cents_per_hour"].is_null());
}

#[tokio::test]
async fn offers_are_capped_deduplicated_and_validated() {
    let app = TestApp::spawn().await;
    let me = app.register_user("offercap").await;
    let my_id = user_id_of(&me);
    set_rank(&app, my_id, ranks::RANK_MAITRE).await;
    app.login("offercap").await;

    let types = [
        "pair_programming",
        "code_review",
        "whiteboard",
        "mock_interview",
        "career_advice",
    ];
    for t in types {
        let resp = app
            .post("/api/talent-offers", &json!({ "offer_type": t }))
            .await;
        assert_eq!(resp.status(), StatusCode::CREATED, "offer type {t}");
    }

    // Free a slot first: with five live offers the cap fires before the
    // uniqueness check, so a duplicate would be refused as "too many"
    // rather than "already there".
    let spare: Uuid = sqlx::query_scalar(
        "SELECT id FROM talent_offers WHERE user_id = $1 AND offer_type = 'whiteboard'",
    )
    .bind(my_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    let resp = app.delete(&format!("/api/talent-offers/{spare}")).await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // The same type twice collides on the uniqueness constraint.
    let resp = app
        .post(
            "/api/talent-offers",
            &json!({ "offer_type": "code_review" }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::CONFLICT);

    // Shape validation.
    for bad in [
        json!({ "offer_type": "therapy" }),
        json!({ "offer_type": "whiteboard", "availability_hours": 40 }),
        json!({ "offer_type": "whiteboard", "availability_hours": 0 }),
    ] {
        let resp = app.post("/api/talent-offers", &bad).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "payload {bad}");
    }
}

#[tokio::test]
async fn browse_is_public_and_filters() {
    let app = TestApp::spawn().await;
    // Unique slug: migration 0057 already seeds a catalog that includes
    // `rust`, and slugs are UNIQUE.
    let skill_id = Uuid::new_v4();
    let skill_slug = format!("offer-skill-{}", &skill_id.to_string()[..8]);
    sqlx::query(
        "INSERT INTO skill_nodes (id, slug, display_name, domain)
         VALUES ($1, $2, 'Offer skill', 'code')",
    )
    .bind(skill_id)
    .bind(&skill_slug)
    .execute(&app.db)
    .await
    .unwrap();

    let me = app.register_user("offerbrowse").await;
    let my_id = user_id_of(&me);
    set_rank(&app, my_id, ranks::RANK_ARTISAN).await;
    verify_wallet(&app, my_id).await;
    app.login("offerbrowse").await;

    app.post(
        "/api/talent-offers",
        &json!({ "offer_type": "pair_programming", "skill_id": skill_id }),
    )
    .await;
    app.post(
        "/api/talent-offers",
        &json!({ "offer_type": "mock_interview", "price_cents_per_hour": 9000 }),
    )
    .await;

    let body: Value = app.get("/api/talent-offers").await.json().await.unwrap();
    assert_eq!(body["data"]["offers"].as_array().unwrap().len(), 2);

    let body: Value = app
        .get("/api/talent-offers?free_only=true")
        .await
        .json()
        .await
        .unwrap();
    let offers = body["data"]["offers"].as_array().unwrap();
    assert_eq!(offers.len(), 1);
    assert_eq!(offers[0]["offer_type"], "pair_programming");

    let body: Value = app
        .get(&format!("/api/talent-offers?skill={skill_slug}"))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["offers"].as_array().unwrap().len(), 1);

    let body: Value = app
        .get("/api/talent-offers?offer_type=mock_interview")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["offers"].as_array().unwrap().len(), 1);

    let resp = app.get("/api/talent-offers?offer_type=nonsense").await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_demoted_author_drops_out_of_browse_without_any_cleanup() {
    let app = TestApp::spawn().await;
    let me = app.register_user("offerdemoted").await;
    let my_id = user_id_of(&me);
    set_rank(&app, my_id, ranks::RANK_ARTISAN).await;
    app.login("offerdemoted").await;
    app.post(
        "/api/talent-offers",
        &json!({ "offer_type": "career_advice" }),
    )
    .await;

    let body: Value = app.get("/api/talent-offers").await.json().await.unwrap();
    assert_eq!(body["data"]["offers"].as_array().unwrap().len(), 1);

    set_rank(&app, my_id, ranks::RANK_RANGER).await;
    let body: Value = app.get("/api/talent-offers").await.json().await.unwrap();
    assert!(
        body["data"]["offers"].as_array().unwrap().is_empty(),
        "rank is re-checked at read time, so no cleanup job is needed"
    );

    // The owner still sees their own, with the reason spelled out.
    let body: Value = app
        .get("/api/users/me/talent-offers")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["offers"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["can_publish"], false);
}

// ═══════════════════════════════════════════════════════════════════
// SKI-46 — vouchings
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn only_a_doyen_may_vouch() {
    let app = TestApp::spawn().await;
    let junior = app.register_user("vouchjunior").await;
    let junior_id = user_id_of(&junior);

    let senior = app.register_user("vouchsenior").await;
    let senior_id = user_id_of(&senior);
    app.login("vouchsenior").await;

    let payload = json!({ "vouched_id": junior_id, "statement": "I know their work." });

    let resp = app.post("/api/vouchings", &payload).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    set_rank(&app, senior_id, ranks::RANK_MAITRE).await;
    let resp = app.post("/api/vouchings", &payload).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN, "Maître is not enough");

    set_rank(&app, senior_id, ranks::RANK_DOYEN).await;
    let resp = app.post("/api/vouchings", &payload).await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Same pair twice is a conflict.
    let resp = app.post("/api/vouchings", &payload).await;
    assert_eq!(resp.status(), StatusCode::CONFLICT);

    // Cannot vouch for yourself.
    let resp = app
        .post("/api/vouchings", &json!({ "vouched_id": senior_id }))
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn vouching_windows_and_cap_are_enforced() {
    let app = TestApp::spawn().await;
    let senior = app.register_user("vouchcap").await;
    let senior_id = user_id_of(&senior);
    set_rank(&app, senior_id, ranks::RANK_DOYEN).await;
    app.login("vouchcap").await;

    // Ten live vouchings is the cap.
    for i in 0..10 {
        let target = app.register_user(&format!("vouchtarget{i}")).await;
        app.login("vouchcap").await;
        let resp = app
            .post(
                "/api/vouchings",
                &json!({ "vouched_id": user_id_of(&target) }),
            )
            .await;
        assert_eq!(resp.status(), StatusCode::CREATED, "vouching {i}");
    }

    let extra = app.register_user("vouchextra").await;
    app.login("vouchcap").await;
    let resp = app
        .post(
            "/api/vouchings",
            &json!({ "vouched_id": user_id_of(&extra) }),
        )
        .await;
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "a vouching you cannot stand behind is worth nothing"
    );

    // Window bounds.
    for days in [1, 5000] {
        let resp = app
            .post(
                "/api/vouchings",
                &json!({ "vouched_id": user_id_of(&extra), "window_days": days }),
            )
            .await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "window {days}");
    }
}

#[tokio::test]
async fn breaking_a_vouching_penalises_the_voucher_without_rewriting_their_rank() {
    let app = TestApp::spawn().await;
    let junior = app.register_user("breakjunior").await;
    let junior_id = user_id_of(&junior);

    let senior = app.register_user("breaksenior").await;
    let senior_id = user_id_of(&senior);
    set_rank(&app, senior_id, ranks::RANK_DOYEN).await;
    verify_wallet(&app, senior_id).await;
    app.login("breaksenior").await;

    let created: Value = app
        .post(
            "/api/vouchings",
            &json!({ "vouched_id": junior_id, "statement": "Vouching for them." }),
        )
        .await
        .json()
        .await
        .unwrap();
    let vouching_id = created["data"]["vouching"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // The senior can publish offers while in good standing.
    let resp = app
        .post("/api/talent-offers", &json!({ "offer_type": "whiteboard" }))
        .await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    let moderator = app.register_user("breakmoderator").await;
    let moderator_id = user_id_of(&moderator);
    grant_capability(&app, moderator_id, "community_moderator").await;
    app.login("breakmoderator").await;

    // The reason is mandatory and substantive.
    let resp = app
        .post(
            &format!("/api/moderation/vouchings/{vouching_id}/break"),
            &json!({ "reason": "bad" }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let resp = app
        .post(
            &format!("/api/moderation/vouchings/{vouching_id}/break"),
            &json!({ "reason": "confirmed plagiarism on three deliverables" }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let report: Value = resp.json().await.unwrap();
    assert_eq!(report["data"]["penalty_applied"], true);
    assert_eq!(report["data"]["voucher_rank_before"], "doyen");
    assert_eq!(
        report["data"]["voucher_rank_effective"], "maitre",
        "the penalty is one step down"
    );

    // The derived rank is untouched — it is what the proofs say, and the
    // proofs have not changed.
    let raw_rank: String = sqlx::query_scalar("SELECT rank FROM user_ranks WHERE user_id = $1")
        .bind(senior_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(
        raw_rank, "doyen",
        "the penalty is a layer, not a rewrite of the derived rank"
    );

    // The governance journal records why.
    let override_reason: String =
        sqlx::query_scalar("SELECT reason FROM rank_overrides WHERE source_vouching_id = $1::UUID")
            .bind(&vouching_id)
            .fetch_one(&app.db)
            .await
            .expect("a broken vouching is journaled");
    assert!(override_reason.contains("broken vouching"));

    // A penalised Doyen is an effective Maître, which is still above the
    // Artisan bar — so offers keep working. The penalty is one step, not a
    // ban, and it bites where it is supposed to: vouching (asserted in
    // `a_penalised_voucher_cannot_keep_vouching`).
    app.login("breaksenior").await;
    let resp = app
        .post(
            "/api/talent-offers",
            &json!({ "offer_type": "mock_interview" }),
        )
        .await;
    assert_eq!(
        resp.status(),
        StatusCode::CREATED,
        "a one-step penalty must not silently revoke unrelated rights"
    );

    // Breaking the same vouching twice is a conflict.
    app.login("breakmoderator").await;
    let resp = app
        .post(
            &format!("/api/moderation/vouchings/{vouching_id}/break"),
            &json!({ "reason": "confirmed plagiarism on three deliverables" }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

/// The penalty must bite for someone sitting exactly on the Artisan bar.
///
/// Vouching itself is Doyen-only, so this exercises the penalty layer
/// directly rather than through a broken vouching.
#[tokio::test]
async fn a_penalised_artisan_loses_publishing_rights() {
    let app = TestApp::spawn().await;
    let me = app.register_user("penartisan").await;
    let my_id = user_id_of(&me);
    set_rank(&app, my_id, ranks::RANK_ARTISAN).await;
    app.login("penartisan").await;

    let resp = app
        .post("/api/talent-offers", &json!({ "offer_type": "whiteboard" }))
        .await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    sqlx::query(
        "UPDATE user_ranks SET penalty_until = NOW() + INTERVAL '90 days' WHERE user_id = $1",
    )
    .bind(my_id)
    .execute(&app.db)
    .await
    .unwrap();

    let resp = app
        .post(
            "/api/talent-offers",
            &json!({ "offer_type": "career_advice" }),
        )
        .await;
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "an effective Ranger is below the bar"
    );

    // Existing offers drop out of browse for the duration.
    let body: Value = app.get("/api/talent-offers").await.json().await.unwrap();
    assert!(body["data"]["offers"].as_array().unwrap().is_empty());

    // And the window expires on its own — no job has to lift it.
    sqlx::query(
        "UPDATE user_ranks SET penalty_until = NOW() - INTERVAL '1 day' WHERE user_id = $1",
    )
    .bind(my_id)
    .execute(&app.db)
    .await
    .unwrap();
    let body: Value = app.get("/api/talent-offers").await.json().await.unwrap();
    assert_eq!(body["data"]["offers"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn a_penalised_voucher_cannot_keep_vouching() {
    let app = TestApp::spawn().await;
    let junior = app.register_user("penjunior").await;
    let junior_id = user_id_of(&junior);
    let other = app.register_user("penother").await;
    let other_id = user_id_of(&other);

    let senior = app.register_user("pensenior").await;
    let senior_id = user_id_of(&senior);
    set_rank(&app, senior_id, ranks::RANK_DOYEN).await;
    app.login("pensenior").await;
    let created: Value = app
        .post("/api/vouchings", &json!({ "vouched_id": junior_id }))
        .await
        .json()
        .await
        .unwrap();
    let vouching_id = created["data"]["vouching"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let moderator = app.register_user("penmoderator").await;
    grant_capability(&app, user_id_of(&moderator), "plagiarism_reviewer").await;
    app.login("penmoderator").await;
    app.post(
        &format!("/api/moderation/vouchings/{vouching_id}/break"),
        &json!({ "reason": "confirmed fraud during the vouching window" }),
    )
    .await;

    app.login("pensenior").await;
    let resp = app
        .post("/api/vouchings", &json!({ "vouched_id": other_id }))
        .await;
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "one bad call must not cascade into more"
    );
}

#[tokio::test]
async fn withdrawing_carries_no_penalty() {
    let app = TestApp::spawn().await;
    let junior = app.register_user("wdjunior").await;
    let junior_id = user_id_of(&junior);
    let senior = app.register_user("wdsenior").await;
    let senior_id = user_id_of(&senior);
    set_rank(&app, senior_id, ranks::RANK_DOYEN).await;
    app.login("wdsenior").await;

    let created: Value = app
        .post("/api/vouchings", &json!({ "vouched_id": junior_id }))
        .await
        .json()
        .await
        .unwrap();
    let vouching_id = created["data"]["vouching"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let resp = app.delete(&format!("/api/vouchings/{vouching_id}")).await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let penalty: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT penalty_until FROM user_ranks WHERE user_id = $1")
            .bind(senior_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(
        penalty.is_none(),
        "penalising an honest withdrawal would push people to stay silent instead"
    );

    // Withdrawing frees the pair to be vouched again later.
    let resp = app
        .post("/api/vouchings", &json!({ "vouched_id": junior_id }))
        .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn breaking_requires_a_moderator_capability() {
    let app = TestApp::spawn().await;
    let junior = app.register_user("modjunior").await;
    let senior = app.register_user("modsenior").await;
    set_rank(&app, user_id_of(&senior), ranks::RANK_DOYEN).await;
    app.login("modsenior").await;
    let created: Value = app
        .post(
            "/api/vouchings",
            &json!({ "vouched_id": user_id_of(&junior) }),
        )
        .await
        .json()
        .await
        .unwrap();
    let vouching_id = created["data"]["vouching"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Even the voucher themselves cannot break it — that path is withdraw,
    // which carries no penalty.
    let resp = app
        .post(
            &format!("/api/moderation/vouchings/{vouching_id}/break"),
            &json!({ "reason": "changed my mind about them" }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn vouchings_surface_on_the_profile_and_in_talent_search() {
    let app = TestApp::spawn().await;
    let junior = app.register_user("srchjunior").await;
    let junior_id = user_id_of(&junior);

    // Put the junior on an orientation so talent search can see them.
    let orientation_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO orientations (id, slug, name, primary_domain, is_curated)
         VALUES ($1, 'search-orientation', 'Search', 'code', TRUE)",
    )
    .bind(orientation_id)
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO user_orientations (user_id, orientation_id, mode)
         VALUES ($1, $2, 'active')",
    )
    .bind(junior_id)
    .bind(orientation_id)
    .execute(&app.db)
    .await
    .unwrap();

    let senior = app.register_user("srchsenior").await;
    set_rank(&app, user_id_of(&senior), ranks::RANK_DOYEN).await;
    app.login("srchsenior").await;
    app.post(
        "/api/vouchings",
        &json!({ "vouched_id": junior_id, "statement": "Solid engineer." }),
    )
    .await;

    // Public profile view.
    let body: Value = app
        .get(&format!("/api/users/{junior_id}/vouchings"))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["count"], 1);
    assert_eq!(body["data"]["vouchings"][0]["statement"], "Solid engineer.");
    // The harness registers users with first/last name "Test User", and the
    // endpoint prefers display_name over username.
    assert_eq!(
        body["data"]["vouchings"][0]["voucher_display_name"],
        "Test User"
    );

    // Talent search reports the count as its own field, not folded into
    // the proof-derived totals.
    let enterprise = app.register_enterprise("SearchCo").await;
    let _ = enterprise;
    let body: Value = app
        .get("/api/talents/search/v3?orientation=search-orientation")
        .await
        .json()
        .await
        .unwrap();
    if let Some(talents) = body["data"]["talents"].as_array()
        && let Some(row) = talents
            .iter()
            .find(|t| t["user_id"].as_str() == Some(&junior_id.to_string()))
    {
        assert_eq!(row["vouched_by_count"], 1);
        assert_eq!(
            row["matched_wpc_total"], 0,
            "an endorsement must never masquerade as verified work"
        );
    }
}
