//! Review rights on AI work, granted by family rather than trade by trade.

mod common;
use common::TestApp;
use uuid::Uuid;

async fn user_with(app: &TestApp, username: &str, capability: Option<&str>) -> Uuid {
    app.register_user(username).await;
    let id: Uuid = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT id FROM users WHERE username = '{username}'"
    )))
    .fetch_one(&app.db)
    .await
    .unwrap();

    if let Some(capability) = capability {
        sqlx::query(
            "INSERT INTO user_capabilities (user_id, capability, granted_reason)
             VALUES ($1, $2, 'test')",
        )
        .bind(id)
        .bind(capability)
        .execute(&app.db)
        .await
        .expect("grant");
    }
    id
}

#[tokio::test]
async fn every_ai_trade_belongs_to_a_reviewer_group() {
    let app = TestApp::spawn().await;

    let ungrouped: Vec<String> = sqlx::query_scalar(
        "SELECT slug FROM orientations
          WHERE primary_domain = 'ai' AND NOT is_archived AND reviewer_group IS NULL",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert!(
        ungrouped.is_empty(),
        "review rights cannot be granted for these at all: {ungrouped:?}"
    );
}

#[tokio::test]
async fn one_grant_covers_the_whole_language_family() {
    let app = TestApp::spawn().await;
    let user = user_with(&app, "ai_rev_lang", Some("ai_reviewer:llm-nlp")).await;

    for slug in ["llm-engineer", "prompt-engineer", "nlp-engineer"] {
        skilluv_backend::middleware::capabilities::require_reviewer_for_orientation(
            &app.db, user, slug,
        )
        .await
        .unwrap_or_else(|e| panic!("{slug} should be reviewable: {e:?}"));
    }
}

#[tokio::test]
async fn a_grant_does_not_leak_into_another_family() {
    let app = TestApp::spawn().await;
    let user = user_with(&app, "ai_rev_narrow", Some("ai_reviewer:data")).await;

    // Judging an Airflow DAG says nothing about judging an alignment
    // experiment.
    let refused = skilluv_backend::middleware::capabilities::require_reviewer_for_orientation(
        &app.db,
        user,
        "ai-safety-researcher",
    )
    .await;

    assert!(refused.is_err(), "data rights must not reach safety work");
}

#[tokio::test]
async fn a_code_reviewer_cannot_review_ai_work() {
    let app = TestApp::spawn().await;
    // Even the code wildcard: the capability is built from the domain, so
    // `code_reviewer:all` names nothing in `ai`.
    let user = user_with(&app, "ai_rev_wrongdomain", Some("code_reviewer:all")).await;

    let refused = skilluv_backend::middleware::capabilities::require_reviewer_for_orientation(
        &app.db,
        user,
        "ml-engineer",
    )
    .await;

    assert!(refused.is_err());
}

#[tokio::test]
async fn the_ai_wildcard_reaches_every_family() {
    let app = TestApp::spawn().await;
    let user = user_with(&app, "ai_rev_all", Some("ai_reviewer:all")).await;

    for slug in [
        "data-engineer",
        "mlops-engineer",
        "computer-vision-engineer",
        "ai-safety-researcher",
        "generative-ai-artist",
    ] {
        skilluv_backend::middleware::capabilities::require_reviewer_for_orientation(
            &app.db, user, slug,
        )
        .await
        .unwrap_or_else(|e| panic!("the wildcard should reach {slug}: {e:?}"));
    }
}

#[tokio::test]
async fn the_code_capabilities_are_still_grantable() {
    let app = TestApp::spawn().await;

    // Migration 0192 rewrote the CHECK. Dropping a value would silently
    // disable whatever guard reads it, and the failure would surface as a
    // permission problem nobody could explain.
    for (n, capability) in [
        "code_reviewer:web",
        "code_reviewer:all",
        "challenge_validator:ai",
        "verified_apprentice",
        "community_curator",
    ]
    .iter()
    .enumerate()
    {
        let user = user_with(&app, &format!("aicapkeep{n}"), Some(capability)).await;
        assert!(!user.is_nil());
    }
}

#[tokio::test]
async fn every_derivable_reviewer_capability_is_grantable() {
    let app = TestApp::spawn().await;

    // `require_reviewer_for_orientation` builds the capability name from the
    // orientation row at runtime, so a value dropped from the CHECK does not
    // fail to compile. It produces a grant the database refuses, with an
    // error naming a constraint rather than the trade nobody can review.
    //
    // Five migrations have restated that list — 0098, 0117, 0120, 0176, 0210
    // — and the sixth will be whichever domain gets review rights next. This
    // is the guard that fails then, here, instead of in production.
    let derivable: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT primary_domain || '_reviewer:' || reviewer_group
           FROM orientations
          WHERE reviewer_group IS NOT NULL AND NOT is_archived
          UNION
         SELECT DISTINCT primary_domain || '_reviewer:all'
           FROM orientations
          WHERE reviewer_group IS NOT NULL AND NOT is_archived
          ORDER BY 1",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert!(
        !derivable.is_empty(),
        "no orientation carries a reviewer group at all"
    );

    let mut refused = Vec::new();
    for (n, capability) in derivable.iter().enumerate() {
        let user = user_with(&app, &format!("capderiv{n}"), None).await;
        let granted = sqlx::query(
            "INSERT INTO user_capabilities (user_id, capability, granted_reason)
             VALUES ($1, $2, 'test')",
        )
        .bind(user)
        .bind(capability)
        .execute(&app.db)
        .await;

        if granted.is_err() {
            refused.push(capability.clone());
        }
    }

    assert!(
        refused.is_empty(),
        "these trades exist and nobody can be granted review rights over them: {refused:?}"
    );
}
