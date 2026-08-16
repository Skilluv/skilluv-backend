//! Code badges: what the engine can decide, and what it must not pretend to.

mod common;
use common::TestApp;
use uuid::Uuid;

/// A user, a code challenge in a given language, and `n` verified
/// deliverables against it.
async fn user_with_deliverables(app: &TestApp, username: &str, language: &str, n: usize) -> Uuid {
    app.register_user(username).await;
    let user: Uuid = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT id FROM users WHERE username = '{username}'"
    )))
    .fetch_one(&app.db)
    .await
    .unwrap();

    let challenge: Uuid = sqlx::query_scalar(
        "INSERT INTO challenge_templates
            (title, description, instructions, skill_domain, difficulty, language, status, is_training)
         VALUES ($1, 'x', 'x', 'code', 2, $2, 'published', TRUE)
         RETURNING id",
    )
    .bind(format!("chal {username} {language}"))
    .bind(language)
    .fetch_one(&app.db)
    .await
    .expect("challenge");

    for _ in 0..n {
        sqlx::query(
            "INSERT INTO deliverables
                (user_id, challenge_id, artifact_type, artifact_url, verifiable_by, verification_status, verified_at)
             VALUES ($1, $2, 'pr_merged', 'https://example.test/pr', 'github_webhook', 'verified', NOW())",
        )
        .bind(user)
        .bind(challenge)
        .execute(&app.db)
        .await
        .expect("deliverable");
    }

    user
}

async fn holds(app: &TestApp, user: Uuid, slug: &str) -> bool {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (
             SELECT 1 FROM user_badges ub
             JOIN badge_rules r ON r.id = ub.rule_id
             WHERE ub.user_id = $1 AND r.slug = $2 AND ub.revoked_at IS NULL)",
    )
    .bind(user)
    .bind(slug)
    .fetch_one(&app.db)
    .await
    .unwrap()
}

#[tokio::test]
async fn a_first_verified_artifact_is_recognised() {
    let app = TestApp::spawn().await;
    let user = user_with_deliverables(&app, "badge_first", "rust", 1).await;

    skilluv_backend::services::badge_engine::recompute_badges_for_user(&app.db, user)
        .await
        .expect("recompute");

    assert!(holds(&app, user, "code-first-artifact").await);
    assert!(!holds(&app, user, "code-craft-master").await);
}

#[tokio::test]
async fn a_threshold_above_twenty_five_can_actually_be_reached() {
    let app = TestApp::spawn().await;
    // The count used to share its query with the sample of source proofs,
    // which was capped at twenty-five. Every rule above that threshold was
    // unreachable: the condition was met and nothing ever fired.
    let user = user_with_deliverables(&app, "badge_thirty", "rust", 30).await;

    skilluv_backend::services::badge_engine::recompute_badges_for_user(&app.db, user)
        .await
        .expect("recompute");

    assert!(
        holds(&app, user, "code-craft-master").await,
        "thirty verified deliverables must reach a rule asking for thirty"
    );
}

#[tokio::test]
async fn counting_languages_is_not_counting_deliverables() {
    let app = TestApp::spawn().await;

    // Ten deliverables in one language is not three languages.
    let narrow = user_with_deliverables(&app, "badge_onelang", "rust", 10).await;
    skilluv_backend::services::badge_engine::recompute_badges_for_user(&app.db, narrow)
        .await
        .unwrap();
    assert!(!holds(&app, narrow, "code-multi-language").await);

    // Three, one each, is.
    let broad = user_with_deliverables(&app, "badge_polyglot", "rust", 1).await;
    for language in ["python", "typescript"] {
        let challenge: Uuid = sqlx::query_scalar(
            "INSERT INTO challenge_templates
                (title, description, instructions, skill_domain, difficulty, language, status, is_training)
             VALUES ($1, 'x', 'x', 'code', 2, $2, 'published', TRUE)
             RETURNING id",
        )
        .bind(format!("poly {language}"))
        .bind(language)
        .fetch_one(&app.db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO deliverables
                (user_id, challenge_id, artifact_type, artifact_url, verifiable_by, verification_status, verified_at)
             VALUES ($1, $2, 'pr_merged', 'https://example.test/pr', 'github_webhook', 'verified', NOW())",
        )
        .bind(broad)
        .bind(challenge)
        .execute(&app.db)
        .await
        .unwrap();
    }

    skilluv_backend::services::badge_engine::recompute_badges_for_user(&app.db, broad)
        .await
        .unwrap();
    assert!(holds(&app, broad, "code-multi-language").await);
}

