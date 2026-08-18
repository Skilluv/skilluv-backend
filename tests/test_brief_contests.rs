//! Brief contests and duels: one brief, N answers, a ranking somebody can
//! defend.
//!
//! The format only earns its place if the result holds up. What this suite
//! pins is what makes it hold up: a panel that can actually judge the craft,
//! nobody voting for themselves or from an account created yesterday, a
//! blended score whose rule is recorded with the result, and a win that
//! leaves a proof instead of a fragment balance.

mod common;
use common::TestApp;
use serde_json::{Value, json};
use uuid::Uuid;

async fn user_id(app: &TestApp, username: &str) -> Uuid {
    sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(username)
        .fetch_one(&app.db)
        .await
        .unwrap()
}

async fn grant(app: &TestApp, user: Uuid, capability: &str) {
    sqlx::query(
        "INSERT INTO user_capabilities (user_id, capability, granted_reason)
         VALUES ($1, $2, 'test') ON CONFLICT DO NOTHING",
    )
    .bind(user)
    .bind(capability)
    .execute(&app.db)
    .await
    .unwrap();
}

/// Back-dates an account, which is what the community vote floor reads.
async fn age_account(app: &TestApp, user: Uuid, days: i32) {
    sqlx::query("UPDATE users SET created_at = NOW() - make_interval(days => $2) WHERE id = $1")
        .bind(user)
        .bind(days)
        .execute(&app.db)
        .await
        .unwrap();
}

fn a_brief() -> String {
    "Concevez l'identité visuelle complète d'une coopérative agricole : logotype, \
     palette, typographie et une application au choix. Le logotype doit rester \
     lisible en favicon et tenir en une seule couleur pour la sérigraphie sur les \
     sacs de récolte. Livrez les sources vectorielles et un court document de \
     guidelines."
        .to_string()
}

async fn a_contest(app: &TestApp, slug: &str, kind: &str, rules: Value) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO tournaments
            (slug, name, kind, starts_at, ends_at, status, rules, scoring_direction, skill_domain)
         VALUES ($1, $1, $2, NOW() - INTERVAL '1 day', NOW() + INTERVAL '7 days',
                 'active', $3, 'higher_is_better', 'design')
         RETURNING id",
    )
    .bind(slug)
    .bind(kind)
    .bind(&rules)
    .fetch_one(&app.db)
    .await
    .expect("contest")
}

async fn enter(app: &TestApp, contest: Uuid, user: Uuid) {
    sqlx::query(
        "INSERT INTO tournament_participants (tournament_id, participant_type, participant_id)
         VALUES ($1, 'user', $2) ON CONFLICT DO NOTHING",
    )
    .bind(contest)
    .bind(user)
    .execute(&app.db)
    .await
    .unwrap();
}

fn an_entry(n: u32) -> Value {
    json!({
        "artifact_url": format!("https://figma.test/entry/{n}"),
        "artifact_type": "design_file",
        "summary": "Une direction fondée sur le sillon et la graine.",
    })
}

async fn submission_id(app: &TestApp, contest: Uuid, user: Uuid) -> Uuid {
    sqlx::query_scalar(
        "SELECT id FROM tournament_submissions
          WHERE tournament_id = $1 AND participant_type = 'user' AND participant_id = $2",
    )
    .bind(contest)
    .bind(user)
    .fetch_one(&app.db)
    .await
    .unwrap()
}

// ═══════════════════════════════════════════════════════════════════
// What the format asks for
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn a_brief_contest_without_a_real_brief_is_not_publishable() {
    // A subject line is not a brief: the answers would differ on things
    // nobody stated, and a jury would be arbitrating an unasked question.
    assert!(
        skilluv_backend::services::contest::validate_rules(
            "brief_contest",
            &json!({"brief": "Un logo", "judging_criteria": ["distinction"]}),
        )
        .is_err()
    );
    assert!(
        skilluv_backend::services::contest::validate_rules(
            "brief_contest",
            &json!({"brief": a_brief(), "judging_criteria": ["distinction", "scalabilité"]}),
        )
        .is_ok()
    );
}

