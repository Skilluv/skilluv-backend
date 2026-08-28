//! The practice catalogue: captured flags and graded labs.
//!
//! ## Two verifications, one shape
//!
//! A flag is one secret compared by hash; a lab is several. Both are graded
//! without a human, both are capped so they cannot be brute-forced, both record
//! the attempt and neither records what was submitted in plaintext — an attempt
//! log of near-miss guesses is a hint, and hints leak.
//!
//! The write-up kinds — walkthroughs, training grounds, audit exercises — are
//! not here. They are graded by somebody reading a submission, which is the
//! deliverable and review machinery every domain already has.
//!
//! ## Why a solve writes two rows
//!
//! `security_flag_attempts` is the audit trail: every attempt, right or wrong,
//! high volume, prunable. `challenge_submissions` is the record of the solve,
//! which the catalogue, the attempt count and the leaderboard read and which is
//! never pruned. The unique index on the first is what makes a second solve
//! impossible even if two requests arrive together, and the "already solved"
//! check is what turns a duplicate submission into a plain refusal rather than
//! a constraint violation.
//!
//! ## First blood
//!
//! The first person in the world to solve a challenge. Read from
//! `min(attempted_at) WHERE correct`, which sits on a row nobody can back-date,
//! rather than from a counter that could be reset. A tie is impossible because
//! the unique index serialises the two inserts.

use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres};
use uuid::Uuid;

use crate::errors::AppError;

/// Flag attempts per person per challenge per hour.
///
/// Ten. Enough for a typo and a format mistake, far short of guessing anything.
pub const FLAG_ATTEMPTS_PER_HOUR: i64 = 10;

/// Hours a lab is closed for after the attempts run out.
pub const LAB_COOLDOWN_HOURS: i64 = 24;

fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    hex::encode(hasher.finalize())
}

// ═══════════════════════════════════════════════════════════════════
// Flags
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct FlagSubmission {
    pub flag: String,
}

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct FlagOutcome {
    pub correct: bool,
    pub attempt_no: i32,
    pub attempts_left_this_hour: i64,
    /// True when nobody had solved this challenge before.
    pub first_solve: bool,
    pub fragments_awarded: i32,
    /// The verification code of the attestation, when one was issued.
    pub attestation_code: Option<String>,
    /// What to say. Not a translated string — the caller renders — but the
    /// distinction the client needs: a wrong flag and a wrong *format* are
    /// different mistakes and the second one is worth telling somebody about.
    pub hint: Option<String>,
}

