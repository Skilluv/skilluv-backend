//! The design catalogue: twenty-six trades, their vocabulary, who reviews
//! them, and against what.
//!
//! What these guard is the thing a seed migration silently gets wrong: a
//! trade with no skills cannot be searched, a trade with no reviewer group
//! cannot be reviewed by anybody, and a grid nobody wrote means a reviewer
//! opens a critique with no statement of what good is.

mod common;
use common::TestApp;
use uuid::Uuid;

// ═══════════════════════════════════════════════════════════════════
// The trades
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn twenty_six_design_trades_are_live_and_curated() {
    let app = TestApp::spawn().await;

    let live: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM orientations
          WHERE primary_domain = 'design' AND is_archived = FALSE AND is_curated = TRUE",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(live, 26);
}

#[tokio::test]
async fn the_old_five_forward_instead_of_dying() {
    let app = TestApp::spawn().await;

    // Archived, not deleted: `user_orientations` references them by id, and a
    // profile that carries one still means something.
    for (old, new) in [
        ("web-designer", "design-web"),
        ("mobile-designer", "design-mobile"),
        ("motion-designer", "design-motion-ui"),
        ("illustrator", "design-illustration"),
        ("3d-artist", "design-game-environment"),
    ] {
        let row: Option<(bool, Option<String>)> = sqlx::query_as(
            "SELECT o.is_archived, r.slug
               FROM orientations o
               LEFT JOIN orientations r ON r.id = o.replaced_by
              WHERE o.slug = $1",
        )
        .bind(old)
        .fetch_optional(&app.db)
        .await
        .unwrap();

        let (archived, replaced_by) = row.unwrap_or_else(|| panic!("{old} disappeared"));
        assert!(archived, "{old} must be archived, not live");
        assert_eq!(
            replaced_by.as_deref(),
            Some(new),
            "{old} must forward to {new}, or a link to it leads nowhere"
        );
    }
}

#[tokio::test]
async fn every_trade_reads_in_french_and_in_english() {
    let app = TestApp::spawn().await;

    let untranslated: Vec<String> = sqlx::query_scalar(
        "SELECT o.slug FROM orientations o
          WHERE o.primary_domain = 'design' AND o.is_archived = FALSE
            AND NOT EXISTS (
                SELECT 1 FROM orientation_translations t
                 WHERE t.orientation_id = o.id AND t.locale = 'en')",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();
    assert!(
        untranslated.is_empty(),
        "trades with no English text: {untranslated:?}"
    );
}

// ═══════════════════════════════════════════════════════════════════
// The vocabulary
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn the_crafts_that_had_no_words_have_them_now() {
    let app = TestApp::spawn().await;

    // One skill per craft that had literally nothing before this migration.
    for slug in [
        "blender-motion",
        "letterform-construction",
        "ui-sound-design",
        "service-blueprinting",
        "chart-type-selection",
        "icon-grid-keyline",
        "xr-comfort-constraints",
        "archviz-lighting-daylight",
        "microcopy-writing",
        "character-silhouette",
    ] {
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM skill_nodes WHERE slug = $1)")
                .bind(slug)
                .fetch_one(&app.db)
                .await
                .unwrap();
        assert!(exists, "{slug} missing from the skill graph");
    }
}

