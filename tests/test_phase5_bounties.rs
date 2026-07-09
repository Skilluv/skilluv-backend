//! Integration tests — Phase 5.6 OSS Bounties.

mod common;

use common::TestApp;
use serde_json::Value;

#[tokio::test]
async fn list_bounties_returns_open_ones() {
    let app = TestApp::spawn().await;

    let user_id: (uuid::Uuid,) = sqlx::query_as(
        r#"
        INSERT INTO users
            (email, username, password_hash, first_name, last_name, display_name, skill_domain, email_verified, role)
        VALUES ('poster@corp.io', 'poster1', 'x', 'P', 'oster', 'Poster', 'code', TRUE, 'user')
        RETURNING id
        "#,
    )
    .fetch_one(&app.db)
    .await
    .expect("insert user");

    let enterprise_id: (uuid::Uuid,) = sqlx::query_as(
        r#"
        INSERT INTO enterprises (owner_id, company_name, slug, company_size)
        VALUES ($1, 'Test Corp', 'test-corp', '11-50') RETURNING id
        "#,
    )
    .bind(user_id.0)
    .fetch_one(&app.db)
    .await
    .expect("insert ent");

    sqlx::query(
        r#"
        INSERT INTO oss_bounties
            (enterprise_id, posted_by_user_id, repo_owner, repo_name, issue_number, issue_url,
             title, description, reward_credits, required_skills, difficulty, tags, status)
        VALUES ($1, $2, 'skilluv', 'core', 42, 'https://github.com/skilluv/core/issues/42',
                'Fix async race condition', 'A tricky race in the poller',
                5.0, ARRAY['rust', 'tokio'], 4, ARRAY['bug'], 'open')
        "#,
    )
    .bind(enterprise_id.0)
    .bind(user_id.0)
    .execute(&app.db)
    .await
    .expect("insert bounty");

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

#[tokio::test]
async fn bounty_filter_by_skill() {
    let app = TestApp::spawn().await;
    let user: (uuid::Uuid,) = sqlx::query_as(
        r#"
        INSERT INTO users (email, username, password_hash, first_name, last_name, display_name, skill_domain, email_verified, role)
        VALUES ('p2@corp.io', 'poster2', 'x', 'P', 'oster', 'Poster', 'code', TRUE, 'user') RETURNING id
        "#,
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    let ent: (uuid::Uuid,) = sqlx::query_as(
        r#"
        INSERT INTO enterprises (owner_id, company_name, slug, company_size)
        VALUES ($1, 'Skill Corp', 'skill-corp', '11-50') RETURNING id
        "#,
    )
    .bind(user.0)
    .fetch_one(&app.db)
    .await
    .unwrap();
    sqlx::query(
        r#"
        INSERT INTO oss_bounties
            (enterprise_id, posted_by_user_id, repo_owner, repo_name, issue_number, issue_url,
             title, description, reward_credits, required_skills, difficulty, tags, status)
        VALUES
            ($1, $2, 'x', 'python-repo', 1, 'https://x/1', 'python bounty', 'x', 3.0, ARRAY['python'], 3, ARRAY[]::TEXT[], 'open'),
            ($1, $2, 'x', 'js-repo', 2, 'https://x/2', 'js bounty', 'x', 3.0, ARRAY['javascript'], 3, ARRAY[]::TEXT[], 'open')
        "#,
    )
    .bind(ent.0)
    .bind(user.0)
    .execute(&app.db)
    .await
    .unwrap();

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
