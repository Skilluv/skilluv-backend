//! Integration tests for SKI-41 — peer coaching.
//!
//! The matching rules are unit-tested in `services::peer_matching`; this
//! suite covers the parts that need a database: the candidate pool query
//! (exclusions, blocks, existing matches) and the session lifecycle.

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

async fn seed_orientation(app: &TestApp, slug: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO orientations (id, slug, name, primary_domain, is_curated)
         VALUES ($1, $2, 'Test orientation', 'code', TRUE)",
    )
    .bind(id)
    .bind(slug)
    .execute(&app.db)
    .await
    .expect("seed orientation");
    id
}

/// Put a user on an orientation with a timezone and languages, and set
/// their rank.
async fn seed_profile(
    app: &TestApp,
    user_id: Uuid,
    orientation_id: Uuid,
    timezone: &str,
    langs: &[&str],
    rank: &str,
) {
    let langs: Vec<String> = langs.iter().map(|s| s.to_string()).collect();
    sqlx::query(
        "INSERT INTO user_orientations
            (user_id, orientation_id, mode, working_languages, timezone)
         VALUES ($1, $2, 'learning', $3, $4)
         ON CONFLICT (user_id, orientation_id) DO UPDATE SET
             working_languages = EXCLUDED.working_languages,
             timezone = EXCLUDED.timezone",
    )
    .bind(user_id)
    .bind(orientation_id)
    .bind(&langs)
    .bind(timezone)
    .execute(&app.db)
    .await
    .expect("seed user_orientation");

    sqlx::query(
        "INSERT INTO user_ranks (user_id, rank) VALUES ($1, $2)
         ON CONFLICT (user_id) DO UPDATE SET rank = EXCLUDED.rank",
    )
    .bind(user_id)
    .bind(rank)
    .execute(&app.db)
    .await
    .expect("seed rank");
}