#[tokio::test]
async fn a_design_answer_is_not_a_repository() {
    let app = TestApp::spawn().await;
    let contest = a_contest(
        &app,
        "identite-coop",
        "brief_contest",
        json!({"brief": a_brief(), "judging_criteria": ["distinction"]}),
    )
    .await;

    app.register_user("entrant_one").await;
    let entrant = user_id(&app, "entrant_one").await;
    enter(&app, contest, entrant).await;
    app.login("entrant_one").await;

    let resp = app
        .post("/api/tournaments/identite-coop/submissions", &an_entry(1))
        .await;
    assert_eq!(
        resp.status(),
        200,
        "a design file is a legitimate answer: {}",
        resp.text().await.unwrap()
    );
}

// ═══════════════════════════════════════════════════════════════════
// The panel
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn a_juror_who_cannot_judge_the_craft_is_not_invited() {
    let app = TestApp::spawn().await;
    let contest = a_contest(
        &app,
        "jury-competence",
        "brief_contest",
        json!({"brief": a_brief(), "judging_criteria": ["distinction"]}),
    )
    .await;

    app.register_user("outsider").await;
    let outsider = user_id(&app, "outsider").await;

    let refused =
        skilluv_backend::services::contest::invite_juror(&app.db, contest, outsider, outsider)
            .await;
    assert!(
        refused.is_err(),
        "a panel that cannot judge the craft makes the result meaningless"
    );

    app.register_user("brand_juror").await;
    let juror = user_id(&app, "brand_juror").await;
    grant(&app, juror, "design_reviewer:brand").await;
    skilluv_backend::services::contest::invite_juror(&app.db, contest, juror, juror)
        .await
        .expect("somebody with review rights in the domain may be asked");
}

#[tokio::test]
async fn a_juror_and_an_entrant_are_never_the_same_person() {
    let app = TestApp::spawn().await;
    let contest = a_contest(
        &app,
        "jury-conflict",
        "brief_contest",
        json!({"brief": a_brief(), "judging_criteria": ["distinction"]}),
    )
    .await;

    app.register_user("double_hat").await;
    let person = user_id(&app, "double_hat").await;
    grant(&app, person, "design_reviewer:brand").await;
    enter(&app, contest, person).await;
    app.login("double_hat").await;
    app.post("/api/tournaments/jury-conflict/submissions", &an_entry(1))
        .await;

    let refused =
        skilluv_backend::services::contest::invite_juror(&app.db, contest, person, person).await;
    assert!(refused.is_err(), "somebody with an entry cannot judge it");
}

#[tokio::test]
async fn the_panel_is_public_before_the_deadline() {
    let app = TestApp::spawn().await;
    let contest = a_contest(
        &app,
        "jury-public",
        "brief_contest",
        json!({"brief": a_brief(), "judging_criteria": ["distinction"]}),
    )
    .await;

    app.register_user("named_juror").await;
    let juror = user_id(&app, "named_juror").await;
    grant(&app, juror, "design_reviewer:brand").await;
    skilluv_backend::services::contest::invite_juror(&app.db, contest, juror, juror)
        .await
        .unwrap();

    // Entrants get to see who will judge them. A secret panel cannot be
    // trusted, and naming it is most of what makes the result defensible.
    let body: Value = app
        .get("/api/tournaments/jury-public/jury")
        .await
        .json()
        .await
        .unwrap();
    let panel = body["data"]["jury"].as_array().unwrap();
    assert_eq!(panel.len(), 1);
    assert_eq!(panel[0]["juror_user_id"], juror.to_string());
    assert!(panel[0]["accepted_at"].is_null(), "invited is not accepted");

    app.login("named_juror").await;
    let resp = app
        .post(
            "/api/tournaments/jury-public/jury/respond",
            &json!({"accept": true}),
        )
        .await;
    assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
}

