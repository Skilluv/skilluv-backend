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
    const KNOWN: &[&str] = &[
        "deliverable_verified",
        "attestation_received",
        "onboarding_bonjour_completed",
        "slice_merged_upstream",
        "deliverable_featured",
        "tournament_podium",
        "tournament_judged",
        "mentorship_mentees_led",
    ];

    let rules: Vec<(String, serde_json::Value)> = sqlx::query_as(
        "SELECT slug, conditions FROM badge_rules WHERE slug LIKE 'design-%'",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    for (slug, conditions) in rules {
        let types = conditions["proof_types"].as_array().cloned().unwrap_or_default();
        for t in types {
            let name = t.as_str().unwrap_or_default();
            assert!(
                KNOWN.contains(&name),
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