#[tokio::test]
async fn a_design_deliverable_earns_no_code_badge() {
    let app = TestApp::spawn().await;
    app.register_user("badge_design").await;
    let user: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'badge_design'")
        .fetch_one(&app.db)
        .await
        .unwrap();

    let challenge: Uuid = sqlx::query_scalar(
        "INSERT INTO challenge_templates
            (title, description, instructions, skill_domain, difficulty, status, is_training)
         VALUES ('maquette', 'x', 'x', 'design', 2, 'published', TRUE)
         RETURNING id",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO deliverables
            (user_id, challenge_id, artifact_type, artifact_url, verifiable_by, verification_status, verified_at)
         VALUES ($1, $2, 'figma_frame', 'https://example.test/f', 'human_review', 'verified', NOW())",
    )
    .bind(user)
    .bind(challenge)
    .execute(&app.db)
    .await
    .unwrap();

    skilluv_backend::services::badge_engine::recompute_badges_for_user(&app.db, user)
        .await
        .unwrap();

    assert!(
        !holds(&app, user, "code-first-artifact").await,
        "a code badge for design work would make every badge meaningless"
    );
}

#[tokio::test]
async fn the_engine_never_awards_a_judgement() {
    let app = TestApp::spawn().await;
    let user = user_with_deliverables(&app, "badge_manual", "rust", 100).await;

    skilluv_backend::services::badge_engine::recompute_badges_for_user(&app.db, user)
        .await
        .unwrap();

    // A hundred deliverables earns the counted distinctions and none of the
    // judged ones — "shipped an audited contract to mainnet" is not a
    // quantity.
    assert!(holds(&app, user, "code-craft-legend").await);
    for slug in [
        "code-blockchain-shipper",
        "code-standards-contributor",
        "code-systems-hero",
    ] {
        assert!(
            !holds(&app, user, slug).await,
            "{slug} must not be derivable from a count"
        );
    }

    // `code-multi-domain` was in this list until migration 0186 put a trade on
    // the slice. It is a count now — of distinct trades, which these hundred
    // deliverables do not have, since none of them belongs to a slice.
    assert!(
        !holds(&app, user, "code-multi-domain").await,
        "a hundred deliverables in no trade is not three trades"
    );
}

#[tokio::test]
async fn a_manual_grant_carries_its_author_and_its_reason() {
    let app = TestApp::spawn().await;
    let user = user_with_deliverables(&app, "badge_granted", "rust", 1).await;

    let rule: Uuid =
        sqlx::query_scalar("SELECT id FROM badge_rules WHERE slug = 'code-systems-hero'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    let badge: Uuid = sqlx::query_scalar("SELECT id FROM badges WHERE slug = '_proof_engine'")
        .fetch_optional(&app.db)
        .await
        .unwrap()
        .unwrap_or_else(Uuid::nil);

    if badge.is_nil() {
        // The sentinel is created by the engine on first run.
        skilluv_backend::services::badge_engine::recompute_badges_for_user(&app.db, user)
            .await
            .unwrap();
    }
    let badge: Uuid = sqlx::query_scalar("SELECT id FROM badges WHERE slug = '_proof_engine'")
        .fetch_one(&app.db)
        .await
        .unwrap();

    // Half a grant is refused: a reason with no author cannot be questioned,
    // an author with no reason cannot be explained.
    let half = sqlx::query(
        "INSERT INTO user_badges (user_id, badge_id, rule_id, granted_by)
         VALUES ($1, $2, $3, $1)",
    )
    .bind(user)
    .bind(badge)
    .bind(rule)
    .execute(&app.db)
    .await;
    assert!(half.is_err(), "an author with no reason must be refused");

    let whole = sqlx::query(
        "INSERT INTO user_badges (user_id, badge_id, rule_id, granted_by, grant_reason)
         VALUES ($1, $2, $3, $1, 'pilote i2c mergé dans le noyau, revue par deux mainteneurs')",
    )
    .bind(user)
    .bind(badge)
    .bind(rule)
    .execute(&app.db)
    .await;
    assert!(whole.is_ok(), "a complete grant must be accepted");
}

#[tokio::test]
async fn a_recompute_leaves_a_manual_grant_alone() {
    let app = TestApp::spawn().await;
    let user = user_with_deliverables(&app, "badge_keepman", "rust", 1).await;

    skilluv_backend::services::badge_engine::recompute_badges_for_user(&app.db, user)
        .await
        .unwrap();

    let rule: Uuid =
        sqlx::query_scalar("SELECT id FROM badge_rules WHERE slug = 'code-devtool-author'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    let badge: Uuid = sqlx::query_scalar("SELECT id FROM badges WHERE slug = '_proof_engine'")
        .fetch_one(&app.db)
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO user_badges (user_id, badge_id, rule_id, granted_by, grant_reason)
         VALUES ($1, $2, $3, $1, 'outil utilisé par plusieurs équipes')",
    )
    .bind(user)
    .bind(badge)
    .bind(rule)
    .execute(&app.db)
    .await
    .unwrap();

    // Evaluating a manual rule would find no proofs and revoke a badge
    // somebody granted deliberately.
    skilluv_backend::services::badge_engine::recompute_badges_for_user(&app.db, user)
        .await
        .unwrap();

    assert!(
        holds(&app, user, "code-devtool-author").await,
        "a recompute must not take back what an operator awarded"
    );
}