#[tokio::test]
async fn every_trade_has_skills_and_at_least_one_core_skill() {
    let app = TestApp::spawn().await;

    let no_skills: Vec<String> = sqlx::query_scalar(
        "SELECT o.slug FROM orientations o
          WHERE o.primary_domain = 'design' AND o.is_archived = FALSE
            AND NOT EXISTS (
                SELECT 1 FROM orientation_skill_map m WHERE m.orientation_id = o.id)",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();
    assert!(
        no_skills.is_empty(),
        "a trade with no skills cannot be matched by search: {no_skills:?}"
    );

    let no_core: Vec<String> = sqlx::query_scalar(
        "SELECT o.slug FROM orientations o
          WHERE o.primary_domain = 'design' AND o.is_archived = FALSE
            AND NOT EXISTS (
                SELECT 1 FROM orientation_skill_map m
                 WHERE m.orientation_id = o.id AND m.is_core)",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();
    assert!(
        no_core.is_empty(),
        "without a core skill, learning can never be promoted to active: {no_core:?}"
    );
}

#[tokio::test]
async fn the_critique_vocabulary_is_on_every_trade() {
    let app = TestApp::spawn().await;

    // Reading a critique, answering it and telling the story of the rounds
    // are part of every design trade here, not of a "soft skills" one nobody
    // would pick.
    let missing: Vec<String> = sqlx::query_scalar(
        "SELECT o.slug FROM orientations o
          WHERE o.primary_domain = 'design' AND o.is_archived = FALSE
            AND NOT EXISTS (
                SELECT 1 FROM orientation_skill_map m
                  JOIN skill_nodes n ON n.id = m.skill_id
                 WHERE m.orientation_id = o.id AND n.slug = 'structured-critique')",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();
    assert!(missing.is_empty(), "trades without critique: {missing:?}");
}

// ═══════════════════════════════════════════════════════════════════
// Who reviews, and against what
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn every_trade_belongs_to_a_reviewer_group() {
    let app = TestApp::spawn().await;

    let ungrouped: Vec<String> = sqlx::query_scalar(
        "SELECT slug FROM orientations
          WHERE primary_domain = 'design' AND is_archived = FALSE
            AND reviewer_group IS NULL",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();
    assert!(
        ungrouped.is_empty(),
        "nobody can be granted review rights for: {ungrouped:?}"
    );

    let groups: i64 = sqlx::query_scalar(
        "SELECT count(DISTINCT reviewer_group) FROM orientations
          WHERE primary_domain = 'design' AND is_archived = FALSE",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(groups, 13);
}

#[tokio::test]
async fn every_group_is_a_grantable_capability() {
    let app = TestApp::spawn().await;
    app.register_user("grid_reader").await;
    let user: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'grid_reader'")
        .fetch_one(&app.db)
        .await
        .unwrap();

    // If the CHECK constraint and the catalogue disagree, one of them is
    // wrong, and this is the only place that would notice.
    let groups: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT reviewer_group FROM orientations
          WHERE primary_domain = 'design' AND reviewer_group IS NOT NULL",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    for group in &groups {
        sqlx::query(
            "INSERT INTO user_capabilities (user_id, capability, granted_reason)
             VALUES ($1, $2, 'test')",
        )
        .bind(user)
        .bind(format!("design_reviewer:{group}"))
        .execute(&app.db)
        .await
        .unwrap_or_else(|e| panic!("design_reviewer:{group} is not grantable: {e}"));
    }
}

#[tokio::test]
async fn every_group_has_a_review_grid_and_the_domain_has_a_default() {
    let app = TestApp::spawn().await;

    let without_grid: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT o.reviewer_group FROM orientations o
          WHERE o.primary_domain = 'design'
            AND o.reviewer_group IS NOT NULL
            AND NOT EXISTS (
                SELECT 1 FROM review_grids g
                 WHERE g.domain = 'design' AND g.reviewer_group = o.reviewer_group)",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();
    assert!(
        without_grid.is_empty(),
        "reviewers open these with nothing to judge against: {without_grid:?}"
    );

    let has_default: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM review_grids
                         WHERE domain = 'design' AND reviewer_group IS NULL)",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(
        has_default,
        "a design challenge outside every group would be judged against nothing"
    );
}

#[tokio::test]
async fn accessibility_is_asked_of_everyone_not_of_a_speciality() {
    let app = TestApp::spawn().await;

    // In the domain default rather than in the product grid: an unreadable
    // chart, an animation that triggers vertigo and an inaudible caption are
    // the same failure, and scoping it to one family excuses the rest.
    let criteria: serde_json::Value = sqlx::query_scalar(
        "SELECT criteria FROM review_grids WHERE domain = 'design' AND reviewer_group IS NULL",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    let names: Vec<String> = criteria
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["criterion"].as_str().unwrap_or_default().to_lowercase())
        .collect();
    assert!(
        names.iter().any(|n| n.contains("accessibilité")),
        "the common grid must ask about accessibility, got {names:?}"
    );
}

// ═══════════════════════════════════════════════════════════════════
// Badges and attestations
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn the_design_badges_are_seeded_and_none_of_them_is_manual() {
    let app = TestApp::spawn().await;

    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM badge_rules
          WHERE slug LIKE 'design-%' AND deprecated_at IS NULL",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(count, 14);

    let manual: Vec<String> = sqlx::query_scalar(
        "SELECT slug FROM badge_rules
          WHERE slug LIKE 'design-%' AND (conditions ->> 'manual')::boolean IS TRUE",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();
    assert!(
        manual.is_empty(),
        "every design badge reads a row that already exists: {manual:?}"
    );
}

#[tokio::test]
async fn design_badge_rules_name_only_things_the_engine_can_read() {
    let app = TestApp::spawn().await;

    // A rule referencing a proof type nothing implements is a badge nobody
    // can ever earn, and no test would otherwise notice.
    //
    // Read from the engine rather than copied. The copy went stale in the
    // direction nobody expects — the engine grew `design_briefs_published`
    // and this list did not, so a correct rule was reported as naming
    // something unimplemented.
    let known = skilluv_backend::services::badge_engine::PROOF_TYPES;

    let rules: Vec<(String, serde_json::Value)> =
        sqlx::query_as("SELECT slug, conditions FROM badge_rules WHERE slug LIKE 'design-%'")
            .fetch_all(&app.db)
            .await
            .unwrap();

    for (slug, conditions) in rules {
        let types = conditions["proof_types"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        for t in types {
            let name = t.as_str().unwrap_or_default();
            assert!(
                known.contains(&name),
                "{slug} asks for proof type '{name}', which the engine does not implement"
            );
        }
    }
}

#[tokio::test]
async fn the_design_attestation_bases_are_accepted_and_need_evidence() {
    let app = TestApp::spawn().await;
    app.register_user("basis_holder").await;
    let user: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'basis_holder'")
        .fetch_one(&app.db)
        .await
        .unwrap();

    // An artefact basis with nothing to open is a label, not a claim.
    let orphan = sqlx::query(
        "INSERT INTO attestations
            (user_id, attestation_type, title, description, basis, verification_code)
         VALUES ($1, 'artefact', 't', 'd', 'design_contest_won', 'DESIGNAAA1')",
    )
    .bind(user)
    .execute(&app.db)
    .await;
    assert!(
        orphan.is_err(),
        "a contest win must name the deliverable it rests on"
    );

    // The editorial one is the exception, and only it.
    sqlx::query(
        "INSERT INTO attestations
            (user_id, attestation_type, title, description, basis, verification_code)
         VALUES ($1, 'artefact', 'Designer de la semaine', 'd', 'featured_designer', 'DESIGNAAA2')",
    )
    .bind(user)
    .execute(&app.db)
    .await
    .expect("an editorial distinction rests on a decision, not on an artefact");
}

// ═══════════════════════════════════════════════════════════════════
// The catalogue of things to actually do
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn every_trade_has_challenges_waiting_for_it() {
    let app = TestApp::spawn().await;

    // Twenty-six trades with an empty catalogue are twenty-six trades the
    // platform claims to support and cannot. A designer who arrives on a
    // motion 3D profile and finds nothing to do leaves.
    let seeded: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM challenge_templates
          WHERE skill_domain = 'design' AND is_training = TRUE",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(seeded >= 130, "expected 130 design drafts, found {seeded}");
}

#[tokio::test]
async fn seeded_challenges_are_drafts_and_carry_their_grid() {
    let app = TestApp::spawn().await;

    // Drafts, because the full brief needs an author who knows the trade and
    // an unreviewed challenge must not reach somebody learning.
    //
    // Scoped past the onboarding challenge, which is published on purpose:
    // "Premier pas" exists to be somebody's first, and it was reviewed. The
    // catalogue seeded here is what must stay in draft.
    let published_seeds: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM challenge_templates
          WHERE skill_domain = 'design' AND is_training = TRUE
            AND is_onboarding = FALSE AND status <> 'draft'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(published_seeds, 0);

    // And each carries a rubric, so verification never runs with no
    // statement of what good means.
    let without_rubric: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM challenge_templates
          WHERE skill_domain = 'design' AND is_training = TRUE
            AND is_onboarding = FALSE
            AND (evaluation_rubric IS NULL
                 OR jsonb_array_length(evaluation_rubric) = 0)",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(without_rubric, 0);
}

// ═══════════════════════════════════════════════════════════════════
// The craft score
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn the_design_score_has_weights_and_a_ladder() {
    let app = TestApp::spawn().await;

    let weights: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM craft_score_weights
          WHERE skill_domain = 'design' AND is_active = TRUE",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(weights >= 12, "found {weights} design weights");

    // A ladder with a hole in it means a score that resolves to no tier, and
    // the service treats that as an internal error rather than guessing.
    // The ladder itself is 0204's, shared by every domain on purpose: a tier
    // is a position on a scale, and a vocabulary per domain would stop
    // anybody comparing a profile to itself across two of them. What design
    // contributes is the weights above.
    let tiers: Vec<(i32, Option<i32>)> = sqlx::query_as(
        "SELECT min_score, max_score FROM craft_score_tiers
          WHERE skill_domain = 'design' ORDER BY min_score",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();
    assert!(!tiers.is_empty());
    assert_eq!(tiers[0].0, 0, "the ladder must start at zero");
    for pair in tiers.windows(2) {
        let (_, upper) = pair[0];
        let (next_min, _) = pair[1];
        assert_eq!(
            upper.map(|u| u + 1),
            Some(next_min),
            "a gap between tiers leaves scores with no name"
        );
    }
    assert!(
        tiers.last().unwrap().1.is_none(),
        "the top tier has no ceiling other than the cap"
    );
}

#[tokio::test]
async fn a_fresh_designer_scores_zero_and_still_gets_a_tier() {
    let app = TestApp::spawn().await;
    app.register_user("fresh_designer").await;
    let user: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'fresh_designer'")
        .fetch_one(&app.db)
        .await
        .unwrap();

    let score = skilluv_backend::services::design_craft_score::compute(&app.db, user)
        .await
        .expect("a profile with no work must still resolve to the first tier");
    assert_eq!(score.score, 0);
    assert_eq!(score.tier_slug, "apprentice");
    assert!(score.breakdown.is_empty());
    assert!(!score.capped);
}

#[tokio::test]
async fn the_design_score_ignores_imported_reputation() {
    let app = TestApp::spawn().await;
    app.register_user("importer").await;
    let user: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'importer'")
        .fetch_one(&app.db)
        .await
        .unwrap();

    let before = skilluv_backend::services::design_craft_score::compute(&app.db, user)
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO external_signals (user_id, provider, url, title)
         VALUES ($1, 'medium', 'https://example.test/portfolio', 'Portfolio')",
    )
    .bind(user)
    .execute(&app.db)
    .await
    .unwrap();

    let after = skilluv_backend::services::design_craft_score::compute(&app.db, user)
        .await
        .unwrap();
    assert_eq!(
        before.score, after.score,
        "a score an import can move stops meaning proven here"
    );
}
