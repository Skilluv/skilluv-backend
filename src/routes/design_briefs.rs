//! Proposing design work, and deciding what becomes work.
//!
//! Design has no ingestion source the way code has GitHub issues, so the
//! source is editorial: somebody writes a brief, somebody reads it, it becomes
//! a slice. These are the two ends of that.
//!
//! Anybody with a completed profile may propose. The gate is at publication,
//! where a person reads the brief — not at proposal, because a capability
//! earned *by* proposing cannot also be required *to* propose.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::{AuthUser, AuthUserComplete};
use crate::services::design_briefs;

pub fn design_brief_routes() -> Router<AppState> {
    Router::new()
        .route("/design/briefs", post(propose))
        .route("/design/briefs/mine", get(mine))
        .route("/design/briefs/{id}/withdraw", post(withdraw))
}

pub fn admin_design_brief_routes() -> Router<AppState> {
    Router::new()
        .route("/admin/design/briefs", get(queue))
        .route("/admin/design/briefs/{id}/publish", post(publish))
        .route("/admin/design/briefs/{id}/reject", post(reject))
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

/// Whoever may decide what becomes work.
///
/// `community_curator` rather than `admin` alone: curation is the thing the
/// capability was created for, and reserving it to administrators would make
/// the queue's length a function of how many administrators there are.
///
/// `domain_curator:design` alongside it, and this is the narrowing rather than
/// a widening. `community_curator` is cross-domain by construction: granting
/// somebody the right to publish a design brief was granting them curation of
/// every domain's community surfaces at once, which is precisely the
/// over-grant a per-domain capability exists to avoid (SKI-334). The domain
/// curator is the capability whose description already says "its challenges,
/// its contests, its featurings" — a design brief becoming a slice is that
/// sentence — so this queue is now reachable by the narrow grant as well as
/// the broad one.
///
/// No `design_curator`. It would be a second name for `domain_curator:design`,
/// and two names for one role is how a permission model stops being readable.
async fn require_curator(state: &AppState, auth: &AuthUser) -> Result<(), AppError> {
    crate::middleware::capabilities::require_any_capability(
        &state.db,
        auth.user_id,
        &[
            "admin",
            "community_curator",
            "domain_curator:design",
            "domain_curator:all",
        ],
    )
    .await
}

/// Write a brief and put it in the queue.
#[utoipa::path(
    post, path = "/api/design/briefs",
    operation_id = "designBriefsPropose",
    tag = "design",
    request_body = design_briefs::ProposeInput,
    responses(
        (status = 201, description = "queued for curation"),
        (status = 400, description = "too short, unknown trade, or unknown subtype",
         body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn propose(
    State(state): State<AppState>,
    auth: AuthUserComplete,
    Json(body): Json<design_briefs::ProposeInput>,
) -> Result<impl IntoResponse, AppError> {
    let proposal = design_briefs::propose(&state.db, auth.user_id, body).await?;
    Ok((
        StatusCode::CREATED,
        Json(wrap(json!({ "brief": proposal }))),
    ))
}

/// Your own briefs, whatever became of them.
#[utoipa::path(
    get, path = "/api/design/briefs/mine",
    operation_id = "designBriefsMine",
    tag = "design",
    responses((status = 200, description = "your briefs, newest first")),
    security(("cookie_auth" = [])),
)]
pub async fn mine(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<impl IntoResponse, AppError> {
    let briefs = design_briefs::mine(&state.db, auth.user_id).await?;
    Ok(Json(wrap(json!({ "briefs": briefs }))))
}

/// Take back a brief nobody has read yet.
#[utoipa::path(
    post, path = "/api/design/briefs/{id}/withdraw",
    operation_id = "designBriefsWithdraw",
    tag = "design",
    params(("id" = Uuid, Path)),
    responses(
        (status = 200, description = "withdrawn"),
        (status = 409, description = "already read, or not yours",
         body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn withdraw(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    design_briefs::withdraw(&state.db, id, auth.user_id).await?;
    Ok(Json(wrap(json!({ "withdrawn": true }))))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct QueueQuery {
    #[param(minimum = 1, maximum = 100)]
    pub limit: Option<i64>,
}

/// Briefs waiting to be read, oldest first.
///
/// Oldest first so nobody waits twice — the same rule the review queue
/// follows, and for the same reason.
#[utoipa::path(
    get, path = "/api/admin/design/briefs",
    operation_id = "designBriefsQueue",
    tag = "admin",
    params(QueueQuery),
    responses((status = 200, description = "the queue, oldest first")),
    security(("cookie_auth" = [])),
)]
pub async fn queue(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<QueueQuery>,
) -> Result<impl IntoResponse, AppError> {
    require_curator(&state, &auth).await?;
    let briefs = design_briefs::queue(&state.db, q.limit.unwrap_or(25)).await?;
    Ok(Json(wrap(json!({ "briefs": briefs }))))
}

/// Accept a brief: it becomes a slice somebody can claim.
#[utoipa::path(
    post, path = "/api/admin/design/briefs/{id}/publish", tag = "admin",
    params(("id" = Uuid, Path)),
    responses(
        (status = 200, description = "published, with the slice it became"),
        (status = 409, description = "already decided", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn publish(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    require_curator(&state, &auth).await?;
    let brief = design_briefs::publish(&state.db, id, auth.user_id).await?;
    Ok(Json(wrap(json!({ "brief": brief }))))
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RejectBody {
    /// Why. At least twenty characters, and shown to the author — a refusal
    /// with no reason is a refusal that comes back next week as the same
    /// brief.
    pub feedback: String,
}

/// Refuse a brief, saying why.
#[utoipa::path(
    post, path = "/api/admin/design/briefs/{id}/reject",
    operation_id = "designBriefsReject",
    tag = "admin",
    params(("id" = Uuid, Path)),
    request_body = RejectBody,
    responses(
        (status = 200, description = "refused, and the author told why"),
        (status = 400, description = "no reason given", body = crate::api_response::ErrorResponse),
        (status = 409, description = "already decided", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn reject(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<RejectBody>,
) -> Result<impl IntoResponse, AppError> {
    require_curator(&state, &auth).await?;
    let brief = design_briefs::reject(&state.db, id, auth.user_id, &body.feedback).await?;
    Ok(Json(wrap(json!({ "brief": brief }))))
}
