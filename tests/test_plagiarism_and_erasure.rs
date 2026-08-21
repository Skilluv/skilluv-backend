//! Accusing somebody of copying, and erasing somebody who leaves.
//!
//! Two tickets, one suite, because they share a principle: a record that
//! other people rely on is not the accused's to destroy, and not the
//! platform's to destroy either.
//!
//! - A disqualified entry is **marked**, never deleted: the other entrants
//!   moved up, and a ranking whose gaps are unexplained is a ranking nobody
//!   can check.
//! - An erased account leaves a **tombstone** for the same reason.

mod common;
use bigdecimal::BigDecimal;
use common::TestApp;
use serde_json::{Value, json};
use skilluv_backend::services::ledger::{Currency, State};
use uuid::Uuid;

/// What a tombstone looks like once the personal data is gone.
type Tombstone = (
    String,                                // username
    String,                                // display name
    String,                                // e-mail
    Option<String>,                        // bio
    Option<String>,                        // avatar
    Option<chrono::DateTime<chrono::Utc>>, // deleted_at
    bool,                                  // profile_active
    bool,                                  // profile_hidden
);

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
         VALUES ($1, $2, 'test_setup') ON CONFLICT DO NOTHING",
    )
    .bind(user)
    .bind(capability)
    .execute(&app.db)
    .await
    .unwrap();
}

/// A concluded contest with one entry by `author`.
async fn an_entry(app: &TestApp, slug: &str, author: Uuid) -> Uuid {
    let tournament: Uuid = sqlx::query_scalar(
        "INSERT INTO tournaments (slug, name, skill_domain, kind, format, status,
                                  starts_at, ends_at)
         VALUES ($1, 'Concours', 'design', 'individual', 'ladder', 'active',
                 NOW() - INTERVAL '2 days', NOW() + INTERVAL '5 days')
         RETURNING id",
    )
    .bind(slug)
    .fetch_one(&app.db)
    .await
    .unwrap();

    // A trigger refuses a submission from somebody who never registered —
    // correctly, since an entry from a non-entrant is an entry in nothing.
    sqlx::query(
        "INSERT INTO tournament_participants
             (tournament_id, participant_type, participant_id)
         VALUES ($1, 'user', $2)",
    )
    .bind(tournament)
    .bind(author)
    .execute(&app.db)
    .await
    .unwrap();

    sqlx::query_scalar(
        "INSERT INTO tournament_submissions
             (tournament_id, participant_type, participant_id, submitted_by,
              artifact_url, artifact_type, summary)
         VALUES ($1, 'user', $2, $2, 'https://figma.test/entry', 'design_file',
                 'Une identité pour une coopérative.')
         RETURNING id",
    )
    .bind(tournament)
    .bind(author)
    .fetch_one(&app.db)
    .await
    .unwrap()
}

const A_REAL_ACCUSATION: &str = "Ce logotype reprend trait pour trait celui publié en 2023 par \
                                 l'atelier Kouassi, jusqu'à la contreforme du A. La version \
                                 originale est datée et horodatée sur le lien fourni.";

const A_REAL_DECISION: &str = "Les deux fichiers ont été comparés : la construction du A et les \
                               espacements sont identiques au pixel près, et l'antériorité de \
                               l'original est établie par l'horodatage.";

// ═══════════════════════════════════════════════════════════════════
// Accusing, answering, deciding
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn an_accusation_nobody_can_answer_is_refused() {
    let app = TestApp::spawn().await;
    app.register_user("pl_author").await;
    app.register_user("pl_accuser").await;
    let author = user_id(&app, "pl_author").await;
    let entry = an_entry(&app, "pl-short", author).await;

    app.login("pl_accuser").await;

    // "C'est copié" gives the person three days to work out what they are
    // answering, and nothing to answer it with.
    let short = app
        .post(
            &format!("/api/contests/submissions/{entry}/flag"),
            &json!({"reason_md": "C'est copié.", "evidence_url": "https://exemple.test/original"}),
        )
        .await;
    assert_eq!(short.status(), 400);

    // And an accusation with no link to the original cannot be checked by
    // anybody, the reviewer included.
    let no_link = app
        .post(
            &format!("/api/contests/submissions/{entry}/flag"),
            &json!({"reason_md": A_REAL_ACCUSATION, "evidence_url": "pas-un-lien"}),
        )
        .await;
    assert_eq!(no_link.status(), 400);
}

