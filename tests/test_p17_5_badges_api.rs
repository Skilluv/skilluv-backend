//! Tests P17.5 : API polymorphique badges.

mod common;
use common::TestApp;

#[tokio::test]
async fn get_badge_rules_lists_only_non_deprecated() {
    let app = TestApp::spawn().await;

    // Insère 1 rule active pour vérifier qu'elle sort.
    sqlx::query(
        "INSERT INTO badge_rules (slug, output_type, display_name, conditions)
         VALUES ('p175-active-rule', 'skill_patch', 'Active', '{}')",
    )
    .execute(&app.db)
    .await
    .unwrap();

    let resp = app.get("/api/badge-rules").await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let rules = body["data"]["rules"].as_array().unwrap();
    assert!(rules.iter().any(|r| r["slug"] == "p175-active-rule"));
    // 9 legacy_* sont deprecated depuis P17.1 → doivent être ABSENTS.
    assert!(
        !rules
            .iter()
            .any(|r| r["slug"].as_str().unwrap_or("").starts_with("legacy_"))
    );
}

#[tokio::test]
async fn get_user_badges_returns_polymorphic_payload_with_rank() {
    let app = TestApp::spawn().await;
    app.register_user("kim175").await;
    let uid: uuid::Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'kim175'")
        .fetch_one(&app.db)
        .await
        .unwrap();

    // Setup: 1 rule skill_patch + 1 user_badge lié.
    let rule_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO badge_rules (slug, output_type, display_name, conditions, rarity)
         VALUES ('p175-react-patch', 'skill_patch', 'React', '{}', 'rare') RETURNING id",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    let badge_id: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM badges WHERE slug = 'first_challenge'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    let proof = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO user_badges (user_id, badge_id, rule_id, source_proofs, rarity)
         VALUES ($1, $2, $3, $4, 'rare')",
    )
    .bind(uid)
    .bind(badge_id)
    .bind(rule_id)
    .bind(&vec![proof])
    .execute(&app.db)
    .await
    .unwrap();

    let resp = app.get(&format!("/api/users/{uid}/badges")).await;
    assert!(resp.status().is_success());
    let body: serde_json::Value = resp.json().await.unwrap();
    let d = &body["data"];
    assert_eq!(d["user_id"], uid.to_string());
    assert_eq!(d["rank"]["rank"], "apprenti", "backfill par 0092");
    let patches = d["skill_patches"].as_array().unwrap();
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0]["rule_slug"], "p175-react-patch");
    assert_eq!(patches[0]["rarity"], "rare");
    assert_eq!(patches[0]["source_proofs_count"], 1);
    assert_eq!(d["total_badges"], 1);
}

#[tokio::test]
async fn get_user_badges_excludes_revoked() {
    let app = TestApp::spawn().await;
    app.register_user("kim175b").await;
    let uid: uuid::Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'kim175b'")
        .fetch_one(&app.db)
        .await
        .unwrap();
    let badge_id: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM badges WHERE slug = 'first_challenge'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    sqlx::query(
        "INSERT INTO user_badges (user_id, badge_id, rarity, revoked_at)
         VALUES ($1, $2, 'common', NOW())",
    )
    .bind(uid)
    .bind(badge_id)
    .execute(&app.db)
    .await
    .unwrap();

    let resp = app.get(&format!("/api/users/{uid}/badges")).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(
        body["data"]["total_badges"], 0,
        "revoked excluded from feed"
    );
}

#[tokio::test]
async fn get_user_badges_polymorphic_buckets_split_by_family() {
    let app = TestApp::spawn().await;
    app.register_user("kim175c").await;
    let uid: uuid::Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'kim175c'")
        .fetch_one(&app.db)
        .await
        .unwrap();

    for (slug, family) in [
        ("p175-medal-a", "medal"),
        ("p175-seal-a", "challenge_seal"),
        ("p175-stamp-a", "event_stamp"),
    ] {
        let rid: uuid::Uuid = sqlx::query_scalar(
            "INSERT INTO badge_rules (slug, output_type, display_name, conditions)
             VALUES ($1, $2, 'X', '{}') RETURNING id",
        )
        .bind(slug)
        .bind(family)
        .fetch_one(&app.db)
        .await
        .unwrap();
        // Chaque row user_badges doit avoir un badge_id distinct (PK = user_id, badge_id).
        let bid: uuid::Uuid = sqlx::query_scalar(
            "INSERT INTO badges (slug, name, description, icon, category, condition_type, condition_value)
             VALUES ($1, $1, '_', '_', 'special', 'derived', 0) RETURNING id",
        )
        .bind(format!("_badge-{slug}")).fetch_one(&app.db).await.unwrap();
        sqlx::query(
            "INSERT INTO user_badges (user_id, badge_id, rule_id, rarity)
             VALUES ($1, $2, $3, 'common')",
        )
        .bind(uid)
        .bind(bid)
        .bind(rid)
        .execute(&app.db)
        .await
        .unwrap();
    }

    let resp = app.get(&format!("/api/users/{uid}/badges")).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let d = &body["data"];
    assert_eq!(d["medals"].as_array().unwrap().len(), 1);
    assert_eq!(d["challenge_seals_count"], 1);
    assert_eq!(d["event_stamps_count"], 1);
    assert_eq!(d["total_badges"], 3);
}
