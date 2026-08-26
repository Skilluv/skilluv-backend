//! Turning a confirmed attack into a defensive exercise.
//!
//! ## The loop this closes
//!
//! Somebody attacks the platform, the finding is confirmed and fixed, and the
//! traffic that produced it is the most useful teaching material a defensive
//! exercise could have: a real attack on a real system, with a known answer.
//! Ticket B-03 asked for that, and it is worth having — a blue-team catalogue
//! built only from other people's published datasets teaches somebody else's
//! incidents.
//!
//! ## What this module does and does not do
//!
//! It does not read logs. The request log lives in the reverse proxy and the
//! container's standard output, not in this database, and a service that
//! claimed to extract it would be a service that returned an empty artefact and
//! said nothing. So the export is an operator's step — the person who has the
//! logs runs the export, redacts it, and uploads it — and this module does
//! everything after that: the challenge, its questions, and the answers that
//! are known because the finding is on the record.
//!
//! Splitting it there is not a compromise. The redaction in particular is a
//! judgement: a log window around one attack contains other people's requests,
//! and deciding what may be published is not something to automate.
//!
//! ## Why the answers are trustworthy
//!
//! Every generated question is answerable from the artefact *and* known from
//! the finding row: the endpoint that was targeted, the weakness class, the
//! date, the severity a validator settled on. Nothing is invented, which is the
//! failure mode 0558 refuses at length — a question whose expected answer the
//! author guessed is a question nobody can ever get right.
//!
//! ## The challenge arrives as a draft
//!
//! Like every other seeded challenge. Somebody reads the artefact, checks that
//! the questions are answerable from it, and publishes. An exercise generated
//! and published unread is one where the first ten people to attempt it find
//! out that the redaction removed the answer.

use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

/// What the operator supplies: the artefact they exported and redacted.
#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct LabFromFinding {
    /// Key in the private bucket, from the proof upload endpoint or from an
    /// operator's own upload.
    pub artifact_key: String,
    pub artifact_bytes: i64,
    /// How long the analysis should take. Shown before the download starts.
    pub estimated_minutes: i32,
    /// Extra questions the operator wants to add, already answered by them.
    #[serde(default)]
    pub extra_questions: Vec<ExtraQuestion>,
    /// Said out loud by the operator: the artefact has been read and nothing
    /// in it identifies anybody who was not part of the attack.
    pub redaction_confirmed: bool,
}

#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ExtraQuestion {
    pub id: String,
    pub question: String,
    pub answer: String,
    #[serde(default)]
    pub hint: Option<String>,
}

fn hash_answer(answer: &str) -> String {
    let mut h = Sha256::new();
    h.update(answer.trim().to_lowercase().as_bytes());
    hex::encode(h.finalize())
}

/// The questions a finding can answer about itself.
///
/// Four always, plus one for the weakness class where that class has an
/// obvious observable. Each one is answerable by reading the artefact and
/// checkable against the finding.
fn questions_for(
    affected_endpoint: Option<&str>,
    cwe_id: Option<&str>,
    severity_tier: &str,
    occurred_on: chrono::NaiveDate,
) -> Vec<serde_json::Value> {
    let mut out = Vec::new();

    out.push(serde_json::json!({
        "id": "date",
        "kind": "text",
        "question": "On which day did this activity happen? Answer as YYYY-MM-DD.",
        "expected_answer_hash": hash_answer(&occurred_on.to_string()),
        "hint": "The timestamps are in the log. Watch the timezone.",
        "case_sensitive": false,
    }));

    if let Some(endpoint) = affected_endpoint {
        out.push(serde_json::json!({
            "id": "endpoint",
            "kind": "text",
            "question": "Which endpoint or path was the target of the activity? \
                         Answer exactly as it appears.",
            "expected_answer_hash": hash_answer(endpoint),
            "hint": "One path takes a disproportionate share of the requests in \
                     the window.",
            "case_sensitive": false,
        }));
    }

    if let Some(cwe) = cwe_id {
        out.push(serde_json::json!({
            "id": "weakness",
            "kind": "text",
            "question": "Which CWE identifier describes what was being \
                         attempted? Answer as CWE-nn.",
            "expected_answer_hash": hash_answer(cwe),
            "hint": "The shape of the payloads names the class.",
            "case_sensitive": false,
        }));
    }

    out.push(serde_json::json!({
        "id": "severity",
        "kind": "choice",
        "question": "Given what the requests achieved, how would you rate this?",
        "choices": ["critical", "high", "medium", "low", "informational"],
        "expected_answer_hash": hash_answer(severity_tier),
        "hint": "Rate the outcome, not the volume. A thousand failed attempts \
                 are not a compromise.",
        "case_sensitive": false,
    }));

    out
}

