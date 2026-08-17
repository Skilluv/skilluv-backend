//! Paid mentoring: monthly arrangements, programmes, one-off slots, and the
//! placement commission that was declared in migration 0107 and never wired.

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use bigdecimal::BigDecimal;
use serde::Deserialize;
use serde_json::{Value, json};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::mentoring_products;

pub fn mentoring_product_routes() -> Router<AppState> {
    Router::new()
        // Monthly.
        .route("/mentors/{id}/subscribe", post(subscribe))
        .route("/users/me/mentor-subscriptions", get(my_subscriptions))
        .route(
            "/mentor-subscriptions/{id}/cancel",
            post(cancel_subscription),
        )
        .route("/mentor-subscriptions/{id}/usage", get(subscription_usage))
        // Volunteer hours.
        .route("/mentors/me/volunteer-hours", post(record_hours))
        // One-off slots.
        .route("/mentors/me/open-slots", post(open_slot))
        .route("/mentors/{id}/open-slots", get(open_slots))
        // Programmes.
        .route("/mentoring-programs", get(open_programs).post(open_program))
        .route("/mentoring-programs/{id}/enrol", post(enrol))
}

pub fn admin_mentoring_routes() -> Router<AppState> {
    Router::new().route(
        "/admin/mentoring/placement-commission",
        post(award_commission),
    )
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
// Monthly
// ═══════════════════════════════════════════════════════════════════

/// Subscribe to a mentor by the month.
#[utoipa::path(
    post, path = "/api/mentors/{id}/subscribe", tag = "mentorship",
    params(("id" = Uuid, Path, description = "Mentor user id")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "The mentor does not offer a monthly arrangement, or has set no price", body = crate::api_response::ErrorResponse),
        (status = 404, description = "No such mentor", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn subscribe(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let subscription = mentoring_products::subscribe(&state.db, id, auth.user_id).await?;
    metrics::counter!("skilluv_mentor_subscriptions_total").increment(1);
    Ok(Json(build_response(
        json!({ "subscription": subscription }),
    )))
}

#[utoipa::path(
    get, path = "/api/users/me/mentor-subscriptions", tag = "mentorship",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn my_subscriptions(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let subscriptions = mentoring_products::my_subscriptions(&state.db, auth.user_id).await?;
    Ok(Json(build_response(
        json!({ "subscriptions": subscriptions }),
    )))
}

#[utoipa::path(
    post, path = "/api/mentor-subscriptions/{id}/cancel", tag = "mentorship",
    params(("id" = Uuid, Path, description = "Subscription id")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "Not yours", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn cancel_subscription(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    mentoring_products::cancel_subscription(&state.db, id, auth.user_id).await?;
    // What was paid for runs to its end.
    Ok(Json(build_response(
        json!({ "auto_renew": false, "access_until_period_end": true }),
    )))
}

/// How many of the included sessions have been used this month.
#[utoipa::path(
    get, path = "/api/mentor-subscriptions/{id}/usage", tag = "mentorship",
    params(("id" = Uuid, Path, description = "Subscription id")),
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn subscription_usage(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let (used, included) = mentoring_products::sessions_used(&state.db, id).await?;
    Ok(Json(build_response(json!({
        "used_this_month": used,
        "included": included,
        "remaining": (included as i64 - used).max(0),
    }))))
}

// ═══════════════════════════════════════════════════════════════════
// Volunteer hours
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, ToSchema)]
pub struct HoursBody {
    pub mentee_user_id: Uuid,
    #[serde(default)]
    pub session_id: Option<Uuid>,
    #[schema(value_type = String)]
    pub hours: BigDecimal,
}

/// Record hours given free.
#[utoipa::path(
    post, path = "/api/mentors/me/volunteer-hours", tag = "mentorship",
    request_body = HoursBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "You charge for your sessions, so these are not volunteer hours", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn record_hours(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<HoursBody>,
) -> Result<Json<Value>, AppError> {
    let total = mentoring_products::record_volunteer_hours(
        &state.db,
        auth.user_id,
        body.mentee_user_id,
        body.session_id,
        body.hours,
    )
    .await?;

    Ok(Json(build_response(json!({
        "hours_with_this_mentee": total,
        "commission_threshold": mentoring_products::VOLUNTEER_THRESHOLD_HOURS,
    }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CommissionBody {
    pub mentor_user_id: Uuid,
    pub mentee_user_id: Uuid,
    pub enterprise_id: Uuid,
    pub placement_amount_cents: i64,
}

/// A mentee was hired. Pay the mentor who got them there.
#[utoipa::path(
    post, path = "/api/admin/mentoring/placement-commission", tag = "admin",
    request_body = CommissionBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Below the volunteer threshold, already paid for those hours, or a duplicate", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn award_commission(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CommissionBody>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let share = mentoring_products::award_placement_commission(
        &state.db,
        body.mentor_user_id,
        body.mentee_user_id,
        body.enterprise_id,
        body.placement_amount_cents,
    )
    .await?;
    Ok(Json(build_response(json!({ "mentor_share_cents": share }))))
}

// ═══════════════════════════════════════════════════════════════════
// One-off slots
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, ToSchema)]
pub struct SlotBody {
    pub date: chrono::NaiveDate,
    #[schema(value_type = String)]
    pub start_time: chrono::NaiveTime,
    #[schema(value_type = String)]
    pub end_time: chrono::NaiveTime,
    #[serde(default)]
    pub timezone: Option<String>,
}

/// Open a single slot, without committing to it every week for ever.
#[utoipa::path(
    post, path = "/api/mentors/me/open-slots", tag = "mentorship",
    request_body = SlotBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "A backwards slot, or a day already past", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn open_slot(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<SlotBody>,
) -> Result<Json<Value>, AppError> {
    let id = mentoring_products::open_slot(
        &state.db,
        auth.user_id,
        body.date,
        body.start_time,
        body.end_time,
        body.timezone.as_deref().unwrap_or("UTC"),
    )
    .await?;
    Ok(Json(build_response(json!({ "slot_id": id }))))
}

/// What a mentor has free to book, one-off slots only.
#[utoipa::path(
    get, path = "/api/mentors/{id}/open-slots", tag = "mentorship",
    params(("id" = Uuid, Path, description = "Mentor user id")),
    responses((status = 200, body = serde_json::Value)),
)]
pub async fn open_slots(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let slots = mentoring_products::open_slots(&state.db, id).await?;
    Ok(Json(build_response(json!({ "slots": slots }))))
}

