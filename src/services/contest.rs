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

/// What a contest format is, read from `tournament_kinds` (migration 0516).
///
/// Three Rust constants used to hold these three answers — which kinds take a
/// submission, which are measured, and which keys their rules must carry — and
/// each had to agree with the database and with the other two. Migration 0228
/// is the record of what happens when a list like that drifts: `code_golf`
/// passed the Rust check and was refused by the database, with an error naming
/// a constraint rather than the thing that was wrong.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct KindSpec {
    pub slug: String,
    pub name: String,
    pub expects_submission: bool,
    pub is_measured: bool,
    pub lower_is_better: bool,
    pub required_rule_keys: Vec<String>,
    /// A panel decides (migration 0438).
    pub is_juried: bool,
    /// Whoever shows up decides, unless the contest's rules say otherwise.
    pub allows_community_vote: bool,
}

/// One format, or a validation error naming the ones that exist.
///
/// A `Validation` rather than a `NotFound`: an unknown kind arrives from a
/// request body, and the caller needs the list rather than a 404.
pub async fn load_kind(db: &PgPool, kind: &str) -> Result<KindSpec, AppError> {
    let found: Option<KindSpec> = sqlx::query_as(
        "SELECT slug, name, expects_submission, is_measured, lower_is_better,
                required_rule_keys, is_juried, allows_community_vote
           FROM tournament_kinds WHERE slug = $1",
    )
    .bind(kind)
    .fetch_optional(db)
    .await?;

    match found {
        Some(spec) => Ok(spec),
        None => {
            let known: Vec<String> =
                sqlx::query_scalar("SELECT slug FROM tournament_kinds ORDER BY sort_order")
                    .fetch_all(db)
                    .await?;
            Err(AppError::Validation(format!(
                "kind must be one of: {}",
                known.join(", ")
            )))
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// Rules
// ═══════════════════════════════════════════════════════════════════

/// What a contest of this kind must state before anybody can enter it.
///
/// Checked at creation rather than at submission: a code golf with no problem
/// link is not a contest with a missing field, it is an announcement nobody
/// can act on, and the moment to catch that is before it is published.
pub fn validate_rules(spec: &KindSpec, rules: &serde_json::Value) -> Result<(), AppError> {
    if !rules.is_object() {
        return Err(AppError::Validation("rules must be an object".into()));
    }

    let kind = spec.slug.as_str();

    for key in &spec.required_rule_keys {
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
                "a brief contest needs at least 200 characters of brief: below that, the answers differ on things nobody stated"
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
    /// Whether the platform checked the artifact belongs to the entrant.
    ///
    /// Only a github.com URL can be checked — its owner segment against the
    /// entrant's connected login — so this is FALSE for a deployed demo, a
    /// hosted design file or a video, which nobody can attribute from a URL.
    /// FALSE means unchecked, never rejected: a juror reads it as "take this
    /// one on trust".
    pub artifact_verified: bool,
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

    let spec = load_kind(db, &kind).await?;
    if !spec.expects_submission {
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

    let measured = spec.is_measured;
    match (measured, input.measured_value) {
        // Without the number there is nothing to rank the entry by.
        //
        // The message names the kind rather than the metric: code golf was the
        // only measured format when this was written, so it said "character
        // count" to everybody — including, once the documentation jam arrived,
        // to somebody counting merged contributions.
        (true, None) => {
            return Err(AppError::Validation(format!(
                "a {kind} entry is ranked on a number and must state its measured_value"
            )));
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

    // Registration first: somebody who is not in this contest should be told
    // that, not told their artifact is wrong. Until now nothing said it until
    // the INSERT tripped a foreign key, which is too late to order the
    // messages by what the reader most needs to hear.
    let entered: bool = sqlx::query_scalar(
        "SELECT EXISTS (
           SELECT 1 FROM tournament_participants
            WHERE tournament_id = $1
              AND participant_type = $2
              AND participant_id = $3)",
    )
    .bind(tournament_id)
    .bind(participant_type)
    .bind(participant_id)
    .fetch_one(db)
    .await?;
    if !entered {
        return Err(AppError::Validation(
            "register for this contest before submitting to it".into(),
        ));
    }

    // Whose work is this. `artifact_url` was free text checked only for being
    // https, so an entrant could hand in a well-known project or a rival's
    // entry and win a prize pool with it.
    //
    // Both URLs, not only the declared artifact: a github.com link anywhere in
    // an entry has to be the entrant's, whatever the entry calls it. What is
    // hosted elsewhere cannot be attributed from a URL and is recorded as
    // unchecked rather than pretended over.
    //
    // Checked against `submitter` and not `participant_id`: a guild entry is
    // handed in by a person, and it is that person's account we can attribute
    // a GitHub URL to.
    let mut urls: Vec<&str> = vec![input.artifact_url.trim()];
    if let Some(url) = input.secondary_url.as_deref() {
        urls.push(url.trim());
    }
    let artifact_verified =
        crate::services::artifact_ownership::verify_entry_urls(db, submitter, &urls).await?;

    let submission: Submission = sqlx::query_as(
        r#"
        INSERT INTO tournament_submissions
            (tournament_id, participant_type, participant_id, submitted_by,
             artifact_url, artifact_type, secondary_url, summary, language,
             measured_value, artifact_verified)
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
        ON CONFLICT (tournament_id, participant_type, participant_id) DO UPDATE
            SET submitted_by = EXCLUDED.submitted_by,
                artifact_url = EXCLUDED.artifact_url,
                artifact_type = EXCLUDED.artifact_type,
                secondary_url = EXCLUDED.secondary_url,
                summary = EXCLUDED.summary,
                language = EXCLUDED.language,
                measured_value = EXCLUDED.measured_value,
                artifact_verified = EXCLUDED.artifact_verified,
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
    .bind(artifact_verified)
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

/// Who is reading the entries.
///
/// Only meaningful while a blind contest is still open; after the deadline
/// every reader is the same reader. Modelled as a type rather than an
/// `Option<Uuid>` because "anonymous", "an entrant" and "a juror" are three
/// different answers and a bare Option can only carry two.
#[derive(Debug, Clone, Copy)]
pub enum Reader {
    /// Nobody is logged in.
    Anonymous,
    /// Somebody logged in, entrant or not.
    User(Uuid),
    /// A juror or a member of staff. Never blinded: a panel that cannot read
    /// the entries cannot judge them.
    Unblinded,
}

/// Every entry in a contest.
///
/// Public by default, and that is the point: a contest whose entries cannot
/// be read is a contest whose result cannot be questioned.
///
/// A contest may declare a blind submission window (`blind_until_close`). It
/// narrows *when*, not *whether*: while the window is open a reader sees only
/// their own entry, and at the deadline the full field opens permanently. The
/// result stays as contestable as before — what is withheld is the ability to
/// read other people's work while there is still time to copy it, which is
/// the format's known failure and not something contestability needed.
pub async fn list_submissions(
    db: &PgPool,
    tournament_id: Uuid,
    reader: Reader,
) -> Result<Vec<Submission>, AppError> {
    // `blind_until_close` is inert once the contest is no longer open, which
    // is why the status is read here rather than trusting the flag alone.
    let blind: bool = sqlx::query_scalar(
        "SELECT blind_until_close AND status IN ('upcoming', 'registration', 'active')
           FROM tournaments WHERE id = $1",
    )
    .bind(tournament_id)
    .fetch_optional(db)
    .await?
    .unwrap_or(false);

    let own_only = match reader {
        _ if !blind => None,
        Reader::Unblinded => None,
        Reader::User(id) => Some(id),
        // Nobody to show an own entry to, so the window shows nothing.
        Reader::Anonymous => Some(Uuid::nil()),
    };

    let rows = sqlx::query_as::<_, Submission>(
        "SELECT * FROM tournament_submissions
          WHERE tournament_id = $1
            AND ($2::uuid IS NULL
                 OR (participant_type = 'user' AND participant_id = $2))
          ORDER BY submitted_at ASC",
    )
    .bind(tournament_id)
    .bind(own_only)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// Whether this account reads a blind contest unblinded.
///
/// A juror who was invited and has not declined, or anybody who may arbitrate
/// contests. Being an entrant grants nothing: that is the whole point.
pub async fn reads_unblinded(
    db: &PgPool,
    tournament_id: Uuid,
    user_id: Uuid,
) -> Result<bool, AppError> {
    let juror: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM tournament_juries
              WHERE tournament_id = $1 AND juror_user_id = $2 AND declined_at IS NULL
         )",
    )
    .bind(tournament_id)
    .bind(user_id)
    .fetch_one(db)
    .await?;
    if juror {
        return Ok(true);
    }

    // `jury_tournament` is the capability that already means "may sit on and
    // run a panel"; `admin` arbitrates. No new capability is invented for
    // this, because a third name for the same authority is how a permission
    // model rots.
    Ok(crate::middleware::capabilities::require_any_capability(
        db,
        user_id,
        &["admin", "jury_tournament"],
    )
    .await
    .is_ok())
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

    let measured = load_kind(db, &kind).await?.is_measured;
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

// ═══════════════════════════════════════════════════════════════════
// Juries
// ═══════════════════════════════════════════════════════════════════
//
// `judged_by` on a submission records who scored one entry. It does not say
// who was asked, who agreed, or who never answered — and a panel that never
// answered is the problem an organiser needs to see before the deadline
// rather than after it.

#[derive(Debug, Clone, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct JuryInvitation {
    pub tournament_id: Uuid,
    pub juror_user_id: Uuid,
    pub invited_by_user_id: Option<Uuid>,
    pub invited_at: chrono::DateTime<chrono::Utc>,
    pub accepted_at: Option<chrono::DateTime<chrono::Utc>>,
    pub declined_at: Option<chrono::DateTime<chrono::Utc>>,
    pub decline_reason: Option<String>,
}

/// Ask somebody to judge.
///
/// A juried contest whose panel cannot judge the craft produces a result that
/// means nothing, so the invitee has to hold review rights for the contest's
/// domain. For a domain scoped by family — design is the only one so far —
/// that is checked against the trade the contest is about.
pub async fn invite_juror(
    db: &PgPool,
    tournament_id: Uuid,
    inviter_id: Uuid,
    juror_id: Uuid,
) -> Result<JuryInvitation, AppError> {
    let contest: Option<(String, Option<String>)> =
        sqlx::query_as("SELECT kind, skill_domain FROM tournaments WHERE id = $1")
            .bind(tournament_id)
            .fetch_optional(db)
            .await?;
    let (kind, domain) =
        contest.ok_or_else(|| AppError::NotFound("tournament not found".into()))?;

    if !load_kind(db, &kind).await?.is_juried {
        return Err(AppError::Validation(format!(
            "a {kind} is not decided by a jury"
        )));
    }

    // Competence is checked against the contest's domain. Refusing an
    // invitation nobody could act on is cheaper than discovering it at
    // deliberation, when the deadline is a week away.
    if let Some(domain) = domain.as_deref() {
        let wildcard = format!("{domain}_reviewer:all");
        let legacy = format!("challenge_validator:{domain}");
        let holds_any_group: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM user_capabilities
                 WHERE user_id = $1
                   AND capability LIKE $2
                   AND revoked_at IS NULL
                   AND (expires_at IS NULL OR expires_at > NOW()))
            "#,
        )
        .bind(juror_id)
        .bind(format!("{domain}_reviewer:%"))
        .fetch_one(db)
        .await?;

        if !holds_any_group {
            crate::middleware::capabilities::require_any_capability(
                db,
                juror_id,
                &[wildcard.as_str(), legacy.as_str(), "admin"],
            )
            .await
            .map_err(|e| match e {
                AppError::Forbidden => AppError::Validation(
                    "this person holds no review rights in this contest's domain".into(),
                ),
                other => other,
            })?;
        }
    }

    let jury: JuryInvitation = sqlx::query_as(
        r#"
        INSERT INTO tournament_juries (tournament_id, juror_user_id, invited_by_user_id)
        VALUES ($1, $2, $3)
        ON CONFLICT (tournament_id, juror_user_id) DO UPDATE
            SET invited_at = NOW(), declined_at = NULL, decline_reason = NULL
        RETURNING *
        "#,
    )
    .bind(tournament_id)
    .bind(juror_id)
    .bind(inviter_id)
    .fetch_one(db)
    .await
    .map_err(map_guard_error)?;

    // An invitation expires, so it travels further than the app — the same
    // reasoning `guild.invitation` follows. Failure to deliver is logged and
    // not raised: the invitation is recorded, and an organiser retrying it
    // would only produce a second row.
    let contest_name: Option<String> =
        sqlx::query_scalar("SELECT name FROM tournaments WHERE id = $1")
            .bind(tournament_id)
            .fetch_optional(db)
            .await?;
    if let Some(name) = contest_name
        && let Err(e) = crate::services::notify::send(
            crate::services::notify::Ctx::db_only(db),
            crate::services::notify::Recipient::User(juror_id),
            "contest.jury_invited",
        )
        .arg("contest", name)
        .payload(serde_json::json!({ "tournament_id": tournament_id }))
        .execute()
        .await
    {
        tracing::warn!(%tournament_id, error = %e, "jury invitation notification not delivered");
    }

    Ok(jury)
}

/// Accept or decline. Declining because the subject is outside your
/// competence is the right answer, and saying so is what lets the organiser
/// widen the panel in time.
pub async fn respond_to_invitation(
    db: &PgPool,
    tournament_id: Uuid,
    juror_id: Uuid,
    accept: bool,
    decline_reason: Option<&str>,
) -> Result<JuryInvitation, AppError> {
    let reason = decline_reason.map(str::trim).filter(|s| !s.is_empty());
    if let Some(reason) = reason {
        crate::validators::check_max_len(reason, "decline_reason", 2000)?;
    }

    let jury: JuryInvitation = sqlx::query_as(
        r#"
        UPDATE tournament_juries
           SET accepted_at    = CASE WHEN $3 THEN NOW() ELSE NULL END,
               declined_at    = CASE WHEN $3 THEN NULL ELSE NOW() END,
               decline_reason = CASE WHEN $3 THEN NULL ELSE $4 END
         WHERE tournament_id = $1 AND juror_user_id = $2
     RETURNING *
        "#,
    )
    .bind(tournament_id)
    .bind(juror_id)
    .bind(accept)
    .bind(reason)
    .fetch_optional(db)
    .await
    .map_err(map_guard_error)?
    .ok_or_else(|| AppError::NotFound("no jury invitation for this contest".into()))?;
    Ok(jury)
}

/// The panel as an organiser needs to read it: who accepted, who declined,
/// who has not answered.
pub async fn list_jury(db: &PgPool, tournament_id: Uuid) -> Result<Vec<JuryInvitation>, AppError> {
    let rows = sqlx::query_as::<_, JuryInvitation>(
        "SELECT * FROM tournament_juries WHERE tournament_id = $1 ORDER BY invited_at ASC",
    )
    .bind(tournament_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// Only somebody who accepted may score.
///
/// Checked here rather than inside `judge`, which also serves the formats
/// scored by an admin where there is no panel at all.
pub async fn require_accepted_juror(
    db: &PgPool,
    tournament_id: Uuid,
    juror_id: Uuid,
) -> Result<(), AppError> {
    let accepted: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM tournament_juries
              WHERE tournament_id = $1 AND juror_user_id = $2 AND accepted_at IS NOT NULL)",
    )
    .bind(tournament_id)
    .bind(juror_id)
    .fetch_one(db)
    .await?;
    if !accepted {
        return Err(AppError::Forbidden);
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════
// The room
// ═══════════════════════════════════════════════════════════════════
//
// A jury answers "is this good craft". The community answers "does this
// land". Neither replaces the other, and a contest says in its rules which
// one decides, or in what proportion.

/// One vote per account per contest, moved rather than stacked.
///
/// Eligibility — the account age floor, the self-vote, the withdrawn entry —
/// is enforced by the trigger from migration 0509, because a vote arrives
/// from more places than this function.
pub async fn cast_community_vote(
    db: &PgPool,
    tournament_id: Uuid,
    voter_id: Uuid,
    submission_id: Uuid,
) -> Result<(), AppError> {
    let kind: Option<String> = sqlx::query_scalar("SELECT kind FROM tournaments WHERE id = $1")
        .bind(tournament_id)
        .fetch_optional(db)
        .await?;
    let kind = kind.ok_or_else(|| AppError::NotFound("tournament not found".into()))?;
    if !load_kind(db, &kind).await?.allows_community_vote
        && !community_vote_enabled_by_rules(db, tournament_id).await?
    {
        return Err(AppError::Validation(format!(
            "a {kind} is not open to a community vote unless its rules say so"
        )));
    }

    sqlx::query(
        r#"
        INSERT INTO tournament_community_votes (tournament_id, voter_user_id, submission_id)
        VALUES ($1, $2, $3)
        ON CONFLICT (tournament_id, voter_user_id) DO UPDATE
            SET submission_id = EXCLUDED.submission_id, voted_at = NOW()
        "#,
    )
    .bind(tournament_id)
    .bind(voter_id)
    .bind(submission_id)
    .execute(db)
    .await
    .map_err(map_guard_error)?;
    Ok(())
}

async fn community_vote_enabled_by_rules(
    db: &PgPool,
    tournament_id: Uuid,
) -> Result<bool, AppError> {
    let mode: Option<String> =
        sqlx::query_scalar("SELECT rules ->> 'voting_mode' FROM tournaments WHERE id = $1")
            .bind(tournament_id)
            .fetch_optional(db)
            .await?
            .flatten();
    Ok(matches!(
        mode.as_deref(),
        Some("community") | Some("hybrid")
    ))
}

/// The live standing by vote count.
pub async fn community_ranking(
    db: &PgPool,
    tournament_id: Uuid,
) -> Result<Vec<(Uuid, i64)>, AppError> {
    let rows: Vec<(Uuid, i64)> = sqlx::query_as(
        r#"
        SELECT s.id, count(v.voter_user_id)::bigint
          FROM tournament_submissions s
          LEFT JOIN tournament_community_votes v ON v.submission_id = s.id
         WHERE s.tournament_id = $1
           AND s.status NOT IN ('rejected', 'disqualified')
         GROUP BY s.id, s.submitted_at
         ORDER BY count(v.voter_user_id) DESC, s.submitted_at ASC
        "#,
    )
    .bind(tournament_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// Votes cast on one entry inside a short window.
///
/// A spike is the cheap signal that a vote is being bought or botted. It is a
/// reason to look, never a verdict: this reports, it does not disqualify.
pub async fn detect_vote_bursts(
    db: &PgPool,
    tournament_id: Uuid,
    window_minutes: i32,
    threshold: i64,
) -> Result<Vec<(Uuid, i64)>, AppError> {
    let rows: Vec<(Uuid, i64)> = sqlx::query_as(
        r#"
        SELECT submission_id, count(*)::bigint AS n
          FROM tournament_community_votes
         WHERE tournament_id = $1
           AND voted_at > NOW() - make_interval(mins => $2)
         GROUP BY submission_id
        HAVING count(*) >= $3
         ORDER BY n DESC
        "#,
    )
    .bind(tournament_id)
    .bind(window_minutes.clamp(1, 1440))
    .bind(threshold.max(1))
    .fetch_all(db)
    .await?;
    Ok(rows)
}

// ═══════════════════════════════════════════════════════════════════
// Scoring
// ═══════════════════════════════════════════════════════════════════

/// Weight of the jury half when a contest does not say otherwise. Craft is
/// judged by people who do the work, reach is judged by the audience, and
/// craft carries more.
pub const DEFAULT_JURY_WEIGHT: f64 = 0.60;

/// Turn jury scores and community votes into the participant score that
/// `tournament::conclude_tournament` ranks and pays on.
///
/// Nothing new decides the winner: the existing conclusion assigns ranks from
/// `tournament_participants.score` and distributes the prize pool 50/30/20.
/// This only computes the number it reads.
pub async fn recompute_contest_scores(db: &PgPool, tournament_id: Uuid) -> Result<u64, AppError> {
    let contest: Option<(String, serde_json::Value)> =
        sqlx::query_as("SELECT kind, rules FROM tournaments WHERE id = $1")
            .bind(tournament_id)
            .fetch_optional(db)
            .await?;
    let (kind, rules) = contest.ok_or_else(|| AppError::NotFound("tournament not found".into()))?;

    let spec = load_kind(db, &kind).await?;
    if !spec.expects_submission {
        return Err(AppError::Validation(format!(
            "a {kind} is not scored from submissions"
        )));
    }

    let default_mode = if spec.allows_community_vote {
        "community"
    } else {
        "jury"
    };
    let mode = rules
        .get("voting_mode")
        .and_then(|v| v.as_str())
        .unwrap_or(default_mode)
        .to_string();
    let jury_weight = rules
        .get("jury_weight")
        .and_then(|v| v.as_f64())
        .filter(|w| (0.0..=1.0).contains(w))
        .unwrap_or(DEFAULT_JURY_WEIGHT);

    // Entries still in the running, with both halves of what decides them.
    let entries: Vec<(String, Uuid, Option<i16>, i64)> = sqlx::query_as(
        r#"
        SELECT s.participant_type, s.participant_id, s.judge_score,
               (SELECT count(*)::bigint FROM tournament_community_votes v
                 WHERE v.submission_id = s.id)
          FROM tournament_submissions s
         WHERE s.tournament_id = $1
           AND s.status NOT IN ('rejected', 'disqualified')
        "#,
    )
    .bind(tournament_id)
    .fetch_all(db)
    .await?;

    if entries.is_empty() {
        return Ok(0);
    }

    // Normalised against the best entry in this contest, not an absolute
    // maximum: a contest whose best answer drew forty votes and one whose
    // best drew four thousand must rank the same way.
    let best_votes = entries.iter().map(|(_, _, _, v)| *v).max().unwrap_or(0);
    let mut updated = 0u64;

    for (ptype, pid, judge_score, votes) in &entries {
        let score = blended_score(&mode, jury_weight, *judge_score, *votes, best_votes);
        let affected = sqlx::query(
            r#"
            UPDATE tournament_participants
               SET score = $1
             WHERE tournament_id = $2 AND participant_type = $3 AND participant_id = $4
            "#,
        )
        .bind(score)
        .bind(tournament_id)
        .bind(ptype)
        .bind(pid)
        .execute(db)
        .await?;
        updated += affected.rows_affected();
    }

    Ok(updated)
}

/// The score a participant is ranked on, from 0 to 100.
///
/// Pure, so the blending rules are testable without a database.
///
/// An entry nobody scored gets zero from the jury half rather than being
/// excluded: a submission the panel never opened must not outrank one they
/// looked at and found weak.
pub fn blended_score(
    mode: &str,
    jury_weight: f64,
    judge_score: Option<i16>,
    votes: i64,
    best_votes: i64,
) -> i32 {
    let jury = judge_score.unwrap_or(0) as f64 / 100.0;
    let community = if best_votes > 0 {
        votes as f64 / best_votes as f64
    } else {
        0.0
    };
    let blended = match mode {
        "community" => community,
        "hybrid" => jury_weight * jury + (1.0 - jury_weight) * community,
        // "jury" and anything unrecognised: a contest that does not say is a
        // juried contest, which is the format's default.
        _ => jury,
    };
    (blended * 100.0).round().clamp(0.0, 100.0) as i32
}

/// Constraint violations raised by migration 0509 are written for humans on
/// purpose; passing them through is more useful than a generic 500.
fn map_guard_error(e: sqlx::Error) -> AppError {
    match &e {
        sqlx::Error::Database(db_err) => AppError::Conflict(db_err.message().to_string()),
        _ => AppError::from(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A format as the table describes it, built here so the rule checking
    /// stays a pure function with pure tests. What the table actually holds is
    /// asserted against these in `tests/test_contest_kinds.rs`.
    fn spec(slug: &str, keys: &[&str]) -> KindSpec {
        KindSpec {
            slug: slug.to_string(),
            name: slug.to_string(),
            expects_submission: true,
            is_measured: slug == "code_golf",
            lower_is_better: slug == "code_golf",
            required_rule_keys: keys.iter().map(|k| k.to_string()).collect(),
            is_juried: matches!(slug, "hackathon" | "tdd_contest" | "brief_contest"),
            allows_community_vote: slug == "duel",
        }
    }

    #[test]
    fn a_golf_without_a_problem_is_not_a_contest() {
        let golf = spec("code_golf", &["language", "problem_url"]);
        assert!(validate_rules(&golf, &json!({"language": "python"})).is_err());
        assert!(
            validate_rules(
                &golf,
                &json!({"language": "python", "problem_url": "https://example.test/p"})
            )
            .is_ok()
        );
    }

    #[test]
    fn an_empty_string_is_not_a_stated_rule() {
        let hackathon = spec("hackathon", &["theme"]);
        assert!(validate_rules(&hackathon, &json!({"theme": "   "})).is_err());
        assert!(validate_rules(&hackathon, &json!({"theme": "offline first"})).is_ok());
    }

    #[test]
    fn a_tdd_contest_says_what_it_judges_before_it_judges() {
        let tdd = spec("tdd_contest", &["problem_url", "judging_criteria"]);
        assert!(validate_rules(&tdd, &json!({"problem_url": "https://x.test/p"})).is_err());
        assert!(
            validate_rules(
                &tdd,
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
        let tdd = spec("tdd_contest", &["problem_url", "judging_criteria"]);
        assert!(
            validate_rules(
                &tdd,
                &json!({"problem_url": "https://x.test/p", "judging_criteria": []})
            )
            .is_err()
        );
    }

    #[test]
    fn a_marathon_target_is_a_number_somebody_could_reach() {
        let marathon = spec("marathon", &["target_merged_prs"]);
        assert!(validate_rules(&marathon, &json!({"target_merged_prs": 5})).is_ok());
        assert!(validate_rules(&marathon, &json!({"target_merged_prs": 0})).is_err());
        assert!(validate_rules(&marathon, &json!({"target_merged_prs": 5000})).is_err());
        assert!(validate_rules(&marathon, &json!({"target_merged_prs": "beaucoup"})).is_err());
    }

    #[test]
    fn kinds_that_never_asked_for_rules_still_do_not() {
        assert!(validate_rules(&spec("individual", &[]), &json!({})).is_ok());
        assert!(validate_rules(&spec("guild_war", &[]), &json!({})).is_ok());
    }

    #[test]
    fn the_direction_follows_the_row_rather_than_the_caller() {
        use crate::services::tournament::scoring_direction_for;
        assert_eq!(
            scoring_direction_for(&spec("code_golf", &[])),
            "lower_is_better"
        );
        for kind in ["hackathon", "tdd_contest", "marathon", "individual"] {
            assert_eq!(scoring_direction_for(&spec(kind, &[])), "higher_is_better");
        }
    }

    #[test]
    fn a_brief_contest_needs_a_real_brief() {
        // A subject line is not a brief: the answers would differ on things
        // nobody stated, and the jury would be arbitrating a question that
        // was never asked.
        assert!(
            validate_rules(
                &spec("brief_contest", &["brief", "judging_criteria"]),
                &json!({"brief": "Fais un logo.", "judging_criteria": ["distinction"]})
            )
            .is_err()
        );
        let long_brief = "a".repeat(250);
        assert!(
            validate_rules(
                &spec("brief_contest", &["brief", "judging_criteria"]),
                &json!({"brief": long_brief, "judging_criteria": ["distinction"]})
            )
            .is_ok()
        );
    }

    #[test]
    fn a_brief_contest_says_what_it_weighs_before_it_weighs_it() {
        let long_brief = "a".repeat(250);
        assert!(
            validate_rules(
                &spec("brief_contest", &["brief", "judging_criteria"]),
                &json!({"brief": long_brief})
            )
            .is_err()
        );
    }

    #[test]
    fn a_duel_is_bounded_in_time() {
        assert!(
            validate_rules(
                &spec("duel", &["task", "closes_at"]),
                &json!({"task": "Un logo"})
            )
            .is_err()
        );
        assert!(
            validate_rules(
                &spec("duel", &["task", "duration_hours"]),
                &json!({"task": "Un logo", "duration_hours": 0})
            )
            .is_err()
        );
        assert!(
            validate_rules(
                &spec("duel", &["task", "duration_hours"]),
                &json!({"task": "Un logo", "duration_hours": 500})
            )
            .is_err()
        );
        assert!(
            validate_rules(
                &spec("duel", &["task", "duration_hours"]),
                &json!({"task": "Un logo", "duration_hours": 48})
            )
            .is_ok()
        );
    }

    #[test]
    fn jury_mode_ignores_the_room() {
        // Ninety from the panel and no votes beats zero from the panel and
        // every vote in the building.
        assert_eq!(blended_score("jury", 0.6, Some(90), 0, 1000), 90);
        assert_eq!(blended_score("jury", 0.6, Some(0), 1000, 1000), 0);
    }

    #[test]
    fn community_mode_ignores_the_panel() {
        assert_eq!(blended_score("community", 0.6, Some(100), 0, 10), 0);
        assert_eq!(blended_score("community", 0.6, None, 10, 10), 100);
    }

    #[test]
    fn hybrid_blends_in_the_declared_proportion() {
        // Perfect panel, no votes: exactly the jury weight.
        assert_eq!(blended_score("hybrid", 0.6, Some(100), 0, 10), 60);
        // No panel score, every vote: exactly the remainder.
        assert_eq!(blended_score("hybrid", 0.6, Some(0), 10, 10), 40);
        // Flip the weight and the balance flips with it.
        assert_eq!(blended_score("hybrid", 0.3, Some(100), 0, 10), 30);
    }

    #[test]
    fn votes_are_normalised_against_the_best_entry() {
        // The same relative standing must score the same whether the contest
        // drew ten votes or ten thousand.
        assert_eq!(
            blended_score("community", 0.6, None, 5, 10),
            blended_score("community", 0.6, None, 5_000, 10_000)
        );
    }

    #[test]
    fn a_contest_nobody_voted_in_does_not_divide_by_zero() {
        assert_eq!(blended_score("community", 0.6, None, 0, 0), 0);
    }

    #[test]
    fn an_unscored_entry_does_not_outrank_a_scored_one() {
        // A submission the panel never opened must not beat one they looked
        // at and found weak.
        assert!(blended_score("jury", 0.6, None, 0, 0) < blended_score("jury", 0.6, Some(1), 0, 0));
    }

    #[test]
    fn an_unknown_voting_mode_falls_back_to_the_jury() {
        // A rules typo must not silently hand the contest to the room.
        assert_eq!(blended_score("populaire", 0.6, Some(80), 0, 10), 80);
    }

    // The invariant that used to live here — no format defaults to both a
    // jury and the room, and neither is decided on entries it never collects
    // — is a CHECK on `tournament_kinds` since migration 0438. A unit test
    // over a Rust constant could only ever have checked the copy.
}
