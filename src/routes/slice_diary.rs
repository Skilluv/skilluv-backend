//! P26 v2 SKI-123 — challenger diary for a slice.
//!
//! Endpoints:
//!   POST /api/slices/{id}/diary          (auth, current claimer only)
//!   GET  /api/slices/{id}/diary          (public — filters is_public=false for non-owners)
//!
//! Design notes:
//! - Body markdown length is CHECKed in DB (1..4000). We echo the same
//!   bound in the route so the caller gets a clean 400 instead of a
//!   Database error surface.
//! - `is_public=false` entries are hidden from anyone except the author
//!   themselves. The current claimer is always the author for POST.

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::AuthService;

pub fn slice_diary_routes() -> Router<AppState> {
    Router::new().route("/slices/{id}/diary", post(create).get(list))
}

/// Cookie-based auth extraction that does NOT fail on missing/invalid.
/// Same shape as the extractor the recruiter search used to carry — kept
/// duplicated locally rather than exported publicly (small helper,
/// less coupling).
fn peek_user(headers: &HeaderMap, jwt_secret: &str) -> Option<Uuid> {
    let cookie = headers.get("cookie")?.to_str().ok()?;
    let token = cookie
        .split(';')
        .map(|s| s.trim())
        .find_map(|s| s.strip_prefix("access_token="))
        .or_else(|| {
            cookie
                .split(';')
                .map(|s| s.trim())
                .find_map(|s| s.strip_prefix("admin_access_token="))
        })?;
    let claims = AuthService::verify_access_token(token, jwt_secret).ok()?;
    claims.sub.parse::<Uuid>().ok()
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = SliceDiaryCreateEntryBody)]
pub struct CreateEntryBody {
    /// Markdown, 1..4000 chars. Longer entries are rejected — the diary
    /// is a running log, not an essay.
    pub body_markdown: String,
    /// Default true — most entries are shared with the community.
    #[serde(default = "default_true")]
    pub is_public: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct DiaryEntry {
    id: Uuid,
    slice_id: Uuid,
    author_user_id: Uuid,
    body_markdown: String,
    is_public: bool,
    created_at: chrono::DateTime<chrono::Utc>,
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

/// Write a diary entry against a slice. The diary is what the work looked
/// like while it was happening, not a summary written afterwards.
#[utoipa::path(
    post, path = "/api/slices/{id}/diary", tag = "slices",
    params(("id" = uuid::Uuid, Path, description = "The slice")),
    request_body = CreateEntryBody,
    responses(
        (status = 201, description = "The entry was written"),
        (status = 403, description = "The diary belongs to whoever claimed the slice", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn create(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slice_id): Path<Uuid>,
    Json(body): Json<CreateEntryBody>,
) -> Result<impl IntoResponse, AppError> {
    let trimmed = body.body_markdown.trim();
    let len = trimmed.chars().count();
    if !(1..=4000).contains(&len) {
        return Err(AppError::Validation(
            "body_markdown must be 1..4000 characters after trim".into(),
        ));
    }

    // Only the current claimer of the slice may post. Past claimers
    // (who have since been rewound to `claimed` or moved on) do NOT
    // qualify — diary is a live log of the CURRENT attempt.
    let is_current_claimer: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM project_slices
             WHERE id = $1 AND claimed_by_user_id = $2
        )
        "#,
    )
    .bind(slice_id)
    .bind(auth.user_id)
    .fetch_one(&state.db)
    .await?;
    if !is_current_claimer {
        return Err(AppError::Forbidden);
    }

    let entry: DiaryEntry = sqlx::query_as(
        r#"
        INSERT INTO slice_diary_entries
            (slice_id, author_user_id, body_markdown, is_public)
        VALUES ($1, $2, $3, $4)
        RETURNING *
        "#,
    )
    .bind(slice_id)
    .bind(auth.user_id)
    .bind(trimmed)
    .bind(body.is_public)
    .fetch_one(&state.db)
    .await?;

    // SKI-286 — @username mentions. Private entries record their mentions
    // too, but the inbox only surfaces them to the author, so a mention in
    // a private diary never reaches the person named.
    crate::services::mentions::record_and_notify(
        &state.db,
        auth.user_id,
        crate::services::mentions::SOURCE_SLICE_DIARY,
        entry.id,
        &entry.body_markdown,
    )
    .await;

    Ok((StatusCode::CREATED, Json(wrap(json!({ "entry": entry })))))
}

/// A slice's diary, in the order it was written.
#[utoipa::path(
    get, path = "/api/slices/{id}/diary", tag = "slices",
    params(("id" = uuid::Uuid, Path, description = "The slice")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "No such slice", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(slice_id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    // Optional viewer: authenticated user sees their own private
    // entries too; anonymous sees only is_public=true.
    let viewer = peek_user(&headers, &state.config.jwt_secret);
    let entries: Vec<DiaryEntry> = sqlx::query_as(
        r#"
        SELECT * FROM slice_diary_entries
         WHERE slice_id = $1
           AND (is_public OR author_user_id = $2)
         ORDER BY created_at DESC
         LIMIT 200
        "#,
    )
    .bind(slice_id)
    // Bind Uuid::nil() when unauth — no row will ever match it.
    .bind(viewer.unwrap_or(Uuid::nil()))
    .fetch_all(&state.db)
    .await?;

    Ok(Json(wrap(json!({ "entries": entries }))))
}
