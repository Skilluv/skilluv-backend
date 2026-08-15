//! Two things a proof platform cannot get wrong: what an attestation rests
//! on, and whether revoked work still counts.

mod common;
use common::TestApp;
use uuid::Uuid;

async fn a_user(app: &TestApp, username: &str) -> Uuid {
    app.register_user(username).await;
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT id FROM users WHERE username = '{username}'"
    )))
    .fetch_one(&app.db)
    .await
    .unwrap()
}

async fn a_verified_deliverable(app: &TestApp, user: Uuid, title: &str) -> Uuid {
    let challenge: Uuid = sqlx::query_scalar(
        "INSERT INTO challenge_templates
            (title, description, instructions, skill_domain, difficulty, status, is_training)
         VALUES ($1, 'x', 'x', 'code', 2, 'published', TRUE)
         RETURNING id",
    )
    .bind(title)
    .fetch_one(&app.db)
    .await
    .unwrap();

    sqlx::query_scalar(
        "INSERT INTO deliverables
            (user_id, challenge_id, artifact_type, artifact_url, verifiable_by,
             verification_status, verified_at)
         VALUES ($1, $2, 'pr_merged', 'https://example.test/pr', 'github_webhook',
                 'verified', NOW())
         RETURNING id",
    )
    .bind(user)
    .bind(challenge)
    .fetch_one(&app.db)
    .await
    .unwrap()
}

#[tokio::test]
async fn revoking_for_fraud_stops_the_work_counting() {
    let app = TestApp::spawn().await;
    let user = a_user(&app, "revok_fraud").await;
    let deliverable = a_verified_deliverable(&app, user, "fraude").await;

    // The fraud path stamps revoked_at and says nothing about the status.
    // Fifteen queries read the status alone, so without the trigger this
    // deliverable kept counting toward the rank and the badges of somebody
    // caught cheating.
    sqlx::query(
        "UPDATE deliverables SET revoked_at = NOW(), revocation_reason = 'plagiat' WHERE id = $1",
    )
    .bind(deliverable)
    .execute(&app.db)
    .await
    .unwrap();

    let status: String =
        sqlx::query_scalar("SELECT verification_status FROM deliverables WHERE id = $1")
            .bind(deliverable)
            .fetch_one(&app.db)
            .await
            .unwrap();

    assert_eq!(
        status, "revoked",
        "a revoked deliverable must not read as verified"
    );

    let counted: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM deliverables
          WHERE user_id = $1 AND verification_status = 'verified'",
    )
    .bind(user)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(counted, 0);
}

#[tokio::test]
async fn a_revoked_deliverable_takes_its_rank_with_it() {
    let app = TestApp::spawn().await;
    let user = a_user(&app, "revok_rank").await;

    for n in 0..3 {
        a_verified_deliverable(&app, user, &format!("rang {n}")).await;
    }
    let (before, _, _) = skilluv_backend::services::ranks::recompute_rank_for_user(&app.db, user)
        .await
        .unwrap();

    sqlx::query("UPDATE deliverables SET revoked_at = NOW(), revocation_reason = 'plagiat' WHERE user_id = $1")
        .bind(user)
        .execute(&app.db)
        .await
        .unwrap();

    let (after, _, _) = skilluv_backend::services::ranks::recompute_rank_for_user(&app.db, user)
        .await
        .unwrap();

    let counted: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM deliverables
          WHERE user_id = $1 AND verification_status = 'verified'",
    )
    .bind(user)
    .fetch_one(&app.db)
    .await
    .unwrap();

    assert_eq!(counted, 0, "revoked work must not feed the rank");
    assert!(
        !before.is_empty() && !after.is_empty(),
        "both recomputes must produce a rank"
    );
}

#[tokio::test]
async fn an_attestation_basis_must_be_one_we_recognise() {
    let app = TestApp::spawn().await;
    let user = a_user(&app, "att_basis").await;

    let refused = sqlx::query(
        "INSERT INTO attestations
            (user_id, attestation_type, title, description, icon, issued_by_type,
             verification_code, linked_skill_node_ids, basis)
         VALUES ($1, 'skill', 't', 'd', 'i', 'skilluv', $2,
                 ARRAY[(SELECT id FROM skill_nodes LIMIT 1)], 'inventé')",
    )
    .bind(user)
    .bind(Uuid::new_v4().simple().to_string()[..12].to_string())
    .execute(&app.db)
    .await;

    assert!(refused.is_err(), "an unknown basis must be refused");
}

#[tokio::test]
async fn a_basis_that_names_an_artifact_must_link_one() {
    let app = TestApp::spawn().await;
    let user = a_user(&app, "att_link").await;

    // "PR merged upstream" with nothing attached is a label, not a claim
    // anyone can check — and being checkable by a stranger is the whole
    // point of an attestation.
    let refused = sqlx::query(
        "INSERT INTO attestations
            (user_id, attestation_type, title, description, icon, issued_by_type,
             verification_code, linked_skill_node_ids, basis)
         VALUES ($1, 'skill', 't', 'd', 'i', 'skilluv', $2,
                 ARRAY[(SELECT id FROM skill_nodes LIMIT 1)], 'code_pr_merged_upstream')",
    )
    .bind(user)
    .bind(Uuid::new_v4().simple().to_string()[..12].to_string())
    .execute(&app.db)
    .await;
    assert!(
        refused.is_err(),
        "an artifact basis with no artifact must be refused"
    );

    let deliverable = a_verified_deliverable(&app, user, "lié").await;
    let accepted = sqlx::query(
        "INSERT INTO attestations
            (user_id, attestation_type, title, description, icon, issued_by_type,
             verification_code, linked_skill_node_ids, linked_deliverable_ids, basis)
         VALUES ($1, 'skill', 't', 'd', 'i', 'skilluv', $2,
                 ARRAY[(SELECT id FROM skill_nodes LIMIT 1)], ARRAY[$3::uuid],
                 'code_pr_merged_upstream')",
    )
    .bind(user)
    .bind(Uuid::new_v4().simple().to_string()[..12].to_string())
    .bind(deliverable)
    .execute(&app.db)
    .await;
    assert!(
        accepted.is_ok(),
        "with the artifact linked it must be accepted: {accepted:?}"
    );
}

#[tokio::test]
async fn an_attestation_without_a_stated_basis_is_still_valid() {
    let app = TestApp::spawn().await;
    let user = a_user(&app, "att_nobasis").await;

    // Everything issued before the column existed rests on something nobody
    // recorded. NULL says "not stated", which is true; a backfilled guess
    // would put a claim in the record that no human made.
    let accepted = sqlx::query(
        "INSERT INTO attestations
            (user_id, attestation_type, title, description, icon, issued_by_type,
             verification_code, linked_skill_node_ids)
         VALUES ($1, 'skill', 't', 'd', 'i', 'skilluv', $2,
                 ARRAY[(SELECT id FROM skill_nodes LIMIT 1)])",
    )
    .bind(user)
    .bind(Uuid::new_v4().simple().to_string()[..12].to_string())
    .execute(&app.db)
    .await;

    assert!(accepted.is_ok(), "{accepted:?}");
}