/// Compare a submitted flag with the one that was planted.
///
/// Case-sensitive, after trimming surrounding whitespace only. Flags are
/// case-sensitive by convention everywhere, and normalising case would make two
/// different planted flags collide.
pub async fn submit_flag(
    db: &PgPool,
    user_id: Uuid,
    challenge_id: Uuid,
    submitted: &str,
) -> Result<FlagOutcome, AppError> {
    #[derive(sqlx::FromRow)]
    struct Challenge {
        kind: Option<String>,
        flag_hash: Option<String>,
        flag_format: Option<String>,
        reward_fragments: i32,
        status: String,
    }

    let c: Option<Challenge> = sqlx::query_as(
        "SELECT security_kind AS kind, security_flag_hash AS flag_hash,
                security_flag_format AS flag_format, reward_fragments, status
           FROM challenge_templates WHERE id = $1",
    )
    .bind(challenge_id)
    .fetch_optional(db)
    .await?;

    let Some(c) = c else {
        return Err(AppError::NotFound("no such challenge".into()));
    };
    if c.kind.as_deref() != Some("ctf_flag") {
        return Err(AppError::Validation(
            "that challenge is not verified by a flag. Look at what it asks for \
             — most of this catalogue is graded by somebody reading a write-up"
                .into(),
        ));
    }
    if c.status != "published" {
        return Err(AppError::Conflict("that challenge is not open".into()));
    }
    let Some(expected) = c.flag_hash else {
        // Refused by a constraint at creation, so this is defence in depth.
        return Err(AppError::Internal("a flag challenge with no flag".into()));
    };

    // ── The cap ─────────────────────────────────────────────────────
    let recent: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM security_flag_attempts
          WHERE challenge_id = $1 AND user_id = $2
            AND attempted_at > NOW() - INTERVAL '1 hour'",
    )
    .bind(challenge_id)
    .bind(user_id)
    .fetch_one(db)
    .await?;

    if recent >= FLAG_ATTEMPTS_PER_HOUR {
        return Err(AppError::Validation(format!(
            "{FLAG_ATTEMPTS_PER_HOUR} attempts an hour on one challenge. A flag \
             is not something to guess — if the format is the problem, it is on \
             the challenge page"
        )));
    }

    let already: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM security_flag_attempts
                         WHERE challenge_id = $1 AND user_id = $2 AND correct)",
    )
    .bind(challenge_id)
    .bind(user_id)
    .fetch_one(db)
    .await?;
    if already {
        return Err(AppError::Conflict(
            "you have already solved this one".into(),
        ));
    }

    let candidate = submitted.trim();
    let correct = sha256_hex(candidate) == expected;

    let attempt_no: i32 = sqlx::query_scalar(
        "SELECT COALESCE(max(attempt_no), 0) + 1 FROM security_flag_attempts
          WHERE challenge_id = $1 AND user_id = $2",
    )
    .bind(challenge_id)
    .bind(user_id)
    .fetch_one(db)
    .await?;

    if !correct {
        sqlx::query(
            "INSERT INTO security_flag_attempts
                 (challenge_id, user_id, submitted_hash, correct, attempt_no)
             VALUES ($1, $2, $3, FALSE, $4)",
        )
        .bind(challenge_id)
        .bind(user_id)
        .bind(sha256_hex(candidate))
        .bind(attempt_no)
        .execute(db)
        .await?;

        metrics::counter!("skilluv_security_flag_attempts_total",
            "result" => "invalid")
        .increment(1);

        // The one hint worth giving: the shape is wrong, so the flag was never
        // going to match however well the challenge was solved.
        let hint = match c.flag_format.as_deref() {
            Some(fmt) if !looks_like(candidate, fmt) => Some(format!(
                "that does not look like the expected format ({fmt})"
            )),
            _ => None,
        };

        return Ok(FlagOutcome {
            correct: false,
            attempt_no,
            attempts_left_this_hour: (FLAG_ATTEMPTS_PER_HOUR - recent - 1).max(0),
            first_solve: false,
            fragments_awarded: 0,
            attestation_code: None,
            hint,
        });
    }

    // ── A solve ─────────────────────────────────────────────────────
    let mut tx = db.begin().await?;

    // The challenge row is locked, not the attempts: `FOR UPDATE` cannot be
    // used with an aggregate, and locking the challenge is what actually
    // serialises two people solving it in the same second. Whichever
    // transaction gets the lock first sees no earlier correct attempt and is
    // the first solve; the other waits and sees one.
    sqlx::query_scalar::<_, i32>("SELECT 1 FROM challenge_templates WHERE id = $1 FOR UPDATE")
        .bind(challenge_id)
        .fetch_one(&mut *tx)
        .await?;

    let earlier_solves: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM security_flag_attempts
          WHERE challenge_id = $1 AND correct",
    )
    .bind(challenge_id)
    .fetch_one(&mut *tx)
    .await?;
    let first_solve = earlier_solves == 0;

    sqlx::query(
        "INSERT INTO security_flag_attempts
             (challenge_id, user_id, submitted_hash, correct, attempt_no)
         VALUES ($1, $2, $3, TRUE, $4)",
    )
    .bind(challenge_id)
    .bind(user_id)
    .bind(sha256_hex(candidate))
    .bind(attempt_no)
    .execute(&mut *tx)
    .await?;

    // First blood is worth half again. Small enough not to make the scoreboard
    // a race for the well-connected, large enough to be worth getting up for.
    let fragments = if first_solve {
        c.reward_fragments + c.reward_fragments / 2
    } else {
        c.reward_fragments
    };

    record_success(&mut tx, user_id, challenge_id, fragments).await?;
    tx.commit().await?;

    metrics::counter!("skilluv_security_flag_attempts_total", "result" => "valid").increment(1);
    if first_solve {
        metrics::counter!("skilluv_security_first_solves_total").increment(1);
    }

    let attestation_code = match crate::services::security_attestations::issue_for_challenge(
        db,
        user_id,
        challenge_id,
    )
    .await
    {
        Ok(_) => attestation_code_for(db, user_id, challenge_id).await,
        Err(e) => {
            tracing::warn!(challenge = %challenge_id, error = %e,
                "flag accepted but its attestation was not issued");
            None
        }
    };

    Ok(FlagOutcome {
        correct: true,
        attempt_no,
        attempts_left_this_hour: (FLAG_ATTEMPTS_PER_HOUR - recent - 1).max(0),
        first_solve,
        fragments_awarded: fragments,
        attestation_code,
        hint: None,
    })
}

