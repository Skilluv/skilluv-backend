//! SKI-36 (Post-MVP T1-01) — polymorphic bookmarks.
//!
//! Endpoints:
//!   POST   /api/bookmarks                 (auth) — create or update
//!   DELETE /api/bookmarks/{id}            (auth) — owner only
//!   GET    /api/users/me/bookmarks        (auth) — filter by type / folder
//!   GET    /api/users/me/bookmarks/folders(auth) — folder facets
//!
//! POST is an upsert on `(user_id, target_type, target_id)`: bookmarking is
//! set membership, so a second POST on the same target re-files it (new
//! folder / notes) instead of erroring or stacking a duplicate. That also
//! makes the front-end "save" button idempotent under double-click.
//!
//! Listing joins nothing at the SQL level — targets live in six different
//! tables. `saved_items::resolve_labels` batch-resolves them afterwards,
//! and rows whose target has since been deleted are dropped from the
//! response.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::saved_items;

/// Hard cap on a single page. Bookmarks are a personal list, not a feed —
/// nobody legitimately pages 500 at a time.
const MAX_LIMIT: i64 = 100;
const DEFAULT_LIMIT: i64 = 50;

pub fn bookmark_routes() -> Router<AppState> {
    Router::new()
        .route("/bookmarks", post(create))
        .route("/bookmarks/{id}", delete(remove))
        .route("/users/me/bookmarks", get(list_mine))
        .route("/users/me/bookmarks/folders", get(list_folders))
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

/// Normalize an optional free-text field: trim, then treat empty as absent.
/// Without this, `""` and `"   "` would be stored as distinct from NULL and
/// break the `folder_slug IS NULL` "unfiled" bucket.
fn normalize_opt(value: Option<String>) -> Option<String> {
    value
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateBookmarkBody {
    pub target_type: String,
    pub target_id: Uuid,
    /// Optional folder, slug-shaped (`[a-z0-9-]`, 1..60).
    #[serde(default)]
    pub folder_slug: Option<String>,
    /// Optional free-text reminder, max 1000 chars.
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct BookmarkRow {
    id: Uuid,
    user_id: Uuid,
    target_type: String,
    target_id: Uuid,
    folder_slug: Option<String>,
    notes: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
}

/// Validate a folder slug against the same shape as the DB CHECK, so a bad
/// value is a 400 rather than a database error surfaced as a 500.
fn validate_folder(slug: &str) -> Result<(), AppError> {
    let ok = (1..=60).contains(&slug.len())
        && slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if ok {
        Ok(())
    } else {
        Err(AppError::Validation(
            "folder_slug must match [a-z0-9-] and be 1..60 characters".into(),
        ))
    }
}

/// Bookmark something, optionally into a folder.
#[utoipa::path(
    post, path = "/api/bookmarks", tag = "profile",
    request_body = CreateBookmarkBody,
    responses(
        (status = 201, description = "Bookmarked"),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn create(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateBookmarkBody>,
) -> Result<impl IntoResponse, AppError> {
    let folder = normalize_opt(body.folder_slug);
    if let Some(f) = folder.as_deref() {
        validate_folder(f)?;
    }
    let notes = normalize_opt(body.notes);
    if let Some(n) = notes.as_deref()
        && n.chars().count() > 1000
    {
        return Err(AppError::Validation(
            "notes must be at most 1000 characters".into(),
        ));
    }

    // Existence + visibility. Rejects saving something the caller could not
    // read in the first place.
    saved_items::assert_target_visible(&state.db, auth.user_id, &body.target_type, body.target_id)
        .await?;

    let row: BookmarkRow = sqlx::query_as(
        r#"
        INSERT INTO bookmarks (user_id, target_type, target_id, folder_slug, notes)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (user_id, target_type, target_id) DO UPDATE SET
            folder_slug = EXCLUDED.folder_slug,
            notes       = EXCLUDED.notes
        RETURNING *
        "#,
    )
    .bind(auth.user_id)
    .bind(&body.target_type)
    .bind(body.target_id)
    .bind(folder.as_deref())
    .bind(notes.as_deref())
    .fetch_one(&state.db)
    .await?;

    Ok((StatusCode::CREATED, Json(wrap(json!({ "bookmark": row })))))
}

/// Remove one of the caller's bookmarks.
#[utoipa::path(
    delete, path = "/api/bookmarks/{id}", tag = "profile",
    params(("id" = uuid::Uuid, Path)),
    responses(
        (status = 204, description = "Removed"),
        (status = 404, description = "No bookmark of yours with that id", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn remove(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    // Scope the DELETE by user_id: another user's bookmark id is simply
    // "not found", which is also what we want to tell a probing caller.
    let affected = sqlx::query("DELETE FROM bookmarks WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(auth.user_id)
        .execute(&state.db)
        .await?
        .rows_affected();

    if affected == 0 {
        return Err(AppError::NotFound(format!("bookmark {id} not found")));
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct ListQuery {
    /// Narrow to one target type.
    #[serde(default)]
    pub target_type: Option<String>,
    /// Narrow to one folder. The literal `unfiled` selects rows with no folder.
    #[serde(default)]
    pub folder_slug: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
}

/// Sentinel folder value selecting bookmarks that have no folder. `unfiled`
/// is a legal slug, so this shadows a real folder of that name — an
/// acceptable trade for keeping the filter a single flat query param.
const UNFILED: &str = "unfiled";

/// The caller's bookmarks, newest first.
#[utoipa::path(
    get, path = "/api/users/me/bookmarks", tag = "profile",
    params(ListQuery),
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn list_mine(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<ListQuery>,
) -> Result<impl IntoResponse, AppError> {
    if let Some(t) = q.target_type.as_deref() {
        saved_items::validate_target_type(t)?;
    }
    let limit = q.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let offset = q.offset.unwrap_or(0).max(0);

    let folder = normalize_opt(q.folder_slug);
    let want_unfiled = folder.as_deref() == Some(UNFILED);
    // When filtering on a real folder we pass it through; `unfiled` becomes
    // a NULL check instead. Both cases stay inside one static query so the
    // planner can reuse the prepared statement.
    let folder_filter = if want_unfiled { None } else { folder.clone() };

    let rows: Vec<BookmarkRow> = sqlx::query_as(
        r#"
        SELECT * FROM bookmarks
         WHERE user_id = $1
           AND ($2::TEXT IS NULL OR target_type = $2)
           AND ($3::TEXT IS NULL OR folder_slug = $3)
           AND (NOT $4::BOOLEAN OR folder_slug IS NULL)
         ORDER BY created_at DESC
         LIMIT $5 OFFSET $6
        "#,
    )
    .bind(auth.user_id)
    .bind(q.target_type.as_deref())
    .bind(folder_filter.as_deref())
    .bind(want_unfiled)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let keys: Vec<(String, Uuid)> = rows
        .iter()
        .map(|r| (r.target_type.clone(), r.target_id))
        .collect();
    let labels = saved_items::resolve_labels(&state.db, &keys).await?;

    // Drop rows whose target has been hard-deleted since it was saved.
    let items: Vec<serde_json::Value> = rows
        .into_iter()
        .filter_map(|r| {
            let label = labels.get(&(r.target_type.clone(), r.target_id))?;
            Some(json!({
                "id": r.id,
                "target_type": r.target_type,
                "target_id": r.target_id,
                "folder_slug": r.folder_slug,
                "notes": r.notes,
                "created_at": r.created_at.to_rfc3339(),
                "target": label,
            }))
        })
        .collect();

    Ok(Json(wrap(json!({
        "bookmarks": items,
        "limit": limit,
        "offset": offset,
    }))))
}

/// Folder facets with counts, so the front end can render the sidebar
/// without pulling every bookmark. Unfiled bookmarks are reported under
/// the `unfiled` key that `list_mine` accepts as a filter.
#[utoipa::path(
    get, path = "/api/users/me/bookmarks/folders", tag = "profile",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn list_folders(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<impl IntoResponse, AppError> {
    let rows: Vec<(Option<String>, i64)> = sqlx::query_as(
        r#"
        SELECT folder_slug, COUNT(*)
          FROM bookmarks
         WHERE user_id = $1
         GROUP BY folder_slug
         ORDER BY COUNT(*) DESC, folder_slug ASC NULLS LAST
        "#,
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await?;

    let folders: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|(slug, count)| {
            json!({
                "folder_slug": slug.unwrap_or_else(|| UNFILED.to_string()),
                "count": count,
            })
        })
        .collect();

    Ok(Json(wrap(json!({ "folders": folders }))))
}
