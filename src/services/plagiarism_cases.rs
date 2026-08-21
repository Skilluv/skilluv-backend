//! Accusing somebody of copying, and letting them answer.
//!
//! ## Why the answer is the point
//!
//! The outcome of an upheld case is a disqualification, a confiscated prize
//! and a public record. Deciding that without hearing the person accused is
//! not a decision, it is a verdict — and the platform's whole argument is that
//! its judgements can be checked.
//!
//! Seventy-two hours: long enough to find the file, the timestamps, the client
//! who commissioned the piece; short enough that a contest is not held open by
//! an accusation nobody follows up.
//!
//! ## Nobody is banned by this module
//!
//! It counts upheld cases and does not act on the count. "Second strike, ban"
//! reads well and would, one Tuesday, ban somebody on an accusation a tired
//! reviewer upheld in four minutes. The count is surfaced so a human can see
//! it; the ban stays a decision a human takes and signs.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::errors::AppError;

/// How long the accused has before a case may be decided without them.
pub const RESPONSE_HOURS: i64 = 72;

/// The shortest accusation somebody can actually answer.
pub const MIN_REASON: usize = 80;
/// The shortest decision that says anything.
pub const MIN_DECISION: usize = 80;

#[derive(Debug, Clone, Serialize, sqlx::FromRow, ToSchema)]
pub struct Case {
    pub id: Uuid,
    pub submission_id: Uuid,
    pub accused_username: Option<String>,
    /// Absent once the accuser's account is gone. The accusation stands on
    /// its evidence, not on who made it.
    pub raised_by_username: Option<String>,
    pub reason_md: String,
    pub evidence_url: String,
    pub raised_at: chrono::DateTime<chrono::Utc>,
    pub respond_by: chrono::DateTime<chrono::Utc>,
    pub response_md: Option<String>,
    pub responded_at: Option<chrono::DateTime<chrono::Utc>>,
    pub status: String,
    pub decision_md: Option<String>,
    pub decided_at: Option<chrono::DateTime<chrono::Utc>>,
    /// How many cases against this person have been upheld, this one
    /// included once it is. Shown to the reviewer, acted on by nobody.
    pub upheld_against_accused: i64,
}

const CASE_SELECT: &str = r#"
    SELECT c.id,
           c.submission_id,
           accused.username AS accused_username,
           raiser.username AS raised_by_username,
           c.reason_md,
           c.evidence_url,
           c.raised_at,
           c.respond_by,
           c.response_md,
           c.responded_at,
           c.status,
           c.decision_md,
           c.decided_at,
           (SELECT count(*) FROM plagiarism_cases prior
             WHERE prior.accused_id = c.accused_id AND prior.status = 'upheld')
             AS upheld_against_accused
      FROM plagiarism_cases c
      LEFT JOIN users accused ON accused.id = c.accused_id
      LEFT JOIN users raiser ON raiser.id = c.raised_by
"#;

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct FlagInput {
    /// At least eighty characters. "C'est copié" is not something anybody can
    /// answer, and the person answering has three days to work out what they
    /// are answering.
    pub reason_md: String,
    /// Where the original is. Required: an accusation with no link to the work
    /// it is compared against cannot be checked by anybody, the reviewer
    /// included.
    pub evidence_url: String,
}