/// A very rough shape check, used only to tell somebody their format is wrong.
///
/// Compares the fixed prefix of the declared format up to the first brace or
/// colon. Never used to decide correctness — a flag is correct when its hash
/// matches and not otherwise.
fn looks_like(candidate: &str, format: &str) -> bool {
    let prefix: String = format
        .chars()
        .take_while(|c| *c != '{' && *c != '<' && *c != ':')
        .collect();
    if prefix.trim().is_empty() {
        return true;
    }
    candidate.starts_with(prefix.trim())
}

/// The submission row a solve leaves behind, and the fragments.
async fn record_success(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    user_id: Uuid,
    challenge_id: Uuid,
    fragments: i32,
) -> Result<(), AppError> {
    let attempt_number: i32 = sqlx::query_scalar(
        "SELECT COALESCE(max(attempt_number), 0) + 1 FROM challenge_submissions
          WHERE challenge_id = $1 AND user_id = $2",
    )
    .bind(challenge_id)
    .bind(user_id)
    .fetch_one(&mut **tx)
    .await?;

    sqlx::query(
        "INSERT INTO challenge_submissions
             (challenge_id, user_id, status, fragments_earned, attempt_number,
              submitted_at, evaluated_at)
         VALUES ($1, $2, 'success', $3, $4, NOW(), NOW())",
    )
    .bind(challenge_id)
    .bind(user_id)
    .bind(fragments)
    .bind(attempt_number)
    .execute(&mut **tx)
    .await?;

    if fragments > 0 {
        sqlx::query(
            "UPDATE users SET total_fragments = total_fragments + $1, updated_at = NOW()
              WHERE id = $2",
        )
        .bind(fragments)
        .bind(user_id)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn attestation_code_for(db: &PgPool, user_id: Uuid, challenge_id: Uuid) -> Option<String> {
    sqlx::query_scalar(
        "SELECT verification_code FROM attestations
          WHERE user_id = $1 AND challenge_template_id = $2 AND revoked_at IS NULL
          ORDER BY issued_at DESC LIMIT 1",
    )
    .bind(user_id)
    .bind(challenge_id)
    .fetch_optional(db)
    .await
    .ok()
    .flatten()
}

// ═══════════════════════════════════════════════════════════════════
// Graded labs
// ═══════════════════════════════════════════════════════════════════

/// How long a link to a lab artefact lives.
///
/// A day, against the hour a finding proof gets. The two are not the same
/// object: a proof is somebody's evidence of an unfixed vulnerability, and a
/// lab artefact is a redacted capture this platform published on purpose for
/// several hundred people to analyse. What the expiry buys here is that the
/// link in a group chat stops working, so the download is attributable to the
/// account that asked for it — not secrecy, which the artefact does not have.
pub const LAB_ARTIFACT_URL_SECONDS: u32 = 24 * 3600;

/// What the artefact endpoint answers.
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct LabArtifact {
    /// Signed, expiring, minted per request.
    pub url: String,
    pub expires_in_seconds: u32,
    /// Repeated from the challenge so a caller that jumped straight here still
    /// has the number to show while the download runs.
    pub size_bytes: i64,
    /// The object's own file name, for the browser's save dialog.
    pub filename: String,
}

/// A link to the artefact of one defensive lab.
///
/// The key never leaves the server: it is read from the challenge and signed
/// here. A client that held the key could ask for a link to any object in the
/// private bucket whose path it could guess, and the bucket also holds the
/// proofs of unfixed vulnerabilities.
pub async fn artifact_link(
    db: &PgPool,
    storage: &crate::services::storage::StorageService,
    challenge_id: Uuid,
) -> Result<LabArtifact, AppError> {
    #[derive(sqlx::FromRow)]
    struct Row {
        kind: Option<String>,
        status: String,
        artifact_key: Option<String>,
        artifact_bytes: Option<i64>,
    }

    let row: Option<Row> = sqlx::query_as(
        "SELECT security_kind AS kind, status,
                security_lab_artifact_key AS artifact_key,
                security_lab_artifact_bytes AS artifact_bytes
           FROM challenge_templates WHERE id = $1",
    )
    .bind(challenge_id)
    .fetch_optional(db)
    .await?;

    let Some(row) = row else {
        return Err(AppError::NotFound("no such challenge".into()));
    };
    if row.kind.as_deref() != Some("defensive_lab") {
        return Err(AppError::Validation(
            "that challenge has no artefact to analyse".into(),
        ));
    }
    if row.status != "published" {
        return Err(AppError::Conflict("that challenge is not open".into()));
    }

    let key = row
        .artifact_key
        .ok_or_else(|| AppError::Internal("a lab with no artefact".into()))?;
    // The same two prefixes the generator writes. Defence in depth against a
    // key that reached the column by some other route: this function signs
    // whatever it is given, and the private bucket holds finding proofs too.
    if (!key.starts_with("security-proofs/") && !key.starts_with("blue-lab/")) || key.contains("..")
    {
        return Err(AppError::Internal(
            "this lab's artefact is not stored where lab artefacts are stored".into(),
        ));
    }

    let filename = key.rsplit('/').next().unwrap_or("artifact").to_string();
    let url = storage
        .presigned_get_url(&key, LAB_ARTIFACT_URL_SECONDS)
        .await?;

    metrics::counter!("skilluv_security_lab_artifact_links_total").increment(1);

    Ok(LabArtifact {
        url,
        expires_in_seconds: LAB_ARTIFACT_URL_SECONDS,
        size_bytes: row.artifact_bytes.unwrap_or(0),
        filename,
    })
}

#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct LabSubmission {
    /// Question id to answer.
    pub answers: std::collections::HashMap<String, String>,
}

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct LabOutcome {
    pub correct_count: usize,
    pub total_count: usize,
    pub score_percent: i32,
    pub passed: bool,
    /// Which questions were wrong. The answers are never echoed back.
    pub wrong_question_ids: Vec<String>,
    /// One hint per wrong question, where the challenge author wrote one.
    pub hints: Vec<String>,
    pub attempt_number: i32,
    pub attempts_left: i32,
    pub fragments_awarded: i32,
    pub attestation_code: Option<String>,
}

/// One question as the challenge stores it.
#[derive(Debug, serde::Deserialize)]
struct Question {
    id: String,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    expected_answer_hash: Option<String>,
    #[serde(default)]
    hint: Option<String>,
    /// When true the answer is compared as typed. Defaults to false, because
    /// almost every answer here is an address, a tool name or a count.
    #[serde(default)]
    case_sensitive: bool,
}

/// Grade a set of answers.
pub async fn submit_answers(
    db: &PgPool,
    user_id: Uuid,
    challenge_id: Uuid,
    submission: LabSubmission,
) -> Result<LabOutcome, AppError> {
    #[derive(sqlx::FromRow)]
    struct Challenge {
        kind: Option<String>,
        questions: Option<serde_json::Value>,
        pass_percent: Option<i16>,
        max_attempts: Option<i16>,
        reward_fragments: i32,
        status: String,
    }

    let c: Option<Challenge> = sqlx::query_as(
        "SELECT security_kind AS kind, security_lab_questions AS questions,
                security_lab_pass_percent AS pass_percent,
                security_lab_max_attempts AS max_attempts,
                reward_fragments, status
           FROM challenge_templates WHERE id = $1",
    )
    .bind(challenge_id)
    .fetch_optional(db)
    .await?;

    let Some(c) = c else {
        return Err(AppError::NotFound("no such challenge".into()));
    };
    if c.kind.as_deref() != Some("defensive_lab") {
        return Err(AppError::Validation(
            "that challenge is not graded by answers".into(),
        ));
    }
    if c.status != "published" {
        return Err(AppError::Conflict("that challenge is not open".into()));
    }

    let questions: Vec<Question> = serde_json::from_value(
        c.questions
            .ok_or_else(|| AppError::Internal("a lab with no questions".into()))?,
    )
    .map_err(|e| AppError::Internal(format!("this lab's questions do not parse: {e}")))?;

    let pass_percent = c.pass_percent.unwrap_or(80) as i32;
    let max_attempts = c.max_attempts.unwrap_or(3) as i32;

    // ── Attempts and cooldown ───────────────────────────────────────
    let used: i32 = sqlx::query_scalar(
        "SELECT count(*)::INT FROM challenge_submissions
          WHERE challenge_id = $1 AND user_id = $2 AND security_grade IS NOT NULL",
    )
    .bind(challenge_id)
    .bind(user_id)
    .fetch_one(db)
    .await?;

    let solved: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM challenge_submissions
                         WHERE challenge_id = $1 AND user_id = $2
                           AND status = 'success')",
    )
    .bind(challenge_id)
    .bind(user_id)
    .fetch_one(db)
    .await?;
    if solved {
        return Err(AppError::Conflict(
            "you have already passed this lab".into(),
        ));
    }

    if used >= max_attempts {
        let last: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
            "SELECT max(submitted_at) FROM challenge_submissions
              WHERE challenge_id = $1 AND user_id = $2",
        )
        .bind(challenge_id)
        .bind(user_id)
        .fetch_one(db)
        .await?;
        let ready_at = last.map(|t| t + chrono::Duration::hours(LAB_COOLDOWN_HOURS));
        if let Some(ready_at) = ready_at
            && ready_at > chrono::Utc::now()
        {
            return Err(AppError::Validation(format!(
                "{max_attempts} attempts used. The lab reopens at {}. Go back \
                 to the artefact — the questions have not changed",
                ready_at.to_rfc3339()
            )));
        }
    }

    // ── Grade ───────────────────────────────────────────────────────
    let total = questions.len();
    let mut correct = 0usize;
    let mut wrong = Vec::new();
    let mut hints = Vec::new();

    for q in &questions {
        let given = submission
            .answers
            .get(&q.id)
            .map(|s| s.trim())
            .unwrap_or("");
        let normalised = if q.case_sensitive {
            given.to_string()
        } else {
            given.to_lowercase()
        };
        let matches = match q.expected_answer_hash.as_deref() {
            Some(expected) => !normalised.is_empty() && sha256_hex(&normalised) == expected,
            // A question with no expected answer cannot be got wrong. Recorded
            // rather than crashing: it is a seeding mistake, and refusing the
            // whole submission would punish the wrong person.
            None => {
                tracing::warn!(challenge = %challenge_id, question = %q.id,
                    "a lab question has no expected answer");
                true
            }
        };
        if matches {
            correct += 1;
        } else {
            wrong.push(q.id.clone());
            if let Some(h) = &q.hint {
                hints.push(format!("{}: {h}", q.id));
            }
        }
        let _ = &q.kind;
    }

    let score_percent = if total == 0 {
        0
    } else {
        ((correct as f64 / total as f64) * 100.0).round() as i32
    };
    let passed = total > 0 && score_percent >= pass_percent;

    // ── Record ──────────────────────────────────────────────────────
    let grade = serde_json::json!({
        "correct": correct,
        "total": total,
        "score_percent": score_percent,
        "wrong": wrong,
        "pass_percent": pass_percent,
    });

    let mut tx = db.begin().await?;
    let attempt_number: i32 = sqlx::query_scalar(
        "SELECT COALESCE(max(attempt_number), 0) + 1 FROM challenge_submissions
          WHERE challenge_id = $1 AND user_id = $2",
    )
    .bind(challenge_id)
    .bind(user_id)
    .fetch_one(&mut *tx)
    .await?;

    let fragments = if passed { c.reward_fragments } else { 0 };

    sqlx::query(
        "INSERT INTO challenge_submissions
             (challenge_id, user_id, status, fragments_earned, attempt_number,
              submitted_at, evaluated_at, security_grade)
         VALUES ($1, $2, $3, $4, $5, NOW(), NOW(), $6)",
    )
    .bind(challenge_id)
    .bind(user_id)
    .bind(if passed { "success" } else { "failure" })
    .bind(fragments)
    .bind(attempt_number)
    .bind(&grade)
    .execute(&mut *tx)
    .await?;

    if fragments > 0 {
        sqlx::query(
            "UPDATE users SET total_fragments = total_fragments + $1, updated_at = NOW()
              WHERE id = $2",
        )
        .bind(fragments)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;

    metrics::counter!("skilluv_security_lab_attempts_total",
        "result" => if passed { "passed" } else { "failed" })
    .increment(1);

    let attestation_code = if passed {
        match crate::services::security_attestations::issue_for_challenge(db, user_id, challenge_id)
            .await
        {
            Ok(_) => attestation_code_for(db, user_id, challenge_id).await,
            Err(e) => {
                tracing::warn!(challenge = %challenge_id, error = %e,
                    "lab passed but its attestation was not issued");
                None
            }
        }
    } else {
        None
    };

    Ok(LabOutcome {
        correct_count: correct,
        total_count: total,
        score_percent,
        passed,
        wrong_question_ids: wrong,
        hints,
        attempt_number,
        attempts_left: (max_attempts - attempt_number).max(0),
        fragments_awarded: fragments,
        attestation_code,
    })
}

