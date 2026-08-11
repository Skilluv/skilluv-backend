//! SKI-37 (Post-MVP T1-02) — private notes on any artifact a user has seen.
//!
//! Endpoints:
//!   PUT    /api/users/me/notes/{target_type}/{target_id}   (auth) — upsert
//!   GET    /api/users/me/notes/{target_type}/{target_id}   (auth)
//!   DELETE /api/users/me/notes/{target_type}/{target_id}   (auth)
//!   GET    /api/users/me/notes                             (auth) — list
//!
//! Notes are private, always. There is no endpoint here that returns
//! another user's notes, and every query is scoped by `user_id` from the
//! JWT rather than from the path — a note is addressed by its target, and
//! the author is implicit.
//!
//! Anti-spam is two-layered: a 1000-char cap (mirrored by a DB CHECK) and
//! a Redis write rate-limit. The cap alone would still let a client burn
//! rows by cycling target ids.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::{AuthUser, RateLimiter};
use crate::services::saved_items;

/// Matches the `length(body) BETWEEN 1 AND 1000` CHECK in migration 0140.
const MAX_BODY_CHARS: usize = 1000;
/// Write budget: 120 note upserts per 10 minutes per user. Generous for a
/// human reading and annotating, cheap to exceed for a script.
const WRITE_MAX: u64 = 120;
const WRITE_WINDOW_SECS: u64 = 600;

const MAX_LIMIT: i64 = 100;
const DEFAULT_LIMIT: i64 = 50;

pub fn user_note_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/users/me/notes/{target_type}/{target_id}",
            put(upsert).get(fetch).delete(remove),
        )
        .route("/users/me/notes", get(list_mine))
}

fn wrap(data: serde_json::Value) -> serde_json::Value {
    json!({
        "data": data,
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpsertNoteBody {
    /// 1..1000 chars after trimming. An all-whitespace body is a 400, not
    /// a silent delete — deleting is an explicit DELETE.
    pub body: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct NoteRow {
    user_id: Uuid,
    target_type: String,
    target_id: Uuid,
    body: String,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

async fn upsert(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((target_type, target_id)): Path<(String, Uuid)>,
    Json(body): Json<UpsertNoteBody>,
) -> Result<impl IntoResponse, AppError> {
    let trimmed = body.body.trim();
    let len = trimmed.chars().count();
    if !(1..=MAX_BODY_CHARS).contains(&len) {
        return Err(AppError::Validation(format!(
            "body must be 1..{MAX_BODY_CHARS} characters after trim"
        )));
    }

    let mut redis = state.redis.clone();
    RateLimiter::check(
        &mut redis,
        "user_notes_write",
        &auth.user_id.to_string(),
        WRITE_MAX,
        WRITE_WINDOW_SECS,
    )
    .await?;

    saved_items::assert_target_visible(&state.db, auth.user_id, &target_type, target_id).await?;

    let row: NoteRow = sqlx::query_as(
        r#"
        INSERT INTO user_notes (user_id, target_type, target_id, body)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (user_id, target_type, target_id) DO UPDATE SET
            body       = EXCLUDED.body,
            updated_at = NOW()
        RETURNING *
        "#,
    )
    .bind(auth.user_id)
    .bind(&target_type)
    .bind(target_id)
    .bind(trimmed)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(wrap(json!({ "note": row }))))
}

async fn fetch(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((target_type, target_id)): Path<(String, Uuid)>,
) -> Result<impl IntoResponse, AppError> {
    saved_items::validate_target_type(&target_type)?;

    let row: Option<NoteRow> = sqlx::query_as(
        "SELECT * FROM user_notes
          WHERE user_id = $1 AND target_type = $2 AND target_id = $3",
    )
    .bind(auth.user_id)
    .bind(&target_type)
    .bind(target_id)
    .fetch_optional(&state.db)
    .await?;

    // A missing note is a normal state for the front end (the editor opens
    // empty), so this is a 200 with `note: null` rather than a 404.
    Ok(Json(wrap(json!({ "note": row }))))
}

async fn remove(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((target_type, target_id)): Path<(String, Uuid)>,
) -> Result<impl IntoResponse, AppError> {
    saved_items::validate_target_type(&target_type)?;

    let affected = sqlx::query(
        "DELETE FROM user_notes
          WHERE user_id = $1 AND target_type = $2 AND target_id = $3",
    )
    .bind(auth.user_id)
    .bind(&target_type)
    .bind(target_id)
    .execute(&state.db)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(AppError::NotFound("note not found".into()));
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct ListNotesQuery {
    #[serde(default)]
    pub target_type: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
}

async fn list_mine(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<ListNotesQuery>,
) -> Result<impl IntoResponse, AppError> {
    if let Some(t) = q.target_type.as_deref() {
        saved_items::validate_target_type(t)?;
    }
    let limit = q.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let offset = q.offset.unwrap_or(0).max(0);

    let rows: Vec<NoteRow> = sqlx::query_as(
        r#"
        SELECT * FROM user_notes
         WHERE user_id = $1
           AND ($2::TEXT IS NULL OR target_type = $2)
         ORDER BY updated_at DESC
         LIMIT $3 OFFSET $4
        "#,
    )
    .bind(auth.user_id)
    .bind(q.target_type.as_deref())
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let keys: Vec<(String, Uuid)> = rows
        .iter()
        .map(|r| (r.target_type.clone(), r.target_id))
        .collect();
    let labels = saved_items::resolve_labels(&state.db, &keys).await?;

    // Same policy as bookmarks: a note whose target was hard-deleted stops
    // being listed rather than rendering as a dangling row.
    let items: Vec<serde_json::Value> = rows
        .into_iter()
        .filter_map(|r| {
            let label = labels.get(&(r.target_type.clone(), r.target_id))?;
            Some(json!({
                "target_type": r.target_type,
                "target_id": r.target_id,
                "body": r.body,
                "created_at": r.created_at.to_rfc3339(),
                "updated_at": r.updated_at.to_rfc3339(),
                "target": label,
            }))
        })
        .collect();

    Ok(Json(wrap(json!({
        "notes": items,
        "limit": limit,
        "offset": offset,
    }))))
}
