//! Contest formats, now that they are rows.
//!
//! Migration 0228 exists because 0223 restated `tournaments_kind_check` and
//! deleted two of 0189's values; nothing failed until somebody tried to create
//! a code golf, and the error named a constraint rather than the thing that
//! was wrong. Migration 0416 replaced the CHECK with a table, and these tests
//! hold what the table now has to keep true.

mod common;
use common::TestApp;
use uuid::Uuid;

use skilluv_backend::services::contest;

/// Nothing that existed has gone missing.
///
/// The list is spelled out rather than counted: a count passes when one format
/// is dropped and another added in the same migration, which is exactly the
/// shape of the accident being guarded against.
#[tokio::test]
async fn every_format_that_ever_existed_still_does() {
    let app = TestApp::spawn().await;

    for slug in [
        // Migration 0030
        "individual",
        "guild_war",
        "hackathon",
        // Migration 0114
        "marathon",
        "defi_solitaire",
        // Migration 0189 — the two that 0223 deleted and 0228 restored
        "code_golf",
        "tdd_contest",
        // Migration 0223
        "benchmark_rush",
        "prompt_battle",
        // Migration 0416
        "audio_sound_battle",
        "audio_composition_contest",
    ] {
        contest::load_kind(&app.db, slug)
            .await
            .unwrap_or_else(|e| panic!("{slug} is no longer a contest format: {e}"));
    }
}

/// An unknown format is refused with the list, not with a constraint name.
#[tokio::test]
async fn an_unknown_format_is_answered_with_the_ones_that_exist() {
    let app = TestApp::spawn().await;

    let err = contest::load_kind(&app.db, "karaoke")
        .await
        .expect_err("karaoke is not a contest format");
    let message = err.to_string();

    assert!(
        message.contains("code_golf") && message.contains("audio_sound_battle"),
        "the refusal has to name what is allowed, and it said: {message}"
    );
}

/// Golf is the only thing won at the bottom of the scale.
///
/// Ranked ascending anywhere else and the shortest, worst entry wins; ranked
/// descending in golf and the longest one does.
#[tokio::test]
async fn only_code_golf_is_won_at_the_bottom() {
    let app = TestApp::spawn().await;

    let inverted: Vec<String> =
        sqlx::query_scalar("SELECT slug FROM tournament_kinds WHERE lower_is_better ORDER BY slug")
            .fetch_all(&app.db)
            .await
            .unwrap();

    assert_eq!(inverted, vec!["code_golf"]);
}

/// A measured format is one where the entrant supplies the number; a judged one
/// is not. Getting this backwards asks a composer for a character count.
#[tokio::test]
async fn the_audio_formats_are_judged_rather_than_measured() {
    let app = TestApp::spawn().await;

    for slug in ["audio_sound_battle", "audio_composition_contest"] {
        let spec = contest::load_kind(&app.db, slug).await.unwrap();
        assert!(spec.expects_submission, "{slug} takes entries");
        assert!(
            !spec.is_measured,
            "{slug} would rank sound by a number, and there is no honest one"
        );
    }
}

/// What a format asks for before anybody can enter it.
#[tokio::test]
async fn a_format_states_what_its_rules_must_carry() {
    let app = TestApp::spawn().await;

    let golf = contest::load_kind(&app.db, "code_golf").await.unwrap();
    assert!(contest::validate_rules(&golf, &serde_json::json!({"language": "rust"})).is_err());
    assert!(
        contest::validate_rules(
            &golf,
            &serde_json::json!({"language": "rust", "problem_url": "https://x.test/p"})
        )
        .is_ok()
    );

    let battle = contest::load_kind(&app.db, "audio_sound_battle")
        .await
        .unwrap();
    assert!(contest::validate_rules(&battle, &serde_json::json!({"brief": "une porte"})).is_err());
    assert!(
        contest::validate_rules(
            &battle,
            &serde_json::json!({"brief": "une porte", "duration_hours": 48, "entrants": 2})
        )
        .is_ok()
    );
}

/// Every format the table holds belongs to a domain the platform knows, or to
/// none at all.
#[tokio::test]
async fn no_format_belongs_to_a_domain_that_does_not_exist() {
    let app = TestApp::spawn().await;

    let orphans: Vec<String> = sqlx::query_scalar(
        "SELECT k.slug FROM tournament_kinds k
          LEFT JOIN skill_domains d ON d.slug = k.skill_domain
         WHERE k.skill_domain IS NOT NULL AND d.slug IS NULL",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert!(orphans.is_empty(), "formats in no domain: {orphans:?}");
}

/// The concrete case the 0223 regression broke, through the service rather
/// than straight SQL — so the scoring direction is exercised too.
///
/// Inherited from `test_tournament_kinds_agree`, which also held two tests
/// asserting that the Rust list and the CHECK constraint agreed. Migration
/// 0416 removed the second list, so those two now assert something that
/// cannot be false, and they are gone rather than kept as decoration.
#[tokio::test]
async fn a_code_golf_can_actually_be_created() {
    let app = TestApp::spawn().await;

    // The concrete case the regression broke, through the service rather than
    // straight SQL — so the scoring direction is exercised too.
    app.register_user("kinds_admin").await;
    let admin: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'kinds_admin'")
        .fetch_one(&app.db)
        .await
        .unwrap();

    let created = skilluv_backend::services::tournament::create_tournament(
        &app.db,
        admin,
        skilluv_backend::services::tournament::CreateTournamentInput {
            season_id: None,
            slug: "golf-after-the-merge".into(),
            name: "Code golf".into(),
            description: None,
            kind: "code_golf".into(),
            format: Some("ladder".into()),
            prize_pool_fragments: None,
            prize_pool_gp: None,
            sponsor_enterprise_id: None,
            sponsor_logo_url: None,
            sponsor_blurb: None,
            registration_opens_at: None,
            starts_at: chrono::Utc::now(),
            ends_at: chrono::Utc::now() + chrono::Duration::days(7),
            skill_domain: Some("code".into()),
            rules: Some(serde_json::json!({
                "language": "python",
                "problem_url": "https://example.test/problem",
            })),
        },
    )
    .await;

    let tournament = created.expect("a code golf must be creatable after every migration");
    assert_eq!(tournament.scoring_direction, "lower_is_better");
}