// ═══════════════════════════════════════════════════════════════════
// The scoreboard
// ═══════════════════════════════════════════════════════════════════

/// Who has solved what, all time and this week.
///
/// Public. Counts solves and first solves separately, because "solved forty"
/// and "was first on four" say different things and a single number hides the
/// second.
pub async fn scoreboard(db: &PgPool) -> Result<serde_json::Value, AppError> {
    let all_time: Vec<serde_json::Value> = sqlx::query_scalar(
        r#"
        WITH firsts AS (
            SELECT DISTINCT ON (challenge_id) challenge_id, user_id
              FROM security_flag_attempts
             WHERE correct
             ORDER BY challenge_id, attempted_at ASC
        )
        SELECT jsonb_build_object(
                   'username', u.username,
                   'display_name', u.display_name,
                   'avatar_url', u.avatar_url,
                   'solves', count(*),
                   'first_solves', count(*) FILTER (
                       WHERE EXISTS (SELECT 1 FROM firsts fs
                                      WHERE fs.challenge_id = a.challenge_id
                                        AND fs.user_id = a.user_id)),
                   'last_solve_at', max(a.attempted_at)
               )
          FROM security_flag_attempts a
          JOIN users u ON u.id = a.user_id
         WHERE a.correct
         GROUP BY u.id, u.username, u.display_name, u.avatar_url
         ORDER BY count(*) DESC, max(a.attempted_at) ASC
         LIMIT 20
        "#,
    )
    .fetch_all(db)
    .await?;

    let weekly: Vec<serde_json::Value> = sqlx::query_scalar(
        r#"
        SELECT jsonb_build_object(
                   'username', u.username,
                   'display_name', u.display_name,
                   'solves', count(*),
                   'last_solve_at', max(a.attempted_at)
               )
          FROM security_flag_attempts a
          JOIN users u ON u.id = a.user_id
         WHERE a.correct AND a.attempted_at > NOW() - INTERVAL '7 days'
         GROUP BY u.id, u.username, u.display_name
         ORDER BY count(*) DESC, max(a.attempted_at) ASC
         LIMIT 20
        "#,
    )
    .fetch_all(db)
    .await?;

    let unsolved: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM challenge_templates c
          WHERE c.security_kind = 'ctf_flag' AND c.status = 'published'
            AND NOT EXISTS (SELECT 1 FROM security_flag_attempts a
                             WHERE a.challenge_id = c.id AND a.correct)",
    )
    .fetch_one(db)
    .await?;

    Ok(serde_json::json!({
        "all_time": all_time,
        "weekly": weekly,
        "unsolved_challenges": unsolved,
    }))
}