#[tokio::test]
async fn anybody_may_accuse_but_not_their_own_entry() {
    let app = TestApp::spawn().await;
    app.register_user("pl_self").await;
    let author = user_id(&app, "pl_self").await;
    let entry = an_entry(&app, "pl-self", author).await;

    app.login("pl_self").await;
    // Allowing it gives a losing entrant a way to withdraw while blaming the
    // process.
    let resp = app
        .post(
            &format!("/api/contests/submissions/{entry}/flag"),
            &json!({"reason_md": A_REAL_ACCUSATION, "evidence_url": "https://exemple.test/o"}),
        )
        .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn one_open_case_per_entry() {
    let app = TestApp::spawn().await;
    app.register_user("pl_twice_author").await;
    app.register_user("pl_twice_a").await;
    app.register_user("pl_twice_b").await;
    let author = user_id(&app, "pl_twice_author").await;
    let entry = an_entry(&app, "pl-twice", author).await;

    let body = json!({
        "reason_md": A_REAL_ACCUSATION,
        "evidence_url": "https://exemple.test/original",
    });

    app.login("pl_twice_a").await;
    assert_eq!(
        app.post(&format!("/api/contests/submissions/{entry}/flag"), &body)
            .await
            .status(),
        201
    );

    // A second accusation while the first is undecided splits the evidence
    // across two files and gives the accused two clocks.
    app.login("pl_twice_b").await;
    assert_eq!(
        app.post(&format!("/api/contests/submissions/{entry}/flag"), &body)
            .await
            .status(),
        409
    );
}

#[tokio::test]
async fn the_accused_reads_the_case_and_answers_it() {
    let app = TestApp::spawn().await;
    app.register_user("pl_answer_author").await;
    app.register_user("pl_answer_accuser").await;
    app.register_user("pl_answer_stranger").await;
    let author = user_id(&app, "pl_answer_author").await;
    let entry = an_entry(&app, "pl-answer", author).await;

    app.login("pl_answer_accuser").await;
    let opened: Value = app
        .post(
            &format!("/api/contests/submissions/{entry}/flag"),
            &json!({
                "reason_md": A_REAL_ACCUSATION,
                "evidence_url": "https://exemple.test/original",
            }),
        )
        .await
        .json()
        .await
        .unwrap();
    let case = opened["data"]["id"].as_str().unwrap().to_string();

    // An open accusation is an allegation. Publishing allegations before they
    // are decided is how a dismissed case still ruins somebody.
    app.login("pl_answer_stranger").await;
    assert_eq!(
        app.get(&format!("/api/contests/plagiarism/{case}"))
            .await
            .status(),
        403
    );

    app.login("pl_answer_author").await;
    assert_eq!(
        app.get(&format!("/api/contests/plagiarism/{case}"))
            .await
            .status(),
        200
    );

    let answered: Value = app
        .post(
            &format!("/api/contests/plagiarism/{case}/respond"),
            &json!({"response_md": "Le fichier source est daté de 2022, voici l'historique."}),
        )
        .await
        .json()
        .await
        .unwrap();
    assert!(answered["data"]["responded_at"].is_string(), "{answered}");
    assert_eq!(
        answered["data"]["status"], "open",
        "answering decides nothing"
    );
}

#[tokio::test]
async fn only_a_reviewer_decides_and_the_decision_has_to_say_something() {
    let app = TestApp::spawn().await;
    app.register_user("pl_dec_author").await;
    app.register_user("pl_dec_accuser").await;
    let author = user_id(&app, "pl_dec_author").await;
    let entry = an_entry(&app, "pl-dec", author).await;

    app.login("pl_dec_accuser").await;
    let opened: Value = app
        .post(
            &format!("/api/contests/submissions/{entry}/flag"),
            &json!({
                "reason_md": A_REAL_ACCUSATION,
                "evidence_url": "https://exemple.test/original",
            }),
        )
        .await
        .json()
        .await
        .unwrap();
    let case = opened["data"]["id"].as_str().unwrap().to_string();

    // The accuser is not the judge.
    assert_eq!(
        app.post(
            &format!("/api/admin/plagiarism/{case}/decide"),
            &json!({"upheld": true, "decision_md": A_REAL_DECISION}),
        )
        .await
        .status(),
        403
    );

    app.register_user("pl_reviewer").await;
    let reviewer = user_id(&app, "pl_reviewer").await;
    grant(&app, reviewer, "plagiarism_reviewer").await;
    app.login("pl_reviewer").await;

    // Dismissing without a word leaves the accusation standing in everybody's
    // memory, so the floor is the same in both directions.
    assert_eq!(
        app.post(
            &format!("/api/admin/plagiarism/{case}/decide"),
            &json!({"upheld": false, "decision_md": "Non."}),
        )
        .await
        .status(),
        400
    );
}

#[tokio::test]
async fn an_upheld_case_marks_the_entry_and_never_deletes_it() {
    let app = TestApp::spawn().await;
    app.register_user("pl_up_author").await;
    app.register_user("pl_up_accuser").await;
    let author = user_id(&app, "pl_up_author").await;
    let entry = an_entry(&app, "pl-up", author).await;

    app.login("pl_up_accuser").await;
    let opened: Value = app
        .post(
            &format!("/api/contests/submissions/{entry}/flag"),
            &json!({
                "reason_md": A_REAL_ACCUSATION,
                "evidence_url": "https://exemple.test/original",
            }),
        )
        .await
        .json()
        .await
        .unwrap();
    let case = opened["data"]["id"].as_str().unwrap().to_string();

    app.register_admin("pl_up_admin").await;
    app.login("pl_up_admin").await;
    let decided = app
        .post(
            &format!("/api/admin/plagiarism/{case}/decide"),
            &json!({"upheld": true, "decision_md": A_REAL_DECISION}),
        )
        .await;
    assert_eq!(decided.status(), 200, "{}", decided.text().await.unwrap());

    let (status, notes): (String, Option<String>) =
        sqlx::query_as("SELECT status, judge_notes FROM tournament_submissions WHERE id = $1")
            .bind(entry)
            .fetch_one(&app.db)
            .await
            .unwrap();

    // Marked, not deleted: the other entrants moved up, and a ranking whose
    // gaps are unexplained is a ranking nobody can check. And the marking
    // carries its reason.
    assert_eq!(status, "disqualified");
    assert!(notes.unwrap().contains("Plagiat retenu"));

    // Deciding twice would re-open a decision that has already cost somebody
    // a placing.
    assert_eq!(
        app.post(
            &format!("/api/admin/plagiarism/{case}/decide"),
            &json!({"upheld": false, "decision_md": A_REAL_DECISION}),
        )
        .await
        .status(),
        409
    );
}

#[tokio::test]
async fn a_dismissed_case_leaves_the_entry_alone() {
    let app = TestApp::spawn().await;
    app.register_user("pl_dis_author").await;
    app.register_user("pl_dis_accuser").await;
    let author = user_id(&app, "pl_dis_author").await;
    let entry = an_entry(&app, "pl-dis", author).await;

    app.login("pl_dis_accuser").await;
    let opened: Value = app
        .post(
            &format!("/api/contests/submissions/{entry}/flag"),
            &json!({
                "reason_md": A_REAL_ACCUSATION,
                "evidence_url": "https://exemple.test/original",
            }),
        )
        .await
        .json()
        .await
        .unwrap();
    let case = opened["data"]["id"].as_str().unwrap().to_string();

    app.register_admin("pl_dis_admin").await;
    app.login("pl_dis_admin").await;
    app.post(
        &format!("/api/admin/plagiarism/{case}/decide"),
        &json!({"upheld": false, "decision_md": A_REAL_DECISION}),
    )
    .await;

    let status: String =
        sqlx::query_scalar("SELECT status FROM tournament_submissions WHERE id = $1")
            .bind(entry)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(status, "submitted");
}

#[tokio::test]
async fn nobody_is_banned_by_a_second_strike() {
    let app = TestApp::spawn().await;
    app.register_user("pl_strike_author").await;
    app.register_user("pl_strike_accuser").await;
    let author = user_id(&app, "pl_strike_author").await;
    app.register_admin("pl_strike_admin").await;

    for n in 1..=2 {
        let entry = an_entry(&app, &format!("pl-strike-{n}"), author).await;
        app.login("pl_strike_accuser").await;
        let opened: Value = app
            .post(
                &format!("/api/contests/submissions/{entry}/flag"),
                &json!({
                    "reason_md": A_REAL_ACCUSATION,
                    "evidence_url": "https://exemple.test/original",
                }),
            )
            .await
            .json()
            .await
            .unwrap();
        let case = opened["data"]["id"].as_str().unwrap().to_string();

        app.login("pl_strike_admin").await;
        app.post(
            &format!("/api/admin/plagiarism/{case}/decide"),
            &json!({"upheld": true, "decision_md": A_REAL_DECISION}),
        )
        .await;
    }

    // The count is surfaced so a human can see it. Banning on it would ban
    // somebody, one Tuesday, on an accusation a tired reviewer upheld in four
    // minutes — so the ban stays a decision a human takes and signs.
    let banned: bool = sqlx::query_scalar("SELECT is_banned FROM users WHERE id = $1")
        .bind(author)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert!(!banned, "a strike count is not a sentence");

    let upheld: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM plagiarism_cases WHERE accused_id = $1 AND status = 'upheld'",
    )
    .bind(author)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(upheld, 2);
}