/// Register a user, put them on the orientation, and enroll them.
async fn seed_enrolled_peer(
    app: &TestApp,
    username: &str,
    orientation_id: Uuid,
    timezone: &str,
    langs: &[&str],
    rank: &str,
) -> Uuid {
    let body = app.register_user(username).await;
    let id = user_id_of(&body);
    seed_profile(app, id, orientation_id, timezone, langs, rank).await;
    app.login(username).await;
    let resp = app
        .post(
            "/api/users/me/peer-matching/enroll",
            &json!({ "orientation_id": orientation_id }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::CREATED, "enroll failed");
    id
}

#[tokio::test]
async fn enrollment_requires_following_the_orientation() {
    let app = TestApp::spawn().await;
    let me = app.register_user("peernoorient").await;
    app.login("peernoorient").await;
    let orientation_id = seed_orientation(&app, "peer-orientation").await;

    let resp = app
        .post(
            "/api/users/me/peer-matching/enroll",
            &json!({ "orientation_id": orientation_id }),
        )
        .await;
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "enrolling in a pool you cannot contribute to is refused"
    );

    seed_profile(
        &app,
        user_id_of(&me),
        orientation_id,
        "UTC+1",
        &["fr"],
        "ranger",
    )
    .await;
    let resp = app
        .post(
            "/api/users/me/peer-matching/enroll",
            &json!({ "orientation_id": orientation_id }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn enrollment_is_an_idempotent_upsert_and_pauses_cleanly() {
    let app = TestApp::spawn().await;
    let me = app.register_user("peerupsert").await;
    let orientation_id = seed_orientation(&app, "upsert-orientation").await;
    seed_profile(
        &app,
        user_id_of(&me),
        orientation_id,
        "UTC",
        &["fr"],
        "ranger",
    )
    .await;
    app.login("peerupsert").await;

    app.post(
        "/api/users/me/peer-matching/enroll",
        &json!({ "orientation_id": orientation_id, "weekly_cadence": 1 }),
    )
    .await;
    let second: Value = app
        .post(
            "/api/users/me/peer-matching/enroll",
            &json!({ "orientation_id": orientation_id, "weekly_cadence": 3 }),
        )
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(second["data"]["enrollment"]["weekly_cadence"], 3);

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM peer_matching_enrollments")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(count, 1, "re-enrolling updates in place");

    // Pausing keeps the cadence.
    let resp = app
        .delete(&format!(
            "/api/users/me/peer-matching/enroll/{orientation_id}"
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let (active, cadence): (bool, i16) =
        sqlx::query_as("SELECT active, weekly_cadence FROM peer_matching_enrollments")
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(!active);
    assert_eq!(cadence, 3, "pausing must not lose the chosen cadence");

    // Pausing twice is a 404 — there is nothing left to pause.
    let resp = app
        .delete(&format!(
            "/api/users/me/peer-matching/enroll/{orientation_id}"
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    // Invalid cadence is refused.
    let resp = app
        .post(
            "/api/users/me/peer-matching/enroll",
            &json!({ "orientation_id": orientation_id, "weekly_cadence": 9 }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn proposals_exclude_distant_ranks_self_and_blocked_users() {
    let app = TestApp::spawn().await;
    let orientation_id = seed_orientation(&app, "proposal-orientation").await;

    // Same rank, same timezone, shared language — the best candidate.
    let good =
        seed_enrolled_peer(&app, "peergood", orientation_id, "UTC+1", &["fr"], "ranger").await;
    // Two ranks away — must be filtered out entirely.
    seed_enrolled_peer(&app, "peerfar", orientation_id, "UTC+1", &["fr"], "maitre").await;
    // Same rank but blocked.
    let blocked = seed_enrolled_peer(
        &app,
        "peerblocked",
        orientation_id,
        "UTC+1",
        &["fr"],
        "ranger",
    )
    .await;
    // Paused enrollment — should not appear.
    let paused = seed_enrolled_peer(
        &app,
        "peerpaused",
        orientation_id,
        "UTC+1",
        &["fr"],
        "ranger",
    )
    .await;
    app.login("peerpaused").await;
    app.delete(&format!(
        "/api/users/me/peer-matching/enroll/{orientation_id}"
    ))
    .await;

    let me = app.register_user("peerseeker").await;
    let my_id = user_id_of(&me);
    seed_profile(&app, my_id, orientation_id, "UTC+1", &["fr"], "ranger").await;
    app.login("peerseeker").await;
    app.post(
        "/api/users/me/peer-matching/enroll",
        &json!({ "orientation_id": orientation_id }),
    )
    .await;

    sqlx::query("INSERT INTO user_blocks (blocker_id, blocked_id) VALUES ($1, $2)")
        .bind(my_id)
        .bind(blocked)
        .execute(&app.db)
        .await
        .unwrap();

    let body: Value = app
        .get(&format!(
            "/api/peer-matching/proposals?orientation_id={orientation_id}"
        ))
        .await
        .json()
        .await
        .unwrap();
    let proposals = body["data"]["proposals"].as_array().unwrap();

    let ids: Vec<String> = proposals
        .iter()
        .map(|p| p["user_id"].as_str().unwrap().to_string())
        .collect();
    assert!(
        ids.contains(&good.to_string()),
        "the peer match is proposed"
    );
    assert!(!ids.contains(&blocked.to_string()), "blocks cut both ways");
    assert!(
        !ids.contains(&paused.to_string()),
        "paused peers are out of the pool"
    );
    assert!(!ids.contains(&my_id.to_string()), "never propose yourself");
    assert_eq!(
        proposals.len(),
        1,
        "the two-rank-away peer is mentorship, not peer coaching"
    );
    assert_eq!(proposals[0]["score"], 100.0);
}

#[tokio::test]
async fn creating_a_match_pairs_both_sides_once() {
    let app = TestApp::spawn().await;
    let orientation_id = seed_orientation(&app, "match-orientation").await;
    let peer =
        seed_enrolled_peer(&app, "matchpeer", orientation_id, "UTC", &["fr"], "ranger").await;

    let me = app.register_user("matchseeker").await;
    let my_id = user_id_of(&me);
    seed_profile(&app, my_id, orientation_id, "UTC", &["fr"], "ranger").await;
    app.login("matchseeker").await;
    app.post(
        "/api/users/me/peer-matching/enroll",
        &json!({ "orientation_id": orientation_id }),
    )
    .await;

    let resp = app
        .post(
            "/api/peer-matching/matches",
            &json!({ "peer_id": peer, "orientation_id": orientation_id }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let created: Value = resp.json().await.unwrap();
    let match_id = created["data"]["match"]["id"].as_str().unwrap().to_string();
    assert!(
        created["data"]["match"]["match_reason"]["score"].is_number(),
        "the pairing records why it was made"
    );

    // The pair is now excluded from proposals.
    let body: Value = app
        .get(&format!(
            "/api/peer-matching/proposals?orientation_id={orientation_id}"
        ))
        .await
        .json()
        .await
        .unwrap();
    assert!(body["data"]["proposals"].as_array().unwrap().is_empty());

    // Which also means the stale proposal cannot be replayed.
    let resp = app
        .post(
            "/api/peer-matching/matches",
            &json!({ "peer_id": peer, "orientation_id": orientation_id }),
        )
        .await;
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "a stale proposal must not be replayable into a duplicate pairing"
    );

    // Both sides see it.
    for name in ["matchseeker", "matchpeer"] {
        app.login(name).await;
        let body: Value = app
            .get("/api/users/me/peer-matches")
            .await
            .json()
            .await
            .unwrap();
        let matches = body["data"]["matches"].as_array().unwrap();
        assert_eq!(matches.len(), 1, "{name} should see the match");
        assert_eq!(matches[0]["match"]["id"].as_str().unwrap(), match_id);
        assert_ne!(
            matches[0]["peer"]["user_id"].as_str().unwrap(),
            "",
            "the peer is resolved to whichever side is not the caller"
        );
    }
}

#[tokio::test]
async fn cannot_match_with_yourself() {
    let app = TestApp::spawn().await;
    let orientation_id = seed_orientation(&app, "self-orientation").await;
    let me = app.register_user("selfmatch").await;
    let my_id = user_id_of(&me);
    seed_profile(&app, my_id, orientation_id, "UTC", &["fr"], "ranger").await;
    app.login("selfmatch").await;
    app.post(
        "/api/users/me/peer-matching/enroll",
        &json!({ "orientation_id": orientation_id }),
    )
    .await;

    let resp = app
        .post(
            "/api/peer-matching/matches",
            &json!({ "peer_id": my_id, "orientation_id": orientation_id }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn sessions_are_per_side_and_cancellable() {
    let app = TestApp::spawn().await;
    let orientation_id = seed_orientation(&app, "session-orientation").await;
    let peer = seed_enrolled_peer(&app, "sesspeer", orientation_id, "UTC", &["fr"], "ranger").await;

    let me = app.register_user("sessseeker").await;
    let my_id = user_id_of(&me);
    seed_profile(&app, my_id, orientation_id, "UTC", &["fr"], "ranger").await;
    app.login("sessseeker").await;
    app.post(
        "/api/users/me/peer-matching/enroll",
        &json!({ "orientation_id": orientation_id }),
    )
    .await;
    let created: Value = app
        .post(
            "/api/peer-matching/matches",
            &json!({ "peer_id": peer, "orientation_id": orientation_id }),
        )
        .await
        .json()
        .await
        .unwrap();
    let match_id = created["data"]["match"]["id"].as_str().unwrap().to_string();

    let when = (chrono::Utc::now() + chrono::Duration::days(3)).to_rfc3339();
    let resp = app
        .post(
            &format!("/api/peer-matches/{match_id}/sessions"),
            &json!({ "session_at": when }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let session: Value = resp.json().await.unwrap();
    let session_id = session["data"]["session"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Each participant writes only their own side. The ordered pair means
    // the side depends on uuid ordering, so assert on the pair as a whole.
    let resp = app
        .client
        .patch(format!("{}/api/peer-sessions/{session_id}", app.addr))
        .json(&json!({ "notes": "seeker notes", "rating": 5 }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    app.login("sesspeer").await;
    let resp = app
        .client
        .patch(format!("{}/api/peer-sessions/{session_id}", app.addr))
        .json(&json!({ "notes": "peer notes", "rating": 4 }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = resp.json().await.unwrap();

    let notes = [
        body["data"]["session"]["notes_a"].as_str().unwrap_or(""),
        body["data"]["session"]["notes_b"].as_str().unwrap_or(""),
    ];
    assert!(
        notes.contains(&"seeker notes") && notes.contains(&"peer notes"),
        "each side's check-in lands in its own column, neither overwriting the other"
    );

    // Rating bounds.
    let resp = app
        .client
        .patch(format!("{}/api/peer-sessions/{session_id}", app.addr))
        .json(&json!({ "rating": 9 }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // Either side may cancel; a canceled session refuses further check-ins.
    let resp = app
        .delete(&format!("/api/peer-sessions/{session_id}"))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let resp = app
        .client
        .patch(format!("{}/api/peer-sessions/{session_id}", app.addr))
        .json(&json!({ "notes": "too late" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn matches_and_sessions_are_invisible_to_outsiders() {
    let app = TestApp::spawn().await;
    let orientation_id = seed_orientation(&app, "outsider-orientation").await;
    let peer = seed_enrolled_peer(&app, "outpeer", orientation_id, "UTC", &["fr"], "ranger").await;

    let me = app.register_user("outseeker").await;
    let my_id = user_id_of(&me);
    seed_profile(&app, my_id, orientation_id, "UTC", &["fr"], "ranger").await;
    app.login("outseeker").await;
    app.post(
        "/api/users/me/peer-matching/enroll",
        &json!({ "orientation_id": orientation_id }),
    )
    .await;
    let created: Value = app
        .post(
            "/api/peer-matching/matches",
            &json!({ "peer_id": peer, "orientation_id": orientation_id }),
        )
        .await
        .json()
        .await
        .unwrap();
    let match_id = created["data"]["match"]["id"].as_str().unwrap().to_string();

    app.register_user("outsider").await;
    app.login("outsider").await;

    let resp = app
        .get(&format!("/api/peer-matches/{match_id}/sessions"))
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let resp = app.delete(&format!("/api/peer-matches/{match_id}")).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn ending_a_match_blocks_new_sessions_but_keeps_history() {
    let app = TestApp::spawn().await;
    let orientation_id = seed_orientation(&app, "end-orientation").await;
    let peer = seed_enrolled_peer(&app, "endpeer", orientation_id, "UTC", &["fr"], "ranger").await;

    let me = app.register_user("endseeker").await;
    let my_id = user_id_of(&me);
    seed_profile(&app, my_id, orientation_id, "UTC", &["fr"], "ranger").await;
    app.login("endseeker").await;
    app.post(
        "/api/users/me/peer-matching/enroll",
        &json!({ "orientation_id": orientation_id }),
    )
    .await;
    let created: Value = app
        .post(
            "/api/peer-matching/matches",
            &json!({ "peer_id": peer, "orientation_id": orientation_id }),
        )
        .await
        .json()
        .await
        .unwrap();
    let match_id = created["data"]["match"]["id"].as_str().unwrap().to_string();

    let resp = app.delete(&format!("/api/peer-matches/{match_id}")).await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let when = (chrono::Utc::now() + chrono::Duration::days(1)).to_rfc3339();
    let resp = app
        .post(
            &format!("/api/peer-matches/{match_id}/sessions"),
            &json!({ "session_at": when }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::CONFLICT);

    // Hidden from the default listing, still retrievable.
    let body: Value = app
        .get("/api/users/me/peer-matches")
        .await
        .json()
        .await
        .unwrap();
    assert!(body["data"]["matches"].as_array().unwrap().is_empty());

    let body: Value = app
        .get("/api/users/me/peer-matches?include_ended=true")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["matches"].as_array().unwrap().len(), 1);

    // Ending it frees the pair to be re-matched later.
    let resp = app
        .post(
            "/api/peer-matching/matches",
            &json!({ "peer_id": peer, "orientation_id": orientation_id }),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
}
