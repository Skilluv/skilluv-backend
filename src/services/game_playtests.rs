//! Playtests — the game domain's first-class evidence (migration 0580).
//!
//! Every other domain validates on a reviewer's judgement and a passing build.
//! Game does not accept "it runs and I like it": a game slice reaches
//! `validated` only after real players have touched it. This module owns that
//! rule — the gate of at least three playtests with an average fun score of
//! three — because the same moment that meets it must also create the verified
//! deliverable, credit the fragments and issue the attestation, and a database
//! trigger could do none of those.
//!
//! A creator opens a recruitment (an open call for testers); testers submit one
//! verdict each, editable in place; a reviewer validates the slice once the
//! gate is met.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::errors::AppError;

/// The domain floor: three testers, and an average fun score of at least this.
pub const MIN_PLAYTESTS: i64 = 3;
pub const MIN_FUN_AVERAGE: f64 = 3.0;

const DIFFICULTY_PERCEPTIONS: &[&str] = &["too_easy", "balanced", "too_hard", "unclear"];

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Playtest {
    pub id: Uuid,
    pub slice_id: Uuid,
    pub playtester_user_id: Uuid,
    pub session_duration_min: Option<i16>,
    pub fun_score: i16,
    pub clarity_score: i16,
    pub difficulty_perception: String,
    pub bugs_encountered_md: Option<String>,
    pub suggestions_md: Option<String>,
    pub would_play_again: bool,
    pub submitted_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Recruitment {
    pub id: Uuid,
    pub slice_id: Uuid,
    pub opened_by: Uuid,
    pub build_url: String,
    pub brief_md: String,
    pub testers_wanted: i16,
    pub allows_anonymous: bool,
    pub opened_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
}

/// Where a slice stands against the validation gate.
#[derive(Debug, Clone, Serialize)]
pub struct GateStatus {
    pub playtests: i64,
    pub average_fun: f64,
    pub meets_gate: bool,
}

// ═══════════════════════════════════════════════════════════════════
// Recruitments
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Deserialize)]
pub struct OpenRecruitmentInput {
    pub slice_id: Uuid,
    pub build_url: Option<String>,
    pub brief_md: String,
    pub testers_wanted: Option<i16>,
    pub allows_anonymous: Option<bool>,
}

/// Open a call for playtesters on a slice. The build URL defaults to the
/// slice's own playable URL. One open recruitment per slice at a time — the
/// exclusion constraint in 0580 enforces it; this turns the raw error into a
/// sentence.
pub async fn open_recruitment(
    db: &PgPool,
    opener: Uuid,
    input: OpenRecruitmentInput,
) -> Result<Recruitment, AppError> {
    if input.brief_md.trim().is_empty() {
        return Err(AppError::Validation(
            "tell testers what to look for — a recruitment needs a brief".into(),
        ));
    }
    let wanted = input.testers_wanted.unwrap_or(3);
    if wanted < 3 {
        return Err(AppError::Validation(
            "the domain floor is three testers".into(),
        ));
    }

    let build_url = match input.build_url {
        Some(u) if !u.trim().is_empty() => u.trim().to_string(),
        _ => sqlx::query_scalar::<_, Option<String>>(
            "SELECT game_playable_url FROM project_slices WHERE id = $1",
        )
        .bind(input.slice_id)
        .fetch_optional(db)
        .await?
        .flatten()
        .ok_or_else(|| {
            AppError::Validation(
                "no build URL: give one, or set the slice's playable URL first".into(),
            )
        })?,
    };

    let already_open: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM game_playtest_recruitments
                         WHERE slice_id = $1 AND closed_at IS NULL)",
    )
    .bind(input.slice_id)
    .fetch_one(db)
    .await?;
    if already_open {
        return Err(AppError::Validation(
            "this slice already has an open recruitment".into(),
        ));
    }

    Ok(sqlx::query_as(
        r#"
        INSERT INTO game_playtest_recruitments
            (slice_id, opened_by, build_url, brief_md, testers_wanted, allows_anonymous)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING *
        "#,
    )
    .bind(input.slice_id)
    .bind(opener)
    .bind(&build_url)
    .bind(input.brief_md.trim())
    .bind(wanted)
    .bind(input.allows_anonymous.unwrap_or(false))
    .fetch_one(db)
    .await?)
}

/// Close a recruitment. Only the creator who opened it may.
pub async fn close_recruitment(
    db: &PgPool,
    id: Uuid,
    opener: Uuid,
) -> Result<Recruitment, AppError> {
    sqlx::query_as(
        r#"
        UPDATE game_playtest_recruitments
           SET closed_at = NOW()
         WHERE id = $1 AND opened_by = $2 AND closed_at IS NULL
        RETURNING *
        "#,
    )
    .bind(id)
    .bind(opener)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| AppError::NotFound("no open recruitment of yours with that id".into()))
}