// ═══════════════════════════════════════════════════════════════════
// Erasure
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn erasure_leaves_a_tombstone_with_nothing_personal_on_it() {
    let app = TestApp::spawn().await;
    app.register_user("er_leaver").await;
    let user = user_id(&app, "er_leaver").await;
    sqlx::query("UPDATE users SET bio = 'Je fais des logotypes.', avatar_url = 'https://a.test/a.png' WHERE id = $1")
        .bind(user)
        .execute(&app.db)
        .await
        .unwrap();

    skilluv_backend::services::erasure::erase(&app.db, user)
        .await
        .expect("erased");

    let (username, display, email, bio, avatar, deleted, active, hidden): Tombstone =
        sqlx::query_as(
            "SELECT username, display_name, email, bio, avatar_url, deleted_at,
                profile_active, profile_hidden
           FROM users WHERE id = $1",
        )
        .bind(user)
        .fetch_one(&app.db)
        .await
        .unwrap();

    // The row survives so that everything pointing at it still points
    // somewhere. Everything it said about the person is gone.
    assert!(username.starts_with("supprime-"), "{username}");
    assert_eq!(display, "Compte supprimé");
    assert!(
        email.ends_with("@invalid"),
        "a stray mailer must reach nobody"
    );
    assert!(bio.is_none());
    assert!(avatar.is_none());
    assert!(deleted.is_some());
    assert!(!active);
    assert!(hidden);
}

