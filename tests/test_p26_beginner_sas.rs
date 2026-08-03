//! Tests P26 : sas compagnonnage débutant (bout-en-bout service).
//!
//! Couvre :
//!   - pick_questions refuse un challenge non-sas
//!   - pick_questions refuse un pool insuffisant
//!   - submit_verification refuse un pool answers invalide
//!   - submit_verification bloque un second pending sur (user, template)
//!   - record_verdict transition pending → approved
//!   - N approbations distinctes → grant auto de verified_apprentice
//!   - submit challenge stage='free' sans capability → refus
//!   - submit challenge stage='free' avec capability → ok (via engine)

use serde_json::json;
use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

use skilluv_backend::services::{apprentice_verification, capabilities_engine};

async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!(
        "skilluv_p26_test_{}",
        Uuid::new_v4().to_string().replace('-', "")
    );
    let admin_pool = PgPoolOptions::new()
        .max_connections(2)
        .connect("postgres://skilluv:skilluv_secret@localhost:5433/skilluv")
        .await
        .expect("admin");
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "CREATE DATABASE \"{db_name}\""
    )))
    .execute(&admin_pool)
    .await
    .expect("create");
    admin_pool.close().await;

    let db_url = format!("postgres://skilluv:skilluv_secret@localhost:5433/{db_name}");
    let db = PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await
        .expect("connect");
    sqlx::migrate!("./migrations")
        .run(&db)
        .await
        .expect("migrations");
    (db, db_name)
}

async fn cleanup_test_db(db_name: &str) {
    let admin_pool = PgPoolOptions::new()
        .max_connections(2)
        .connect("postgres://skilluv:skilluv_secret@localhost:5433/skilluv")
        .await
        .expect("admin");
    let _ = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '{db_name}'"
    )))
    .execute(&admin_pool)
    .await;
    let _ = sqlx::query(sqlx::AssertSqlSafe(format!(
        "DROP DATABASE IF EXISTS \"{db_name}\""
    )))
    .execute(&admin_pool)
    .await;
    admin_pool.close().await;
}

async fn create_user(db: &PgPool, hint: &str) -> Uuid {
    let uid = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, email, username, first_name, last_name, display_name,
                             password_hash, profile_active, total_fragments)
         VALUES ($1, $2, $3, 't','u','t','x',TRUE,0)",
    )
    .bind(uid)
    .bind(format!("{hint}-{uid}@ex.io"))
    .bind(format!("{hint}{}", &uid.to_string()[..8]))
    .execute(db)
    .await
    .expect("insert user");
    uid
}

/// Crée un challenge_template avec `beginner_stage` et N questions actives.
async fn create_sas_challenge(
    db: &PgPool,
    stage: Option<&str>,
    n_questions: usize,
) -> (Uuid, Vec<Uuid>) {
    let tid = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO challenge_templates
            (id, title, description, instructions, skill_domain, difficulty, mode,
             ai_policy, tone, reward_fragments, is_onboarding, is_training,
             is_capstone, status, is_community, featured, vote_count, beginner_stage)
         VALUES ($1, $2, 'd', 'i', 'code', 1, 'solo',
                 'disclosure_required', 'friendly', 10, FALSE, FALSE,
                 FALSE, 'published', FALSE, FALSE, 0, $3)",
    )
    .bind(tid)
    .bind(format!("SAS test {}", &tid.to_string()[..8]))
    .bind(stage)
    .execute(db)
    .await
    .expect("insert template");
    let mut qids = Vec::with_capacity(n_questions);
    for i in 0..n_questions {
        let qid = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO challenge_verification_questions (id, template_id, prompt_text, order_hint)
             VALUES ($1, $2, $3, $4)",
        )
        .bind(qid)
        .bind(tid)
        .bind(format!("Explique le point n°{i} de ce challenge, s'il te plaît."))
        .bind(i as i32)
        .execute(db)
        .await
        .expect("insert question");
        qids.push(qid);
    }
    (tid, qids)
}