/// Create the draft exercise.
///
/// Refuses a finding that is not confirmed: an exercise built from a report
/// nobody reproduced would teach an answer that may not be true.
pub async fn draft_from_finding(
    db: &PgPool,
    finding_id: Uuid,
    author: Uuid,
    input: LabFromFinding,
) -> Result<Uuid, AppError> {
    if !input.redaction_confirmed {
        return Err(AppError::Validation(
            "confirm that the artefact has been read and redacted. A log window \
             around one attack contains other people's requests, and deciding \
             what may be published is not something this platform will do on \
             your behalf"
                .into(),
        ));
    }
    if !input.artifact_key.starts_with("security-proofs/")
        && !input.artifact_key.starts_with("blue-lab/")
    {
        return Err(AppError::Validation(
            "the artefact has to be an uploaded object of ours".into(),
        ));
    }
    if input.artifact_bytes <= 0 {
        return Err(AppError::Validation("an empty artefact".into()));
    }
    if !(5..=1440).contains(&input.estimated_minutes) {
        return Err(AppError::Validation(
            "an estimate between five minutes and a day".into(),
        ));
    }

    #[derive(sqlx::FromRow)]
    struct Finding {
        title: String,
        status: String,
        severity_tier: String,
        cwe_id: Option<String>,
        affected_endpoint: Option<String>,
        created_at: chrono::DateTime<chrono::Utc>,
        target_host: Option<String>,
    }

    let f: Option<Finding> = sqlx::query_as(
        "SELECT title, status, severity_tier, cwe_id, affected_endpoint,
                created_at, target_host
           FROM security_findings WHERE id = $1",
    )
    .bind(finding_id)
    .fetch_optional(db)
    .await?;

    let Some(f) = f else {
        return Err(AppError::NotFound("no such finding".into()));
    };
    if !matches!(f.status.as_str(), "confirmed" | "fixed" | "published") {
        return Err(AppError::Conflict(
            "only a confirmed finding becomes an exercise. Before that, the \
             answers are a claim"
                .into(),
        ));
    }

    let mut questions = questions_for(
        f.affected_endpoint.as_deref(),
        f.cwe_id.as_deref(),
        &f.severity_tier,
        f.created_at.date_naive(),
    );

    for extra in &input.extra_questions {
        if extra.answer.trim().is_empty() {
            return Err(AppError::Validation(format!(
                "question '{}' has no answer. A question nobody can get right \
                 is worse than no question",
                extra.id
            )));
        }
        questions.push(serde_json::json!({
            "id": extra.id,
            "kind": "text",
            "question": extra.question,
            "expected_answer_hash": hash_answer(&extra.answer),
            "hint": extra.hint,
            "case_sensitive": false,
        }));
    }

    // The difficulty follows the severity, because a critical leaves more in
    // the logs and is easier to spot — the exercise is harder when the attack
    // was quieter, not when it was worse.
    let difficulty: i16 = match f.severity_tier.as_str() {
        "critical" => 2,
        "high" => 3,
        "medium" => 4,
        _ => 5,
    };

    let where_ = f.target_host.as_deref().unwrap_or("this platform");
    let description = format!(
        "A real attack on {where_}, exported from the logs after the finding \
         was confirmed and fixed. Nothing in it is simulated.\n\n\
         Work out what happened and answer the questions. The write-up is \
         optional and is what a reviewer will read if you want the attestation \
         to say more than a score."
    );
    let instructions = format!(
        "## What there is to do\n\n\
         Download the artefact and establish what was attempted, against what, \
         and whether it succeeded.\n\n\
         ## What is expected\n\n\
         Answers to the questions, each of which is answerable from the \
         artefact alone. Eighty per cent to pass.\n\n\
         ## What this material is\n\n\
         Logs from a real system, redacted by an operator. Addresses are kept \
         because they are the object of the analysis; credentials, tokens and \
         anything identifying a person who was not part of the attack are \
         not.\n\n\
         ## Where it came from\n\n\
         Generated from a confirmed finding — \"{}\" — after its fix shipped. \
         The reporter is credited on the hall of fame; this exercise is the \
         other half of what their report produced.",
        f.title
    );

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO challenge_templates (
             title, description, instructions, skill_domain, difficulty,
             status, is_training, ai_policy, created_by, duration_minutes,
             reward_fragments, security_kind, security_difficulty_tier,
             security_lab_artifact_key, security_lab_artifact_bytes,
             security_lab_questions, security_lab_pass_percent,
             security_lab_max_attempts, security_attribution_md)
         VALUES ($1, $2, $3, 'security', $4,
                 'draft', TRUE, 'disclosure_required', $5, $6,
                 $7, 'defensive_lab', $8,
                 $9, $10,
                 $11, 80,
                 3, $12)
         RETURNING id",
    )
    .bind(format!("Read the attack: {}", f.title))
    .bind(&description)
    .bind(&instructions)
    .bind(difficulty)
    .bind(author)
    .bind(input.estimated_minutes)
    // Deliberately modest. A lab built from our own incident is teaching
    // material, and paying for it like a finding would make it worth
    // manufacturing incidents.
    .bind(40i32)
    .bind(match difficulty {
        2 => "easy",
        3 => "medium",
        4 => "hard",
        _ => "insane",
    })
    .bind(&input.artifact_key)
    .bind(input.artifact_bytes)
    .bind(serde_json::Value::Array(questions))
    .bind(
        "Logs from this platform, exported and redacted by an operator after \
           the finding they document was fixed.",
    )
    .fetch_one(db)
    .await?;

    // Recorded on the finding's own history, so that "what came of my report"
    // has an answer beyond the fix.
    sqlx::query(
        "INSERT INTO security_finding_events
             (finding_id, actor_user_id, event, reason, detail)
         VALUES ($1, $2, 'blue_lab_generated',
                 'the traffic became a defensive exercise', $3)",
    )
    .bind(finding_id)
    .bind(author)
    .bind(serde_json::json!({ "challenge_id": id }))
    .execute(db)
    .await?;

    metrics::counter!("skilluv_security_labs_generated_total").increment(1);

    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_generated_question_has_an_answer() {
        let qs = questions_for(
            Some("POST /api/auth/login"),
            Some("CWE-307"),
            "high",
            chrono::NaiveDate::from_ymd_opt(2026, 3, 14).unwrap(),
        );
        assert_eq!(qs.len(), 4);
        for q in &qs {
            let hash = q["expected_answer_hash"].as_str().unwrap();
            assert_eq!(hash.len(), 64, "question {} has no usable answer", q["id"]);
            assert!(q["question"].as_str().unwrap().len() > 20);
        }
    }

    #[test]
    fn a_finding_with_less_on_it_asks_fewer_questions() {
        // A finding with no endpoint and no weakness class can still produce a
        // usable exercise: the date and the severity are always known. Asking
        // an unanswerable question instead would be worse than asking two.
        let qs = questions_for(
            None,
            None,
            "medium",
            chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
        );
        assert_eq!(qs.len(), 2);
        let ids: Vec<&str> = qs.iter().map(|q| q["id"].as_str().unwrap()).collect();
        assert_eq!(ids, vec!["date", "severity"]);
    }

    #[test]
    fn answers_are_normalised_before_hashing() {
        // The grader lowercases and trims before comparing, so the generator
        // has to hash the same way or nothing would ever match.
        assert_eq!(hash_answer("CWE-89"), hash_answer("  cwe-89 "));
        assert_ne!(hash_answer("CWE-89"), hash_answer("CWE-79"));
    }
}
