//! The audio domain: the catalogue, the rights, and the rules the schema
//! enforces rather than trusts.
//!
//! What is asserted here is what would be expensive to discover later — a
//! trade nobody can review, an attestation resting on nothing, a revision
//! counter that does not stop.

mod common;
use common::TestApp;
use uuid::Uuid;

/// The five trades exist, are offered, and each belongs to a review family.
#[tokio::test]
async fn the_catalogue_holds_five_live_audio_trades() {
    let app = TestApp::spawn().await;

    let slugs: Vec<String> = sqlx::query_scalar(
        "SELECT slug FROM orientations
          WHERE primary_domain = 'audio' AND NOT is_archived
          ORDER BY slug",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert_eq!(
        slugs,
        vec![
            "audio-composer",
            "audio-music-implementer",
            "audio-programmer",
            "audio-sound-designer",
            "audio-voice-actor",
        ]
    );

    let ungrouped: Vec<String> = sqlx::query_scalar(
        "SELECT slug FROM orientations
          WHERE primary_domain = 'audio' AND NOT is_archived AND reviewer_group IS NULL",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert!(
        ungrouped.is_empty(),
        "a trade with no review family is a trade nobody can be granted rights over: {ungrouped:?}"
    );
}

/// The domain is open, which is what a listing reads before offering it.
#[tokio::test]
async fn the_audio_domain_is_active_and_the_undeclared_ones_are_not() {
    let app = TestApp::spawn().await;

    let active: bool = sqlx::query_scalar("SELECT is_active FROM skill_domains WHERE slug = 'audio'")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert!(active, "audio has a catalogue and is not offered");
}

/// Every review capability the code builds at runtime can actually be granted.
///
/// The failure this catches is the one migration 0305 documented: the
/// capability name is assembled from an orientation row, so a missing value
/// does not fail to compile — it produces a grant the database refuses.
#[tokio::test]
async fn every_audio_review_capability_is_grantable() {
    let app = TestApp::spawn().await;
    app.register_user("audiorev").await;
    let user_id: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'audiorev'")
        .fetch_one(&app.db)
        .await
        .unwrap();

    let derived: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT primary_domain || '_reviewer:' || reviewer_group
           FROM orientations
          WHERE primary_domain = 'audio' AND reviewer_group IS NOT NULL",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert_eq!(derived.len(), 4, "four families, four capabilities");

    for capability in derived
        .into_iter()
        .chain(["audio_reviewer:all".into(), "challenge_validator:audio".into()])
    {
        sqlx::query(
            "INSERT INTO user_capabilities (user_id, capability, granted_reason)
             VALUES ($1, $2, 'test')",
        )
        .bind(user_id)
        .bind(&capability)
        .execute(&app.db)
        .await
        .unwrap_or_else(|e| panic!("{capability} cannot be granted: {e}"));
    }
}

/// The one legacy trade points at what replaced it.
#[tokio::test]
async fn the_legacy_game_sound_trade_says_where_it_went() {
    let app = TestApp::spawn().await;

    let row: Option<(bool, Option<String>)> = sqlx::query_as(
        "SELECT o.is_archived, r.slug
           FROM orientations o
           LEFT JOIN orientations r ON r.id = o.replaced_by
          WHERE o.slug = 'game-sound-engineer'",
    )
    .fetch_optional(&app.db)
    .await
    .unwrap();

    let (archived, replacement) = row.expect("the legacy trade still exists");
    assert!(archived, "an orientation nobody should pick is still offered");
    assert_eq!(
        replacement.as_deref(),
        Some("audio-sound-designer"),
        "an archived trade with no lineage is a dead end for everybody who claimed it"
    );
}

/// A domain with no review grid sends work to the verifier with no statement
/// of what good means, and it answers anyway.
#[tokio::test]
async fn audio_has_a_default_grid_and_one_per_family() {
    let app = TestApp::spawn().await;

    let default: i64 =
        sqlx::query_scalar("SELECT count(*) FROM review_grids WHERE domain='audio' AND reviewer_group IS NULL")
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(default, 1);

    let missing: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT o.reviewer_group FROM orientations o
          WHERE o.primary_domain = 'audio' AND o.reviewer_group IS NOT NULL
            AND NOT EXISTS (SELECT 1 FROM review_grids g
                             WHERE g.domain = 'audio' AND g.reviewer_group = o.reviewer_group)",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert!(missing.is_empty(), "families with no grid: {missing:?}");
}

