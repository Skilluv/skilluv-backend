//! Contests — what each format asks for, and what a participant hands in.
//!
//! Five formats sit on top of the `tournaments` machinery (migrations 0189
//! and 0235):
//!
//!   * a **hackathon** on code — a `hackathon` with `skill_domain = 'code'`,
//!     a theme, and a project plus a writeup at the end;
//!   * **code golf** — the shortest working solution to a stated problem, one
//!     language at a time, ranked ascending;
//!   * a **TDD contest** — the same problem for everybody, judged on the tests
//!     as much as on the code that passes them;
//!   * a **brief contest** — one written brief, N answers, a jury ranks them.
//!     Not a hackathon: nobody builds against a clock. Design uses it most,
//!     but an agency briefing three copywriters is the same event, so the
//!     kind carries no domain in its name;
//!   * a **duel** — two people, one task, the room votes.
//!
//! Rules live in a JSONB column rather than in columns of their own: the keys
//! differ per format and a table with a `theme` column that is NULL for two
//! formats out of three describes nothing.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

pub const VALID_ARTIFACT_TYPES: &[&str] = &[
    "repository",
    "pull_request",
    "gist",
    "writeup",
    "demo",
    // Migration 0235: answers that are not a repository.
    "design_file",
    "image_set",
    "video",
    "audio",
];

pub const VALID_SUBMISSION_STATUSES: &[&str] =
    &["submitted", "accepted", "rejected", "disqualified"];

/// Kinds that expect a submission. The others are scored from activity
/// elsewhere on the platform, and asking for a link would be theatre.
pub const KINDS_WITH_SUBMISSIONS: &[&str] =
    &["hackathon", "code_golf", "tdd_contest", "brief_contest", "duel"];

/// Kinds whose result is decided by people rather than by a measured number,
/// and which therefore need a panel before they can be closed.
pub const JURIED_KINDS: &[&str] = &["hackathon", "tdd_contest", "brief_contest"];

/// Kinds where the room votes. `duel` is the pure case; a brief contest can
/// opt in through its rules.
pub const COMMUNITY_VOTED_KINDS: &[&str] = &["duel"];

/// Kinds ranked by a number the submitter measures rather than a judge's
/// opinion.
pub const MEASURED_KINDS: &[&str] = &["code_golf"];

// ═══════════════════════════════════════════════════════════════════
// Rules
// ═══════════════════════════════════════════════════════════════════