/// Raise a case against a contest entry.
///
/// Open to any authenticated member, not only to jurors. Plagiarism is
/// usually spotted by the one person who recognises the original, and that is
/// rarely whoever happens to be judging.
pub async fn flag(
    db: &PgPool,
    submission_id: Uuid,
    raised_by: Uuid,
    input: FlagInput,
) -> Result<Case, AppError> {
    let reason = input.reason_md.trim();
    if reason.chars().count() < MIN_REASON {
        return Err(AppError::Validation(format!(
            "say what was copied and from where, in at least {MIN_REASON} characters — the \
             person accused has three days to answer this"
        )));
    }
    crate::validators::check_max_len(reason, "reason_md", 4000)?;

    let evidence = input.evidence_url.trim();
    if !evidence.starts_with("https://") || evidence.len() > 2048 {
        return Err(AppError::Validation(
            "evidence_url must be an https link to the original".into(),
        ));
    }

    let owner: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT s.submitted_by, t.status
           FROM tournament_submissions s
           JOIN tournaments t ON t.id = s.tournament_id
          WHERE s.id = $1",
    )
    .bind(submission_id)
    .fetch_optional(db)
    .await?;

    let (accused_id, _tournament_status) =
        owner.ok_or_else(|| AppError::NotFound("no such submission".into()))?;

    // Accusing yourself is not a thing anybody does honestly, and allowing it
    // gives a losing entrant a way to withdraw while blaming the process.
    if accused_id == raised_by {
        return Err(AppError::Validation(
            "you cannot raise a case against your own entry".into(),
        ));
    }

    let id: Option<Uuid> = sqlx::query_scalar(
        r#"
        INSERT INTO plagiarism_cases
             (submission_id, accused_id, raised_by, reason_md, evidence_url, respond_by)
        VALUES ($1, $2, $3, $4, $5, NOW() + ($6 || ' hours')::INTERVAL)
        ON CONFLICT DO NOTHING
        RETURNING id
        "#,
    )
    .bind(submission_id)
    .bind(accused_id)
    .bind(raised_by)
    .bind(reason)
    .bind(evidence)
    .bind(RESPONSE_HOURS.to_string())
    .fetch_optional(db)
    .await?;

    // The partial unique index refused it: a case is already open on this
    // entry. A second one would split the evidence across two files and give
    // the accused two clocks.
    let id = id.ok_or_else(|| AppError::Conflict("a case is already open on this entry".into()))?;

    by_id(db, id).await
}

pub async fn by_id(db: &PgPool, id: Uuid) -> Result<Case, AppError> {
    let sql = format!("{CASE_SELECT} WHERE c.id = $1");
    sqlx::query_as::<_, Case>(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NotFound("no such case".into()))
}

/// The accused answers.
///
/// Accepted after the window too. The window is a floor on the decision, not
/// a deadline on the person: an answer that arrives late is still worth
/// recording, and a reviewer who has not decided yet should read it.
pub async fn respond(
    db: &PgPool,
    case_id: Uuid,
    user_id: Uuid,
    response_md: &str,
) -> Result<Case, AppError> {
    let response = response_md.trim();
    if response.is_empty() {
        return Err(AppError::Validation(
            "an empty answer is not an answer".into(),
        ));
    }
    crate::validators::check_max_len(response, "response_md", 8000)?;

    let row: Option<(Uuid, String)> =
        sqlx::query_as("SELECT accused_id, status FROM plagiarism_cases WHERE id = $1")
            .bind(case_id)
            .fetch_optional(db)
            .await?;

    let (accused_id, status) = row.ok_or_else(|| AppError::NotFound("no such case".into()))?;
    if accused_id != user_id {
        return Err(AppError::Forbidden);
    }
    if status != "open" {
        return Err(AppError::Conflict(
            "this case has already been decided".into(),
        ));
    }

    sqlx::query(
        "UPDATE plagiarism_cases
            SET response_md = $2, responded_at = NOW()
          WHERE id = $1 AND status = 'open'",
    )
    .bind(case_id)
    .bind(response)
    .execute(db)
    .await?;

    by_id(db, case_id).await
}

/// The queue somebody works: oldest first, because the accused is waiting.
pub async fn open_cases(db: &PgPool, limit: i64) -> Result<Vec<Case>, AppError> {
    let sql = format!("{CASE_SELECT} WHERE c.status = 'open' ORDER BY c.raised_at ASC LIMIT $1");
    sqlx::query_as::<_, Case>(sqlx::AssertSqlSafe(sql))
        .bind(limit)
        .fetch_all(db)
        .await
        .map_err(Into::into)
}