// ═══════════════════════════════════════════════════════════════════
// The room
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn a_vote_costs_an_account_that_is_not_brand_new() {
    let app = TestApp::spawn().await;
    let contest = a_contest(
        &app,
        "duel-logos",
        "duel",
        json!({"task": "Un logotype", "duration_hours": 48}),
    )
    .await;

    app.register_user("duellist").await;
    let duellist = user_id(&app, "duellist").await;
    enter(&app, contest, duellist).await;
    app.login("duellist").await;
    app.post("/api/tournaments/duel-logos/submissions", &an_entry(1))
        .await;
    let entry = submission_id(&app, contest, duellist).await;

    // Fresh account: below the thirty-day floor. Creating accounts is free,
    // and a vote that costs nothing is worth nothing.
    app.register_user("fresh_voter").await;
    app.login("fresh_voter").await;
    let resp = app
        .post(
            "/api/tournaments/duel-logos/community-vote",
            &json!({"submission_id": entry}),
        )
        .await;
    assert!(
        resp.status().is_client_error(),
        "an account created today must not decide a contest"
    );

    // Self-vote.
    app.login("duellist").await;
    let resp = app
        .post(
            "/api/tournaments/duel-logos/community-vote",
            &json!({"submission_id": entry}),
        )
        .await;
    assert!(resp.status().is_client_error());

    // A settled account votes, and voting again moves that one vote.
    app.register_user("settled_voter").await;
    let voter = user_id(&app, "settled_voter").await;
    age_account(&app, voter, 90).await;
    app.login("settled_voter").await;
    for _ in 0..2 {
        let resp = app
            .post(
                "/api/tournaments/duel-logos/community-vote",
                &json!({"submission_id": entry}),
            )
            .await;
        assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    }

    let votes: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM tournament_community_votes WHERE tournament_id = $1",
    )
    .bind(contest)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(votes, 1, "one account, one voice");
}

#[tokio::test]
async fn a_juried_contest_is_not_open_to_the_room_unless_it_says_so() {
    let app = TestApp::spawn().await;
    let contest = a_contest(
        &app,
        "jury-only-contest",
        "brief_contest",
        json!({"brief": a_brief(), "judging_criteria": ["distinction"]}),
    )
    .await;

    app.register_user("jc_entrant").await;
    let entrant = user_id(&app, "jc_entrant").await;
    enter(&app, contest, entrant).await;
    app.login("jc_entrant").await;
    app.post(
        "/api/tournaments/jury-only-contest/submissions",
        &an_entry(1),
    )
    .await;
    let entry = submission_id(&app, contest, entrant).await;

    app.register_user("jc_voter").await;
    let voter = user_id(&app, "jc_voter").await;
    age_account(&app, voter, 90).await;
    app.login("jc_voter").await;
    let resp = app
        .post(
            "/api/tournaments/jury-only-contest/community-vote",
            &json!({"submission_id": entry}),
        )
        .await;
    assert!(
        resp.status().is_client_error(),
        "the room does not vote in a contest that never said it could"
    );
}

// ═══════════════════════════════════════════════════════════════════
// The result
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn the_jury_decides_a_juried_contest_and_the_win_leaves_a_proof() {
    let app = TestApp::spawn().await;
    let contest = a_contest(
        &app,
        "coop-identity",
        "brief_contest",
        json!({"brief": a_brief(), "judging_criteria": ["distinction", "scalabilité"]}),
    )
    .await;

    app.register_user("designer_a").await;
    app.register_user("designer_b").await;
    let a = user_id(&app, "designer_a").await;
    let b = user_id(&app, "designer_b").await;

    for (name, user, n) in [("designer_a", a, 1u32), ("designer_b", b, 2)] {
        enter(&app, contest, user).await;
        app.login(name).await;
        let resp = app
            .post("/api/tournaments/coop-identity/submissions", &an_entry(n))
            .await;
        assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    }

    // A judged with 90, B with 40.
    app.register_admin("contest_admin").await;
    let admin = user_id(&app, "contest_admin").await;
    grant(&app, admin, "design_reviewer:brand").await;
    skilluv_backend::services::contest::invite_juror(&app.db, contest, admin, admin)
        .await
        .unwrap();
    skilluv_backend::services::contest::respond_to_invitation(&app.db, contest, admin, true, None)
        .await
        .unwrap();

    for (user, score) in [(a, 90i16), (b, 40)] {
        let entry = submission_id(&app, contest, user).await;
        sqlx::query(
            "UPDATE tournament_submissions
                SET status = 'accepted', judge_score = $2, judged_by = $3, judged_at = NOW()
              WHERE id = $1",
        )
        .bind(entry)
        .bind(score)
        .bind(admin)
        .execute(&app.db)
        .await
        .unwrap();
    }

    // The scores reach the ranking, and the ranking pays.
    skilluv_backend::services::contest::recompute_contest_scores(&app.db, contest)
        .await
        .unwrap();
    skilluv_backend::services::tournament::conclude_tournament(&app.db, contest)
        .await
        .unwrap();

    let winner_rank: i32 = sqlx::query_scalar(
        "SELECT rank FROM tournament_participants
          WHERE tournament_id = $1 AND participant_id = $2",
    )
    .bind(contest)
    .bind(a)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(winner_rank, 1);

    // And the win leaves something on the profile. Before migration 0411,
    // winning a contest moved nothing at all.
    let report = skilluv_backend::services::design_attestations::award_contest_podium(
        &app.db, contest,
    )
    .await
    .unwrap();
    assert_eq!(report.deliverables_written, 2);
    assert_eq!(report.attestations_issued, 2);

    let basis: String = sqlx::query_scalar(
        "SELECT basis FROM attestations WHERE user_id = $1 AND basis IS NOT NULL",
    )
    .bind(a)
    .fetch_one(&app.db)
    .await
    .expect("the winner gets an attestation somebody can check");
    assert_eq!(basis, "design_contest_won");

    let verified: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM deliverables
          WHERE user_id = $1 AND verification_status = 'verified'",
    )
    .bind(a)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(verified, 1, "a contest win is a proof like any other");

    // Awarding twice must not pay twice.
    let again = skilluv_backend::services::design_attestations::award_contest_podium(
        &app.db, contest,
    )
    .await
    .unwrap();
    assert_eq!(again.deliverables_written, 0);
    assert_eq!(again.attestations_issued, 0);
}