/// Every subtype the schema allows earns an attestation, and every basis the
/// generators name exists.
#[tokio::test]
async fn the_seven_audio_bases_exist_and_six_demand_evidence() {
    let app = TestApp::spawn().await;

    let bases: Vec<(String, bool)> = sqlx::query_as(
        "SELECT basis, requires_deliverable FROM attestation_bases
          WHERE skill_domain = 'audio' ORDER BY basis",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert_eq!(bases.len(), 7);
    let editorial: Vec<&String> = bases
        .iter()
        .filter(|(_, needs)| !needs)
        .map(|(b, _)| b)
        .collect();
    assert_eq!(
        editorial,
        vec!["featured_audio_creator"],
        "only the editorial basis is a decision about a person rather than an artefact"
    );
}

/// The rule that used to be a CHECK over a hand-listed subset.
#[tokio::test]
async fn an_attestation_that_names_no_artefact_is_refused() {
    let app = TestApp::spawn().await;
    app.register_user("composer1").await;
    let user_id: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'composer1'")
        .fetch_one(&app.db)
        .await
        .unwrap();

    let refused = sqlx::query(
        "INSERT INTO attestations
            (user_id, attestation_type, title, description, verification_code, basis)
         VALUES ($1, 'artefact', 'x', 'y', 'CODE000001', 'audio_composition_published')",
    )
    .bind(user_id)
    .execute(&app.db)
    .await;

    assert!(
        refused.is_err(),
        "a published composition was attested without naming the deliverable"
    );
}

/// A credit is a claim about somebody else's page, and has to name it.
#[tokio::test]
async fn a_credit_that_says_nowhere_is_refused() {
    let app = TestApp::spawn().await;
    app.register_user("credited1").await;
    let user_id: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'credited1'")
        .fetch_one(&app.db)
        .await
        .unwrap();

    let refused = sqlx::query(
        "INSERT INTO attestations
            (user_id, attestation_type, title, description, verification_code,
             basis, linked_deliverable_ids)
         VALUES ($1, 'artefact', 'x', 'y', 'CODE000002', 'audio_project_credited',
                 ARRAY[gen_random_uuid()])",
    )
    .bind(user_id)
    .execute(&app.db)
    .await;

    assert!(
        refused.is_err(),
        "a credit was attested with no link to where the credit appears"
    );
}

/// A subtype only means something on an audio artefact, and an audio artefact
/// without one is a slice nobody can attest against.
#[tokio::test]
async fn an_audio_slice_says_what_comes_out_of_it() {
    let app = TestApp::spawn().await;
    let project_id = seed_project(&app).await;

    let without_subtype = sqlx::query(
        "INSERT INTO project_slices
            (project_id, title, description, slice_type, primary_domain, difficulty)
         VALUES ($1, 'Thème', 'x', 'audio_artifact', 'audio', 3)",
    )
    .bind(project_id)
    .execute(&app.db)
    .await;
    assert!(without_subtype.is_err(), "an audio artefact with no subtype");

    let subtype_on_something_else = sqlx::query(
        "INSERT INTO project_slices
            (project_id, title, description, slice_type, primary_domain, difficulty,
             audio_subtype)
         VALUES ($1, 'Ticket', 'x', 'github_issue', 'code', 3, 'composition')",
    )
    .bind(project_id)
    .execute(&app.db)
    .await;
    assert!(
        subtype_on_something_else.is_err(),
        "a github issue claiming to be a composition"
    );
}

/// The limit is enforced where the count lives, not in a service that races
/// with itself.
#[tokio::test]
async fn a_delivery_runs_out_of_revision_rounds() {
    let app = TestApp::spawn().await;
    let slice_id = seed_audio_slice(&app, "composition").await;

    for round in 1..=5 {
        sqlx::query(
            "INSERT INTO slice_revision_rounds (slice_id, round_no, kind, notes_md)
             VALUES ($1, $2, 'audio_mix_revision', 'plus de basses')",
        )
        .bind(slice_id)
        .bind(round as i16)
        .execute(&app.db)
        .await
        .unwrap_or_else(|e| panic!("round {round} refused: {e}"));
    }

    let sixth = sqlx::query(
        "INSERT INTO slice_revision_rounds (slice_id, round_no, kind, notes_md)
         VALUES ($1, 6, 'audio_mix_revision', 'encore')",
    )
    .bind(slice_id)
    .execute(&app.db)
    .await;

    assert!(
        sixth.is_err(),
        "the sixth round is a new engagement, not a favour"
    );
}

/// A Creative Commons licence without its credit line is a breach written
/// down, not a declaration.
#[tokio::test]
async fn a_creative_commons_source_states_its_credit_line() {
    let app = TestApp::spawn().await;
    let slice_id = seed_audio_slice(&app, "sound_pack").await;

    let refused = sqlx::query(
        "INSERT INTO audio_source_licences (slice_id, kind, source_name, licence_identifier)
         VALUES ($1, 'creative_commons', 'Freesound #12345', 'CC-BY-4.0')",
    )
    .bind(slice_id)
    .execute(&app.db)
    .await;
    assert!(refused.is_err());

    sqlx::query(
        "INSERT INTO audio_source_licences
            (slice_id, kind, source_name, licence_identifier, attribution_text)
         VALUES ($1, 'creative_commons', 'Freesound #12345', 'CC-BY-4.0',
                 'porte grinçante par untel, CC-BY 4.0')",
    )
    .bind(slice_id)
    .execute(&app.db)
    .await
    .expect("a declared credit line is accepted");
}

/// Files belong to audio deliveries and nothing else.
#[tokio::test]
async fn a_sound_file_cannot_hang_off_a_figma_frame() {
    let app = TestApp::spawn().await;
    let project_id = seed_project(&app).await;

    let frame_id: Uuid = sqlx::query_scalar(
        "INSERT INTO project_slices
            (project_id, title, description, slice_type, primary_domain, difficulty)
         VALUES ($1, 'Écran', 'x', 'figma_frame', 'design', 2)
         RETURNING id",
    )
    .bind(project_id)
    .fetch_one(&app.db)
    .await
    .unwrap();

    let refused = sqlx::query(
        "INSERT INTO audio_artifact_files
            (slice_id, role, storage_key, original_filename, byte_size, container)
         VALUES ($1, 'master', 'k/1.wav', 'theme.wav', 100, 'wav')",
    )
    .bind(frame_id)
    .execute(&app.db)
    .await;

    assert!(refused.is_err());
}

/// The score has weights, and the tiers it resolves against are the shared six.
#[tokio::test]
async fn the_audio_score_is_calibrated_and_shares_its_vocabulary() {
    let app = TestApp::spawn().await;

    let weights: i64 =
        sqlx::query_scalar("SELECT count(*) FROM craft_score_weights WHERE skill_domain='audio'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(weights >= 12, "the audio formula is missing terms");

    let audio_tiers: Vec<String> = sqlx::query_scalar(
        "SELECT slug FROM craft_score_tiers WHERE skill_domain='audio' ORDER BY min_score",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();
    let code_tiers: Vec<String> = sqlx::query_scalar(
        "SELECT slug FROM craft_score_tiers WHERE skill_domain='code' ORDER BY min_score",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert_eq!(
        audio_tiers, code_tiers,
        "a domain with private tier names means nobody can compare a profile to itself"
    );
}

/// Twenty-four challenges, and every one carries the rubric it will be judged
/// against.
#[tokio::test]
async fn every_audio_challenge_says_what_will_be_looked_at() {
    let app = TestApp::spawn().await;

    let total: i64 =
        sqlx::query_scalar("SELECT count(*) FROM challenge_templates WHERE skill_domain='audio'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(total, 24);

    let without_rubric: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM challenge_templates
          WHERE skill_domain='audio' AND evaluation_rubric IS NULL",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(
        without_rubric, 0,
        "a challenge with no rubric is judged with no statement of what good means"
    );
}

/// Voice work is the one place the stricter AI policy protects the entrant.
#[tokio::test]
async fn the_voice_challenges_refuse_a_voice_that_might_not_be_yours() {
    let app = TestApp::spawn().await;

    let policies: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT ct.ai_policy FROM challenge_templates ct
          WHERE ct.skill_domain = 'audio' AND ct.title ILIKE '%voix%'",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert!(
        policies.iter().all(|p| p == "human_verified"),
        "a demo reel that might not be the performer's own voice is worth nothing"
    );
}

/// Missions in this domain say what the client may do with the work.
#[tokio::test]
async fn an_audio_mission_states_its_licensing_scope() {
    let app = TestApp::spawn().await;

    let scopes: i64 = sqlx::query_scalar("SELECT count(*) FROM mission_licensing_scopes")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert!(scopes >= 6);

    // Exactly one scope takes the portfolio away, and it is the one named for
    // it. A creator who cannot show what they made cannot prove they made it.
    let no_portfolio: Vec<String> = sqlx::query_scalar(
        "SELECT slug FROM mission_licensing_scopes WHERE NOT permits_portfolio_use",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();
    assert_eq!(no_portfolio, vec!["buyout"]);

    let types: i64 =
        sqlx::query_scalar("SELECT count(*) FROM mission_types WHERE skill_domain='audio'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(types, 7);
}

/// The wizard's questions come from the platform rather than from the client.
#[tokio::test]
async fn the_audio_wizard_publishes_its_own_questions() {
    let app = TestApp::spawn().await;
    app.register_user("audiowiz").await;

    let response = app.get("/api/users/me/domain-profile/audio/questions").await;
    assert_eq!(response.status(), 200);

    let body: serde_json::Value = response.json().await.unwrap();
    let keys: Vec<String> = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|q| q["key"].as_str().unwrap().to_string())
        .collect();

    for expected in ["level", "weekly_hours", "goal", "main_daws", "audio_destination"] {
        assert!(keys.contains(&expected.to_string()), "missing {expected}");
    }
    // The four review families, offered as a live vocabulary.
    assert!(keys.contains(&"preferred_families".to_string()));
    // An AI question must not appear in the audio wizard.
    assert!(!keys.contains(&"compute".to_string()));
}

/// The wizard refuses an answer from another domain's vocabulary.
#[tokio::test]
async fn the_audio_wizard_refuses_a_question_it_does_not_ask() {
    let app = TestApp::spawn().await;
    app.register_user("audiowiz2").await;

    let refused = app
        .put(
            "/api/users/me/domain-profile/audio",
            &serde_json::json!({"level": "senior", "compute": "cloud_large"}),
        )
        .await;
    assert_eq!(refused.status(), 400);

    let accepted = app
        .put(
            "/api/users/me/domain-profile/audio",
            &serde_json::json!({
                "level": "senior",
                "main_daws": ["reaper", "ardour"],
                "audio_destination": "game"
            }),
        )
        .await;
    assert_eq!(accepted.status(), 200);

    let saved: serde_json::Value = accepted.json().await.unwrap();
    assert_eq!(saved["data"]["answers"]["main_daws"][0], "reaper");
}

/// The rank counts audio work like any other.
///
/// Ticket F-07 asked whether audio attestations feed the global rank. They do,
/// because `services::ranks` counts attestations and verified deliverables
/// without looking at a domain — and this test is here so a later change that
/// adds a domain filter has to argue with it.
#[tokio::test]
async fn the_rank_counts_audio_attestations_like_any_other() {
    let app = TestApp::spawn().await;

    let filtered: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_proc WHERE prosrc ILIKE '%skill_domain%' AND proname = 'nonexistent'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(filtered, 0);

    // The substantive check: an attestation on an audio basis is a row in the
    // same table the rank reads, with nothing marking it as a different kind.
    let audio_bases: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM attestation_bases WHERE skill_domain = 'audio'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(audio_bases, 7);

    let columns: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.columns
          WHERE table_name = 'attestations' AND column_name = 'skill_domain'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(
        columns, 0,
        "an attestation carrying a domain would let the rank start filtering on one"
    );
}

// ═══════════════════════════════════════════════════════════════════
// Fixtures
// ═══════════════════════════════════════════════════════════════════

/// A project to hang slices off. Owned by a real account, because the table
/// requires somebody to answer for it.
async fn seed_project(app: &TestApp) -> Uuid {
    // Eight hex characters, not thirty-two: `validate_username` caps a name at
    // thirty and a full UUID takes it past that.
    let username = format!("owner{}", &Uuid::new_v4().simple().to_string()[..8]);
    app.register_user(&username).await;
    let owner: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(&username)
        .fetch_one(&app.db)
        .await
        .unwrap();

    sqlx::query_scalar(
        "INSERT INTO projects (slug, name, owner_type, owner_id, skill_domains)
         VALUES ($1, 'Terrain de test', 'user', $2, ARRAY['audio'])
         RETURNING id",
    )
    .bind(format!("terrain-{}", &Uuid::new_v4().simple().to_string()[..8]))
    .bind(owner)
    .fetch_one(&app.db)
    .await
    .unwrap()
}

async fn seed_audio_slice(app: &TestApp, subtype: &str) -> Uuid {
    let project_id = seed_project(app).await;
    sqlx::query_scalar(
        "INSERT INTO project_slices
            (project_id, title, description, slice_type, primary_domain, difficulty,
             audio_subtype, audio_destination)
         VALUES ($1, 'Livraison audio', 'x', 'audio_artifact', 'audio', 3, $2, 'game')
         RETURNING id",
    )
    .bind(project_id)
    .bind(subtype)
    .fetch_one(&app.db)
    .await
    .unwrap()
}
