//! Tests d'intégration P14.5 : endpoints admin fraud.

mod common;

use bigdecimal::BigDecimal;
use common::TestApp;
use serde_json::json;
use uuid::Uuid;

use skilluv_backend::services::{fingerprint, plagiarism};

// ═══════════════════════════════════════════════════════════════════
// require_admin : non-admin → 403
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn queue_refuses_non_admin() {
    let app = TestApp::spawn().await;
    app.register_user("u_notadmin").await;
    app.login("u_notadmin").await;

    let resp = app.get("/api/admin/fraud/queue").await;
    assert_eq!(resp.status(), 403);
    drop(app);
}

// ═══════════════════════════════════════════════════════════════════
// GET /admin/fraud/queue liste flagged + suspects
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn queue_returns_flagged_and_suspects() {
    let app = TestApp::spawn().await;
    app.register_admin("adm_fraud").await;
    app.login("adm_fraud").await;

    // Seed un deliverable flagged
    let owner_id: Uuid = sqlx::query_scalar(
        "INSERT INTO users
            (email, username, password_hash, first_name, last_name, display_name,
             skill_domain, email_verified, role)
         VALUES ($1, $2, 'x', 'V', 'ictim', 'V', 'code', TRUE, 'user')
         RETURNING id",
    )
    .bind(format!("v-{}@ex.io", Uuid::new_v4()))
    .bind(format!("v{}", &Uuid::new_v4().to_string()[..8]))
    .fetch_one(&app.db)
    .await
    .expect("v");
    let ch_id: Uuid = sqlx::query_scalar(
        "INSERT INTO challenge_templates
            (title, description, instructions, skill_domain, difficulty,
             is_training, status)
         VALUES ('T', 'D', 'I', 'code', 2, TRUE, 'published') RETURNING id",
    )
    .fetch_one(&app.db)
    .await
    .expect("ch");
    let del_id: Uuid = sqlx::query_scalar(
        "INSERT INTO deliverables
            (challenge_id, user_id, artifact_type, artifact_url, verifiable_by,
             verification_status, plagiarism_score)
         VALUES ($1, $2, 'other', $3, 'human_review', 'verified', 0.95)
         RETURNING id",
    )
    .bind(ch_id)
    .bind(owner_id)
    .bind(format!("skilluv:t:{}", Uuid::new_v4()))
    .fetch_one(&app.db)
    .await
    .expect("d");

    // Flag un user suspected
    sqlx::query(
        "UPDATE users SET suspected_multi_account = TRUE,
                          suspected_multi_account_at = NOW(),
                          suspected_multi_account_reason = 'test'
         WHERE id = $1",
    )
    .bind(owner_id)
    .execute(&app.db)
    .await
    .expect("flag");

    let resp = app.get("/api/admin/fraud/queue").await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let flagged = body["data"]["flagged_deliverables"].as_array().unwrap();
    assert!(
        flagged
            .iter()
            .any(|d| d["deliverable_id"].as_str() == Some(&del_id.to_string()))
    );
    let suspects = body["data"]["suspected_users"].as_array().unwrap();
    assert!(
        suspects
            .iter()
            .any(|u| u["user_id"].as_str() == Some(&owner_id.to_string()))
    );

    drop(app);
}

// ═══════════════════════════════════════════════════════════════════
// mark-valid clear le flag
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn mark_deliverable_valid_clears_flag() {
    let app = TestApp::spawn().await;
    app.register_admin("adm_valid").await;
    app.login("adm_valid").await;

    let user_id: Uuid = sqlx::query_scalar(
        "INSERT INTO users
            (email, username, password_hash, first_name, last_name, display_name,
             skill_domain, email_verified, role)
         VALUES ($1, $2, 'x', 'V', 'V', 'V', 'code', TRUE, 'user')
         RETURNING id",
    )
    .bind(format!("v-{}@ex.io", Uuid::new_v4()))
    .bind(format!("v{}", &Uuid::new_v4().to_string()[..8]))
    .fetch_one(&app.db)
    .await
    .expect("u");
    let ch: Uuid = sqlx::query_scalar(
        "INSERT INTO challenge_templates
            (title, description, instructions, skill_domain, difficulty,
             is_training, status)
         VALUES ('T', 'D', 'I', 'code', 2, TRUE, 'published') RETURNING id",
    )
    .fetch_one(&app.db)
    .await
    .expect("c");
    let del: Uuid = sqlx::query_scalar(
        "INSERT INTO deliverables
            (challenge_id, user_id, artifact_type, artifact_url, verifiable_by,
             verification_status, plagiarism_score, plagiarism_similar_to)
         VALUES ($1, $2, 'other', $3, 'human_review', 'verified', 0.95, NULL)
         RETURNING id",
    )
    .bind(ch)
    .bind(user_id)
    .bind(format!("skilluv:t:{}", Uuid::new_v4()))
    .fetch_one(&app.db)
    .await
    .expect("d");

    let resp = app
        .post(
            &format!("/api/admin/fraud/deliverables/{del}/mark-valid"),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), 200);

    let score: Option<BigDecimal> =
        sqlx::query_scalar("SELECT plagiarism_score FROM deliverables WHERE id = $1")
            .bind(del)
            .fetch_one(&app.db)
            .await
            .expect("s");
    assert!(score.is_none());

    drop(app);
}

