//! Revision rounds on a delivery, in any domain.
//!
//! ## Why this is not under `/api/audio`
//!
//! It was. Migration 0412 built `slice_revision_rounds` with the round kinds
//! and the per-domain ceiling as rows, and the three handlers that used it
//! read `revision_round_limits` through the slice's own `primary_domain` —
//! they were domain-agnostic from the first line. Only the URL was not.
//!
//! Communication would have made that a second copy, education a third, and
//! each copy is a place the round counter can be enforced differently. Three
//! handlers, one path, and the domain comes from the slice.
//!
//! ## Who may do what
//!
//! Opening a round is for whoever commissioned the work rather than whoever
//! did it: a round the maker can open is a round the maker can spend. Closing
//! one is for the person who opened it, for the same reason — a counter one
//! side can run down alone is not a count both sides agree on.

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::middleware::AuthUser;

pub fn slice_revision_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/slices/{slice_id}/revisions",
            get(list_revisions).post(request_revision),
        )
        .route(
            "/revisions/{round_id}/resolve",
            axum::routing::post(resolve_revision),
        )
}

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct RevisionRow {
    pub id: Uuid,
    pub round_no: i16,
    pub kind: String,
    pub notes_md: String,
    pub requested_at: chrono::DateTime<chrono::Utc>,
    pub resolved_at: Option<chrono::DateTime<chrono::Utc>>,
    pub resolution_note: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RequestRevisionBody {
    /// One of `revision_round_kinds` for the slice's domain.
    pub kind: String,
    pub notes_md: String,
}

/// The rounds this delivery has been through, and how many remain.
#[utoipa::path(
    get, path = "/api/slices/{slice_id}/revisions", tag = "slices",
    params(("slice_id" = Uuid, Path, description = "Slice")),
    responses((status = 200, description = "Rounds", body = ApiResponse<serde_json::Value>)),
    security(("cookie_auth" = [])),
)]
pub async fn list_revisions(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(slice_id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    let rows: Vec<RevisionRow> = sqlx::query_as(
        "SELECT id, round_no, kind, notes_md, requested_at, resolved_at, resolution_note
           FROM slice_revision_rounds WHERE slice_id = $1 ORDER BY round_no",
    )
    .bind(slice_id)
    .fetch_all(&state.db)
    .await?;

    // The ceiling belongs to the slice's domain, and a domain with no row has
    // no ceiling — which is not the same as a ceiling of zero.
    let allowed: Option<i16> = sqlx::query_scalar(
        "SELECT l.max_rounds FROM project_slices ps
           JOIN revision_round_limits l ON l.skill_domain = ps.primary_domain
          WHERE ps.id = $1",
    )
    .bind(slice_id)
    .fetch_optional(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(json!({
        "rounds": rows,
        "max_rounds": allowed,
        "remaining": allowed.map(|a| (a as i64 - rows.len() as i64).max(0)),
    }))))
}

/// Ask for a change.
///
/// Open to whoever commissioned the work rather than to whoever did it: a
/// round the maker can open is a round the maker can spend.
#[utoipa::path(
    post, path = "/api/slices/{slice_id}/revisions", tag = "slices",
    params(("slice_id" = Uuid, Path, description = "Slice")),
    request_body = RequestRevisionBody,
    responses(
        (status = 200, description = "Opened", body = ApiResponse<serde_json::Value>),
        (status = 400, description = "No rounds left, or unknown kind", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn request_revision(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slice_id): Path<Uuid>,
    Json(body): Json<RequestRevisionBody>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    if body.notes_md.trim().is_empty() {
        return Err(AppError::Validation(
            "a round has to say what to change — a rejection with no statement \
             cannot be acted on"
                .into(),
        ));
    }
    crate::validators::check_max_len(&body.notes_md, "notes_md", 8000)?;

    // The database counts the rounds and enforces the limit, because the count
    // is the one both sides quote and a check here would race with itself.
    let id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO slice_revision_rounds
            (slice_id, round_no, kind, requested_by, notes_md)
        VALUES (
            $1,
            (SELECT COALESCE(max(round_no), 0) + 1
               FROM slice_revision_rounds WHERE slice_id = $1),
            $2, $3, $4)
        RETURNING id
        "#,
    )
    .bind(slice_id)
    .bind(&body.kind)
    .bind(auth.user_id)
    .bind(body.notes_md.trim())
    .fetch_one(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(json!({ "id": id }))))
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ResolveRevisionBody {
    pub resolution_note: Option<String>,
}

/// Close a round.
///
/// Only the person who opened it. A counter the maker can run down alone is
/// not a count both sides agree on, which is the only kind worth keeping.
#[utoipa::path(
    post, path = "/api/revisions/{round_id}/resolve", tag = "slices",
    params(("round_id" = Uuid, Path, description = "Round")),
    request_body = ResolveRevisionBody,
    responses(
        (status = 200, description = "Closed", body = ApiResponse<serde_json::Value>),
        (status = 403, description = "Not the person who asked", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn resolve_revision(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(round_id): Path<Uuid>,
    Json(body): Json<ResolveRevisionBody>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    let done = sqlx::query(
        "UPDATE slice_revision_rounds
            SET resolved_at = NOW(), resolved_by = $2, resolution_note = $3
          WHERE id = $1 AND requested_by = $2 AND resolved_at IS NULL",
    )
    .bind(round_id)
    .bind(auth.user_id)
    .bind(&body.resolution_note)
    .execute(&state.db)
    .await?;

    if done.rows_affected() == 0 {
        // Closed by whoever opened it, and only once.
        return Err(AppError::Forbidden);
    }
    Ok(Json(ApiResponse::new(json!({ "resolved": true }))))
}
