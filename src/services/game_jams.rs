//! Game jams (migration 0581).
//!
//! A jam is a tournament with a theme, a deadline and community voting across
//! several axes. It reuses the tournament machinery — participants, ranks, the
//! podium — rather than reinventing it, and adds only what a jam needs on top:
//! the theme and its axes, the axis votes, and the post-mortem a submission
//! carries. Concluding a jam ranks it through [`crate::services::tournament`]
//! and then issues the jam attestations.
//!
//! Scoring is a submission's average vote across every axis, so a game that is
//! fun and rough does not lose to a polished one nobody enjoyed on sheer
//! turnout: a submission with five votes and a submission with fifty are judged
//! on the same scale.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::tournament;

/// The three jam kinds (migration 0581). A jam is created as one of these.
pub const JAM_KINDS: &[&str] = &["game_jam_48h", "game_jam_72h", "game_jam_week"];

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct GameJam {
    pub id: Uuid,
    pub tournament_id: Uuid,
    pub theme: String,
    pub theme_revealed_at: Option<DateTime<Utc>>,
    pub submission_deadline: DateTime<Utc>,
    pub voting_deadline: DateTime<Utc>,
    pub scoring_axes: serde_json::Value,
    pub solo_or_team: String,
    pub team_size_max: i16,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateJamInput {
    pub kind: String,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub theme: String,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub submission_deadline: DateTime<Utc>,
    pub voting_deadline: DateTime<Utc>,
    pub scoring_axes: Option<Vec<String>>,
    pub solo_or_team: Option<String>,
    pub team_size_max: Option<i16>,
}

/// Create a jam and the tournament behind it.
pub async fn create(
    db: &PgPool,
    creator: Uuid,
    input: CreateJamInput,
) -> Result<GameJam, AppError> {
    if !JAM_KINDS.contains(&input.kind.as_str()) {
        return Err(AppError::Validation(format!(
            "'{}' is not a game jam kind",
            input.kind
        )));
    }
    if input.theme.trim().is_empty() {
        return Err(AppError::Validation("a jam has a theme".into()));
    }
    if input.voting_deadline < input.submission_deadline {
        return Err(AppError::Validation(
            "voting closes on or after submissions close".into(),
        ));
    }
    let solo_or_team = input.solo_or_team.unwrap_or_else(|| "both".into());
    if !["solo_only", "team_only", "both"].contains(&solo_or_team.as_str()) {
        return Err(AppError::Validation("invalid solo_or_team".into()));
    }
    let axes = input
        .scoring_axes
        .filter(|a| !a.is_empty())
        .unwrap_or_else(|| {
            vec![
                "fun".into(),
                "theme".into(),
                "art".into(),
                "audio".into(),
                "innovation".into(),
            ]
        });

    // The kind requires the theme and the submission deadline as rules; pass
    // them so the tournament layer's rule check is satisfied.
    let rules = serde_json::json!({
        "theme": input.theme.trim(),
        "submission_deadline": input.submission_deadline.to_rfc3339(),
    });

    let t = tournament::create_tournament(
        db,
        creator,
        tournament::CreateTournamentInput {
            season_id: None,
            slug: input.slug,
            name: input.name,
            description: input.description,
            kind: input.kind,
            format: None,
            prize_pool_fragments: None,
            prize_pool_gp: None,
            sponsor_enterprise_id: None,
            sponsor_logo_url: None,
            sponsor_blurb: None,
            registration_opens_at: None,
            starts_at: input.starts_at,
            ends_at: input.ends_at,
            skill_domain: Some("game".into()),
            rules: Some(rules),
        },
    )
    .await?;

    Ok(sqlx::query_as(
        r#"
        INSERT INTO game_jams
            (tournament_id, theme, submission_deadline, voting_deadline,
             scoring_axes, solo_or_team, team_size_max)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING *
        "#,
    )
    .bind(t.id)
    .bind(input.theme.trim())
    .bind(input.submission_deadline)
    .bind(input.voting_deadline)
    .bind(serde_json::json!(axes))
    .bind(&solo_or_team)
    .bind(input.team_size_max.unwrap_or(4).max(1))
    .fetch_one(db)
    .await?)
}

/// One jam by id.
pub async fn get(db: &PgPool, jam_id: Uuid) -> Result<GameJam, AppError> {
    sqlx::query_as("SELECT * FROM game_jams WHERE id = $1")
        .bind(jam_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NotFound("jam not found".into()))
}

#[derive(Debug, Clone, Deserialize)]
pub struct SubmitInput {
    pub participant_type: String,
    pub participant_id: Uuid,
    pub artifact_url: String,
    pub summary: String,
    pub source_code_url: Option<String>,
    pub postmortem_md: Option<String>,
}

/// Register a participant and record their submission. Enforces the jam's
/// solo-or-team rule.
pub async fn submit(
    db: &PgPool,
    submitter: Uuid,
    jam_id: Uuid,
    input: SubmitInput,
) -> Result<Uuid, AppError> {
    let jam = get(db, jam_id).await?;
    match (jam.solo_or_team.as_str(), input.participant_type.as_str()) {
        ("solo_only", "guild") => return Err(AppError::Validation("this jam is solo only".into())),
        ("team_only", "user") => return Err(AppError::Validation("this jam is team only".into())),
        (_, "user") | (_, "guild") => {}
        _ => {
            return Err(AppError::Validation(
                "participant is a user or a guild".into(),
            ));
        }
    }
    if input.artifact_url.trim().is_empty() || input.summary.trim().is_empty() {
        return Err(AppError::Validation(
            "a submission needs a build URL and a summary".into(),
        ));
    }

    // Register on the tournament, then record the submission against it.
    match input.participant_type.as_str() {
        "user" => {
            tournament::register_individual(db, jam.tournament_id, input.participant_id).await?;
        }
        "guild" => {
            tournament::register_guild(db, jam.tournament_id, submitter, input.participant_id)
                .await?;
        }
        _ => unreachable!(),
    }

    let submission_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO tournament_submissions
            (tournament_id, participant_type, participant_id, submitted_by,
             artifact_url, artifact_type, summary)
        VALUES ($1, $2, $3, $4, $5, 'demo', $6)
        RETURNING id
        "#,
    )
    .bind(jam.tournament_id)
    .bind(&input.participant_type)
    .bind(input.participant_id)
    .bind(submitter)
    .bind(input.artifact_url.trim())
    .bind(input.summary.trim())
    .fetch_one(db)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO game_jam_submission_details (submission_id, source_code_url, postmortem_md)
        VALUES ($1, $2, $3)
        ON CONFLICT (submission_id) DO UPDATE SET
            source_code_url = EXCLUDED.source_code_url,
            postmortem_md = EXCLUDED.postmortem_md
        "#,
    )
    .bind(submission_id)
    .bind(input.source_code_url.as_deref().map(str::trim))
    .bind(input.postmortem_md.as_deref().map(str::trim))
    .execute(db)
    .await?;

    Ok(submission_id)
}

