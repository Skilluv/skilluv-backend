//! SKI-46 (Post-MVP T3-03) — reputation staking endpoints.
//!
//! Endpoints:
//!   POST   /api/vouchings                        (auth, Doyen)
//!   GET    /api/users/{id}/vouchings             (public if profile is)
//!   GET    /api/users/me/vouchings               (auth — what I back)
//!   DELETE /api/vouchings/{id}                   (voucher — withdraw)
//!   POST   /api/moderation/vouchings/{id}/break  (moderator)
//!
//! Breaking a vouching costs the voucher a rank for three months, so it is
//! a moderator action behind an explicit endpoint, never a side effect of
//! a fraud flag being set somewhere else.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::{AuthUser, OptionalAuth, capabilities};
use crate::services::vouchings;

/// Capabilities allowed to break a vouching. Reuses the P25 moderation
/// family; `plagiarism_reviewer` is included because a confirmed plagiarism
/// case is the archetypal trigger.
const MODERATOR_CAPS: &[&str] = &["community_moderator", "plagiarism_reviewer"];

pub fn vouching_routes() -> Router<AppState> {
    Router::new()
        .route("/vouchings", post(create))
        .route("/vouchings/{id}", axum::routing::delete(withdraw))
        .route("/users/{user_id}/vouchings", get(list_for_user))
        .route("/users/me/vouchings", get(list_mine))
        .route("/moderation/vouchings/{id}/break", post(break_vouching))
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
pub struct CreateVouchingBody {
    pub vouched_id: Uuid,
    /// 30..365. Defaults to 180 — six months, the ticket's window.
    #[serde(default)]
    pub window_days: Option<i64>,
    /// `rank_temporary` (default) puts the voucher's rank at stake;
    /// `reputation_only` is a public statement with no rank consequence.
    #[serde(default)]
    pub at_stake_kind: Option<String>,
    /// Public justification, shown on the vouched user's profile.
    #[serde(default)]
    pub statement: Option<String>,
}

async fn create(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateVouchingBody>,
) -> Result<impl IntoResponse, AppError> {
    let vouching = vouchings::create(
        &state.db,
        auth.user_id,
        body.vouched_id,
        body.window_days.unwrap_or(180),
        body.at_stake_kind
            .as_deref()
            .unwrap_or(vouchings::AT_STAKE_RANK),
        body.statement.as_deref().unwrap_or(""),
    )
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(wrap(json!({ "vouching": vouching }))),
    ))
}

/// Vouchings backing a user, with the voucher's identity resolved.
#[utoipa::path(
    get, path = "/api/users/{user_id}/vouchings", tag = "profile",
    params(("user_id" = uuid::Uuid, Path, description = "Whose profile")),
    responses((status = 200, body = serde_json::Value)),
)]
pub async fn list_for_user(
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

    let rows: Vec<(
        Uuid,
        Uuid,
        String,
        String,
        chrono::DateTime<chrono::Utc>,
        String,
    )> = sqlx::query_as(
        r#"
        SELECT v.id,
               v.voucher_id,
               COALESCE(NULLIF(u.display_name, ''), u.username),
               v.statement,
               v.active_until,
               v.at_stake_kind
          FROM vouchings v
          JOIN users u ON u.id = v.voucher_id
         WHERE v.vouched_id = $1
           AND v.broken_at IS NULL
           AND v.active_until > NOW()
         ORDER BY v.created_at DESC
        "#,
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await?;

    let vouchings_json: Vec<serde_json::Value> = rows
        .into_iter()
        .map(
            |(id, voucher_id, voucher_name, statement, active_until, at_stake_kind)| {
                json!({
                    "id": id,
                    "voucher_id": voucher_id,
                    "voucher_display_name": voucher_name,
                    "statement": statement,
                    "active_until": active_until.to_rfc3339(),
                    "at_stake_kind": at_stake_kind,
                })
            },
        )
        .collect();

    Ok(Json(wrap(json!({
        "vouchings": vouchings_json,
        "count": vouchings_json.len(),
    }))))
}

/// Vouchings the caller wrote for other people.
#[utoipa::path(
    get, path = "/api/users/me/vouchings", tag = "profile",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn list_mine(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<impl IntoResponse, AppError> {
    let given = vouchings::list_for_voucher(&state.db, auth.user_id).await?;
    let received = vouchings::list_for_vouched(&state.db, auth.user_id).await?;
    Ok(Json(wrap(json!({
        "given": given,
        "received": received,
        "max_live": vouchings::MAX_LIVE_VOUCHINGS,
    }))))
}

async fn withdraw(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    vouchings::withdraw(&state.db, id, auth.user_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BreakBody {
    /// At least 8 characters — this costs someone their rank.
    pub reason: String,
}

async fn break_vouching(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<BreakBody>,
) -> Result<impl IntoResponse, AppError> {
    capabilities::require_any_capability(&state.db, auth.user_id, MODERATOR_CAPS).await?;
    let report = vouchings::break_vouching(&state.db, id, auth.user_id, &body.reason).await?;
    Ok(Json(wrap(json!(report))))
}