// ═══════════════════════════════════════════════════════════════════
// Playtests
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Deserialize)]
pub struct SubmitInput {
    pub slice_id: Uuid,
    pub session_duration_min: Option<i16>,
    pub fun_score: i16,
    pub clarity_score: i16,
    pub difficulty_perception: String,
    pub bugs_encountered_md: Option<String>,
    pub suggestions_md: Option<String>,
    pub would_play_again: bool,
}

/// Submit — or edit — a verdict on a slice. One per person per slice; a second
/// submission updates the first, because a verdict is a verdict, not a ballot.
/// A creator may not playtest their own slice.
pub async fn submit(db: &PgPool, tester: Uuid, input: SubmitInput) -> Result<Playtest, AppError> {
    for (name, v) in [
        ("fun_score", input.fun_score),
        ("clarity_score", input.clarity_score),
    ] {
        if !(1..=5).contains(&v) {
            return Err(AppError::Validation(format!("{name} is 1 to 5")));
        }
    }
    if !DIFFICULTY_PERCEPTIONS.contains(&input.difficulty_perception.as_str()) {
        return Err(AppError::Validation(
            "difficulty is one of too_easy, balanced, too_hard, unclear".into(),
        ));
    }
    if let Some(d) = input.session_duration_min {
        if d < 0 {
            return Err(AppError::Validation(
                "a session length is not negative".into(),
            ));
        }
    }

    if let Some(creator) = slice_creator(db, input.slice_id).await? {
        if creator == tester {
            return Err(AppError::Validation(
                "you cannot playtest your own slice".into(),
            ));
        }
    }

    let row: Playtest = sqlx::query_as(
        r#"
        INSERT INTO game_playtests
            (slice_id, playtester_user_id, session_duration_min, fun_score,
             clarity_score, difficulty_perception, bugs_encountered_md,
             suggestions_md, would_play_again)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        ON CONFLICT (slice_id, playtester_user_id) DO UPDATE SET
            session_duration_min = EXCLUDED.session_duration_min,
            fun_score = EXCLUDED.fun_score,
            clarity_score = EXCLUDED.clarity_score,
            difficulty_perception = EXCLUDED.difficulty_perception,
            bugs_encountered_md = EXCLUDED.bugs_encountered_md,
            suggestions_md = EXCLUDED.suggestions_md,
            would_play_again = EXCLUDED.would_play_again,
            submitted_at = NOW()
        RETURNING *
        "#,
    )
    .bind(input.slice_id)
    .bind(tester)
    .bind(input.session_duration_min)
    .bind(input.fun_score)
    .bind(input.clarity_score)
    .bind(&input.difficulty_perception)
    .bind(input.bugs_encountered_md.as_deref())
    .bind(input.suggestions_md.as_deref())
    .bind(input.would_play_again)
    .fetch_one(db)
    .await?;

    // Giving a playtest can cross the playtest-hero milestone and moves the
    // tester's own craft score (playtests_contributed). Recompute so it does
    // not wait for the next sweep — best-effort.
    if let Err(e) = crate::services::game_attestations::issue_playtest_hero(db, tester).await {
        tracing::warn!(user = %tester, error = %e, "playtest hero check failed");
    }
    if let Err(e) = crate::services::game_profile::recompute(db, tester).await {
        tracing::warn!(user = %tester, error = %e, "tester score recompute failed");
    }

    Ok(row)
}

/// The playtests on a slice, newest first.
pub async fn list_for_slice(db: &PgPool, slice_id: Uuid) -> Result<Vec<Playtest>, AppError> {
    Ok(sqlx::query_as(
        "SELECT * FROM game_playtests WHERE slice_id = $1 ORDER BY submitted_at DESC",
    )
    .bind(slice_id)
    .fetch_all(db)
    .await?)
}

// ═══════════════════════════════════════════════════════════════════
// The validation gate
// ═══════════════════════════════════════════════════════════════════

/// Where a slice stands: how many playtests, their average fun, and whether the
/// gate is met.
pub async fn gate_status(db: &PgPool, slice_id: Uuid) -> Result<GateStatus, AppError> {
    let row: (i64, Option<f64>) = sqlx::query_as(
        "SELECT count(*), avg(fun_score)::float8 FROM game_playtests WHERE slice_id = $1",
    )
    .bind(slice_id)
    .fetch_one(db)
    .await?;
    let playtests = row.0;
    let average_fun = row.1.unwrap_or(0.0);
    Ok(GateStatus {
        playtests,
        average_fun,
        meets_gate: playtests >= MIN_PLAYTESTS && average_fun >= MIN_FUN_AVERAGE,
    })
}

/// The user who owns a slice's work, when the project is one person's. `None`
/// for a guild-owned (team) project, whose validation flows through
/// [`crate::services::game_composition`] instead.
pub async fn slice_creator(db: &PgPool, slice_id: Uuid) -> Result<Option<Uuid>, AppError> {
    Ok(sqlx::query_scalar(
        r#"
        SELECT p.owner_id
          FROM project_slices ps
          JOIN projects p ON p.id = ps.project_id
         WHERE ps.id = $1 AND p.owner_type = 'user'
        "#,
    )
    .bind(slice_id)
    .fetch_optional(db)
    .await?)
}