/// Cast — or change — a vote on one axis of one submission. One vote per person
/// per axis (migration 0581); a second overwrites the first.
pub async fn vote(
    db: &PgPool,
    voter: Uuid,
    jam_id: Uuid,
    submission_id: Uuid,
    axis: &str,
    score: i16,
) -> Result<(), AppError> {
    if !(1..=5).contains(&score) {
        return Err(AppError::Validation("a vote is 1 to 5".into()));
    }
    let jam = get(db, jam_id).await?;
    let axes: Vec<String> = serde_json::from_value(jam.scoring_axes).unwrap_or_default();
    if !axes.iter().any(|a| a == axis) {
        return Err(AppError::Validation(format!(
            "'{axis}' is not one of this jam's axes"
        )));
    }

    // The submission must belong to this jam's tournament, and a voter may not
    // vote on their own submission.
    let own: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM tournament_submissions s
             WHERE s.id = $1 AND s.tournament_id = $2
               AND (s.submitted_by = $3
                    OR (s.participant_type = 'user' AND s.participant_id = $3)))
        "#,
    )
    .bind(submission_id)
    .bind(jam.tournament_id)
    .bind(voter)
    .fetch_one(db)
    .await?;
    if own {
        return Err(AppError::Validation(
            "you cannot vote on your own submission".into(),
        ));
    }

    sqlx::query(
        r#"
        INSERT INTO game_jam_axis_votes (submission_id, voter_user_id, axis, score)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (submission_id, voter_user_id, axis) DO UPDATE SET
            score = EXCLUDED.score, voted_at = NOW()
        "#,
    )
    .bind(submission_id)
    .bind(voter)
    .bind(axis)
    .bind(score)
    .execute(db)
    .await?;
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct FinalizeReport {
    pub submissions_scored: i64,
    pub attestations_issued: usize,
}