#[tokio::test]
async fn taking_part_is_not_an_achievement() {
    let app = TestApp::spawn().await;
    let contest = a_contest(
        &app,
        "wide-contest",
        "brief_contest",
        json!({"brief": a_brief(), "judging_criteria": ["distinction"]}),
    )
    .await;

    // Four entrants, so somebody finishes fourth.
    let mut people = Vec::new();
    for i in 0..4 {
        let name = format!("wide_entrant_{i}");
        app.register_user(&name).await;
        let id = user_id(&app, &name).await;
        enter(&app, contest, id).await;
        app.login(&name).await;
        app.post("/api/tournaments/wide-contest/submissions", &an_entry(i))
            .await;
        let entry = submission_id(&app, contest, id).await;
        sqlx::query(
            "UPDATE tournament_submissions SET status = 'accepted', judge_score = $2,
                    judged_by = $3, judged_at = NOW() WHERE id = $1",
        )
        .bind(entry)
        .bind(90 - (i as i16) * 10)
        .bind(id)
        .execute(&app.db)
        .await
        .unwrap();
        people.push(id);
    }

    skilluv_backend::services::contest::recompute_contest_scores(&app.db, contest)
        .await
        .unwrap();
    skilluv_backend::services::tournament::conclude_tournament(&app.db, contest)
        .await
        .unwrap();
    let report = skilluv_backend::services::design_attestations::award_contest_podium(
        &app.db, contest,
    )
    .await
    .unwrap();

    // Podium only. A proof that means "showed up" devalues every other row.
    assert_eq!(report.deliverables_written, 3);

    let fourth = people[3];
    let proofs: i64 = sqlx::query_scalar("SELECT count(*) FROM deliverables WHERE user_id = $1")
        .bind(fourth)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(proofs, 0);
}

#[tokio::test]
async fn a_duel_is_decided_by_the_room_not_by_a_panel() {
    let app = TestApp::spawn().await;
    let contest = a_contest(
        &app,
        "duel-decided",
        "duel",
        json!({"task": "Un logotype", "duration_hours": 24}),
    )
    .await;

    app.register_user("duel_a").await;
    app.register_user("duel_b").await;
    let a = user_id(&app, "duel_a").await;
    let b = user_id(&app, "duel_b").await;

    for (name, user, n) in [("duel_a", a, 1u32), ("duel_b", b, 2)] {
        enter(&app, contest, user).await;
        app.login(name).await;
        app.post("/api/tournaments/duel-decided/submissions", &an_entry(n))
            .await;
    }
    let entry_b = submission_id(&app, contest, b).await;

    // Two settled accounts vote for B; nobody scores anything.
    for i in 0..2 {
        let name = format!("duel_voter_{i}");
        app.register_user(&name).await;
        let voter = user_id(&app, &name).await;
        age_account(&app, voter, 90).await;
        app.login(&name).await;
        let resp = app
            .post(
                "/api/tournaments/duel-decided/community-vote",
                &json!({"submission_id": entry_b}),
            )
            .await;
        assert_eq!(resp.status(), 200, "{}", resp.text().await.unwrap());
    }

    skilluv_backend::services::contest::recompute_contest_scores(&app.db, contest)
        .await
        .unwrap();
    skilluv_backend::services::tournament::conclude_tournament(&app.db, contest)
        .await
        .unwrap();

    let winner: Uuid = sqlx::query_scalar(
        "SELECT participant_id FROM tournament_participants
          WHERE tournament_id = $1 AND rank = 1",
    )
    .bind(contest)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(winner, b, "in a duel the room decides, and it voted for B");
}

