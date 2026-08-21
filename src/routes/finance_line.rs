//! The finance line.
//!
//! The routes a contributor can reach are all things they start themselves:
//! asking for an advance on their own invoice, asking to be introduced to a
//! partner, subscribing to the payment guarantee. Nothing here is offered to
//! somebody because a model thought they looked short of money.

use axum::extract::{Path, Query, State};
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
use crate::services::finance_line;

pub fn finance_routes() -> Router<AppState> {
    Router::new()
        .route("/finance/partners", get(list_partners))
        .route("/finance/referrals", post(request_referral))
        .route("/users/me/advances", get(my_advances).post(request_advance))
        .route("/finance/guarantee", post(subscribe_guarantee))
}

pub fn admin_finance_routes() -> Router<AppState> {
    Router::new()
        .route("/admin/finance/partnerships", post(open_partnership))
        .route(
            "/admin/finance/partnerships/{id}/activate",
            post(activate_partnership),
        )
        .route(
            "/admin/finance/referrals/{id}/decision",
            post(record_decision),
        )
        .route("/admin/finance/advances/{id}/disburse", post(disburse))
        .route("/admin/finance/advances/{id}/repaid", post(mark_repaid))
        .route("/admin/finance/advances/{id}/write-off", post(write_off))
        .route("/admin/finance/guarantee-claims", post(honour_guarantee))
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

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct PartnersQuery {
    /// Narrow to the partners licensed where you are.
    #[serde(default)]
    pub country: Option<String>,
}

/// Who a contributor can actually be introduced to.
///
/// Active partnerships only, which means the ones with a stated regulatory
/// basis and a signed contract. A draft partnership is invisible rather than
/// greyed out: an introduction we cannot lawfully make should not be
/// advertised as coming soon.
#[utoipa::path(
    get, path = "/api/finance/partners", tag = "work",
    params(PartnersQuery),
    responses((status = 200, body = serde_json::Value)),
)]
pub async fn list_partners(
    State(state): State<AppState>,
    Query(q): Query<PartnersQuery>,
) -> Result<Json<Value>, AppError> {
    let partners = finance_line::open_partnerships(&state.db, q.country.as_deref()).await?;
    Ok(Json(build_response(json!({ "partners": partners }))))
}