/// What a contest of this kind must state before anybody can enter it.
///
/// Checked at creation rather than at submission: a code golf with no problem
/// link is not a contest with a missing field, it is an announcement nobody
/// can act on, and the moment to catch that is before it is published.
pub fn validate_rules(kind: &str, rules: &serde_json::Value) -> Result<(), AppError> {
    if !rules.is_object() {
        return Err(AppError::Validation("rules must be an object".into()));
    }

    let required: &[&str] = match kind {
        // The theme is the whole premise; without it the entries have nothing
        // in common and there is nothing to compare.
        "hackathon" => &["theme"],
        // A golf without a language is not comparable, and without a problem
        // there is nothing to solve.
        "code_golf" => &["language", "problem_url"],
        // The problem, and what the judges will actually look at — announced
        // up front, because "judged on test quality" means nothing until it
        // says which qualities.
        "tdd_contest" => &["problem_url", "judging_criteria"],
        // The number of merged pull requests somebody commits to, over the
        // window the contest runs for.
        "marathon" => &["target_merged_prs"],
        // The brief itself, and what the jury will weigh. A brief contest
        // with a vague brief is how a contest becomes unpaid guesswork, and
        // the moment to catch that is before anybody spends a weekend on it.
        "brief_contest" => &["brief", "judging_criteria"],
        // What the two of them are being asked to do, and how long they have.
        "duel" => &["task", "duration_hours"],
        _ => &[],
    };

    for key in required {
        let value = rules.get(key);
        let stated = match value {
            None | Some(serde_json::Value::Null) => false,
            Some(serde_json::Value::String(s)) => !s.trim().is_empty(),
            Some(serde_json::Value::Array(a)) => !a.is_empty(),
            Some(_) => true,
        };
        if !stated {
            return Err(AppError::Validation(format!(
                "a {kind} must state '{key}' in its rules"
            )));
        }
    }

    if kind == "brief_contest" {
        // Long enough to be a brief. Under this it is a subject line, and the
        // answers will differ on things nobody stated.
        let brief_len = rules
            .get("brief")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().chars().count())
            .unwrap_or(0);
        if brief_len < 200 {
            return Err(AppError::Validation(
                "a brief contest needs at least 200 characters of brief: below that                  the answers differ on things nobody stated"
                    .into(),
            ));
        }
    }

    if kind == "duel" {
        let hours = rules.get("duration_hours").and_then(|v| v.as_i64());
        match hours {
            Some(n) if (1..=168).contains(&n) => {}
            _ => {
                return Err(AppError::Validation(
                    "duration_hours must be a whole number of hours between 1 and 168".into(),
                ));
            }
        }
    }

    if kind == "marathon" {
        let target = rules.get("target_merged_prs").and_then(|v| v.as_i64());
        match target {
            Some(n) if (1..=200).contains(&n) => {}
            _ => {
                return Err(AppError::Validation(
                    "target_merged_prs must be a whole number between 1 and 200".into(),
                ));
            }
        }
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════
// Submissions
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Submission {
    pub id: Uuid,
    pub tournament_id: Uuid,
    pub participant_type: String,
    pub participant_id: Uuid,
    pub submitted_by: Uuid,
    pub artifact_url: String,
    pub artifact_type: String,
    pub secondary_url: Option<String>,
    pub summary: String,
    pub language: Option<String>,
    pub measured_value: Option<i32>,
    pub status: String,
    pub judge_score: Option<i16>,
    pub judged_by: Option<Uuid>,
    pub judged_at: Option<chrono::DateTime<chrono::Utc>>,
    pub judge_notes: Option<String>,
    pub submitted_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SubmitInput {
    /// Defaults to `user`. A guild entry is one submission for the guild.
    pub participant_type: Option<String>,
    /// Defaults to the caller. Naming a guild requires membership.
    pub participant_id: Option<Uuid>,
    pub artifact_url: String,
    pub artifact_type: String,
    pub secondary_url: Option<String>,
    pub summary: String,
    pub language: Option<String>,
    pub measured_value: Option<i32>,
}

/// Hand in an entry, or revise the one already handed in.
///
/// Revising in place rather than stacking: every one of these formats asks
/// for one answer, and "best of whatever you sent" is a different contest.
/// A revision clears any judgement, because a judged score belongs to the
/// artifact it was given for.
pub async fn submit(
    db: &PgPool,
    tournament_id: Uuid,
    submitter: Uuid,
    input: SubmitInput,
) -> Result<Submission, AppError> {
    let (kind, status, _direction): (String, String, String) =
        sqlx::query_as("SELECT kind, status, scoring_direction FROM tournaments WHERE id = $1")
            .bind(tournament_id)
            .fetch_optional(db)
            .await?
            .ok_or_else(|| AppError::NotFound("tournament not found".into()))?;

    if !KINDS_WITH_SUBMISSIONS.contains(&kind.as_str()) {
        return Err(AppError::Validation(format!(
            "a {kind} is scored from activity, not from a submission"
        )));
    }
    // Before it opens there is nothing to answer; after it concludes the
    // ranking is published and a late entry would rewrite it.
    if !matches!(status.as_str(), "active" | "registration") {
        return Err(AppError::Validation(format!(
            "this contest is {status} — submissions are only taken while it runs"
        )));
    }

    if !VALID_ARTIFACT_TYPES.contains(&input.artifact_type.as_str()) {
        return Err(AppError::Validation(format!(
            "artifact_type must be one of: {}",
            VALID_ARTIFACT_TYPES.join(", ")
        )));
    }
    check_url(&input.artifact_url, "artifact_url")?;
    if let Some(url) = &input.secondary_url {
        check_url(url, "secondary_url")?;
    }
    if input.summary.trim().is_empty() {
        return Err(AppError::Validation(
            "summary must say what was built and how".into(),
        ));
    }
    crate::validators::check_max_len(&input.summary, "summary", 4000)?;

    let measured = MEASURED_KINDS.contains(&kind.as_str());
    match (measured, input.measured_value) {
        // Without the number there is nothing to rank a golf entry by.
        (true, None) => {
            return Err(AppError::Validation(
                "a code golf entry must state its measured_value (character count)".into(),
            ));
        }
        (true, Some(n)) if n <= 0 => {
            return Err(AppError::Validation(
                "measured_value must be greater than zero".into(),
            ));
        }
        // A number on a judged entry would look like a score and is not one.
        (false, Some(_)) => {
            return Err(AppError::Validation(format!(
                "a {kind} is judged, not measured — leave measured_value empty"
            )));
        }
        _ => {}
    }

    let participant_type = input.participant_type.as_deref().unwrap_or("user");
    if !crate::services::tournament::VALID_PARTICIPANT_TYPES.contains(&participant_type) {
        return Err(AppError::Validation("invalid participant_type".into()));
    }
    let participant_id = input.participant_id.unwrap_or(submitter);

    if participant_type == "user" && participant_id != submitter {
        return Err(AppError::Validation(
            "submitting on somebody else's behalf is not allowed".into(),
        ));
    }
    if participant_type == "guild" {
        let member: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM guild_members
                             WHERE guild_id = $1 AND user_id = $2)",
        )
        .bind(participant_id)
        .bind(submitter)
        .fetch_one(db)
        .await?;
        if !member {
            return Err(AppError::Validation(
                "only a member can submit for a guild".into(),
            ));
        }
    }

    let submission: Submission = sqlx::query_as(
        r#"
        INSERT INTO tournament_submissions
            (tournament_id, participant_type, participant_id, submitted_by,
             artifact_url, artifact_type, secondary_url, summary, language,
             measured_value)
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
        ON CONFLICT (tournament_id, participant_type, participant_id) DO UPDATE
            SET submitted_by = EXCLUDED.submitted_by,
                artifact_url = EXCLUDED.artifact_url,
                artifact_type = EXCLUDED.artifact_type,
                secondary_url = EXCLUDED.secondary_url,
                summary = EXCLUDED.summary,
                language = EXCLUDED.language,
                measured_value = EXCLUDED.measured_value,
                -- A judgement belongs to the artifact it was given for.
                status = 'submitted',
                judge_score = NULL,
                judged_by = NULL,
                judged_at = NULL,
                judge_notes = NULL
        RETURNING *
        "#,
    )
    .bind(tournament_id)
    .bind(participant_type)
    .bind(participant_id)
    .bind(submitter)
    .bind(input.artifact_url.trim())
    .bind(&input.artifact_type)
    .bind(input.secondary_url.as_deref().map(str::trim))
    .bind(input.summary.trim())
    .bind(input.language.as_deref())
    .bind(input.measured_value)
    .fetch_one(db)
    .await
    .map_err(registration_error)?;

    // A measured contest needs no judge to know the standing, so the score
    // follows the entry immediately and the leaderboard is live.
    if measured && let Some(value) = submission.measured_value {
        crate::services::tournament::set_participant_score(
            db,
            tournament_id,
            participant_type,
            participant_id,
            value,
        )
        .await?;
    }

    Ok(submission)
}