#[tokio::test]
async fn a_vote_spike_is_reported_not_punished() {
    let app = TestApp::spawn().await;
    let contest = a_contest(
        &app,
        "duel-burst",
        "duel",
        json!({"task": "Un logotype", "duration_hours": 24}),
    )
    .await;

    app.register_user("burst_target").await;
    let target = user_id(&app, "burst_target").await;
    enter(&app, contest, target).await;
    app.login("burst_target").await;
    app.post("/api/tournaments/duel-burst/submissions", &an_entry(1))
        .await;
    let entry = submission_id(&app, contest, target).await;

    for i in 0..3 {
        let name = format!("burst_voter_{i}");
        app.register_user(&name).await;
        let voter = user_id(&app, &name).await;
        age_account(&app, voter, 90).await;
        app.login(&name).await;
        app.post(
            "/api/tournaments/duel-burst/community-vote",
            &json!({"submission_id": entry}),
        )
        .await;
    }

    let bursts =
        skilluv_backend::services::contest::detect_vote_bursts(&app.db, contest, 60, 3)
            .await
            .unwrap();
    assert_eq!(bursts.len(), 1);
    assert_eq!(bursts[0].0, entry);
    assert_eq!(bursts[0].1, 3);

    // Reporting is all it does: the entry is still in the running, because
    // deciding a vote was bought is a human judgement with consequences.
    let status: String =
        sqlx::query_scalar("SELECT status FROM tournament_submissions WHERE id = $1")
            .bind(entry)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(status, "submitted");
}