// ═══════════════════════════════════════════════════════════════════
// Programmes
// ═══════════════════════════════════════════════════════════════════

/// Cohorts a mentee can join.
///
/// Corporate runs are absent by construction: their places are allocated by
/// the client who paid for them, not browsed.
#[utoipa::path(
    get, path = "/api/mentoring-programs", tag = "mentorship",
    responses((status = 200, body = serde_json::Value)),
)]
pub async fn open_programs(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let programs = mentoring_products::open_programs(&state.db).await?;
    Ok(Json(build_response(json!({ "programs": programs }))))
}

#[utoipa::path(
    post, path = "/api/mentoring-programs", tag = "mentorship",
    request_body(content = serde_json::Value, description = "ProgramInput"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "A kind priced the wrong way, or an empty brief", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn open_program(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<mentoring_products::ProgramInput>,
) -> Result<Json<Value>, AppError> {
    let program = mentoring_products::open_program(&state.db, auth.user_id, input).await?;
    Ok(Json(build_response(json!({ "program": program }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct EnrolBody {
    /// For a corporate run, the client's employee — who may not have a
    /// Skilluv account.
    #[serde(default)]
    pub mentee_email: Option<String>,
    #[serde(default)]
    pub mentee_name: Option<String>,
}

#[utoipa::path(
    post, path = "/api/mentoring-programs/{id}/enrol", tag = "mentorship",
    params(("id" = Uuid, Path, description = "Programme id")),
    request_body = EnrolBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Full, closed, already enrolled, or a cohort enrolled by email", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn enrol(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<EnrolBody>,
) -> Result<Json<Value>, AppError> {
    let program = mentoring_products::program(&state.db, id).await?;

    // A cohort mentee enrols themselves. A corporate place is filled by the
    // company that is paying for it, so only they may name somebody else.
    let (user, email) = if program.kind == "corporate" {
        let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
        if program.enterprise_id != Some(enterprise.id) {
            return Err(AppError::NotFound("programme not found".into()));
        }
        (None, body.mentee_email.as_deref())
    } else {
        (Some(auth.user_id), None)
    };

    let member_id =
        mentoring_products::enrol(&state.db, id, user, email, body.mentee_name.as_deref()).await?;

    Ok(Json(build_response(json!({ "member_id": member_id }))))
}
