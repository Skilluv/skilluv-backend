//! The internal pipeline, and the consolidated view a company sees of itself.
//!
//! Two audiences from one set of tables. The company sees what it has and
//! what it has spent; Skilluv sees the same thing plus who it is talking to
//! and what is about to lapse.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::sales_pipeline;

pub fn enterprise_overview_routes() -> Router<AppState> {
    Router::new().route("/enterprise/overview", get(my_overview))
}

pub fn admin_sales_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/admin/sales/opportunities",
            get(pipeline).post(open_opportunity),
        )
        .route("/admin/sales/opportunities/{id}", get(read_opportunity))
        .route("/admin/sales/opportunities/{id}/stage", post(set_stage))
        .route(
            "/admin/sales/opportunities/{id}/activities",
            post(record_activity),
        )
        .route("/admin/sales/overdue", get(overdue))
        .route("/admin/sales/renewals", get(renewals))
        .route("/admin/sales/enterprises/{id}", get(enterprise_file))
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

// ═══════════════════════════════════════════════════════════════════
// What a company sees of itself
// ═══════════════════════════════════════════════════════════════════

/// Everything this company has with Skilluv, in one answer.
///
/// One query rather than eighteen, because every product registers itself in
/// `enterprise_products` — which is the reason that table exists.
#[utoipa::path(
    get, path = "/api/enterprise/overview", tag = "enterprise",
    responses(
        (status = 200, body = serde_json::Value),
        (status = 403, description = "Not acting for an enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn my_overview(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;

    let products = sales_pipeline::products_of(&state.db, enterprise.id).await?;
    let spend = sales_pipeline::spend_by_stream(&state.db, enterprise.id).await?;
    let renewals = sales_pipeline::renewals_within(&state.db, 90).await?;
    let could_also =
        sales_pipeline::unused_products_in_familiar_pillars(&state.db, enterprise.id).await?;

    let mine: Vec<_> = renewals
        .into_iter()
        .filter(|r| r.enterprise_id == Some(enterprise.id))
        .collect();

    Ok(Json(build_response(json!({
        "products": products,
        "spend_by_stream": spend,
        "renewing_within_90_days": mine,
        // A list, not a ranking. With no sales history there is nothing to
        // rank against, and a confident ordering built on nothing is worse
        // than a list.
        "also_available_in_pillars_you_use": could_also,
    }))))
}

// ═══════════════════════════════════════════════════════════════════
// The pipeline
// ═══════════════════════════════════════════════════════════════════

#[utoipa::path(
    post, path = "/api/admin/sales/opportunities", tag = "admin",
    request_body(content = serde_json::Value, description = "OpportunityInput"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "No organisation named", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn open_opportunity(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<sales_pipeline::OpportunityInput>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let opportunity = sales_pipeline::open_opportunity(&state.db, auth.user_id, input).await?;
    Ok(Json(build_response(json!({ "opportunity": opportunity }))))
}

#[utoipa::path(
    get, path = "/api/admin/sales/opportunities", tag = "admin",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn pipeline(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let open = sales_pipeline::pipeline(&state.db).await?;

    let pairs: Vec<(String, bigdecimal::BigDecimal)> = open
        .iter()
        .filter_map(|o| o.estimated_value.clone().map(|v| (o.stage.clone(), v)))
        .collect();

    Ok(Json(build_response(json!({
        "opportunities": open,
        "weighted_value": sales_pipeline::weighted_value(&pairs),
        // Said plainly: with no closed deals to calibrate against, the
        // weights are guesses and the total is a sum of guesses.
        "weighted_value_note": "Somme pondérée par des poids d'étape choisis a priori. \
                                Aucune affaire close ne les a encore calibrés.",
    }))))
}

#[utoipa::path(
    get, path = "/api/admin/sales/opportunities/{id}", tag = "admin",
    params(("id" = Uuid, Path, description = "Opportunity id")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "No such opportunity", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn read_opportunity(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let opportunity = sales_pipeline::opportunity(&state.db, id).await?;
    let activities = sales_pipeline::activities(&state.db, id).await?;
    Ok(Json(build_response(
        json!({ "opportunity": opportunity, "activities": activities }),
    )))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct StageBody {
    pub stage: String,
    #[serde(default)]
    pub lost_reason: Option<String>,
}

#[utoipa::path(
    post, path = "/api/admin/sales/opportunities/{id}/stage", tag = "admin",
    params(("id" = Uuid, Path, description = "Opportunity id")),
    request_body = StageBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Not a stage, or a loss with no reason", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn set_stage(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<StageBody>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let opportunity =
        sales_pipeline::set_stage(&state.db, id, &body.stage, body.lost_reason.as_deref()).await?;
    Ok(Json(build_response(json!({ "opportunity": opportunity }))))
}

#[utoipa::path(
    post, path = "/api/admin/sales/opportunities/{id}/activities", tag = "admin",
    params(("id" = Uuid, Path, description = "Opportunity id")),
    request_body(content = serde_json::Value, description = "ActivityInput"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Not a kind, or an empty summary", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn record_activity(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<sales_pipeline::ActivityInput>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let activity_id = sales_pipeline::record_activity(&state.db, id, auth.user_id, input).await?;
    Ok(Json(build_response(json!({ "activity_id": activity_id }))))
}

/// Everything somebody said they would do and has not.
#[utoipa::path(
    get, path = "/api/admin/sales/overdue", tag = "admin",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn overdue(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let overdue = sales_pipeline::overdue_next_steps(&state.db).await?;
    Ok(Json(build_response(json!({ "overdue": overdue }))))
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct RenewalQuery {
    /// How far ahead to look. Ninety days by default: long enough to act,
    /// short enough that the list is not everything.
    #[serde(default)]
    pub within_days: Option<i64>,
}

#[utoipa::path(
    get, path = "/api/admin/sales/renewals", tag = "admin",
    params(RenewalQuery),
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn renewals(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<RenewalQuery>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let days = q.within_days.unwrap_or(90).clamp(1, 730);
    let renewals = sales_pipeline::renewals_within(&state.db, days).await?;
    Ok(Json(build_response(
        json!({ "renewals": renewals, "within_days": days }),
    )))
}

/// One company's whole file: what they have, what they spent, what lapses.
#[utoipa::path(
    get, path = "/api/admin/sales/enterprises/{id}", tag = "admin",
    params(("id" = Uuid, Path, description = "Enterprise id")),
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn enterprise_file(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;

    let products = sales_pipeline::products_of(&state.db, id).await?;
    let spend = sales_pipeline::spend_by_stream(&state.db, id).await?;
    let renewals: Vec<_> = sales_pipeline::renewals_within(&state.db, 365)
        .await?
        .into_iter()
        .filter(|r| r.enterprise_id == Some(id))
        .collect();
    let could_also = sales_pipeline::unused_products_in_familiar_pillars(&state.db, id).await?;

    Ok(Json(build_response(json!({
        "products": products,
        "spend_by_stream": spend,
        "renewals": renewals,
        "not_yet_used_in_familiar_pillars": could_also,
    }))))
}
