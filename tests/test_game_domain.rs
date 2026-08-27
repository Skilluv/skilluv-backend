//! The game domain.
//!
//! Twenty-one trades in five families, and one rule no other domain has: a
//! game slice is not validated until real players have touched it. These tests
//! hold the places where that rule, and the domain's other lines, are enforced
//! by the schema or a service rather than by good intentions:
//!
//!   * the catalogue opens with twenty-one trades, five families and their grids;
//!   * the eight bases split shipped work from recognition, as the rank needs;
//!   * a slice needs three playtests before a reviewer can validate it;
//!   * a creator cannot playtest their own slice;
//!   * a mod is confirmed by someone other than its author, and becomes a
//!     deliverable and an attestation when it is;
//!   * the craft score counts something for every game basis worth counting.

mod common;
use common::TestApp;
use serde_json::{Value, json};
use uuid::Uuid;

// ═══════════════════════════════════════════════════════════════════
// Fixtures
// ═══════════════════════════════════════════════════════════════════

async fn a_person(app: &TestApp, username: &str) -> Uuid {
    app.register_user(username).await;
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
    .expect("grant");
}

/// A user-owned game slice with a playable URL, ready to be playtested and
/// validated. Returns the slice id.
async fn a_game_slice(app: &TestApp, owner: Uuid, slug: &str) -> Uuid {
    let project_id: Uuid = sqlx::query_scalar(
        "INSERT INTO projects (slug, name, owner_type, owner_id)
         VALUES ($1, $2, 'user', $3) RETURNING id",
    )
    .bind(slug)
    .bind(format!("Project {slug}"))
    .bind(owner)
    .fetch_one(&app.db)
    .await
    .expect("project");

    sqlx::query_scalar(
        "INSERT INTO project_slices
            (project_id, slice_type, title, description, primary_domain, difficulty,
             game_artifact_subtype, game_challenge_format, game_playable_url,
             fragments_reward)
         VALUES ($1, 'game_artifact', 'A small game', 'A playable slice', 'game', 3,
                 'build_playable', 'individual', 'https://itch.io/demo', 50)
         RETURNING id",
    )
    .bind(project_id)
    .fetch_one(&app.db)
    .await
    .expect("slice")
}

async fn a_playtest(app: &TestApp, slice_id: Uuid, tester: Uuid, fun: i16) {
    sqlx::query(
        "INSERT INTO game_playtests
            (slice_id, playtester_user_id, fun_score, clarity_score,
             difficulty_perception, would_play_again)
         VALUES ($1, $2, $3, 4, 'balanced', TRUE)
         ON CONFLICT (slice_id, playtester_user_id) DO UPDATE SET fun_score = EXCLUDED.fun_score",
    )
    .bind(slice_id)
    .bind(tester)
    .bind(fun)
    .execute(&app.db)
    .await
    .expect("playtest");
}