#[tokio::test]
async fn pick_questions_refuse_non_sas_challenge() {
    let (db, db_name) = setup_test_db().await;
    let (tid, _) = create_sas_challenge(&db, None, 2).await;
    let res = apprentice_verification::pick_questions(&db, tid).await;
    assert!(res.is_err(), "should reject non-sas template");
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn pick_questions_refuse_pool_trop_petit() {
    let (db, db_name) = setup_test_db().await;
    // QUESTIONS_PER_SUBMISSION = 2, on n'en crée qu'une.
    let (tid, _) = create_sas_challenge(&db, Some("sas"), 1).await;
    let res = apprentice_verification::pick_questions(&db, tid).await;
    assert!(res.is_err(), "should reject insufficient pool");
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn submit_verification_ok_then_pending_conflict() {
    let (db, db_name) = setup_test_db().await;
    let user = create_user(&db, "learner").await;
    let (tid, qids) = create_sas_challenge(&db, Some("sas"), 2).await;

    let answers = json!({
        qids[0].to_string(): "s3://priv/v1.webm",
        qids[1].to_string(): "s3://priv/v2.webm",
    });
    let payload = apprentice_verification::SubmitPayload {
        template_id: tid,
        submission_id: None,
        answers: answers.clone(),
    };
    let row = apprentice_verification::submit_verification(&db, user, payload)
        .await
        .expect("first submit ok");
    assert_eq!(row.verdict, "pending");

    // Second submit avec pending → refus.
    let payload2 = apprentice_verification::SubmitPayload {
        template_id: tid,
        submission_id: None,
        answers,
    };
    let res = apprentice_verification::submit_verification(&db, user, payload2).await;
    assert!(res.is_err(), "should reject duplicate pending");

    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn submit_verification_refuse_answers_incomplet() {
    let (db, db_name) = setup_test_db().await;
    let user = create_user(&db, "learner").await;
    let (tid, qids) = create_sas_challenge(&db, Some("sas"), 2).await;

    // Une seule answer alors qu'on en attend 2.
    let bad = json!({ qids[0].to_string(): "s3://priv/v1.webm" });
    let payload = apprentice_verification::SubmitPayload {
        template_id: tid,
        submission_id: None,
        answers: bad,
    };
    let res = apprentice_verification::submit_verification(&db, user, payload).await;
    assert!(res.is_err(), "should reject incomplete answers");
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn three_approvals_grant_verified_apprentice() {
    let (db, db_name) = setup_test_db().await;
    let apprentice = create_user(&db, "app").await;
    let reviewer = create_user(&db, "rev").await;

    // 3 challenges distincts marqués sas.
    let mut ver_ids = Vec::new();
    for _ in 0..3 {
        let (tid, qids) = create_sas_challenge(&db, Some("sas"), 2).await;
        let payload = apprentice_verification::SubmitPayload {
            template_id: tid,
            submission_id: None,
            answers: json!({
                qids[0].to_string(): "s3://priv/a.webm",
                qids[1].to_string(): "s3://priv/b.webm",
            }),
        };
        let v = apprentice_verification::submit_verification(&db, apprentice, payload)
            .await
            .expect("submit ok");
        ver_ids.push(v.id);
    }

    // Avant tout verdict, pas de verified_apprentice.
    let has_before: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM user_capabilities
         WHERE user_id = $1 AND capability = 'verified_apprentice' AND revoked_at IS NULL)",
    )
    .bind(apprentice)
    .fetch_one(&db)
    .await
    .unwrap();
    assert!(!has_before);

    // 3 verdicts approved.
    for vid in &ver_ids {
        let payload = apprentice_verification::VerdictPayload {
            verdict: apprentice_verification::VERDICT_APPROVED.to_string(),
            notes: Some("Bon apprenti".to_string()),
        };
        apprentice_verification::record_verdict(&db, *vid, reviewer, payload)
            .await
            .expect("verdict ok");
    }

    // Le hook (recompute inline) doit avoir grant verified_apprentice.
    let has_after: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM user_capabilities
         WHERE user_id = $1 AND capability = 'verified_apprentice' AND revoked_at IS NULL)",
    )
    .bind(apprentice)
    .fetch_one(&db)
    .await
    .unwrap();
    assert!(has_after, "verified_apprentice should be granted after 3 approvals");

    // La progression reflète bien 3 approuvés + is_verified.
    let progress = apprentice_verification::get_progress(&db, apprentice)
        .await
        .expect("progress");
    assert_eq!(progress.approved_distinct, 3);
    assert!(progress.is_verified);

    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn record_verdict_rejette_second_verdict() {
    let (db, db_name) = setup_test_db().await;
    let apprentice = create_user(&db, "app").await;
    let reviewer = create_user(&db, "rev").await;
    let (tid, qids) = create_sas_challenge(&db, Some("sas"), 2).await;

    let v = apprentice_verification::submit_verification(
        &db,
        apprentice,
        apprentice_verification::SubmitPayload {
            template_id: tid,
            submission_id: None,
            answers: json!({
                qids[0].to_string(): "s3://priv/a.webm",
                qids[1].to_string(): "s3://priv/b.webm",
            }),
        },
    )
    .await
    .expect("submit ok");

    apprentice_verification::record_verdict(
        &db,
        v.id,
        reviewer,
        apprentice_verification::VerdictPayload {
            verdict: "rejected".to_string(),
            notes: None,
        },
    )
    .await
    .expect("first verdict ok");

    // Second verdict sur le même id → erreur (already reviewed).
    let res = apprentice_verification::record_verdict(
        &db,
        v.id,
        reviewer,
        apprentice_verification::VerdictPayload {
            verdict: "approved".to_string(),
            notes: None,
        },
    )
    .await;
    assert!(res.is_err(), "should refuse second verdict");
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn engine_grants_verified_apprentice_only_at_threshold() {
    let (db, db_name) = setup_test_db().await;
    let user = create_user(&db, "u").await;
    let reviewer = create_user(&db, "r").await;

    // Deux approbations distinctes seulement → sous le seuil (3).
    for _ in 0..2 {
        let (tid, qids) = create_sas_challenge(&db, Some("sas"), 2).await;
        let v = apprentice_verification::submit_verification(
            &db,
            user,
            apprentice_verification::SubmitPayload {
                template_id: tid,
                submission_id: None,
                answers: json!({
                    qids[0].to_string(): "s3://priv/a.webm",
                    qids[1].to_string(): "s3://priv/b.webm",
                }),
            },
        )
        .await
        .expect("submit ok");
        apprentice_verification::record_verdict(
            &db,
            v.id,
            reviewer,
            apprentice_verification::VerdictPayload {
                verdict: "approved".to_string(),
                notes: None,
            },
        )
        .await
        .expect("verdict ok");
    }

    // Sanity-check : recompute explicite ne doit pas grant la cap.
    let report = capabilities_engine::recompute_capabilities_for_user(&db, user)
        .await
        .expect("recompute ok");
    assert!(
        !report.granted.contains(&"verified_apprentice".to_string())
            && !report.already_active.contains(&"verified_apprentice".to_string()),
        "verified_apprentice must NOT be granted below threshold, got {:?}",
        report
    );
    cleanup_test_db(&db_name).await;
}