/// Decide a case.
///
/// Upholding it disqualifies the entry — marked, never deleted. Deleting it
/// would erase the fact that it was entered at all, and the other entrants
/// moved up: a ranking whose gaps are unexplained is a ranking nobody can
/// check.
///
/// Dismissing it requires as much writing as upholding it. An accusation
/// dropped without a word leaves the accusation standing in everybody's
/// memory.
pub async fn decide(
    db: &PgPool,
    case_id: Uuid,
    reviewer: Uuid,
    upheld: bool,
    decision_md: &str,
) -> Result<Case, AppError> {
    let decision = decision_md.trim();
    if decision.chars().count() < MIN_DECISION {
        return Err(AppError::Validation(format!(
            "say why in at least {MIN_DECISION} characters — the person accused reads this, \
             and so does anybody who later asks what happened"
        )));
    }
    crate::validators::check_max_len(decision, "decision_md", 8000)?;

    let mut tx = db.begin().await?;

    let submission: Option<Uuid> = sqlx::query_scalar(
        "SELECT submission_id FROM plagiarism_cases WHERE id = $1 AND status = 'open' FOR UPDATE",
    )
    .bind(case_id)
    .fetch_optional(&mut *tx)
    .await?;

    let submission = submission
        .ok_or_else(|| AppError::Conflict("this case is not open, or does not exist".into()))?;

    sqlx::query(
        "UPDATE plagiarism_cases
            SET status = $2, decision_md = $3, decided_by = $4, decided_at = NOW()
          WHERE id = $1 AND status = 'open'",
    )
    .bind(case_id)
    .bind(if upheld { "upheld" } else { "dismissed" })
    .bind(decision)
    .bind(reviewer)
    .execute(&mut *tx)
    .await?;

    let mut confiscate_from: Option<(Uuid, Uuid)> = None;

    if upheld {
        // `refusal_carries_a_reason` requires the note, and the decision is
        // the note: the entry's own trail has to say what happened without a
        // second lookup.
        sqlx::query(
            "UPDATE tournament_submissions
                SET status = 'disqualified',
                    judge_notes = $2,
                    updated_at = NOW()
              WHERE id = $1",
        )
        .bind(submission)
        .bind(format!("Plagiat retenu : {decision}"))
        .execute(&mut *tx)
        .await?;

        // Whose prize, if the disqualified entry won one. A guild entry has
        // none: `contest_prizes::award` pays user participants only, because a
        // prize paid to a guild has no account to land in.
        confiscate_from = sqlx::query_as(
            "SELECT tournament_id, participant_id
               FROM tournament_submissions
              WHERE id = $1 AND participant_type = 'user'",
        )
        .bind(submission)
        .fetch_optional(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    // The confiscation the module header promised.
    //
    // `award` puts a prize in `pending` rather than `available` and says why:
    // "the release window is what makes a contested result recoverable". This
    // is the only thing that ever contests one, and until now it disqualified
    // the entry and left the money — so a contest could hold a winner who was
    // disqualified and paid at the same time, in the same person.
    //
    // After the commit, and deliberately. The decision is the thing that must
    // not be lost: it has been written, the accused can read it, and the
    // ledger posting carries an idempotency key so a repeat is safe. A prize
    // that cannot be taken back — already released, already withdrawn — is a
    // debt to recover through people, and it must not turn a decided case
    // back into an open one.
    if let Some((tournament_id, participant_id)) = confiscate_from {
        match crate::services::contest_prizes::confiscate(db, tournament_id, participant_id).await {
            Ok(Some(amount)) => {
                tracing::info!(
                    case = %case_id, tournament = %tournament_id, %amount,
                    "prize confiscated and returned to the contest escrow"
                );
            }
            Ok(None) => {}
            Err(err) => {
                tracing::error!(
                    case = %case_id, tournament = %tournament_id, %err,
                    "plagiarism upheld but the prize could not be taken back — recover this by hand"
                );
                metrics::counter!("skilluv_prize_manual_confiscation_needed_total").increment(1);
            }
        }
    }

    by_id(db, case_id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_floors_are_the_same_on_both_sides() {
        // An accusation nobody can answer and a decision nobody can argue
        // with are the same failure. They get the same floor.
        assert_eq!(MIN_REASON, MIN_DECISION);
        assert_eq!(MIN_REASON, 80);
    }

    #[test]
    fn three_days_is_the_window() {
        // Long enough to find the file and the timestamps; short enough that
        // a contest is not held open by an accusation nobody follows up.
        assert_eq!(RESPONSE_HOURS, 72);
    }
}
