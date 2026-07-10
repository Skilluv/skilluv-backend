//! Integration tests — OSS Bounties (P9.2 : project_slices comme source unique).

mod common;

use common::TestApp;
use serde_json::{Value, json};

async fn seed_bounty(
    app: &TestApp,
    repo_owner: &str,
    repo_name: &str,
    title: &str,
    required_skills: Vec<&str>,
) -> uuid::Uuid {
    let user_id: (uuid::Uuid,) = sqlx::query_as(
        r#"
        INSERT INTO users
            (email, username, password_hash, first_name, last_name, display_name,
             skill_domain, email_verified, role)
        VALUES ($1, $2, 'x', 'P', 'oster', 'Poster', 'code', TRUE, 'user')
        RETURNING id
        "#,
    )
    .bind(format!("{}@corp.io", uuid::Uuid::new_v4()))
    .bind(format!("p{}", &uuid::Uuid::new_v4().to_string()[..8]))
    .fetch_one(&app.db)
    .await
    .expect("insert user");

    let enterprise_id: (uuid::Uuid,) = sqlx::query_as(
        r#"
        INSERT INTO enterprises (owner_id, company_name, slug, company_size)
        VALUES ($1, $2, $3, '11-50') RETURNING id
        "#,
    )
    .bind(user_id.0)
    .bind(format!("Test Corp {}", &uuid::Uuid::new_v4().to_string()[..6]))
    .bind(format!("test-corp-{}", &uuid::Uuid::new_v4().to_string()[..8]))
    .fetch_one(&app.db)
    .await
    .expect("insert ent");

    let project_id: (uuid::Uuid,) = sqlx::query_as(
        r#"
        INSERT INTO projects (slug, name, owner_type, owner_id,
                              github_repo_owner, github_repo_name)
        VALUES ($1, $2, 'user', $3, $4, $5)
        RETURNING id
        "#,
    )
    .bind(format!("p-{}", uuid::Uuid::new_v4()))
    .bind(format!("{repo_owner}/{repo_name}"))
    .bind(user_id.0)
    .bind(repo_owner)
    .bind(repo_name)
    .fetch_one(&app.db)
    .await
    .expect("insert project");

    let meta = json!({
        "issue_url": format!("https://github.com/{repo_owner}/{repo_name}/issues/1"),
        "tags": [],
        "required_skills": required_skills,
    });

    let slice_id: (uuid::Uuid,) = sqlx::query_as(
        r#"
        INSERT INTO project_slices
            (project_id, slice_type, external_ref, external_metadata,
             title, description,
             primary_domain, difficulty, fragments_reward, credits_reward,
             status, funded_by_user_id, funder_enterprise_id, created_by_user_id,
             ingested_from)
        VALUES ($1, 'github_issue', '1', $2,
                $3, 'desc',
                'code', 4, 100, 5.0,
                'open', $4, $5, $4, 'manual')
        RETURNING id
        "#,
    )
    .bind(project_id.0)
    .bind(&meta)
    .bind(title)
    .bind(user_id.0)
    .bind(enterprise_id.0)
    .fetch_one(&app.db)
    .await
    .expect("insert slice bounty");

    slice_id.0
}

#[tokio::test]
async fn list_bounties_returns_open_ones() {
    let app = TestApp::spawn().await;

    let _ = seed_bounty(&app, "skilluv", "core", "Fix async race condition", vec!["rust", "tokio"])
        .await;

    let resp = app
        .client
        .get(format!("{}/api/bounties", app.addr))
        .send()
        .await
        .expect("GET bounties");
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let items = body["data"]["bounties"].as_array().unwrap();
    assert!(items.iter().any(|b| b["title"] == "Fix async race condition"));
    drop(app);
}

// ═══════════════════════════════════════════════════════════════════
// P9.2 end-to-end : create → claim → submit_pr via l'API HTTP
// ═══════════════════════════════════════════════════════════════════

/// Helper : seed enterprise_credits pour permettre create_bounty (5 credits
/// séquestrés). Le default balance des enterprises est 0.
async fn seed_credits_for(app: &TestApp, enterprise_slug: &str, amount: i64) {
    sqlx::query(
        r#"
        INSERT INTO enterprise_credits (enterprise_id, balance, total_purchased)
        SELECT id, $1, $1 FROM enterprises WHERE slug = $2
        ON CONFLICT (enterprise_id) DO UPDATE SET
            balance = enterprise_credits.balance + EXCLUDED.balance
        "#,
    )
    .bind(bigdecimal::BigDecimal::from(amount))
    .bind(enterprise_slug)
    .execute(&app.db)
    .await
    .expect("seed credits");
}