// ═══════════════════════════════════════════════════════════════════
// mark user valid clear suspected flag
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn mark_user_valid_clears_suspicion() {
    let app = TestApp::spawn().await;
    app.register_admin("adm_uv").await;
    app.login("adm_uv").await;

    let uid: Uuid = sqlx::query_scalar(
        "INSERT INTO users
            (email, username, password_hash, first_name, last_name, display_name,
             skill_domain, email_verified, role, suspected_multi_account,
             suspected_multi_account_at, suspected_multi_account_reason)
         VALUES ($1, $2, 'x', 'V', 'V', 'V', 'code', TRUE, 'user',
                 TRUE, NOW(), 'test flag')
         RETURNING id",
    )
    .bind(format!("v-{}@ex.io", Uuid::new_v4()))
    .bind(format!("v{}", &Uuid::new_v4().to_string()[..8]))
    .fetch_one(&app.db)
    .await
    .expect("u");

    let resp = app
        .post(
            &format!("/api/admin/fraud/users/{uid}/mark-valid"),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), 200);

    let flagged: bool =
        sqlx::query_scalar("SELECT suspected_multi_account FROM users WHERE id = $1")
            .bind(uid)
            .fetch_one(&app.db)
            .await
            .expect("f");
    assert!(!flagged);

    drop(app);
}

// ═══════════════════════════════════════════════════════════════════
// detect-multi-accounts endpoint : trigger job
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn detect_multi_accounts_endpoint_returns_report() {
    let app = TestApp::spawn().await;
    app.register_admin("adm_det").await;
    app.login("adm_det").await;

    // Seed 4 users partageant fingerprint
    for _ in 0..4 {
        let uid: Uuid = sqlx::query_scalar(
            "INSERT INTO users
                (email, username, password_hash, first_name, last_name, display_name,
                 skill_domain, email_verified, role)
             VALUES ($1, $2, 'x', 'V', 'V', 'V', 'code', TRUE, 'user')
             RETURNING id",
        )
        .bind(format!("v-{}@ex.io", Uuid::new_v4()))
        .bind(format!("v{}", &Uuid::new_v4().to_string()[..8]))
        .fetch_one(&app.db)
        .await
        .expect("u");
        fingerprint::record_fingerprint(&app.db, uid, "9.9.9.9", "SharedUA", None)
            .await
            .expect("r");
    }

    let resp = app
        .post(
            "/api/admin/fraud/detect-multi-accounts",
            &json!({ "window_hours": 24, "min_group_size": 3 }),
        )
        .await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["groups_detected"], 1);
    assert_eq!(body["data"]["users_flagged"], 4);

    drop(app);
}

// ═══════════════════════════════════════════════════════════════════
// scan-deliverable endpoint : redirige vers le service
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn scan_deliverable_endpoint_computes_similarity() {
    let app = TestApp::spawn().await;
    app.register_admin("adm_scan").await;
    app.login("adm_scan").await;

    let ua: Uuid = sqlx::query_scalar(
        "INSERT INTO users
            (email, username, password_hash, first_name, last_name, display_name,
             skill_domain, email_verified, role)
         VALUES ($1, $2, 'x', 'V', 'V', 'V', 'code', TRUE, 'user')
         RETURNING id",
    )
    .bind(format!("a-{}@ex.io", Uuid::new_v4()))
    .bind(format!("a{}", &Uuid::new_v4().to_string()[..8]))
    .fetch_one(&app.db)
    .await
    .expect("a");
    let ub: Uuid = sqlx::query_scalar(
        "INSERT INTO users
            (email, username, password_hash, first_name, last_name, display_name,
             skill_domain, email_verified, role)
         VALUES ($1, $2, 'x', 'V', 'V', 'V', 'code', TRUE, 'user')
         RETURNING id",
    )
    .bind(format!("b-{}@ex.io", Uuid::new_v4()))
    .bind(format!("b{}", &Uuid::new_v4().to_string()[..8]))
    .fetch_one(&app.db)
    .await
    .expect("b");
    let ch: Uuid = sqlx::query_scalar(
        "INSERT INTO challenge_templates
            (title, description, instructions, skill_domain, difficulty,
             is_training, status)
         VALUES ('T', 'D', 'I', 'code', 2, TRUE, 'published') RETURNING id",
    )
    .fetch_one(&app.db)
    .await
    .expect("c");
    let da: Uuid = sqlx::query_scalar(
        "INSERT INTO deliverables
            (challenge_id, user_id, artifact_type, artifact_url, verifiable_by,
             verification_status)
         VALUES ($1, $2, 'other', $3, 'human_review', 'verified') RETURNING id",
    )
    .bind(ch)
    .bind(ua)
    .bind(format!("skilluv:t:{}", Uuid::new_v4()))
    .fetch_one(&app.db)
    .await
    .expect("da");
    let db_: Uuid = sqlx::query_scalar(
        "INSERT INTO deliverables
            (challenge_id, user_id, artifact_type, artifact_url, verifiable_by,
             verification_status)
         VALUES ($1, $2, 'other', $3, 'human_review', 'verified') RETURNING id",
    )
    .bind(ch)
    .bind(ub)
    .bind(format!("skilluv:t:{}", Uuid::new_v4()))
    .fetch_one(&app.db)
    .await
    .expect("db");

    let v = vec![1.0f32; 4];
    plagiarism::store_embedding(&app.db, da, None, &v)
        .await
        .expect("sa");
    plagiarism::store_embedding(&app.db, db_, None, &v)
        .await
        .expect("sb");

    let resp = app
        .post(
            &format!("/api/admin/fraud/scan-deliverable/{db_}"),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["compared_count"], 1);
    let score = body["data"]["best_score"].as_f64().unwrap();
    assert!((score - 1.0).abs() < 0.01);

    drop(app);
}
