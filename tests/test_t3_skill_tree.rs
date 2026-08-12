//! Integration tests for SKI-47 — skill tree with prerequisites.

mod common;

use common::TestApp;
use reqwest::StatusCode;
use serde_json::{Value, json};
use uuid::Uuid;

fn user_id_of(register_body: &Value) -> Uuid {
    register_body["data"]["user"]["id"]
        .as_str()
        .expect("register response carries a user id")
        .parse()
        .expect("user id is a uuid")
}

async fn seed_skill(app: &TestApp, slug: &str, domain: &str, parent: Option<Uuid>) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO skill_nodes (id, slug, display_name, domain, parent_id)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(id)
    .bind(slug)
    .bind(slug)
    .bind(domain)
    .bind(parent)
    .execute(&app.db)
    .await
    .expect("seed skill");
    id
}

async fn set_prereqs(app: &TestApp, skill: Uuid, prereqs: &[Uuid]) {
    sqlx::query("UPDATE skill_nodes SET prerequisite_skill_ids = $2 WHERE id = $1")
        .bind(skill)
        .bind(prereqs)
        .execute(&app.db)
        .await
        .expect("set prerequisites");
}

async fn prove_skill(app: &TestApp, user_id: Uuid, skill: Uuid, count: i32, level: i16) {
    sqlx::query(
        "INSERT INTO user_skills (user_id, skill_id, proven_count, proficiency_level)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (user_id, skill_id) DO UPDATE SET
             proven_count = EXCLUDED.proven_count,
             proficiency_level = EXCLUDED.proficiency_level",
    )
    .bind(user_id)
    .bind(skill)
    .bind(count)
    .bind(level)
    .execute(&app.db)
    .await
    .expect("prove skill");
}

/// Find a node anywhere in the returned tree.
fn find_node<'a>(tree: &'a [Value], slug: &str) -> Option<&'a Value> {
    for node in tree {
        if node["slug"] == slug {
            return Some(node);
        }
        if let Some(children) = node["children"].as_array()
            && let Some(found) = find_node(children, slug)
        {
            return Some(found);
        }
    }
    None
}

#[tokio::test]
async fn statuses_reflect_prerequisites_and_progress() {
    let app = TestApp::spawn().await;
    let me = app.register_user("treeuser").await;
    let my_id = user_id_of(&me);

    let javascript = seed_skill(&app, "javascript", "code", None).await;
    let react = seed_skill(&app, "react", "code", None).await;
    set_prereqs(&app, react, &[javascript]).await;

    let body: Value = app
        .get(&format!("/api/users/{my_id}/skill-tree"))
        .await
        .json()
        .await
        .unwrap();
    let tree = body["data"]["tree"].as_array().unwrap();

    let js = find_node(tree, "javascript").expect("javascript is in the tree");
    assert_eq!(js["status"], "unlocked", "no prerequisites, nothing proven");

    let rx = find_node(tree, "react").expect("react is in the tree");
    assert_eq!(rx["status"], "locked");
    let missing = rx["missing_prerequisites"].as_array().unwrap();
    assert_eq!(missing.len(), 1);
    assert_eq!(
        missing[0]["slug"], "javascript",
        "a locked node names what is missing"
    );

    // Proving the prerequisite unlocks the dependent.
    prove_skill(&app, my_id, javascript, 3, 2).await;
    let body: Value = app
        .get(&format!("/api/users/{my_id}/skill-tree"))
        .await
        .json()
        .await
        .unwrap();
    let tree = body["data"]["tree"].as_array().unwrap();
    assert_eq!(
        find_node(tree, "javascript").unwrap()["status"],
        "in_progress"
    );
    assert_eq!(find_node(tree, "react").unwrap()["status"], "unlocked");

    // Top proficiency reads as mastered, not as unfinished work.
    prove_skill(&app, my_id, javascript, 30, 5).await;
    let body: Value = app
        .get(&format!("/api/users/{my_id}/skill-tree"))
        .await
        .json()
        .await
        .unwrap();
    let tree = body["data"]["tree"].as_array().unwrap();
    assert_eq!(find_node(tree, "javascript").unwrap()["status"], "mastered");

    // Counts cover the whole catalog, which migration 0057 seeds with
    // hundreds of nodes — assert on the buckets being populated, not on
    // absolute totals that any catalog change would invalidate.
    let counts = &body["data"]["counts"];
    assert!(
        counts["mastered"].as_i64().unwrap() >= 1,
        "the mastered skill is tallied"
    );
    assert!(
        counts["unlocked"].as_i64().unwrap() >= 1,
        "react became unlocked once its prerequisite was proven"
    );
}

#[tokio::test]
async fn taxonomy_children_are_nested() {
    let app = TestApp::spawn().await;
    let me = app.register_user("treenest").await;
    let my_id = user_id_of(&me);

    let engines = seed_skill(&app, "game-engines", "game", None).await;
    seed_skill(&app, "godot-3d", "game", Some(engines)).await;

    let body: Value = app
        .get(&format!("/api/users/{my_id}/skill-tree"))
        .await
        .json()
        .await
        .unwrap();
    let tree = body["data"]["tree"].as_array().unwrap();

    let root = tree
        .iter()
        .find(|n| n["slug"] == "game-engines")
        .expect("parent is a root");
    let children = root["children"].as_array().unwrap();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0]["slug"], "godot-3d");
}