/// Close a jam: score every submission by its average vote, rank the field
/// through the tournament layer, and issue the jam attestations. Idempotent
/// enough to re-run — scoring and attestations both converge — but the
/// tournament refuses a second conclusion, which is caught and treated as done.
pub async fn finalize(db: &PgPool, jam_id: Uuid) -> Result<FinalizeReport, AppError> {
    let jam = get(db, jam_id).await?;

    // Average vote per submission, on a 1–5 scale scaled to an integer the
    // tournament's rank order can use (higher is better for a jam).
    #[derive(sqlx::FromRow)]
    struct SubScore {
        participant_type: String,
        participant_id: Uuid,
        avg_x100: Option<f64>,
    }
    let scored: Vec<SubScore> = sqlx::query_as(
        r#"
        SELECT s.participant_type, s.participant_id,
               (avg(v.score) * 100)::float8 AS avg_x100
          FROM tournament_submissions s
          LEFT JOIN game_jam_axis_votes v ON v.submission_id = s.id
         WHERE s.tournament_id = $1
         GROUP BY s.participant_type, s.participant_id
        "#,
    )
    .bind(jam.tournament_id)
    .fetch_all(db)
    .await?;

    let submissions_scored = scored.len() as i64;
    for s in &scored {
        let score = s.avg_x100.unwrap_or(0.0).round() as i32;
        tournament::set_participant_score(
            db,
            jam.tournament_id,
            &s.participant_type,
            s.participant_id,
            score,
        )
        .await?;
    }

    // Rank the field. A jam already concluded is not an error here — the
    // attestations below are idempotent and worth re-running.
    match tournament::conclude_tournament(db, jam.tournament_id).await {
        Ok(_) => {}
        Err(AppError::Validation(msg)) if msg.contains("already concluded") => {}
        Err(e) => return Err(e),
    }

    let issued = crate::services::game_attestations::finalize_jam_attestations(db, jam_id).await?;

    // Recompute the proof of everyone who took part, so ranks and badges follow
    // the jam in the same pass — best-effort per participant.
    for s in &scored {
        let members = participant_members(db, &s.participant_type, s.participant_id).await?;
        for m in members {
            if let Err(e) = crate::services::proof_hooks::recompute_all_for_user(db, m).await {
                tracing::warn!(user = %m, error = %e, "proof recompute failed after jam finalize");
            }
        }
    }

    Ok(FinalizeReport {
        submissions_scored,
        attestations_issued: issued.len(),
    })
}

async fn participant_members(
    db: &PgPool,
    participant_type: &str,
    participant_id: Uuid,
) -> Result<Vec<Uuid>, AppError> {
    match participant_type {
        "user" => Ok(vec![participant_id]),
        "guild" => Ok(
            sqlx::query_scalar("SELECT user_id FROM guild_members WHERE guild_id = $1")
                .bind(participant_id)
                .fetch_all(db)
                .await?,
        ),
        _ => Ok(Vec::new()),
    }
}