/// The deliverable artifact type a game subtype produces.
fn artifact_type_for_subtype(subtype: &str) -> &'static str {
    match subtype {
        "build_playable" | "code_module" => "playable_build",
        "asset_3d" | "asset_2d_sprite" | "animation_pack" => "game_asset",
        "level_pack" => "game_scene",
        "gdd_document" => "documentation",
        "mod_package" => "game_mod",
        _ => "other",
    }
}

/// Validate a game slice: the gate is met, a reviewer signs off, and the slice
/// becomes a verified deliverable — fragments credited, skills propagated,
/// `game_artifact_validated` issued, the creator's proof recomputed. The same
/// shape design uses (a reviewer's approval creates the verified deliverable),
/// with the playtest gate in front of it.
pub async fn validate_slice(
    db: &PgPool,
    slice_id: Uuid,
    reviewer_id: Uuid,
) -> Result<Uuid, AppError> {
    #[derive(sqlx::FromRow)]
    struct Slice {
        slice_type: String,
        subtype: Option<String>,
        playable_url: Option<String>,
        fragments_reward: i32,
        creator: Option<Uuid>,
    }

    let slice: Slice = sqlx::query_as(
        r#"
        SELECT ps.slice_type,
               ps.game_artifact_subtype AS subtype,
               ps.game_playable_url AS playable_url,
               ps.fragments_reward,
               (SELECT p.owner_id FROM projects p
                 WHERE p.id = ps.project_id AND p.owner_type = 'user') AS creator
          FROM project_slices ps
         WHERE ps.id = $1
        "#,
    )
    .bind(slice_id)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| AppError::NotFound("slice not found".into()))?;

    if slice.slice_type != "game_artifact" {
        return Err(AppError::Validation("not a game slice".into()));
    }
    let Some(subtype) = slice.subtype.as_deref() else {
        return Err(AppError::Validation(
            "a game slice names its subtype".into(),
        ));
    };
    let Some(creator) = slice.creator else {
        return Err(AppError::Validation(
            "this is a team slice — validate it through the team composition flow".into(),
        ));
    };
    if creator == reviewer_id {
        return Err(AppError::Validation(
            "a slice is validated by someone other than its creator".into(),
        ));
    }

    let gate = gate_status(db, slice_id).await?;
    if !gate.meets_gate {
        return Err(AppError::Validation(format!(
            "a game slice needs at least {MIN_PLAYTESTS} playtests with an average fun \
             score of {MIN_FUN_AVERAGE} before it is validated — it has {} at {:.1}",
            gate.playtests, gate.average_fun
        )));
    }

    let url = slice.playable_url.ok_or_else(|| {
        AppError::Validation("the slice has no playable URL to record as its artefact".into())
    })?;

    let mut tx: Transaction<'_, Postgres> = db.begin().await?;

    // Idempotent: a slice already carrying a verified deliverable is done.
    let existing: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM deliverables
          WHERE slice_id = $1 AND user_id = $2 AND revoked_at IS NULL LIMIT 1",
    )
    .bind(slice_id)
    .bind(creator)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(id) = existing {
        tx.rollback().await?;
        return Ok(id);
    }

    let deliverable_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO deliverables
            (slice_id, user_id, artifact_type, artifact_url,
             verifiable_by, verification_status, verified_at, verified_by_user_id,
             fragments_awarded, credits_awarded, public, submitted_at, created_at)
        VALUES ($1, $2, $3, $4, 'human_review', 'verified', NOW(), $5,
                $6, 0, TRUE, NOW(), NOW())
        RETURNING id
        "#,
    )
    .bind(slice_id)
    .bind(creator)
    .bind(artifact_type_for_subtype(subtype))
    .bind(&url)
    .bind(reviewer_id)
    .bind(slice.fragments_reward.max(0))
    .fetch_one(&mut *tx)
    .await?;

    if slice.fragments_reward > 0 {
        sqlx::query(
            "UPDATE users SET total_fragments = total_fragments + $1, updated_at = NOW()
              WHERE id = $2",
        )
        .bind(slice.fragments_reward)
        .bind(creator)
        .execute(&mut *tx)
        .await?;
    }

    crate::services::deliverables::DeliverablesService::propagate_skills(
        &mut tx,
        slice_id,
        creator,
        deliverable_id,
    )
    .await?;

    tx.commit().await?;

    // Attest and recompute — best-effort, outside the transaction, so a proof
    // hiccup never rolls back a validation that genuinely happened.
    if let Err(e) = crate::services::game_attestations::issue_for_slice(db, slice_id).await {
        tracing::warn!(slice = %slice_id, error = %e, "slice attestation failed after validate");
    }
    if let Err(e) = crate::services::proof_hooks::recompute_all_for_user(db, creator).await {
        tracing::warn!(user = %creator, error = %e, "proof recompute failed after validate");
    }

    Ok(deliverable_id)
}