#[tokio::test]
async fn cross_domain_prerequisites_still_unlock_under_a_domain_filter() {
    let app = TestApp::spawn().await;
    let me = app.register_user("treecross").await;
    let my_id = user_id_of(&me);

    // The prerequisite lives in another domain, so a naive implementation
    // that filtered before computing the proven set would leave the node
    // locked.
    let blender = seed_skill(&app, "blender-basics", "design", None).await;
    let godot = seed_skill(&app, "godot-3d-cross", "game", None).await;
    set_prereqs(&app, godot, &[blender]).await;
    prove_skill(&app, my_id, blender, 2, 2).await;

    let body: Value = app
        .get(&format!("/api/users/{my_id}/skill-tree?domain=game"))
        .await
        .json()
        .await
        .unwrap();
    let tree = body["data"]["tree"].as_array().unwrap();

    assert!(
        find_node(tree, "blender-basics").is_none(),
        "the domain filter narrows what is returned"
    );
    let godot_node = find_node(tree, "godot-3d-cross").expect("in-domain node is returned");
    assert_eq!(
        godot_node["status"], "unlocked",
        "prerequisites are evaluated against the whole catalog, not the filtered view"
    );
}

#[tokio::test]
async fn domain_filter_is_validated() {
    let app = TestApp::spawn().await;
    let me = app.register_user("treedomain").await;
    let my_id = user_id_of(&me);

    let resp = app
        .get(&format!("/api/users/{my_id}/skill-tree?domain=wizardry"))
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn hidden_profile_tree_is_404_for_others() {
    let app = TestApp::spawn().await;
    let me = app.register_user("treehidden").await;
    let my_id = user_id_of(&me);
    sqlx::query("UPDATE users SET profile_hidden = TRUE WHERE id = $1")
        .bind(my_id)
        .execute(&app.db)
        .await
        .unwrap();

    // A fresh client: `register_user` leaves the caller logged in, so
    // reusing app's client would test the owner's view.
    let anon = reqwest::Client::new();
    let resp = anon
        .get(format!("{}/api/users/{my_id}/skill-tree", app.addr))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    app.login("treehidden").await;
    let resp = app.get(&format!("/api/users/{my_id}/skill-tree")).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn admin_can_set_prerequisites_but_not_create_a_cycle() {
    let app = TestApp::spawn().await;
    let a = seed_skill(&app, "cycle-a", "code", None).await;
    let b = seed_skill(&app, "cycle-b", "code", None).await;
    let c = seed_skill(&app, "cycle-c", "code", None).await;

    app.register_admin("treeadmin").await;
    app.login("treeadmin").await;

    // a requires b, b requires c — a legal chain.
    let resp = app
        .put(
            &format!("/api/admin/skills/{a}/prerequisites"),
            &json!({ "prerequisite_skill_ids": [b] }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let resp = app
        .put(
            &format!("/api/admin/skills/{b}/prerequisites"),
            &json!({ "prerequisite_skill_ids": [c] }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    // c requiring a would close the loop: every skill on it would be
    // permanently locked.
    let resp = app
        .put(
            &format!("/api/admin/skills/{c}/prerequisites"),
            &json!({ "prerequisite_skill_ids": [a] }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // Direct self-reference too.
    let resp = app
        .put(
            &format!("/api/admin/skills/{a}/prerequisites"),
            &json!({ "prerequisite_skill_ids": [a] }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // Unknown prerequisite ids are refused rather than silently locking
    // the node against something that can never be proven.
    let resp = app
        .put(
            &format!("/api/admin/skills/{a}/prerequisites"),
            &json!({ "prerequisite_skill_ids": [Uuid::new_v4()] }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    // Clearing works.
    let resp = app
        .put(
            &format!("/api/admin/skills/{a}/prerequisites"),
            &json!({ "prerequisite_skill_ids": [] }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = resp.json().await.unwrap();
    assert!(
        body["data"]["prerequisite_skill_ids"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn duplicate_prerequisites_are_deduplicated() {
    let app = TestApp::spawn().await;
    let a = seed_skill(&app, "dedup-a", "code", None).await;
    let b = seed_skill(&app, "dedup-b", "code", None).await;

    app.register_admin("treededup").await;
    app.login("treededup").await;

    let body: Value = app
        .put(
            &format!("/api/admin/skills/{a}/prerequisites"),
            &json!({ "prerequisite_skill_ids": [b, b, b] }),
        )
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(
        body["data"]["prerequisite_skill_ids"]
            .as_array()
            .unwrap()
            .len(),
        1,
        "the same prerequisite listed twice is one prerequisite"
    );
}

#[tokio::test]
async fn setting_prerequisites_requires_admin() {
    let app = TestApp::spawn().await;
    let a = seed_skill(&app, "gate-a", "code", None).await;
    app.register_user("treeplain").await;
    app.login("treeplain").await;

    let resp = app
        .put(
            &format!("/api/admin/skills/{a}/prerequisites"),
            &json!({ "prerequisite_skill_ids": [] }),
        )
        .await;
    assert!(
        resp.status() == StatusCode::FORBIDDEN || resp.status() == StatusCode::UNAUTHORIZED,
        "got {}",
        resp.status()
    );
}
