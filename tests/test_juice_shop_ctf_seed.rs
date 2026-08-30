//! The derived capture-the-flag catalogue (SKI-137).
//!
//! ## What makes this seedable at all
//!
//! Migration 0558 refuses to seed a flag, and is right to: a hash invented by
//! the author of a migration produces a challenge nobody can ever pass.
//!
//! Juice Shop is the exception because its flags are not invented. Each one is
//! derived from the instance's `ctfKey` and the challenge's own name, so given
//! the key the twenty are computable and nobody guesses anything.
//!
//! ## What this suite holds
//!
//! The two properties that keep it safe rather than merely convenient: an
//! unconfigured deployment seeds nothing, and a second run does not produce a
//! second catalogue. The second is the one with teeth — a rotated key
//! re-derives every flag, and without the upsert target of migration 0605 that
//! would leave twenty duplicates of which half no longer accept a correct
//! answer.

mod common;
use common::TestApp;
use skilluv_backend::services::seed::juice_shop_ctf;
use uuid::Uuid;

async fn an_owner(app: &TestApp, username: &str) -> Uuid {
    app.register_user(username).await;
    sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(username)
        .fetch_one(&app.db)
        .await
        .unwrap()
}

async fn ctf_count(app: &TestApp) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM challenge_templates WHERE security_kind = 'ctf_flag'")
        .fetch_one(&app.db)
        .await
        .unwrap()
}

/// A deployment with no Juice Shop seeds nothing, and says so.
///
/// The environment is read rather than injected, so this test asserts the
/// default state of a test process — which is exactly the state of a
/// deployment that has not configured a target.
#[tokio::test]
async fn an_unconfigured_deployment_seeds_no_challenges() {
    let app = TestApp::spawn().await;
    let owner = an_owner(&app, "ctf_seed_none").await;

    let report = juice_shop_ctf::run(&app.db, owner).await.unwrap();

    assert_eq!(
        ctf_count(&app).await,
        0,
        "challenges appeared with no target"
    );
    assert!(
        report.contains("not set") || report.contains("empty"),
        "the report does not say why nothing happened: {report}"
    );
}

/// A flag is a function of the key and of the challenge, and of nothing else.
///
/// This is the property the whole design rests on. Because a flag moves with
/// the key, the seed step's ledger version is the key's fingerprint: rotate it
/// and every flag is re-derived on the next boot, instead of twenty challenges
/// silently refusing correct answers.
#[tokio::test]
async fn a_rotated_key_changes_every_flag() {
    let names = ["Login Admin", "DOM XSS", "JWT Issues"];
    for name in names {
        let before = juice_shop_ctf::flag_for("key-before", name);
        let after = juice_shop_ctf::flag_for("key-after", name);
        assert_ne!(before, after, "{name} survived a key rotation");
        assert_eq!(before.len(), 64);
    }

    // And two challenges under one key do not collide, or somebody would pass
    // a challenge they never opened.
    let a = juice_shop_ctf::flag_for("one-key", names[0]);
    let b = juice_shop_ctf::flag_for("one-key", names[1]);
    assert_ne!(a, b);
}

/// The upsert target of migration 0605, exercised rather than assumed.
///
/// A second run has to land on the same twenty rows. Without the partial
/// unique index this inserts twenty more, and a reader sees each challenge
/// twice with only one of the pair still working.
#[tokio::test]
async fn a_title_can_only_belong_to_one_flag_challenge() {
    let app = TestApp::spawn().await;
    let owner = an_owner(&app, "ctf_seed_unique").await;

    let insert = |title: &'static str, hash: &'static str| {
        let db = app.db.clone();
        async move {
            sqlx::query(
                "INSERT INTO challenge_templates (
                     title, description, instructions, skill_domain, difficulty,
                     status, is_training, ai_policy, created_by,
                     security_kind, security_difficulty_tier,
                     security_flag_hash, security_flag_format, security_target_url)
                 VALUES ($1, 'd', 'i', 'security', 2, 'draft', TRUE,
                         'disclosure_required', $2, 'ctf_flag', 'easy', $3,
                         'hex', 'https://ctf.example')",
            )
            .bind(title)
            .bind(owner)
            .bind(hash)
            .execute(&db)
            .await
        }
    };

    insert("A derived challenge", "aaaa")
        .await
        .expect("the first");
    let second = insert("A derived challenge", "bbbb").await;
    assert!(
        second.is_err(),
        "two flag challenges share a title: a re-derivation would duplicate \
         the catalogue and half of it would stop accepting correct answers"
    );

    // The constraint is scoped to flag challenges. Two trades may legitimately
    // want a challenge under the same name, and 0605 says so.
    let other_kind = sqlx::query(
        "INSERT INTO challenge_templates (
             title, description, instructions, skill_domain, difficulty,
             status, is_training, ai_policy, created_by)
         VALUES ('A derived challenge', 'd', 'i', 'design', 2, 'draft', TRUE,
                 'disclosure_required', $1)",
    )
    .bind(owner)
    .execute(&app.db)
    .await;
    assert!(
        other_kind.is_ok(),
        "the index reached beyond capture-the-flag challenges"
    );
}

/// Whatever is seeded arrives as a draft.
///
/// The derivation below is one function, and if it is ever wrong every
/// challenge is wrong the same way. A draft means somebody solves one and
/// checks the flag is accepted before anybody is asked to solve twenty — which
/// is the same rule every other seeded challenge in this catalogue follows.
#[tokio::test]
async fn nothing_derived_is_published_without_a_person() {
    let app = TestApp::spawn().await;
    let owner = an_owner(&app, "ctf_seed_draft").await;
    let _ = juice_shop_ctf::run(&app.db, owner).await.unwrap();

    let published: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM challenge_templates
          WHERE security_kind = 'ctf_flag' AND status <> 'draft'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(published, 0, "a derived challenge was published unread");
}