/// Ask to be introduced.
#[utoipa::path(
    post, path = "/api/finance/referrals", tag = "work",
    request_body(content = serde_json::Value, description = "ReferralInput"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "An inactive partnership, an empty purpose, or below the partner's rank floor", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn request_referral(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<finance_line::ReferralInput>,
) -> Result<Json<Value>, AppError> {
    let id = finance_line::request_referral(&state.db, auth.user_id, input).await?;

    // What was passed on, returned to the person it is about. They are
    // entitled to see it without asking, and it is what the partner priced
    // on.
    let snapshot: serde_json::Value =
        sqlx::query_scalar("SELECT shared_snapshot FROM partnership_referrals WHERE id = $1")
            .bind(id)
            .fetch_one(&state.db)
            .await?;

    Ok(Json(build_response(
        json!({ "referral_id": id, "shared_with_partner": snapshot }),
    )))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AdvanceBody {
    pub invoice_id: Uuid,
    /// Between 30 and 90 per cent of the invoice.
    #[schema(value_type = String)]
    pub advance_percent: BigDecimal,
}

/// Ask for an advance on one's own issued invoice.
#[utoipa::path(
    post, path = "/api/users/me/advances", tag = "work",
    request_body = AdvanceBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "The invoice is not advanceable, the rank floor, an outstanding write-off, or a percentage out of band", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn request_advance(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<AdvanceBody>,
) -> Result<Json<Value>, AppError> {
    let advance = finance_line::request_advance(
        &state.db,
        auth.user_id,
        body.invoice_id,
        body.advance_percent,
    )
    .await?;

    let net = &advance.advance_amount - &advance.fee_amount;
    Ok(Json(build_response(json!({
        "advance": advance,
        // The number they care about, stated before anybody agrees to
        // anything.
        "you_would_receive": net,
    }))))
}

#[utoipa::path(
    get, path = "/api/users/me/advances", tag = "work",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn my_advances(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let advances = finance_line::advances_for(&state.db, auth.user_id).await?;
    Ok(Json(build_response(json!({ "advances": advances }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct GuaranteeBody {
    pub tier: String,
}

#[utoipa::path(
    post, path = "/api/finance/guarantee", tag = "work",
    request_body = GuaranteeBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Not a tier", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn subscribe_guarantee(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<GuaranteeBody>,
) -> Result<Json<Value>, AppError> {
    let expires = finance_line::subscribe_guarantee(&state.db, auth.user_id, &body.tier).await?;
    let cover = finance_line::guarantee_tier(&body.tier);
    Ok(Json(build_response(json!({
        "expires_at": expires,
        "monthly_fee": cover.map(|c| c.0),
        "max_per_mission": cover.map(|c| c.1),
        "annual_cap": cover.map(|c| c.2),
    }))))
}

// ═══════════════════════════════════════════════════════════════════
// Admin
// ═══════════════════════════════════════════════════════════════════

#[utoipa::path(
    post, path = "/api/admin/finance/partnerships", tag = "admin",
    request_body(content = serde_json::Value, description = "PartnershipInput"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "An unknown kind, or no country", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn open_partnership(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<finance_line::PartnershipInput>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let partnership = finance_line::open_partnership(&state.db, input).await?;
    Ok(Json(build_response(json!({ "partnership": partnership }))))
}

/// Turn a partnership on, once the paperwork exists.
#[utoipa::path(
    post, path = "/api/admin/finance/partnerships/{id}/activate", tag = "admin",
    params(("id" = Uuid, Path, description = "Partnership id")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "No stated regulatory basis, or no signed contract", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn activate_partnership(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let partnership = finance_line::activate_partnership(&state.db, id).await?;
    Ok(Json(build_response(json!({ "partnership": partnership }))))
}

#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = FinanceLineDecisionBody)]
pub struct DecisionBody {
    pub approved: bool,
    #[serde(default)]
    #[schema(value_type = Option<String>)]
    pub approved_amount: Option<BigDecimal>,
    #[serde(default)]
    #[schema(value_type = Option<String>)]
    pub monthly_premium: Option<BigDecimal>,
    #[serde(default)]
    pub note: Option<String>,
}

#[utoipa::path(
    post, path = "/api/admin/finance/referrals/{id}/decision", tag = "admin",
    params(("id" = Uuid, Path, description = "Referral id")),
    request_body = DecisionBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Already decided, or an approval that says nothing", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn record_decision(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<DecisionBody>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let commission = finance_line::record_decision(
        &state.db,
        id,
        body.approved,
        body.approved_amount,
        body.monthly_premium,
        body.note.as_deref(),
    )
    .await?;
    Ok(Json(build_response(
        json!({ "approved": body.approved, "commission": commission }),
    )))
}

#[utoipa::path(
    post, path = "/api/admin/finance/advances/{id}/disburse", tag = "admin",
    params(("id" = Uuid, Path, description = "Advance id")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Not a new request", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn disburse(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let net = finance_line::disburse(&state.db, id).await?;
    Ok(Json(build_response(json!({ "paid_out": net }))))
}

#[utoipa::path(
    post, path = "/api/admin/finance/advances/{id}/repaid", tag = "admin",
    params(("id" = Uuid, Path, description = "Advance id")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "No outstanding advance with that id", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn mark_repaid(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    finance_line::mark_repaid(&state.db, id).await?;
    Ok(Json(build_response(json!({ "repaid": true }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct WriteOffBody {
    pub reason: String,
}

/// The client never paid. Skilluv carries it; the contributor keeps the money.
#[utoipa::path(
    post, path = "/api/admin/finance/advances/{id}/write-off", tag = "admin",
    params(("id" = Uuid, Path, description = "Advance id")),
    request_body = WriteOffBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "No reason given", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn write_off(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<WriteOffBody>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    finance_line::write_off(&state.db, id, &body.reason).await?;
    Ok(Json(build_response(json!({ "written_off": true }))))
}

#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = FinanceLineClaimBody)]
pub struct ClaimBody {
    pub user_id: Uuid,
    #[serde(default)]
    pub invoice_id: Option<Uuid>,
    #[schema(value_type = String)]
    pub amount: BigDecimal,
    pub reason: String,
}

/// Pay a contributor for work a client refused to pay for.
#[utoipa::path(
    post, path = "/api/admin/finance/guarantee-claims", tag = "admin",
    request_body = ClaimBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "No live guarantee, or the year's cover is used up", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn honour_guarantee(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<ClaimBody>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let paid = finance_line::honour_guarantee(
        &state.db,
        body.user_id,
        body.invoice_id,
        body.amount,
        &body.reason,
    )
    .await?;
    Ok(Json(build_response(json!({ "paid": paid }))))
}
