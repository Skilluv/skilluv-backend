//! Featured game creator of the week (migration 0584).
//!
//! The same editorial recognition every domain has, with the extras a game
//! landing page wants: a short bio, the games put forward, optional itch
//! embeds and a short interview. One row per week — the unique on
//! `week_starts_at` makes two featurings for the same week impossible, which is
//! the whole point of "of the week". Publishing a row issues the
//! `featured_game_creator` attestation and recomputes the person's proof.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::config::PUBLIC_SITE_URL;
use crate::errors::AppError;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Featured {
    pub id: Uuid,
    pub user_id: Uuid,
    pub week_starts_at: NaiveDate,
    pub week_ends_at: NaiveDate,
    pub bio_md: String,
    pub highlighted_projects: Vec<Uuid>,
    pub itch_embeds: Option<serde_json::Value>,
    pub interview_qa_json: Option<serde_json::Value>,
    pub published_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FeatureInput {
    pub user_id: Uuid,
    pub week_starts_at: NaiveDate,
    pub week_ends_at: NaiveDate,
    pub bio_md: String,
    #[serde(default)]
    pub highlighted_projects: Vec<Uuid>,
    pub itch_embeds: Option<serde_json::Value>,
    pub interview_qa_json: Option<serde_json::Value>,
}

/// Feature a creator for a week. The bio is the citation the attestation
/// carries — an editorial choice has to say why.
pub async fn feature(db: &PgPool, input: FeatureInput) -> Result<Featured, AppError> {
    if input.bio_md.trim().is_empty() {
        return Err(AppError::Validation(
            "a featuring says why — write the bio".into(),
        ));
    }
    if input.week_ends_at < input.week_starts_at {
        return Err(AppError::Validation(
            "the week ends on or after it starts".into(),
        ));
    }

    let row: Featured = sqlx::query_as(
        r#"
        INSERT INTO game_featured
            (user_id, week_starts_at, week_ends_at, bio_md, highlighted_projects,
             itch_embeds, interview_qa_json)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING *
        "#,
    )
    .bind(input.user_id)
    .bind(input.week_starts_at)
    .bind(input.week_ends_at)
    .bind(input.bio_md.trim())
    .bind(&input.highlighted_projects)
    .bind(&input.itch_embeds)
    .bind(&input.interview_qa_json)
    .fetch_one(db)
    .await
    .map_err(|e| {
        // The unique on the week is the common failure — say what it means.
        if e.to_string().contains("week_starts_at") {
            AppError::Validation("a creator is already featured for that week".into())
        } else {
            AppError::from(e)
        }
    })?;

    // Issue the attestation and recompute proof — best-effort, so a proof
    // hiccup never undoes a featuring that was published.
    let profile_url = format!("{PUBLIC_SITE_URL}/game/creators/{}", input.user_id);
    if let Err(e) = crate::services::game_attestations::featured_game_creator(
        db,
        input.user_id,
        &profile_url,
        input.bio_md.trim(),
    )
    .await
    {
        tracing::warn!(user = %input.user_id, error = %e, "featured game creator attestation failed");
    }
    if let Err(e) = crate::services::proof_hooks::recompute_all_for_user(db, input.user_id).await {
        tracing::warn!(user = %input.user_id, error = %e, "proof recompute failed after featuring");
    }

    Ok(row)
}

/// The featuring for a given week, if any.
pub async fn of_week(db: &PgPool, week_starts_at: NaiveDate) -> Result<Option<Featured>, AppError> {
    Ok(
        sqlx::query_as("SELECT * FROM game_featured WHERE week_starts_at = $1")
            .bind(week_starts_at)
            .fetch_optional(db)
            .await?,
    )
}

/// The most recent featurings, newest first — for the landing page.
pub async fn recent(db: &PgPool, limit: i64) -> Result<Vec<Featured>, AppError> {
    Ok(
        sqlx::query_as("SELECT * FROM game_featured ORDER BY week_starts_at DESC LIMIT $1")
            .bind(limit.clamp(1, 100))
            .fetch_all(db)
            .await?,
    )
}

/// The latest featuring of one person, for `/game/featured/{username}`.
pub async fn latest_for_user(db: &PgPool, user_id: Uuid) -> Result<Option<Featured>, AppError> {
    Ok(sqlx::query_as(
        "SELECT * FROM game_featured WHERE user_id = $1 ORDER BY week_starts_at DESC LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(db)
    .await?)
}