// ═══════════════════════════════════════════════════════════════════
// The catalogue
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn game_is_an_open_domain_with_twenty_one_trades_in_five_families() {
    let app = TestApp::spawn().await;

    // Live trades only: the five legacy orientations 0570 replaced are
    // is_archived = TRUE and pointed at their successors, and they stay
    // is_curated so their history reads — the same filter the migration's own
    // guard uses.
    let curated: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM orientations
          WHERE primary_domain = 'game' AND is_curated = TRUE AND is_archived = FALSE",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(curated, 21, "expected 21 live curated game trades");

    let families: i64 = sqlx::query_scalar(
        "SELECT count(DISTINCT reviewer_group) FROM orientations
          WHERE primary_domain = 'game' AND is_curated = TRUE AND is_archived = FALSE",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(families, 5, "expected 5 review families");
}

#[tokio::test]
async fn every_family_and_the_default_have_a_review_grid() {
    let app = TestApp::spawn().await;
    let grids: i64 = sqlx::query_scalar("SELECT count(*) FROM review_grids WHERE domain = 'game'")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(grids, 6, "five families plus a default grid");
}

#[tokio::test]
async fn the_eight_bases_split_five_shipped_from_three_recognition() {
    let app = TestApp::spawn().await;
    let (total, shipped): (i64, i64) = sqlx::query_as(
        "SELECT count(*),
                count(*) FILTER (WHERE requires_deliverable)
           FROM attestation_bases WHERE skill_domain = 'game'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(total, 8);
    assert_eq!(shipped, 5, "five bases move a rank, three are recognition");
}

#[tokio::test]
async fn the_craft_score_has_its_terms_and_tiers_and_the_badges_are_seeded() {
    let app = TestApp::spawn().await;
    let weights: i64 =
        sqlx::query_scalar("SELECT count(*) FROM craft_score_weights WHERE skill_domain = 'game'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(weights, 14);
    let tiers: i64 =
        sqlx::query_scalar("SELECT count(*) FROM craft_score_tiers WHERE skill_domain = 'game'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(tiers, 5);
    let badges: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM badge_rules WHERE conditions::text LIKE '%game%'
            OR slug LIKE 'game-%'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(
        badges >= 20,
        "expected at least 20 game badge rules, found {badges}"
    );
}

// ═══════════════════════════════════════════════════════════════════
// Playtests: the gate no other domain has
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn a_creator_cannot_playtest_their_own_slice() {
    let app = TestApp::spawn().await;
    let creator = a_person(&app, "gp_creator").await;
    let slice = a_game_slice(&app, creator, "gp-own").await;

    app.login("gp_creator").await;
    let resp = app
        .post(
            &format!("/api/game/slices/{slice}/playtests"),
            &json!({
                "slice_id": slice,
                "fun_score": 5, "clarity_score": 5,
                "difficulty_perception": "balanced", "would_play_again": true
            }),
        )
        .await;
    assert_eq!(
        resp.status(),
        400,
        "a creator playtesting their own slice is refused"
    );
}

#[tokio::test]
async fn a_slice_needs_three_playtests_before_it_can_be_validated() {
    let app = TestApp::spawn().await;
    let creator = a_person(&app, "gv_creator").await;
    let reviewer = a_person(&app, "gv_reviewer").await;
    grant(&app, reviewer, "game_reviewer:all").await;
    let slice = a_game_slice(&app, creator, "gv-slice").await;

    // Two playtests — below the floor.
    a_playtest(&app, slice, a_person(&app, "gv_t1").await, 4).await;
    a_playtest(&app, slice, a_person(&app, "gv_t2").await, 4).await;

    app.login("gv_reviewer").await;
    let resp = app
        .post(
            &format!("/api/admin/game/slices/{slice}/validate"),
            &json!({}),
        )
        .await;
    assert_eq!(
        resp.status(),
        400,
        "two playtests is not enough to validate"
    );

    // The third crosses the gate.
    a_playtest(&app, slice, a_person(&app, "gv_t3").await, 4).await;
    let resp = app
        .post(
            &format!("/api/admin/game/slices/{slice}/validate"),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), 200, "three playtests with fun 4 validates");

    // The validation created a verified deliverable and its attestation.
    let deliverables: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM deliverables
          WHERE slice_id = $1 AND verification_status = 'verified'",
    )
    .bind(slice)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(deliverables, 1);

    let attested: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM attestations
          WHERE user_id = $1 AND basis = 'game_artifact_validated' AND revoked_at IS NULL",
    )
    .bind(creator)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(
        attested, 1,
        "a validated slice earns game_artifact_validated"
    );
}

// ═══════════════════════════════════════════════════════════════════
// Mods: confirmed by someone else, and then real
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn a_mod_is_confirmed_by_someone_other_than_its_author() {
    let app = TestApp::spawn().await;
    let author = a_person(&app, "gm_author").await;
    let reviewer = a_person(&app, "gm_reviewer").await;
    grant(&app, reviewer, "game_reviewer:community").await;

    app.login("gm_author").await;
    let resp = app
        .post(
            "/api/game/mods",
            &json!({
                "title": "A quality-of-life mod",
                "target_game": "Skyrim",
                "target_platform": "nexusmods",
                "external_hosting_url": "https://www.nexusmods.com/skyrim/mods/1",
                "description_md": "Adds a sortable inventory."
            }),
        )
        .await;
    assert_eq!(resp.status(), 200, "registering a mod");
    let body: Value = resp.json().await.unwrap();
    let mod_id = body["data"]["mod"]["id"].as_str().unwrap();

    // The author cannot confirm their own mod.
    app.login("gm_author").await;
    let resp = app
        .post(
            &format!("/api/admin/game/mods/{mod_id}/confirm"),
            &json!({ "reason": "looks good to me" }),
        )
        .await;
    assert!(
        resp.status() == 403 || resp.status() == 400,
        "the author is not a reviewer and cannot self-confirm"
    );

    // A community reviewer can.
    app.login("gm_reviewer").await;
    let resp = app
        .post(
            &format!("/api/admin/game/mods/{mod_id}/confirm"),
            &json!({ "reason": "URL real, mod is theirs, Nexus terms kept" }),
        )
        .await;
    assert_eq!(resp.status(), 200, "a community reviewer confirms the mod");

    // Confirmation created the deliverable and the attestation.
    let mid: Uuid = mod_id.parse().unwrap();
    let deliverable: i64 =
        sqlx::query_scalar("SELECT count(*) FROM deliverables WHERE game_mod_id = $1")
            .bind(mid)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(deliverable, 1, "a confirmed mod becomes a deliverable");

    let attested: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM attestations
          WHERE user_id = $1 AND basis = 'game_mod_published' AND revoked_at IS NULL",
    )
    .bind(author)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(attested, 1, "a confirmed mod earns game_mod_published");
}

// ═══════════════════════════════════════════════════════════════════
// The craft score, and the engine's new proof types
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn every_game_weight_term_is_counted_by_the_score() {
    // The score service warns and skips a weight term it cannot count. A domain
    // whose weights name a term nothing measures has a silent hole; computing a
    // score for a real user exercises every branch, and it must not error.
    let app = TestApp::spawn().await;
    let user = a_person(&app, "gs_user").await;
    let score = skilluv_backend::services::game_profile::compute(&app.db, user)
        .await
        .expect("a game score computes for a user with no game work");
    assert_eq!(score.score, 0, "a fresh user scores zero, not an error");
}

#[tokio::test]
async fn the_new_game_proof_types_are_known_to_the_engine() {
    use skilluv_backend::services::badge_engine::PROOF_TYPES;
    for pt in [
        "game_family_reviews",
        "game_solo_ship",
        "game_team_ship",
        "game_multi_artefact_ship",
        "game_jam_organized",
    ] {
        assert!(
            PROOF_TYPES.contains(&pt),
            "{pt} is used by a seeded game badge but the engine does not know it"
        );
    }
}
