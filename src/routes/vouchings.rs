//! SKI-46 (Post-MVP T3-03) — reputation staking endpoints.
//!
//! Endpoints:
//!   POST   /api/vouchings                        (auth, Doyen)
//!   GET    /api/users/{id}/vouchings             (public if profile is)
//!   GET    /api/users/me/vouchings               (auth — what I back)
//!   DELETE /api/vouchings/{id}                   (voucher — withdraw)
//!   GET    /api/moderation/vouchings            (moderator — the queue)
//!   POST   /api/moderation/vouchings/{id}/break  (moderator)
//!
//! Breaking a vouching costs the voucher a rank for three months, so it is
//! a moderator action behind an explicit endpoint, never a side effect of
//! a fraud flag being set somewhere else.

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
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
        .route("/moderation/vouchings", get(moderation_queue))
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

#[derive(Debug, Deserialize, utoipa::ToSchema)]
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

/// Vouch for somebody. A vouching carries the voucher's own standing,
/// which is why it can be broken.
#[utoipa::path(
    post, path = "/api/vouchings",
    operation_id = "vouchingsCreate",
    tag = "profile",
    request_body = CreateVouchingBody,
    responses(
        (status = 201, description = "Vouched"),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn create(
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

    let vouchings = vouchings::list_for_vouched_resolved(&state.db, user_id).await?;

    Ok(Json(wrap(json!({
        "vouchings": vouchings,
        "count": vouchings.len(),
    }))))
}

/// Vouchings the caller wrote for other people.
#[utoipa::path(
    get, path = "/api/users/me/vouchings",
    operation_id = "vouchingsListMine",
    tag = "profile",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn list_mine(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<impl IntoResponse, AppError> {
    // Resolved on both sides: a "my cautions" page built on the raw rows
    // could only print UUIDs, which is the same dead end SKI-301 reports on
    // the public listing.
    let given = vouchings::list_given_resolved(&state.db, auth.user_id).await?;
    let received = vouchings::list_received_resolved(&state.db, auth.user_id).await?;
    Ok(Json(wrap(json!({
        "given": given,
        "received": received,
        "max_live": vouchings::MAX_LIVE_VOUCHINGS,
    }))))
}

/// Withdraw a vouching the caller wrote. Their standing backed it, so it
/// is theirs to take back.
#[utoipa::path(
    delete, path = "/api/vouchings/{id}",
    operation_id = "vouchingsWithdraw",
    tag = "profile",
    params(("id" = uuid::Uuid, Path)),
    responses(
        (status = 204, description = "Withdrawn"),
        (status = 404, description = "No vouching of yours with that id", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn withdraw(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    vouchings::withdraw(&state.db, id, auth.user_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = VouchingBreakBody)]
pub struct BreakBody {
    /// At least 8 characters — this costs someone their rank.
    pub reason: String,
}

/// Break a vouching. Moderators only, and the reason is recorded.
#[utoipa::path(
    post, path = "/api/moderation/vouchings/{id}/break", tag = "moderation",
    params(("id" = uuid::Uuid, Path, description = "The vouching to break")),
    request_body = BreakBody,
    responses(
        (status = 200, description = "Broken, with the reason recorded"),
        (status = 403, description = "Not a moderator", body = crate::api_response::ErrorResponse),
        (status = 404, description = "No such vouching", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn break_vouching(
    State(state): State<AppState>,
    auth: AuthUser,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(body): Json<BreakBody>,
) -> Result<impl IntoResponse, AppError> {
    capabilities::require_any_capability(&state.db, auth.user_id, MODERATOR_CAPS).await?;
    let report = vouchings::break_vouching(&state.db, id, auth.user_id, &body.reason).await?;

    // SKI-299 — this costs the voucher a rank for ninety days. `broken_by`
    // records who, but only on the row itself, which the same moderator can
    // keep editing; the append-only journal is what makes the decision
    // reviewable afterwards by someone who was not in the room.
    crate::services::audit::record(
        &state.db,
        crate::services::audit::AuditEntry {
            actor_type: crate::services::audit::ActorType::Admin,
            actor_id: Some(auth.user_id),
            action: "vouching.break",
            target_type: Some("vouching"),
            target_id: Some(id),
            metadata: Some(json!({
                "voucher_id": report.vouching.voucher_id,
                "vouched_id": report.vouching.vouched_id,
                "at_stake_kind": report.vouching.at_stake_kind,
                "reason": body.reason,
                "penalty_applied": report.penalty_applied,
                "voucher_rank_before": report.voucher_rank_before,
                "voucher_rank_effective": report.voucher_rank_effective,
                "penalty_until": report.penalty_until.map(|d| d.to_rfc3339()),
            })),
            headers: Some(&headers),
        },
    )
    .await;

    Ok(Json(wrap(json!(report))))
}

#[derive(Debug, Deserialize)]
pub struct QueueQuery {
    /// `live` (default) | `broken` | `expired`.
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub voucher_id: Option<Uuid>,
    #[serde(default)]
    pub vouched_id: Option<Uuid>,
    #[serde(default)]
    pub at_stake_kind: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
}

/// SKI-297 — the queue the break endpoint always needed.
///
/// Same gate as the break itself: a moderator who may end a vouching may
/// read the list of them, and splitting the two would mean granting the
/// destructive half without the half that tells you where to point it.
#[utoipa::path(
    get, path = "/api/moderation/vouchings", tag = "moderation",
    operation_id = "moderationVouchingsQueue",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn moderation_queue(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<QueueQuery>,
) -> Result<impl IntoResponse, AppError> {
    capabilities::require_any_capability(&state.db, auth.user_id, MODERATOR_CAPS).await?;

    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let offset = q.offset.unwrap_or(0).max(0);
    let status = q
        .status
        .unwrap_or_else(|| vouchings::STATUS_LIVE.to_string());

    let (vouchings_rows, total) = vouchings::moderation_queue(
        &state.db,
        vouchings::QueueFilter {
            status: status.clone(),
            voucher_id: q.voucher_id,
            vouched_id: q.vouched_id,
            at_stake_kind: q.at_stake_kind,
            limit,
            offset,
        },
    )
    .await?;

    Ok(Json(wrap(json!({
        "vouchings": vouchings_rows,
        "status": status,
        "total": total,
        "limit": limit,
        "offset": offset,
    }))))
}
