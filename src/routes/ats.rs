//! The applicant tracker, over HTTP.
//!
//! Everything here is scoped to the calling person's enterprise. There is no
//! admin view of somebody's pipeline and no cross-company listing: these rows
//! belong to the company that entered them, and Skilluv holding them does not
//! make them Skilluv's to read.

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::ats;

pub fn ats_routes() -> Router<AppState> {
    Router::new()
        .route("/ats/plans", get(plans))
        .route("/ats/subscription", get(my_subscription).post(subscribe))
        .route("/ats/openings", get(my_openings).post(open_position))
        .route("/ats/openings/{id}/close", post(close_position))
        .route("/ats/openings/{id}/pipeline", get(pipeline))
        .route("/ats/openings/{id}/candidates", post(add_candidate))
        .route("/ats/candidates/{id}/move", post(move_candidate))
}

fn build_response(data: Value) -> Value {
    json!({
        "data": data,
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })
}

/// The enterprise this person acts for.
///
/// An ATS call from somebody with no company is not an authorisation problem
/// to be logged — it is a person on the wrong page, and the message says so.
async fn enterprise_of(state: &AppState, auth: &AuthUser) -> Result<Uuid, AppError> {
    sqlx::query_scalar("SELECT id FROM enterprises WHERE owner_id = $1")
        .bind(auth.user_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| {
            AppError::Validation("the applicant tracker belongs to a company account".into())
        })
}

/// What the tracker costs, and what each plan allows.
#[utoipa::path(
    get, path = "/api/ats/plans", tag = "enterprise",
    responses((status = 200, body = serde_json::Value)),
)]
pub async fn plans(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let plans: Vec<Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object(
                    'slug', slug, 'label', label,
                    'max_open_positions', max_open_positions,
                    'max_candidates_per_opening', max_candidates_per_opening,
                    'included_credits', included_credits,
                    'monthly_fee', monthly_fee, 'currency', currency,
                    'retention_days', retention_days)
           FROM ats_plans WHERE is_active ORDER BY sort_order",
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(build_response(json!({ "plans": plans }))))
}

#[derive(Deserialize, ToSchema)]
pub struct SubscribeBody {
    pub plan: String,
}

/// Start the tracker, or change plan.
///
/// The free tier is claimed here like any other: nothing is assumed on a
/// company's behalf, so "has a tracker" has one answer rather than a default
/// buried in a handler.
#[utoipa::path(
    post, path = "/api/ats/subscription", tag = "enterprise",
    request_body = SubscribeBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Not a plan, or not a company account", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn subscribe(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<SubscribeBody>,
) -> Result<Json<Value>, AppError> {
    let enterprise = enterprise_of(&state, &auth).await?;
    let chosen = ats::choose_plan(&state.db, enterprise, &body.plan, None).await?;
    Ok(Json(build_response(json!({ "subscription": chosen }))))
}

#[utoipa::path(
    get, path = "/api/ats/subscription", tag = "enterprise",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn my_subscription(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let enterprise = enterprise_of(&state, &auth).await?;
    let plan = ats::plan_for(&state.db, enterprise).await?;
    Ok(Json(build_response(json!({ "plan": plan }))))
}

#[utoipa::path(
    post, path = "/api/ats/openings", tag = "enterprise",
    request_body = crate::services::ats::OpeningInput,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "No tracker, no title, or the plan's ceiling reached", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn open_position(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<ats::OpeningInput>,
) -> Result<Json<Value>, AppError> {
    let enterprise = enterprise_of(&state, &auth).await?;
    let opening = ats::open_position(&state.db, enterprise, auth.user_id, input).await?;
    Ok(Json(build_response(json!({ "opening": opening }))))
}

#[utoipa::path(
    get, path = "/api/ats/openings", tag = "enterprise",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn my_openings(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let enterprise = enterprise_of(&state, &auth).await?;
    let openings = ats::openings_for(&state.db, enterprise).await?;
    Ok(Json(build_response(json!({ "openings": openings }))))
}

/// Close a position. The candidates stay, and so do their erasure dates.
#[utoipa::path(
    post, path = "/api/ats/openings/{id}/close", tag = "enterprise",
    params(("id" = Uuid, Path, description = "Opening id")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "Not an open position of yours", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn close_position(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let enterprise = enterprise_of(&state, &auth).await?;
    let opening = ats::close_position(&state.db, id, enterprise).await?;
    Ok(Json(build_response(json!({ "opening": opening }))))
}

/// The pipeline, stage by stage.
#[utoipa::path(
    get, path = "/api/ats/openings/{id}/pipeline", tag = "enterprise",
    params(("id" = Uuid, Path, description = "Opening id")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "Not an opening of yours", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn pipeline(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let enterprise = enterprise_of(&state, &auth).await?;
    let stages = ats::pipeline(&state.db, id, enterprise).await?;
    // What the opening does not say, beside the people it is about. A
    // recruiter reads this screen when deciding who to talk to, which is the
    // moment a missing salary range still costs nothing to fix.
    let gaps = ats::gaps(&state.db, id).await?;
    Ok(Json(build_response(
        json!({ "stages": stages, "gaps": gaps }),
    )))
}

#[utoipa::path(
    post, path = "/api/ats/openings/{id}/candidates", tag = "enterprise",
    params(("id" = Uuid, Path, description = "Opening id")),
    request_body = crate::services::ats::CandidateInput,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Nobody to contact, an unknown source, or the plan's ceiling", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn add_candidate(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<ats::CandidateInput>,
) -> Result<Json<Value>, AppError> {
    let enterprise = enterprise_of(&state, &auth).await?;
    let candidate = ats::add_candidate(&state.db, id, enterprise, input).await?;
    Ok(Json(build_response(json!({ "candidate_id": candidate }))))
}

#[derive(Deserialize, ToSchema)]
pub struct MoveBody {
    pub to_stage_id: Uuid,
    /// Required when the destination is the refusing stage.
    #[serde(default)]
    pub reason: Option<String>,
}

/// Move somebody along, or tell them no.
///
/// A refusal needs a sentence. That is the one rule this endpoint enforces
/// that a company might not have chosen for itself, and it is deliberate:
/// Skilluv sells the tooling, not the tooling that makes silence easy.
#[utoipa::path(
    post, path = "/api/ats/candidates/{id}/move", tag = "enterprise",
    params(("id" = Uuid, Path, description = "Candidate id")),
    request_body = MoveBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "A refusal with no reason", body = crate::api_response::ErrorResponse),
        (status = 404, description = "Not a candidate of yours", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn move_candidate(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<MoveBody>,
) -> Result<Json<Value>, AppError> {
    let enterprise = enterprise_of(&state, &auth).await?;
    ats::move_candidate(
        &state.db,
        id,
        enterprise,
        body.to_stage_id,
        body.reason.as_deref(),
        auth.user_id,
    )
    .await?;
    Ok(Json(build_response(json!({ "moved": true }))))
}
