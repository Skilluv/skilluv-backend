//! Registering and confirming mods (migration 0583).
//!
//! A mod is content that lives inside someone else's game, on a platform we do
//! not own — Nexus, the Steam Workshop, CurseForge. Skilluv never hosts the
//! package; it holds the proof and the metadata. A creator registers the mod
//! with its hosting URL; a community reviewer confirms it against three things
//! — the URL is real, the mod is theirs, the vendor's terms were kept — or
//! refuses it with a reason.
//!
//! A confirmed mod becomes a deliverable so it counts toward the cross-domain
//! rank (migration 0585), and earns the `game_mod_published` attestation. When
//! the mod was registered against a `mod_package` slice, the deliverable is the
//! slice's own; otherwise one is created here.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

/// The platforms a mod may be hosted on (migration 0583).
pub const TARGET_PLATFORMS: &[&str] = &[
    "nexusmods",
    "steam_workshop",
    "curseforge",
    "moddb",
    "thunderstore",
    "other",
];

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct GameMod {
    pub id: Uuid,
    pub author_user_id: Uuid,
    pub slice_id: Option<Uuid>,
    pub title: String,
    pub target_game: String,
    pub target_platform: String,
    pub external_hosting_url: String,
    pub external_downloads_count: i32,
    pub description_md: String,
    pub status: String,
    pub reviewed_by: Option<Uuid>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub review_reason: Option<String>,
    pub registered_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RegisterInput {
    pub slice_id: Option<Uuid>,
    pub title: String,
    pub target_game: String,
    pub target_platform: String,
    pub external_hosting_url: String,
    pub external_downloads_count: Option<i32>,
    pub description_md: String,
}

/// Register a mod. It starts `registered`, waiting on a reviewer.
pub async fn register(
    db: &PgPool,
    author_user_id: Uuid,
    input: RegisterInput,
) -> Result<GameMod, AppError> {
    let title = input.title.trim();
    if title.is_empty() {
        return Err(AppError::Validation("a mod needs a title".into()));
    }
    crate::validators::check_max_len(title, "title", 200)?;
    if !TARGET_PLATFORMS.contains(&input.target_platform.as_str()) {
        return Err(AppError::Validation(format!(
            "'{}' is not a mod hosting platform",
            input.target_platform
        )));
    }
    let url = input.external_hosting_url.trim();
    if !url.starts_with("https://") {
        return Err(AppError::Validation(
            "a mod's hosting URL must be a public https link — the proof is that page being real"
                .into(),
        ));
    }
    if input.description_md.trim().is_empty() {
        return Err(AppError::Validation(
            "say what the mod does — a reviewer confirms against a description".into(),
        ));
    }

    // A slice, when named, must be the author's own mod_package slice.
    if let Some(slice_id) = input.slice_id {
        let ok: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                 SELECT 1 FROM project_slices ps
                 WHERE ps.id = $1 AND ps.game_artifact_subtype = 'mod_package'
                   AND (
                     EXISTS (SELECT 1 FROM projects p
                              WHERE p.id = ps.project_id
                                AND p.owner_type = 'user' AND p.owner_id = $2)
                     OR EXISTS (SELECT 1 FROM deliverables d
                                 WHERE d.slice_id = ps.id AND d.user_id = $2)
                   ))",
        )
        .bind(slice_id)
        .bind(author_user_id)
        .fetch_one(db)
        .await
        .unwrap_or(false);
        if !ok {
            return Err(AppError::Validation(
                "that slice is not one of your mod_package slices".into(),
            ));
        }
    }

    Ok(sqlx::query_as(
        r#"
        INSERT INTO game_mods
            (author_user_id, slice_id, title, target_game, target_platform,
             external_hosting_url, external_downloads_count, description_md)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING *
        "#,
    )
    .bind(author_user_id)
    .bind(input.slice_id)
    .bind(title)
    .bind(input.target_game.trim())
    .bind(&input.target_platform)
    .bind(url)
    .bind(input.external_downloads_count.unwrap_or(0).max(0))
    .bind(input.description_md.trim())
    .fetch_one(db)
    .await?)
}