/// The trigger from migration 0189 speaks in SQL; this says the same thing in
/// words the person who hit the endpoint can act on.
fn registration_error(e: sqlx::Error) -> AppError {
    let message = e.to_string();
    if message.contains("unregistered participant") {
        return AppError::Validation("register for this contest before submitting to it".into());
    }
    AppError::from(e)
}

fn check_url(url: &str, field: &str) -> Result<(), AppError> {
    let url = url.trim();
    if !url.starts_with("https://") {
        return Err(AppError::Validation(format!(
            "{field} must be an https URL"
        )));
    }
    crate::validators::check_max_len(url, field, 2000)
}

pub async fn list_submissions(
    db: &PgPool,
    tournament_id: Uuid,
) -> Result<Vec<Submission>, AppError> {
    let rows = sqlx::query_as::<_, Submission>(
        "SELECT * FROM tournament_submissions
          WHERE tournament_id = $1
          ORDER BY submitted_at ASC",
    )
    .bind(tournament_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

#[derive(Debug, Clone, Deserialize)]
pub struct JudgeInput {
    /// `accepted`, `rejected` or `disqualified`.
    pub status: String,
    /// 0..100. Required to accept a judged entry, refused on a measured one.
    pub judge_score: Option<i16>,
    pub judge_notes: Option<String>,
}

/// Record a judgement, and carry it onto the leaderboard.
pub async fn judge(
    db: &PgPool,
    submission_id: Uuid,
    judge_id: Uuid,
    input: JudgeInput,
) -> Result<Submission, AppError> {
    if !matches!(
        input.status.as_str(),
        "accepted" | "rejected" | "disqualified"
    ) {
        return Err(AppError::Validation(
            "status must be accepted, rejected or disqualified".into(),
        ));
    }

    let existing: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT s.tournament_id, t.kind
           FROM tournament_submissions s
           JOIN tournaments t ON t.id = s.tournament_id
          WHERE s.id = $1",
    )
    .bind(submission_id)
    .fetch_optional(db)
    .await?;
    let (tournament_id, kind) =
        existing.ok_or_else(|| AppError::NotFound("submission not found".into()))?;

    let measured = MEASURED_KINDS.contains(&kind.as_str());
    if measured && input.judge_score.is_some() {
        return Err(AppError::Validation(
            "a code golf is ranked by its measured value — a judge score would contradict it"
                .into(),
        ));
    }
    if !measured && input.status == "accepted" && input.judge_score.is_none() {
        return Err(AppError::Validation(
            "accepting a judged entry requires a score".into(),
        ));
    }
    if let Some(score) = input.judge_score
        && !(0..=100).contains(&score)
    {
        return Err(AppError::Validation(
            "judge_score must be between 0 and 100".into(),
        ));
    }

    let notes = input
        .judge_notes
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if matches!(input.status.as_str(), "rejected" | "disqualified") && notes.is_none() {
        return Err(AppError::Validation(
            "refusing an entry requires a reason the participant can read".into(),
        ));
    }
    if let Some(notes) = notes {
        crate::validators::check_max_len(notes, "judge_notes", 4000)?;
    }

    let mut tx = db.begin().await?;
    let submission: Submission = sqlx::query_as(
        r#"
        UPDATE tournament_submissions
           SET status = $2,
               judge_score = $3,
               judge_notes = $4,
               judged_by = $5,
               judged_at = NOW()
         WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(submission_id)
    .bind(&input.status)
    .bind(input.judge_score)
    .bind(notes)
    .bind(judge_id)
    .fetch_one(&mut *tx)
    .await?;

    // A refused entry scores nothing. Leaving its measured value on the
    // leaderboard would rank work that was thrown out.
    let score = match (input.status.as_str(), measured) {
        ("accepted", true) => submission.measured_value.unwrap_or(0),
        ("accepted", false) => submission.judge_score.unwrap_or(0) as i32,
        _ => 0,
    };
    sqlx::query(
        "UPDATE tournament_participants SET score = $1
          WHERE tournament_id = $2 AND participant_type = $3 AND participant_id = $4",
    )
    .bind(score)
    .bind(tournament_id)
    .bind(&submission.participant_type)
    .bind(submission.participant_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(submission)
}

// ═══════════════════════════════════════════════════════════════════
// Marathons
// ═══════════════════════════════════════════════════════════════════

/// Count what each participant actually merged inside the window, and put
/// that on the leaderboard.
///
/// Nobody submits anything to a marathon: the work is upstream contributions
/// that were going to happen anyway, and asking somebody to also file them
/// here would be asking twice. So the score is read from the deliverables,
/// recomputed on demand rather than stored — which is what makes the
/// leaderboard live, and what stops a revoked contribution from continuing
/// to count.
///
/// Only individual participants are counted. A guild marathon would need a
/// rule for what a guild's number means, and inventing one here would be
/// answering a question nobody has asked.
pub async fn recompute_marathon_scores(db: &PgPool, tournament_id: Uuid) -> Result<u64, AppError> {
    let updated = sqlx::query(
        r#"
        UPDATE tournament_participants p
           -- A correlated subquery rather than a LATERAL: Postgres refuses to
           -- let a LATERAL in the FROM clause reference the table being
           -- updated, and this one needs both `p` and `t`.
           SET score = (
                   SELECT count(*)::INT
                     FROM deliverables d
                    WHERE d.user_id = p.participant_id
                      AND d.artifact_type = 'pr_merged'
                      AND d.verification_status = 'verified'
                      AND d.revoked_at IS NULL
                      AND d.verified_at >= t.starts_at
                      AND d.verified_at <= t.ends_at
               )
          FROM tournaments t
         WHERE p.tournament_id = $1
           AND t.id = p.tournament_id
           AND t.kind = 'marathon'
           AND p.participant_type = 'user'
        "#,
    )
    .bind(tournament_id)
    .execute(db)
    .await?;
    Ok(updated.rows_affected())
}

/// Award the marathon badge to everybody who reached the target they signed
/// up for.
///
/// Granted rather than derived, and signed by whoever concluded the marathon:
/// the reason carries the edition and the count, so a reader who asks what
/// the badge means gets an answer instead of a name.
pub async fn grant_marathon_badges(
    db: &PgPool,
    tournament_id: Uuid,
    granted_by: Uuid,
) -> Result<u64, AppError> {
    let tournament: Option<(String, String, serde_json::Value)> =
        sqlx::query_as("SELECT kind, name, rules FROM tournaments WHERE id = $1")
            .bind(tournament_id)
            .fetch_optional(db)
            .await?;
    let Some((kind, name, rules)) = tournament else {
        return Err(AppError::NotFound("tournament not found".into()));
    };
    if kind != "marathon" {
        return Ok(0);
    }
    let Some(target) = rules.get("target_merged_prs").and_then(|v| v.as_i64()) else {
        // A marathon created before the rules column existed states no
        // target. Granting a badge for an engagement nobody recorded would
        // make the badge mean nothing.
        return Ok(0);
    };

    // The engine creates this on its first run. A marathon that concludes
    // before any badge was ever computed would otherwise grant nothing and
    // say it granted nothing, which reads like "nobody qualified".
    let sentinel: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO badges (slug, name, description, icon, category, condition_type, condition_value)
        VALUES ('_proof_engine', 'Proof Engine badge', 'Managed by badge_rules', '_', 'special', 'derived', 0)
        ON CONFLICT (slug) DO UPDATE SET name = EXCLUDED.name
        RETURNING id
        "#,
    )
    .fetch_one(db)
    .await?;

    let granted = sqlx::query(
        r#"
        INSERT INTO user_badges (user_id, badge_id, rule_id, granted_by, grant_reason)
        SELECT p.participant_id,
               $5,
               r.id,
               $3,
               format('%s : %s pull requests fusionnées, engagement de %s tenu',
                      $4::TEXT, p.score::TEXT, $2::TEXT)
          FROM tournament_participants p
          CROSS JOIN badge_rules r
         WHERE p.tournament_id = $1
           AND p.participant_type = 'user'
           AND p.score >= $2
           AND r.slug = 'code-oss-marathon-hero'
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(tournament_id)
    .bind(target as i32)
    .bind(granted_by)
    .bind(&name)
    .bind(sentinel)
    .execute(db)
    .await?;

    Ok(granted.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_golf_without_a_problem_is_not_a_contest() {
        assert!(validate_rules("code_golf", &json!({"language": "python"})).is_err());
        assert!(
            validate_rules(
                "code_golf",
                &json!({"language": "python", "problem_url": "https://example.test/p"})
            )
            .is_ok()
        );
    }

    #[test]
    fn an_empty_string_is_not_a_stated_rule() {
        assert!(validate_rules("hackathon", &json!({"theme": "   "})).is_err());
        assert!(validate_rules("hackathon", &json!({"theme": "offline first"})).is_ok());
    }

    #[test]
    fn a_tdd_contest_says_what_it_judges_before_it_judges() {
        assert!(
            validate_rules("tdd_contest", &json!({"problem_url": "https://x.test/p"})).is_err()
        );
        assert!(
            validate_rules(
                "tdd_contest",
                &json!({
                    "problem_url": "https://x.test/p",
                    "judging_criteria": ["test coverage", "readability"]
                })
            )
            .is_ok()
        );
    }

    #[test]
    fn an_empty_criteria_list_says_nothing() {
        assert!(
            validate_rules(
                "tdd_contest",
                &json!({"problem_url": "https://x.test/p", "judging_criteria": []})
            )
            .is_err()
        );
    }

    #[test]
    fn a_marathon_target_is_a_number_somebody_could_reach() {
        assert!(validate_rules("marathon", &json!({"target_merged_prs": 5})).is_ok());
        assert!(validate_rules("marathon", &json!({"target_merged_prs": 0})).is_err());
        assert!(validate_rules("marathon", &json!({"target_merged_prs": 5000})).is_err());
        assert!(validate_rules("marathon", &json!({"target_merged_prs": "beaucoup"})).is_err());
    }

    #[test]
    fn kinds_that_never_asked_for_rules_still_do_not() {
        assert!(validate_rules("individual", &json!({})).is_ok());
        assert!(validate_rules("guild_war", &json!({})).is_ok());
    }

    #[test]
    fn golf_is_the_only_thing_won_at_the_bottom() {
        use crate::services::tournament::scoring_direction_for;
        assert_eq!(scoring_direction_for("code_golf"), "lower_is_better");
        for kind in ["hackathon", "tdd_contest", "marathon", "individual"] {
            assert_eq!(scoring_direction_for(kind), "higher_is_better");
        }
    }
}