#[tokio::test]
async fn erasure_keeps_what_other_people_rely_on() {
    let app = TestApp::spawn().await;
    app.register_user("er_entrant").await;
    let user = user_id(&app, "er_entrant").await;
    let entry = an_entry(&app, "er-contest", user).await;

    skilluv_backend::services::erasure::erase(&app.db, user)
        .await
        .unwrap();

    // A contest where the second place vanished leaves first and third
    // unexplained, and the winner's own attestation cites a ranking that no
    // longer adds up.
    let survives: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM tournament_submissions WHERE id = $1)")
            .bind(entry)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(survives, "the entry was taken with the account");
}

#[tokio::test]
async fn erasure_takes_the_tokens_with_it() {
    let app = TestApp::spawn().await;
    app.register_user("er_connected").await;
    let user = user_id(&app, "er_connected").await;

    sqlx::query(
        "INSERT INTO design_cloud_connections
             (user_id, provider, access_token_ciphertext, access_token_nonce, scopes)
         VALUES ($1, 'figma', '\\xdeadbeef'::BYTEA, '\\x02'::BYTEA, ARRAY['file_read'])",
    )
    .bind(user)
    .execute(&app.db)
    .await
    .unwrap();

    skilluv_backend::services::erasure::erase(&app.db, user)
        .await
        .unwrap();

    // Leaving one behind would leave Skilluv able to read somebody's Figma
    // after they left.
    let left: i64 =
        sqlx::query_scalar("SELECT count(*) FROM design_cloud_connections WHERE user_id = $1")
            .bind(user)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(left, 0);
}