/// How many challenges this person solved before anybody else.
///
/// Read by the badge engine, which needs a count and not a listing.
pub async fn first_solve_count(db: &PgPool, user_id: Uuid) -> Result<i64, AppError> {
    Ok(sqlx::query_scalar(
        r#"
        SELECT count(*) FROM (
            SELECT DISTINCT ON (challenge_id) challenge_id, user_id
              FROM security_flag_attempts
             WHERE correct
             ORDER BY challenge_id, attempted_at ASC
        ) f WHERE f.user_id = $1
        "#,
    )
    .bind(user_id)
    .fetch_one(db)
    .await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashing_a_flag_is_stable_and_case_sensitive() {
        assert_eq!(sha256_hex("SKILLUV{a_b}"), sha256_hex("SKILLUV{a_b}"));
        assert_ne!(sha256_hex("SKILLUV{a_b}"), sha256_hex("skilluv{a_b}"));
        assert_eq!(sha256_hex("x").len(), 64);
    }

    #[test]
    fn the_format_check_only_looks_at_the_fixed_prefix() {
        assert!(looks_like("SKILLUV{found_it}", "SKILLUV{lower_snake_case}"));
        assert!(!looks_like("flag{found_it}", "SKILLUV{lower_snake_case}"));
        assert!(looks_like("JUICESHOP:scoreBoard", "JUICESHOP:<key>"));
        // A format with no fixed prefix cannot say anything, so it says
        // nothing rather than guessing.
        assert!(looks_like("anything", "{whatever}"));
    }

    #[test]
    fn a_shape_check_never_decides_correctness() {
        // The point of this test is the property, not the function: a candidate
        // with the right shape and the wrong content must hash differently, and
        // the shape check must not be consulted for correctness anywhere.
        let expected = sha256_hex("SKILLUV{the_real_one}");
        let plausible = "SKILLUV{not_the_real_one}";
        assert!(looks_like(plausible, "SKILLUV{lower_snake_case}"));
        assert_ne!(sha256_hex(plausible), expected);
    }
}
