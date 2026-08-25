//! SKI-42 (Post-MVP T2-03) — external reputation signals.
//!
//! Endpoints:
//!   POST   /api/users/me/external-signals        (auth)
//!   GET    /api/users/me/external-signals        (auth)
//!   DELETE /api/users/me/external-signals/{id}   (auth)
//!   GET    /api/users/{id}/external-signals      (public if the profile is)
//!   GET    /api/moderation/external-signals      (moderator — review queue)
//!   POST   /api/moderation/external-signals/{id}/verify   (moderator)
//!   DELETE /api/moderation/external-signals/{id}          (moderator)
//!
//! Every response separates `verified` from `declared`. That split is the
//! feature: an external signal is context, never proof, and the API shape
//! makes it hard for a client to render the two as one list by accident.
//!
//! See migration 0145 for why nothing here touches the proof engine.

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
use crate::middleware::{AuthUser, OptionalAuth, capabilities};
use crate::services::external_signals;

pub fn external_signal_routes() -> Router<AppState> {
    Router::new()
        .route("/users/me/external-signals", post(create).get(list_mine))
        .route("/users/me/external-signals/{id}", delete(remove))
        .route("/users/{user_id}/external-signals", get(list_public))
        .route("/moderation/external-signals", get(list_pending))
        .route(
            "/moderation/external-signals/{id}/verify",
            post(moderator_verify),
        )
        .route(
            "/moderation/external-signals/{id}",
            delete(moderator_delete),
        )
}

/// Capabilities allowed to review external signals. Reuses the P25
/// community-moderation family rather than inventing a new one — vetting a
/// claimed blog post is the same job as vetting any other user content.
const MODERATOR_CAPS: &[&str] = &["community_moderator", "community_curator"];

fn wrap(data: serde_json::Value) -> serde_json::Value {
    json!({
        "data": data,
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })
}

/// Split a signal list into the two buckets the UI must keep apart.
fn split_buckets(signals: Vec<external_signals::ExternalSignal>) -> serde_json::Value {
    let (verified, declared): (Vec<_>, Vec<_>) =
        signals.into_iter().partition(|s| s.verified_at.is_some());
    json!({
        // Ownership confirmed — still not a Skilluv proof.
        "verified": verified,
        // Self-declared, unconfirmed.
        "declared": declared,
        // Spelled out in the payload so a client cannot render these
        // alongside badges or attestations by mistake.
        "disclaimer": "Signaux externes : contexte déclaré hors Skilluv. \
                       Ils ne comptent pas dans les preuves, le rang ni les badges.",
    })
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateSignalBody {
    /// `github` | `medium` | `dev_to` | `conf_ref` | `behance` |
    /// `dribbble` | `artstation` | `vimeo` | `foundry`
    pub provider: String,
    pub url: String,
    pub title: String,
    #[serde(default)]
    pub meta: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct CreateSignalResponse {
    signal: external_signals::ExternalSignal,
    /// True when the signal self-verified through the existing GitHub
    /// OAuth link, so the client can skip the "pending review" copy.
    auto_verified: bool,
}

async fn create(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateSignalBody>,
) -> Result<impl IntoResponse, AppError> {
    let signal = external_signals::create(
        &state.db,
        auth.user_id,
        external_signals::CreateParams {
            provider: &body.provider,
            url: &body.url,
            title: &body.title,
            meta: body.meta,
        },
    )
    .await?;

    let auto_verified = signal.verified_at.is_some();
    Ok((
        StatusCode::CREATED,
        Json(wrap(json!(CreateSignalResponse {
            signal,
            auto_verified
        }))),
    ))
}

/// The caller's own signals, verified and declared alike.
#[utoipa::path(
    get, path = "/api/users/me/external-signals", tag = "profile",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn list_mine(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<impl IntoResponse, AppError> {
    let signals = external_signals::list_for_user(&state.db, auth.user_id).await?;
    Ok(Json(wrap(split_buckets(signals))))
}

async fn remove(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let affected = sqlx::query("DELETE FROM external_signals WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(auth.user_id)
        .execute(&state.db)
        .await?
        .rows_affected();
    if affected == 0 {
        return Err(AppError::NotFound(format!(
            "external signal {id} not found"
        )));
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Public view of a profile's signals.
///
/// Unverified signals are visible here too — hiding them would make the
/// "declared vs verified" distinction invisible precisely where it
/// matters, on the profile a recruiter reads.
#[utoipa::path(
    get, path = "/api/users/{user_id}/external-signals", tag = "profile",
    params(("user_id" = uuid::Uuid, Path, description = "Whose profile")),
    responses((status = 200, body = serde_json::Value)),
)]
pub async fn list_public(
    State(state): State<AppState>,
    OptionalAuth(auth): OptionalAuth,
    Path(user_id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let is_owner = auth.map(|a| a.user_id) == Some(user_id);
    let readable: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM users
              WHERE id = $1
                AND ($2::BOOLEAN OR (profile_hidden = FALSE AND is_banned = FALSE))
         )",
    )
    .bind(user_id)
    .bind(is_owner)
    .fetch_one(&state.db)
    .await?;
    if !readable {
        return Err(AppError::NotFound(format!("user {user_id} not found")));
    }

    let signals = external_signals::list_for_user(&state.db, user_id).await?;
    Ok(Json(wrap(split_buckets(signals))))
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct PendingQuery {
    #[serde(default)]
    pub limit: Option<i64>,
}

/// Signals still waiting on a human to verify them. Moderators only.
#[utoipa::path(
    get, path = "/api/moderation/external-signals", tag = "moderation",
    params(PendingQuery),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 403, description = "Not a moderator", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn list_pending(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<PendingQuery>,
) -> Result<impl IntoResponse, AppError> {
    capabilities::require_any_capability(&state.db, auth.user_id, MODERATOR_CAPS).await?;

    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let signals: Vec<external_signals::ExternalSignal> = sqlx::query_as(
        "SELECT * FROM external_signals
          WHERE verified_at IS NULL
          ORDER BY created_at ASC
          LIMIT $1",
    )
    .bind(limit)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(wrap(json!({ "pending": signals }))))
}

async fn moderator_verify(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    capabilities::require_any_capability(&state.db, auth.user_id, MODERATOR_CAPS).await?;

    let signal: Option<external_signals::ExternalSignal> = sqlx::query_as(
        "UPDATE external_signals
            SET verified_at = NOW(),
                verification_method = 'manual_review',
                verified_by = $2
          WHERE id = $1 AND verified_at IS NULL
          RETURNING *",
    )
    .bind(id)
    .bind(auth.user_id)
    .fetch_optional(&state.db)
    .await?;

    let signal = signal
        .ok_or_else(|| AppError::NotFound(format!("pending external signal {id} not found")))?;
    Ok(Json(wrap(json!({ "signal": signal }))))
}

/// Remove a signal as a moderator (bogus or abusive claim).
async fn moderator_delete(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    capabilities::require_any_capability(&state.db, auth.user_id, MODERATOR_CAPS).await?;

    let affected = sqlx::query("DELETE FROM external_signals WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?
        .rows_affected();
    if affected == 0 {
        return Err(AppError::NotFound(format!(
            "external signal {id} not found"
        )));
    }
    Ok(StatusCode::NO_CONTENT)
}