#[tokio::test]
async fn erasing_twice_changes_nothing() {
    let app = TestApp::spawn().await;
    app.register_user("er_twice").await;
    let user = user_id(&app, "er_twice").await;

    assert!(
        skilluv_backend::services::erasure::erase(&app.db, user)
            .await
            .unwrap()
    );
    let name_after_first: String = sqlx::query_scalar("SELECT username FROM users WHERE id = $1")
        .bind(user)
        .fetch_one(&app.db)
        .await
        .unwrap();

    // A retried request must not produce a second tombstone.
    assert!(
        !skilluv_backend::services::erasure::erase(&app.db, user)
            .await
            .unwrap()
    );
    let name_after_second: String = sqlx::query_scalar("SELECT username FROM users WHERE id = $1")
        .bind(user)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(name_after_first, name_after_second);
}

#[tokio::test]
async fn the_export_carries_the_design_half() {
    let app = TestApp::spawn().await;
    app.register_user("er_exported").await;
    let user = user_id(&app, "er_exported").await;

    // An export that stopped at challenges told somebody less than the
    // platform knows about them, which is the one thing an export may not do.
    for table in [
        "user_domain_profiles",
        "external_signals",
        "tournament_submissions",
        "mission_ratings",
        "plagiarism_cases",
        "design_cloud_connections",
    ] {
        let exists: bool = sqlx::query_scalar("SELECT to_regclass($1) IS NOT NULL")
            .bind(table)
            .fetch_one(&app.db)
            .await
            .unwrap();
        assert!(exists, "{table} is exported but does not exist");
    }

    // And the tokens are not in it: a live credential in a zip that travels
    // by e-mail is worse than not exporting it at all.
    let source = include_str!("../src/services/data_export.rs");
    assert!(
        source.contains("fetch_connections"),
        "connections are exported through their own projection"
    );
    assert!(
        !source.contains("fetch_table(&db, \"design_cloud_connections\""),
        "design_cloud_connections must not go through SELECT *"
    );

    let _ = user;
}

/// Fund a contest and pay its single finisher the whole pool.
///
/// The state a contest is in when somebody looks at the winning entry and
/// recognises it: money in, ranks written, prize in the winner's pending
/// balance and not yet withdrawable.
async fn a_paid_contest(app: &TestApp, tournament: Uuid, winner: Uuid) {
    let enterprise: Uuid = sqlx::query_scalar(
        "INSERT INTO enterprises (owner_id, company_name, slug, company_size)
         VALUES ($1, 'Mécène test', 'ent-' || substr($2::text, 1, 8), '11-50')
         RETURNING id",
    )
    .bind(winner)
    .bind(tournament)
    .fetch_one(&app.db)
    .await
    .unwrap();

    // `fund` writes `prize_cash_amount` and `prize_cash_currency` itself; the
    // contest only has to be concluded, because a prize is awarded from a
    // ranking and `award` refuses one that has none.
    skilluv_backend::services::contest_prizes::fund(
        &app.db,
        tournament,
        enterprise,
        BigDecimal::from(900),
        Currency::Eur,
        "stripe",
        format!("pi_test_{tournament}"),
    )
    .await
    .expect("fund");

    sqlx::query("UPDATE tournament_participants SET rank = 1 WHERE tournament_id = $1")
        .bind(tournament)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("UPDATE tournaments SET status = 'concluded' WHERE id = $1")
        .bind(tournament)
        .execute(&app.db)
        .await
        .unwrap();

    skilluv_backend::services::contest_prizes::award(&app.db, tournament)
        .await
        .expect("award");
}

async fn pending(app: &TestApp, user: Uuid) -> BigDecimal {
    skilluv_backend::services::ledger::user_balance(&app.db, user, State::Pending, Currency::Eur)
        .await
        .unwrap()
}

async fn tournament_of(app: &TestApp, submission: Uuid) -> Uuid {
    sqlx::query_scalar("SELECT tournament_id FROM tournament_submissions WHERE id = $1")
        .bind(submission)
        .fetch_one(&app.db)
        .await
        .unwrap()
}