// ═══════════════════════════════════════════════════════════════════
// Reading the contests back
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn the_contest_list_narrows_in_sql_not_in_the_browser() {
    let app = TestApp::spawn().await;

    a_contest(&app, "brief-design-a", "brief_contest", json!({"brief": a_brief()})).await;
    let duel = a_contest(&app, "duel-design-a", "duel", json!({"brief": a_brief()})).await;
    sqlx::query("UPDATE tournaments SET skill_domain = NULL WHERE slug = 'duel-design-a'")
        .execute(&app.db)
        .await
        .unwrap();
    let _ = duel;
    a_contest(&app, "brief-code-a", "brief_contest", json!({"brief": a_brief()})).await;
    sqlx::query("UPDATE tournaments SET skill_domain = 'code' WHERE slug = 'brief-code-a'")
        .execute(&app.db)
        .await
        .unwrap();

    // Without this filter the design contest page asked for two hundred rows
    // and sorted them client-side, which stops working at the two hundred and
    // first tournament — silently, by dropping the oldest.
    let body: Value = app
        .get("/api/tournaments?kind=brief_contest&skill_domain=design")
        .await
        .json()
        .await
        .unwrap();
    let slugs: Vec<&str> = body["data"]["tournaments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["slug"].as_str().unwrap())
        .collect();

    assert!(slugs.contains(&"brief-design-a"), "{slugs:?}");
    assert!(!slugs.contains(&"brief-code-a"), "another domain: {slugs:?}");
    assert!(!slugs.contains(&"duel-design-a"), "another kind: {slugs:?}");
}

#[tokio::test]
async fn a_domain_filter_keeps_the_contests_open_to_everyone() {
    let app = TestApp::spawn().await;

    a_contest(&app, "brief-open", "brief_contest", json!({"brief": a_brief()})).await;
    sqlx::query("UPDATE tournaments SET skill_domain = NULL WHERE slug = 'brief-open'")
        .execute(&app.db)
        .await
        .unwrap();

    // A cross-domain contest is exactly the one that wants the widest field.
    // Hiding it from the design page would be the opposite of the intent.
    let body: Value = app
        .get("/api/tournaments?skill_domain=design")
        .await
        .json()
        .await
        .unwrap();
    let slugs: Vec<&str> = body["data"]["tournaments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["slug"].as_str().unwrap())
        .collect();
    assert!(slugs.contains(&"brief-open"), "{slugs:?}");
}

#[tokio::test]
async fn a_podium_names_the_people_on_it() {
    let app = TestApp::spawn().await;
    app.register_user("podium_first").await;
    app.register_user("podium_second").await;
    let first = user_id(&app, "podium_first").await;
    let second = user_id(&app, "podium_second").await;

    let contest = a_contest(
        &app,
        "brief-podium",
        "brief_contest",
        json!({"brief": a_brief()}),
    )
    .await;
    enter(&app, contest, first).await;
    enter(&app, contest, second).await;
    sqlx::query(
        "UPDATE tournament_participants SET rank = CASE WHEN participant_id = $2 THEN 1 ELSE 2 END,
                                            score = CASE WHEN participant_id = $2 THEN 90 ELSE 70 END
          WHERE tournament_id = $1",
    )
    .bind(contest)
    .bind(first)
    .execute(&app.db)
    .await
    .unwrap();

    // A ranking that can only print UUIDs is not a podium.
    let body: Value = app
        .get("/api/tournaments/brief-podium/leaderboard")
        .await
        .json()
        .await
        .unwrap();
    let rows = body["data"]["leaderboard"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["username"], "podium_first");
    assert_eq!(rows[0]["rank"], 1);
    assert!(rows[0]["display_name"].is_string());
    assert_eq!(rows[1]["username"], "podium_second");
}

#[tokio::test]
async fn a_deleted_account_leaves_a_nameless_line_not_a_hole() {
    let app = TestApp::spawn().await;
    app.register_user("podium_ghost").await;
    app.register_user("podium_stays").await;
    let ghost = user_id(&app, "podium_ghost").await;
    let stays = user_id(&app, "podium_stays").await;

    let contest = a_contest(
        &app,
        "brief-ghost",
        "brief_contest",
        json!({"brief": a_brief()}),
    )
    .await;
    enter(&app, contest, ghost).await;
    enter(&app, contest, stays).await;

    // The participation outlives the account on purpose: removing the line
    // would rewrite the standing of everybody ranked below it.
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(ghost)
        .execute(&app.db)
        .await
        .unwrap();

    let resp = app.get("/api/tournaments/brief-ghost/leaderboard").await;
    assert_eq!(resp.status().as_u16(), 200);
    let body: Value = resp.json().await.unwrap();
    let rows = body["data"]["leaderboard"].as_array().unwrap();
    assert_eq!(rows.len(), 2, "the ranking keeps its shape");
    let nameless = rows
        .iter()
        .find(|r| r["participant_id"] == json!(ghost.to_string()))
        .expect("the line is still there");
    assert!(nameless["username"].is_null());
    assert!(nameless["display_name"].is_null());
    let _ = stays;
}

// ═══════════════════════════════════════════════════════════════════
// The blind submission window
// ═══════════════════════════════════════════════════════════════════

/// Enter a contest properly, through the endpoint, so the submission row
/// exists the way a real entry would.
async fn submit(app: &TestApp, slug: &str, username: &str, n: u32) {
    app.login(username).await;
    let resp = app
        .post(&format!("/api/tournaments/{slug}/submissions"), &an_entry(n))
        .await;
    assert!(resp.status().is_success(), "{:?}", resp.text().await);
}

#[tokio::test]
async fn a_blind_window_shows_an_entrant_their_own_work_and_nobody_elses() {
    let app = TestApp::spawn().await;
    app.register_user("blind_one").await;
    app.register_user("blind_two").await;
    let one = user_id(&app, "blind_one").await;
    let two = user_id(&app, "blind_two").await;

    let contest = a_contest(
        &app,
        "brief-blind",
        "brief_contest",
        json!({"brief": a_brief()}),
    )
    .await;
    sqlx::query("UPDATE tournaments SET blind_until_close = TRUE WHERE id = $1")
        .bind(contest)
        .execute(&app.db)
        .await
        .unwrap();
    enter(&app, contest, one).await;
    enter(&app, contest, two).await;
    submit(&app, "brief-blind", "blind_one", 1).await;
    submit(&app, "brief-blind", "blind_two", 2).await;

    // Mimicry is the format's known failure: the first strong answer pulls
    // every later one towards it, and it is invisible in the result.
    app.login("blind_one").await;
    let body: Value = app
        .get("/api/tournaments/brief-blind/submissions")
        .await
        .json()
        .await
        .unwrap();
    let rows = body["data"]["submissions"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "only their own");
    assert_eq!(rows[0]["participant_id"], json!(one.to_string()));

    // And the reader is told, because a gallery showing one entry without
    // saying the rest are withheld reads as a contest nobody entered.
    assert_eq!(body["data"]["blinded"], json!(true));
    assert!(body["data"]["blind_until"].is_string());
}

#[tokio::test]
async fn the_panel_is_never_blinded() {
    let app = TestApp::spawn().await;
    app.register_user("blind_juror").await;
    app.register_user("blind_entrant").await;
    let juror = user_id(&app, "blind_juror").await;
    let entrant = user_id(&app, "blind_entrant").await;
    grant(&app, juror, "jury_tournament").await;

    let contest = a_contest(
        &app,
        "brief-blind-jury",
        "brief_contest",
        json!({"brief": a_brief()}),
    )
    .await;
    sqlx::query("UPDATE tournaments SET blind_until_close = TRUE WHERE id = $1")
        .bind(contest)
        .execute(&app.db)
        .await
        .unwrap();
    enter(&app, contest, entrant).await;
    submit(&app, "brief-blind-jury", "blind_entrant", 3).await;

    // Blinding the panel too would not be a flag, it would be a different
    // contest calendar — judging could only start after the deadline.
    sqlx::query(
        "INSERT INTO tournament_juries (tournament_id, juror_user_id, accepted_at)
         VALUES ($1, $2, NOW())",
    )
    .bind(contest)
    .bind(juror)
    .execute(&app.db)
    .await
    .unwrap();

    app.login("blind_juror").await;
    let body: Value = app
        .get("/api/tournaments/brief-blind-jury/submissions")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["submissions"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["blinded"], json!(false));
}

#[tokio::test]
async fn the_field_opens_at_the_deadline_and_stays_open() {
    let app = TestApp::spawn().await;
    app.register_user("blind_closer").await;
    let entrant = user_id(&app, "blind_closer").await;

    let contest = a_contest(
        &app,
        "brief-blind-closed",
        "brief_contest",
        json!({"brief": a_brief()}),
    )
    .await;
    sqlx::query("UPDATE tournaments SET blind_until_close = TRUE WHERE id = $1")
        .bind(contest)
        .execute(&app.db)
        .await
        .unwrap();
    enter(&app, contest, entrant).await;
    submit(&app, "brief-blind-closed", "blind_closer", 4).await;

    // A result nobody can check against the whole field is not a result. The
    // window narrows *when*, never *whether* — including for a reader who
    // never had an account.
    sqlx::query("UPDATE tournaments SET status = 'concluded' WHERE id = $1")
        .bind(contest)
        .execute(&app.db)
        .await
        .unwrap();

    let body: Value = reqwest::Client::new()
        .get(format!(
            "{}/api/tournaments/brief-blind-closed/submissions",
            app.addr
        ))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["submissions"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["blinded"], json!(false));
}

#[tokio::test]
async fn a_contest_that_did_not_ask_for_blindness_is_public_throughout() {
    let app = TestApp::spawn().await;
    app.register_user("open_entrant").await;
    let entrant = user_id(&app, "open_entrant").await;

    let contest = a_contest(
        &app,
        "brief-open-field",
        "brief_contest",
        json!({"brief": a_brief()}),
    )
    .await;
    enter(&app, contest, entrant).await;
    submit(&app, "brief-open-field", "open_entrant", 5).await;

    // The default is unchanged by migration 0418: nothing becomes less
    // readable because the option now exists.
    let body: Value = reqwest::Client::new()
        .get(format!(
            "{}/api/tournaments/brief-open-field/submissions",
            app.addr
        ))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["data"]["submissions"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["blinded"], json!(false));
}
