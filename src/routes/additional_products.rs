//! Long placements, corporate learning seats, open calls for proposals.

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
use crate::services::additional_products as products;

pub fn additional_product_routes() -> Router<AppState> {
    Router::new()
        // Long placements.
        .route("/enterprise/placements", get(my_placements).post(propose))
        .route("/users/me/placements", get(my_placements_as_junior))
        .route("/placements/{id}/respond", post(respond))
        // Corporate learning.
        .route("/learning/plans", get(list_plans))
        .route("/enterprise/learning", post(subscribe_learning))
        .route("/enterprise/learning/{id}/seats", post(invite_seat))
        .route("/enterprise/learning/{id}/usage", get(seat_usage))
        .route("/learning/{id}/activate", post(activate_seat))
        // Open calls.
        .route("/rfps", get(list_rfps))
        .route("/enterprise/rfps", post(open_rfp))
        .route(
            "/rfps/{id}/proposals",
            get(read_proposals).post(propose_on_rfp),
        )
        .route("/enterprise/rfp-proposals/{id}/decide", post(decide))
        .route("/enterprise/rfps/{id}/award", post(award))
}

pub fn admin_additional_product_routes() -> Router<AppState> {
    Router::new()
        .route("/admin/placements/{id}/bill-month", post(bill_month))
        .route("/admin/placements/{id}/end", post(end_placement))
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
// Long placements
// ═══════════════════════════════════════════════════════════════════

#[utoipa::path(
    post, path = "/api/enterprise/placements",
    operation_id = "additionalProductsPropose",
    tag = "enterprise",
    request_body(content = serde_json::Value, description = "PlacementInput"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "A monitoring fee with nobody assigned, or no salary", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not acting for an enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn propose(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<products::PlacementInput>,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
    let placement =
        products::propose_placement(&state.db, enterprise.id, auth.user_id, input).await?;

    let _ = sqlx::query(
        "INSERT INTO enterprise_products
            (enterprise_id, product_type, source_table, source_id,
             contract_value, currency, created_by)
         VALUES ($1, 'long_term_placement', 'long_term_placements', $2, $3, $4, $5)",
    )
    .bind(enterprise.id)
    .bind(placement.id)
    .bind(&placement.upfront_fee)
    .bind(placement.currency.as_str())
    .bind(auth.user_id)
    .execute(&state.db)
    .await;

    // Tell the person it concerns. Without this the proposal sits at
    // 'proposed' until somebody says it aloud — the whole failure SKI-331
    // named. Best-effort, like every notify: a delivery hiccup must not undo a
    // proposal that was recorded.
    let _ = crate::services::notify::send(
        &state,
        crate::services::notify::Recipient::User(placement.junior_user_id),
        "placement.proposed",
    )
    .arg("company", enterprise.company_name.clone())
    .payload(json!({ "placement_id": placement.id }))
    .execute()
    .await;

    Ok(Json(build_response(json!({
        "placement": placement,
        "note": "Le placement démarre quand la personne accepte. La garantie porte sur \
                 le service rendu, pas sur elle : rien ici ne l'oblige à rester.",
    }))))
}

#[utoipa::path(
    get, path = "/api/enterprise/placements", tag = "enterprise",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn my_placements(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
    let placements = products::placements_for_enterprise(&state.db, enterprise.id).await?;
    Ok(Json(build_response(json!({ "placements": placements }))))
}

/// The placements offered to me — the read a junior needs to find a proposal
/// and answer it (SKI-331). Proposed ones first, each naming the company and
/// the mentor. `respond` stays the enterprise-view sibling's opposite number:
/// only the targeted junior may take it.
#[utoipa::path(
    get, path = "/api/users/me/placements", tag = "work",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn my_placements_as_junior(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let placements = products::placements_for_junior(&state.db, auth.user_id).await?;
    Ok(Json(build_response(json!({ "placements": placements }))))
}

#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = AdditionalProductsRespondBody)]
pub struct RespondBody {
    pub accept: bool,
}

#[utoipa::path(
    post, path = "/api/placements/{id}/respond", tag = "work",
    params(("id" = Uuid, Path, description = "Placement id")),
    request_body = RespondBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "Nothing waiting on your answer", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
    operation_id = "additionalProductsRespond",
)]
pub async fn respond(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<RespondBody>,
) -> Result<Json<Value>, AppError> {
    let placement =
        products::respond_to_placement(&state.db, id, auth.user_id, body.accept).await?;
    Ok(Json(build_response(json!({ "placement": placement }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct MonthBody {
    pub month: chrono::NaiveDate,
}

#[utoipa::path(
    post, path = "/api/admin/placements/{id}/bill-month", tag = "admin",
    params(("id" = Uuid, Path, description = "Placement id")),
    request_body = MonthBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Not active, no monitoring fee, or already billed", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn bill_month(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<MonthBody>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;

    use chrono::Datelike;
    let month = chrono::NaiveDate::from_ymd_opt(body.month.year(), body.month.month(), 1)
        .unwrap_or(body.month);

    let billed = products::bill_monitoring_month(&state.db, id, month).await?;
    Ok(Json(build_response(
        json!({ "billed": billed, "month": month }),
    )))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct EndBody {
    pub reason: String,
}

/// End a placement, and say whether the guarantee applies.
#[utoipa::path(
    post, path = "/api/admin/placements/{id}/end", tag = "admin",
    params(("id" = Uuid, Path, description = "Placement id")),
    request_body = EndBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Not a reason we record", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn end_placement(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<EndBody>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let owed = products::end_placement(&state.db, id, &body.reason).await?;
    Ok(Json(build_response(json!({
        "guarantee_applies": owed,
        "reason": body.reason,
    }))))
}

// ═══════════════════════════════════════════════════════════════════
// Corporate learning
// ═══════════════════════════════════════════════════════════════════

#[utoipa::path(
    get, path = "/api/learning/plans", tag = "enterprise",
    responses((status = 200, body = serde_json::Value)),
)]
pub async fn list_plans(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let plans = products::learning_plans(&state.db).await?;
    Ok(Json(build_response(json!({ "plans": plans }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct LearningBody {
    pub plan: String,
    pub seats: i16,
}

#[utoipa::path(
    post, path = "/api/enterprise/learning", tag = "enterprise",
    request_body = LearningBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "No such plan", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn subscribe_learning(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<LearningBody>,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
    let subscription =
        products::subscribe_learning(&state.db, enterprise.id, &body.plan, body.seats).await?;
    Ok(Json(build_response(
        json!({ "subscription": subscription }),
    )))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SeatBody {
    pub employee_user_id: Uuid,
}

#[utoipa::path(
    post, path = "/api/enterprise/learning/{id}/seats", tag = "enterprise",
    params(("id" = Uuid, Path, description = "Subscription id")),
    request_body = SeatBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "No seats left", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn invite_seat(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<SeatBody>,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
    let subscription = products::learning_subscription(&state.db, id).await?;
    if subscription.enterprise_id != enterprise.id {
        return Err(AppError::NotFound("subscription not found".into()));
    }

    products::invite_to_seat(&state.db, id, body.employee_user_id).await?;
    Ok(Json(build_response(json!({ "invited": true }))))
}

/// Take a seat. Their own act, not their employer's.
#[utoipa::path(
    post, path = "/api/learning/{id}/activate", tag = "work",
    params(("id" = Uuid, Path, description = "Subscription id")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "No seat waiting for you", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn activate_seat(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    products::activate_seat(&state.db, id, auth.user_id).await?;
    Ok(Json(build_response(json!({ "activated": true }))))
}

/// Seats bought, handed out, and actually taken.
#[utoipa::path(
    get, path = "/api/enterprise/learning/{id}/usage", tag = "enterprise",
    params(("id" = Uuid, Path, description = "Subscription id")),
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn seat_usage(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
    let subscription = products::learning_subscription(&state.db, id).await?;
    if subscription.enterprise_id != enterprise.id {
        return Err(AppError::NotFound("subscription not found".into()));
    }

    let (bought, assigned, active) = products::seat_usage(&state.db, id).await?;
    Ok(Json(build_response(json!({
        "seats_bought": bought,
        "seats_assigned": assigned,
        // The honest number. A seat assigned and never used is not a user.
        "seats_taken_up": active,
    }))))
}

// ═══════════════════════════════════════════════════════════════════
// Open calls
// ═══════════════════════════════════════════════════════════════════

#[utoipa::path(
    get, path = "/api/rfps", tag = "work",
    responses((status = 200, body = serde_json::Value)),
)]
pub async fn list_rfps(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let rfps = products::open_rfps(&state.db).await?;
    Ok(Json(build_response(json!({ "rfps": rfps }))))
}

#[utoipa::path(
    post, path = "/api/enterprise/rfps", tag = "enterprise",
    request_body(content = serde_json::Value, description = "RfpInput"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Too thin a context, no budget range, or deadlines out of order", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn open_rfp(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<products::RfpInput>,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
    let rfp = products::open_rfp(&state.db, enterprise.id, auth.user_id, input).await?;
    Ok(Json(build_response(json!({ "rfp": rfp }))))
}

#[utoipa::path(
    post, path = "/api/rfps/{id}/proposals", tag = "work",
    params(("id" = Uuid, Path, description = "Call id")),
    request_body(content = serde_json::Value, description = "ProposalInput"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Closed, past the deadline, too thin, or already proposed", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn propose_on_rfp(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<products::ProposalInput>,
) -> Result<Json<Value>, AppError> {
    let proposal_id = products::submit_proposal(&state.db, id, auth.user_id, input).await?;
    Ok(Json(build_response(json!({ "proposal_id": proposal_id }))))
}

#[utoipa::path(
    get, path = "/api/rfps/{id}/proposals", tag = "enterprise",
    params(("id" = Uuid, Path, description = "Call id")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "Not your call", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn read_proposals(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
    let rfp = products::rfp(&state.db, id).await?;
    if rfp.enterprise_id != enterprise.id {
        return Err(AppError::NotFound("call not found".into()));
    }
    let proposals = products::rfp_proposals(&state.db, id).await?;
    Ok(Json(build_response(json!({ "proposals": proposals }))))
}

#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = AdditionalProductsDecisionBody)]
pub struct DecisionBody {
    pub selected: bool,
    #[serde(default)]
    pub note: Option<String>,
}

#[utoipa::path(
    post, path = "/api/enterprise/rfp-proposals/{id}/decide", tag = "enterprise",
    params(("id" = Uuid, Path, description = "Proposal id")),
    request_body = DecisionBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "A refusal with no reason", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
    operation_id = "additionalProductsDecide",
)]
pub async fn decide(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<DecisionBody>,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;

    let owner: Option<Uuid> = sqlx::query_scalar(
        "SELECT r.enterprise_id FROM rfp_proposals p
           JOIN open_rfps r ON r.id = p.rfp_id
          WHERE p.id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?;
    if owner != Some(enterprise.id) {
        return Err(AppError::NotFound("proposal not found".into()));
    }

    products::decide_proposal(&state.db, id, body.selected, body.note.as_deref()).await?;
    Ok(Json(build_response(json!({ "selected": body.selected }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AwardBody {
    pub winner_proposal_id: Uuid,
}

#[utoipa::path(
    post, path = "/api/enterprise/rfps/{id}/award", tag = "enterprise",
    params(("id" = Uuid, Path, description = "Call id")),
    request_body = AwardBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Proposals still have no answer", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn award(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<AwardBody>,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
    let rfp = products::rfp(&state.db, id).await?;
    if rfp.enterprise_id != enterprise.id {
        return Err(AppError::NotFound("call not found".into()));
    }

    let booked = products::award_rfp(&state.db, id, body.winner_proposal_id).await?;
    Ok(Json(build_response(
        json!({ "awarded": true, "facilitation_fee": booked }),
    )))
}