/// Upholding a case takes the prize back.
///
/// `contest_prizes::award` puts a prize into `pending` rather than `available`
/// and says why in as many words: "the release window is what makes a
/// contested result recoverable". Upholding a plagiarism case is the only
/// thing that ever contests one, and it disqualified the entry and left the
/// money — so a contest could hold, in one person, a winner who was
/// disqualified and a winner who was paid.
#[tokio::test]
async fn upholding_a_case_takes_the_prize_back() {
    let app = TestApp::spawn().await;
    app.register_user("pl_prize_author").await;
    app.register_user("pl_prize_accuser").await;
    let author = user_id(&app, "pl_prize_author").await;
    let entry = an_entry(&app, "pl-prize", author).await;
    let tournament = tournament_of(&app, entry).await;

    a_paid_contest(&app, tournament, author).await;
    // Half the pool, not all of it: the split is 50/30/20 and one finisher
    // takes first place only. The other 450 went back to the sponsor when the
    // prize was awarded, because inventing a redistribution would pay somebody
    // more than the contest promised.
    assert_eq!(pending(&app, author).await, BigDecimal::from(450));

    app.login("pl_prize_accuser").await;
    let opened: Value = app
        .post(
            &format!("/api/contests/submissions/{entry}/flag"),
            &json!({
                "reason_md": A_REAL_ACCUSATION,
                "evidence_url": "https://exemple.test/original",
            }),
        )
        .await
        .json()
        .await
        .unwrap();
    let case = opened["data"]["id"].as_str().unwrap().to_string();

    app.register_admin("pl_prize_admin").await;
    app.login("pl_prize_admin").await;
    let decided = app
        .post(
            &format!("/api/admin/plagiarism/{case}/decide"),
            &json!({"upheld": true, "decision_md": A_REAL_DECISION}),
        )
        .await;
    assert_eq!(decided.status(), 200, "{}", decided.text().await.unwrap());

    // Nothing owed to the author any more.
    assert_eq!(pending(&app, author).await, BigDecimal::from(0));

    // And the money is back in the pot rather than vanished. The books balance
    // either way; "balanced" and "somewhere somebody can decide about it" are
    // not the same thing. Back to the escrow rather than to the sponsor or the
    // runner-up, because both of those are decisions and neither belongs in a
    // function nobody is reading.
    // `ledger_balance` is the raw signed figure and an escrow is a liability,
    // so money held reads negative — the same convention `ledger_user_balance`
    // hides for user accounts. Negated here rather than asserted at -450,
    // because a reader should not have to know that to read the test.
    let escrow: BigDecimal = sqlx::query_scalar("SELECT -ledger_balance($1)")
        .bind(format!("escrow:tournament:{tournament}:EUR"))
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(escrow, BigDecimal::from(450));
}

/// Dismissing one leaves the prize alone.
///
/// The complement, and the one that would break first if the confiscation
/// were wired to the decision rather than to its outcome.
#[tokio::test]
async fn dismissing_a_case_leaves_the_prize_alone() {
    let app = TestApp::spawn().await;
    app.register_user("pl_keep_author").await;
    app.register_user("pl_keep_accuser").await;
    let author = user_id(&app, "pl_keep_author").await;
    let entry = an_entry(&app, "pl-keep", author).await;
    let tournament = tournament_of(&app, entry).await;

    a_paid_contest(&app, tournament, author).await;

    app.login("pl_keep_accuser").await;
    let opened: Value = app
        .post(
            &format!("/api/contests/submissions/{entry}/flag"),
            &json!({
                "reason_md": A_REAL_ACCUSATION,
                "evidence_url": "https://exemple.test/original",
            }),
        )
        .await
        .json()
        .await
        .unwrap();
    let case = opened["data"]["id"].as_str().unwrap().to_string();

    app.register_admin("pl_keep_admin").await;
    app.login("pl_keep_admin").await;
    let decided = app
        .post(
            &format!("/api/admin/plagiarism/{case}/decide"),
            &json!({"upheld": false, "decision_md": A_REAL_DECISION}),
        )
        .await;
    assert_eq!(decided.status(), 200, "{}", decided.text().await.unwrap());

    assert_eq!(pending(&app, author).await, BigDecimal::from(450));
}