#[tokio::test]
async fn bounty_full_lifecycle_via_api() {
    let app = TestApp::spawn().await;

    // 1. Enterprise crée un bounty
    app.register_enterprise("BountyCorp").await;
    app.login("bountycorp").await;
    seed_credits_for(&app, "bountycorp", 100).await;

    let resp = app
        .post(
            "/api/bounties",
            &json!({
                "repo_owner": "acme",
                "repo_name": "widgets",
                "issue_number": 7,
                "issue_url": "https://github.com/acme/widgets/issues/7",
                "title": "Fix widget rendering",
                "description": "The widget hides on Safari",
                "reward_credits": "10.0",
                "fragments_bonus": 200,
                "required_skills": ["css", "safari"],
                "tags": ["bug"],
                "difficulty": 3,
            }),
        )
        .await;
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let bounty_id = body["data"]["bounty_id"].as_str().unwrap().to_string();

    // 2. Talent claim
    app.register_user("talentfoo").await;
    app.login("talentfoo").await;
    let resp = app
        .post(&format!("/api/bounties/{bounty_id}/claim"), &json!({}))
        .await;
    assert_eq!(resp.status(), 200);

    // 3. Talent soumet la PR
    let resp = app
        .post(
            &format!("/api/bounties/{bounty_id}/pr"),
            &json!({
                "pull_request_url": "https://github.com/acme/widgets/pull/12",
                "pull_request_number": 12,
            }),
        )
        .await;
    assert_eq!(resp.status(), 200);

    // 4. get_bounty renvoie status "in_review" + active_claims=1
    let resp = app.get(&format!("/api/bounties/{bounty_id}")).await;
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["status"], "in_review");
    assert_eq!(body["data"]["active_claims"], 1);

    drop(app);
}

#[tokio::test]
async fn cancel_bounty_refunds_credits() {
    let app = TestApp::spawn().await;

    app.register_enterprise("CancelCorp").await;
    app.login("cancelcorp").await;
    seed_credits_for(&app, "cancelcorp", 100).await;

    let resp = app
        .post(
            "/api/bounties",
            &json!({
                "repo_owner": "acme",
                "repo_name": "cancelme",
                "issue_number": 1,
                "issue_url": "https://github.com/acme/cancelme/issues/1",
                "title": "Nope",
                "description": "Changed my mind",
                "reward_credits": "20.0",
                "difficulty": 2,
            }),
        )
        .await;
    let body: Value = resp.json().await.unwrap();
    let bounty_id = body["data"]["bounty_id"].as_str().unwrap();

    // Solde après spend séquestre : 100 - 20 = 80
    let balance_before: bigdecimal::BigDecimal = sqlx::query_scalar(
        "SELECT balance FROM enterprise_credits WHERE enterprise_id =
         (SELECT id FROM enterprises WHERE slug = 'cancelcorp')",
    )
    .fetch_one(&app.db)
    .await
    .expect("balance");
    assert_eq!(balance_before, bigdecimal::BigDecimal::from(80));

    let resp = app.post(&format!("/api/bounties/{bounty_id}/cancel"), &json!({})).await;
    assert_eq!(resp.status(), 200);

    let balance_after: bigdecimal::BigDecimal = sqlx::query_scalar(
        "SELECT balance FROM enterprise_credits WHERE enterprise_id =
         (SELECT id FROM enterprises WHERE slug = 'cancelcorp')",
    )
    .fetch_one(&app.db)
    .await
    .expect("balance");
    assert_eq!(
        balance_after,
        bigdecimal::BigDecimal::from(100),
        "refund credits after cancel"
    );

    drop(app);
}

#[tokio::test]
async fn bounty_filter_by_skill() {
    let app = TestApp::spawn().await;

    seed_bounty(&app, "x", "python-repo", "python bounty", vec!["python"]).await;
    seed_bounty(&app, "x", "js-repo", "js bounty", vec!["javascript"]).await;

    let resp = app
        .client
        .get(format!("{}/api/bounties?skill=python", app.addr))
        .send()
        .await
        .unwrap();
    let body: Value = resp.json().await.unwrap();
    let items = body["data"]["bounties"].as_array().unwrap();
    assert!(items.iter().all(|b| b["title"] == "python bounty"));
    assert_eq!(items.len(), 1);
    drop(app);
}