/// Confirm a mod: a reviewer vouches for it. Creates the deliverable (unless
/// the mod rests on a slice, whose deliverable already exists), issues the
/// `game_mod_published` attestation, and recomputes the author's proof so the
/// rank and badges follow in the same pass.
pub async fn confirm(
    db: &PgPool,
    mod_id: Uuid,
    reviewer_id: Uuid,
    reason: &str,
) -> Result<GameMod, AppError> {
    if reason.trim().is_empty() {
        return Err(AppError::Validation(
            "a confirmation carries a reason — say what you checked".into(),
        ));
    }

    let m: GameMod = sqlx::query_as("SELECT * FROM game_mods WHERE id = $1")
        .bind(mod_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NotFound("mod not found".into()))?;
    if m.status != "registered" {
        return Err(AppError::Validation(format!(
            "this mod is already {}, not waiting on a review",
            m.status
        )));
    }
    if m.author_user_id == reviewer_id {
        return Err(AppError::Validation(
            "a mod is confirmed by someone other than its author".into(),
        ));
    }

    let mut tx = db.begin().await?;

    // A standalone mod becomes its own deliverable; a slice mod already has one.
    if m.slice_id.is_none() {
        sqlx::query(
            r#"
            INSERT INTO deliverables
                (game_mod_id, user_id, artifact_type, artifact_url,
                 verifiable_by, verification_status, verified_at, verified_by_user_id,
                 fragments_awarded, credits_awarded, public, submitted_at, created_at)
            VALUES ($1, $2, 'game_mod', $3, 'human_review', 'verified', NOW(), $4,
                    0, 0, TRUE, $5, NOW())
            ON CONFLICT DO NOTHING
            "#,
        )
        .bind(mod_id)
        .bind(m.author_user_id)
        .bind(&m.external_hosting_url)
        .bind(reviewer_id)
        .bind(m.registered_at)
        .execute(&mut *tx)
        .await?;
    }

    let updated: GameMod = sqlx::query_as(
        r#"
        UPDATE game_mods
           SET status = 'confirmed', reviewed_by = $2, reviewed_at = NOW(),
               review_reason = $3
         WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(mod_id)
    .bind(reviewer_id)
    .bind(reason.trim())
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;

    // Attest, then recompute — best-effort, like every proof hook: a failure
    // here is logged, never allowed to undo a confirmation that happened.
    if let Err(e) = crate::services::game_attestations::issue_for_mod(db, mod_id).await {
        tracing::warn!(mod_id = %mod_id, error = %e, "mod attestation failed after confirm");
    }
    if let Err(e) = crate::services::proof_hooks::recompute_all_for_user(db, m.author_user_id).await
    {
        tracing::warn!(user = %m.author_user_id, error = %e, "proof recompute failed after mod confirm");
    }

    Ok(updated)
}

/// Refuse a mod, with a reason the author reads.
pub async fn refuse(
    db: &PgPool,
    mod_id: Uuid,
    reviewer_id: Uuid,
    reason: &str,
) -> Result<GameMod, AppError> {
    if reason.trim().is_empty() {
        return Err(AppError::Validation(
            "a refusal says why — the author has to know what to fix".into(),
        ));
    }
    let m: GameMod = sqlx::query_as("SELECT * FROM game_mods WHERE id = $1")
        .bind(mod_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NotFound("mod not found".into()))?;
    if m.status != "registered" {
        return Err(AppError::Validation(format!(
            "this mod is already {}",
            m.status
        )));
    }

    Ok(sqlx::query_as(
        r#"
        UPDATE game_mods
           SET status = 'refused', reviewed_by = $2, reviewed_at = NOW(),
               review_reason = $3
         WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(mod_id)
    .bind(reviewer_id)
    .bind(reason.trim())
    .fetch_one(db)
    .await?)
}

/// Update a confirmed mod's download count — the figure the `mods_viral`
/// craft-score term reads. A reviewer sets it against the hosting page; the
/// author cannot inflate their own score by editing it.
pub async fn update_downloads(
    db: &PgPool,
    mod_id: Uuid,
    reviewer_id: Uuid,
    downloads: i32,
) -> Result<GameMod, AppError> {
    if downloads < 0 {
        return Err(AppError::Validation(
            "a download count is not negative".into(),
        ));
    }
    let m: GameMod = sqlx::query_as(
        r#"
        UPDATE game_mods
           SET external_downloads_count = $2
         WHERE id = $1 AND status = 'confirmed'
        RETURNING *
        "#,
    )
    .bind(mod_id)
    .bind(downloads)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| AppError::NotFound("confirmed mod not found".into()))?;

    // Crossing the viral threshold changes the craft score — recompute so the
    // profile does not wait for the next sweep.
    if let Err(e) = crate::services::game_profile::recompute(db, m.author_user_id).await {
        tracing::warn!(user = %m.author_user_id, error = %e, "game score recompute failed after downloads update");
    }
    let _ = reviewer_id; // authorised at the route; recorded in the audit log there
    Ok(m)
}

/// One mod.
pub async fn get(db: &PgPool, mod_id: Uuid) -> Result<GameMod, AppError> {
    sqlx::query_as("SELECT * FROM game_mods WHERE id = $1")
        .bind(mod_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NotFound("mod not found".into()))
}

/// A person's mods, newest first.
pub async fn list_for_author(db: &PgPool, author_user_id: Uuid) -> Result<Vec<GameMod>, AppError> {
    Ok(sqlx::query_as(
        "SELECT * FROM game_mods WHERE author_user_id = $1 ORDER BY registered_at DESC",
    )
    .bind(author_user_id)
    .fetch_all(db)
    .await?)
}

/// The mods waiting on a review, oldest first — a reviewer's queue.
pub async fn list_pending(db: &PgPool, limit: i64) -> Result<Vec<GameMod>, AppError> {
    Ok(sqlx::query_as(
        "SELECT * FROM game_mods WHERE status = 'registered' ORDER BY registered_at ASC LIMIT $1",
    )
    .bind(limit.clamp(1, 200))
    .fetch_all(db)
    .await?)
}
