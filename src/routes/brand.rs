//! The brand line — sponsors, campaigns, ambassadors, and the audience.
//!
//! Four products with one thing in common: a company pays for access to a
//! community that did not sign up to be sold to. Every route here has a
//! matching restraint, and they are the reason the module holds together:
//!
//!   * a sponsor sees the leads who consented, and a count of the rest;
//!   * a campaign piece reaches the sponsor only after Skilluv has judged it
//!     real work, so criticism cannot be rejected as "quality";
//!   * an ambassadorship is accepted by the person lending their name, and
//!     by nobody else;
//!   * the one thing an individual can buy is a replay, because talents do
//!     not pay to be seen.

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
use crate::services::{ambassadors, launch_campaigns, sponsorship};

pub fn brand_routes() -> Router<AppState> {
    Router::new()
        // Public.
        .route("/sponsorship/packages", get(list_packages))
        .route("/audience/plans", get(list_audience_plans))
        // Client.
        .route(
            "/enterprise/sponsorships",
            get(my_sponsorships).post(propose_sponsorship),
        )
        .route("/enterprise/sponsorships/{id}/leads", get(read_leads))
        .route(
            "/enterprise/sponsorships/{id}/leads/export",
            post(export_leads),
        )
        .route(
            "/enterprise/annual-sponsorships",
            post(open_annual_contract),
        )
        .route(
            "/enterprise/launch-campaigns",
            get(my_campaigns).post(open_campaign),
        )
        .route(
            "/enterprise/launch-campaigns/{id}/pieces",
            get(read_pieces_as_sponsor),
        )
        .route("/enterprise/launch-pieces/{id}/decide", post(decide_piece))
        .route(
            "/enterprise/ambassador-programs",
            get(my_ambassador_programs).post(open_ambassador_program),
        )
        .route(
            "/enterprise/ambassador-programs/{id}/ambassadors",
            get(read_ambassadors),
        )
        // Talent.
        .route("/launch-campaigns/open", get(open_campaigns))
        .route("/launch-campaigns/{id}/pieces", post(submit_piece))
        .route("/ambassador-programs/open", get(open_ambassador_programs))
        .route("/ambassador-programs/{id}/respond", post(respond_to_invite))
        .route(
            "/ambassador-programs/{id}/deliverables",
            post(record_deliverable),
        )
        .route("/events/{id}/stands/{sponsorship_id}", post(visit_stand))
        // Audience.
        .route("/audience/subscribe", post(subscribe))
        .route("/audience/cancel", post(cancel_subscription))
        .route("/users/me/audience", get(my_audience_access))
}

pub fn admin_brand_routes() -> Router<AppState> {
    Router::new()
        .route("/admin/sponsorships/{id}/sign", post(sign_sponsorship))
        .route("/admin/sponsorships/{id}/honour", post(honour_sponsorship))
        .route("/admin/sponsorships/{id}/cancel", post(cancel_sponsorship))
        .route(
            "/admin/sponsored-content",
            post(commission_content).get(list_sponsored_content),
        )
        .route(
            "/admin/sponsored-content/{id}/publish",
            post(publish_content),
        )
        .route("/admin/launch-campaigns/{id}/open", post(open_submissions))
        .route("/admin/launch-pieces/{id}/quality", post(review_quality))
        .route("/admin/launch-campaigns/{id}/close", post(close_campaign))
        .route(
            "/admin/ambassador-programs/{id}/invite",
            post(invite_ambassador),
        )
        .route(
            "/admin/ambassador-programs/{id}/activate",
            post(activate_program),
        )
        .route("/admin/ambassador-programs/{id}/pay", post(pay_stipend))
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
// Sponsorship
// ═══════════════════════════════════════════════════════════════════

/// The published sponsorship grid.
#[utoipa::path(
    get, path = "/api/sponsorship/packages", tag = "enterprise",
    responses((status = 200, body = serde_json::Value)),
)]
pub async fn list_packages(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let packages = sponsorship::packages(&state.db).await?;
    Ok(Json(build_response(json!({ "packages": packages }))))
}

#[utoipa::path(
    post, path = "/api/enterprise/sponsorships", tag = "enterprise",
    request_body(content = serde_json::Value, description = "SponsorshipInput"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Unknown tier, a finished event, or a benefit the tier does not include", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not acting for an enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn propose_sponsorship(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<sponsorship::SponsorshipInput>,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
    let proposal = sponsorship::propose(&state.db, enterprise.id, auth.user_id, input).await?;

    metrics::counter!(
        "skilluv_sponsorships_total",
        "tier" => proposal.package_tier.clone()
    )
    .increment(1);
    Ok(Json(build_response(json!({ "sponsorship": proposal }))))
}

#[utoipa::path(
    get, path = "/api/enterprise/sponsorships", tag = "enterprise",
    responses(
        (status = 200, body = serde_json::Value),
        (status = 403, description = "Not acting for an enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn my_sponsorships(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
    let list = sponsorship::for_enterprise(&state.db, enterprise.id).await?;
    Ok(Json(build_response(json!({ "sponsorships": list }))))
}

/// The sponsorship, if the caller is the sponsor.
async fn sponsorship_owned_by_caller(
    state: &AppState,
    auth: &AuthUser,
    id: Uuid,
) -> Result<sponsorship::Sponsorship, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(state, auth).await?;
    let row = sponsorship::by_id(&state.db, id).await?;
    if row.enterprise_id != enterprise.id {
        return Err(AppError::NotFound("sponsorship not found".into()));
    }
    Ok(row)
}

/// The leads from a stand: the ones who consented, and how many did not.
#[utoipa::path(
    get, path = "/api/enterprise/sponsorships/{id}/leads", tag = "enterprise",
    params(("id" = Uuid, Path, description = "Sponsorship id")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "Not your sponsorship", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn read_leads(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    sponsorship_owned_by_caller(&state, &auth, id).await?;
    let (leads, anonymous) = sponsorship::leads_for_sponsor(&state.db, id).await?;

    // The count without the names: the sponsor learns the stand worked
    // without learning who was at it.
    Ok(Json(build_response(json!({
        "leads": leads,
        "visitors_without_consent": anonymous,
    }))))
}

#[utoipa::path(
    post, path = "/api/enterprise/sponsorships/{id}/leads/export", tag = "enterprise",
    params(("id" = Uuid, Path, description = "Sponsorship id")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "Not your sponsorship", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn export_leads(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    sponsorship_owned_by_caller(&state, &auth, id).await?;
    let exported = sponsorship::mark_exported(&state.db, id).await?;
    let (leads, _) = sponsorship::leads_for_sponsor(&state.db, id).await?;
    Ok(Json(build_response(
        json!({ "exported": exported, "leads": leads }),
    )))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct StandVisitBody {
    pub interaction: String,
    #[serde(default)]
    pub note: Option<String>,
    /// Whether the sponsor may have your details. False records the visit
    /// and nothing else.
    pub contact_consent: bool,
}

/// A participant walks up to a stand.
#[utoipa::path(
    post, path = "/api/events/{id}/stands/{sponsorship_id}", tag = "profile",
    params(
        ("id" = Uuid, Path, description = "Event id"),
        ("sponsorship_id" = Uuid, Path, description = "Sponsorship id"),
    ),
    request_body = StandVisitBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Not an interaction we record", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn visit_stand(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((_event_id, sponsorship_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<StandVisitBody>,
) -> Result<Json<Value>, AppError> {
    sponsorship::record_lead(
        &state.db,
        sponsorship_id,
        auth.user_id,
        &body.interaction,
        body.note.as_deref(),
        body.contact_consent,
    )
    .await?;
    Ok(Json(build_response(
        json!({ "recorded": true, "shared_with_sponsor": body.contact_consent }),
    )))
}

#[utoipa::path(
    post, path = "/api/enterprise/annual-sponsorships", tag = "enterprise",
    request_body(content = serde_json::Value, description = "AnnualContractInput"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "A discount out of band, or unsigned", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not acting for an enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn open_annual_contract(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<sponsorship::AnnualContractInput>,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
    let contract = sponsorship::open_annual_contract(&state.db, enterprise.id, input).await?;

    let _ = sqlx::query(
        "INSERT INTO enterprise_products
            (enterprise_id, product_type, source_table, source_id,
             contract_value, currency, created_by)
         VALUES ($1, 'annual_sponsorship', 'annual_sponsorship_contracts', $2, $3, $4, $5)",
    )
    .bind(enterprise.id)
    .bind(contract.id)
    .bind(&contract.total_fee)
    .bind(contract.currency.as_str())
    .bind(auth.user_id)
    .execute(&state.db)
    .await;

    Ok(Json(build_response(json!({ "contract": contract }))))
}

#[utoipa::path(
    post, path = "/api/admin/sponsorships/{id}/sign", tag = "admin",
    params(("id" = Uuid, Path, description = "Sponsorship id")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Not a proposed sponsorship", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn sign_sponsorship(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let signed = sponsorship::sign(&state.db, id).await?;
    Ok(Json(build_response(json!({ "sponsorship": signed }))))
}

#[utoipa::path(
    post, path = "/api/admin/sponsorships/{id}/honour", tag = "admin",
    params(("id" = Uuid, Path, description = "Sponsorship id")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Not a signed sponsorship", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn honour_sponsorship(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let booked = sponsorship::honour(&state.db, id).await?;
    Ok(Json(build_response(json!({ "revenue_booked": booked }))))
}

#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = BrandReasonBody)]
pub struct ReasonBody {
    pub reason: String,
}

#[utoipa::path(
    post, path = "/api/admin/sponsorships/{id}/cancel", tag = "admin",
    params(("id" = Uuid, Path, description = "Sponsorship id")),
    request_body = ReasonBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "No reason given", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn cancel_sponsorship(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ReasonBody>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    sponsorship::cancel(&state.db, id, &body.reason).await?;
    Ok(Json(build_response(json!({ "cancelled": true }))))
}

#[utoipa::path(
    post, path = "/api/admin/sponsored-content", tag = "admin",
    request_body(content = serde_json::Value, description = "ContentInput"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Not a content type we run", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn commission_content(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<sponsorship::ContentInput>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let id = sponsorship::commission_content(&state.db, input).await?;
    Ok(Json(build_response(json!({ "content_id": id }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PublishBody {
    pub url: String,
}

#[utoipa::path(
    post, path = "/api/admin/sponsored-content/{id}/publish", tag = "admin",
    params(("id" = Uuid, Path, description = "Content id")),
    request_body = PublishBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "A non-https URL, or already published", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn publish_content(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<PublishBody>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let booked = sponsorship::publish_content(&state.db, id, &body.url).await?;
    Ok(Json(build_response(json!({ "revenue_booked": booked }))))
}

// ═══════════════════════════════════════════════════════════════════
// Launch campaigns
// ═══════════════════════════════════════════════════════════════════

#[utoipa::path(
    post, path = "/api/enterprise/launch-campaigns", tag = "enterprise",
    request_body(content = serde_json::Value, description = "CampaignInput"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "An empty brief, an unknown content type, or a pot too small to pay one piece", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not acting for an enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
    operation_id = "brandOpenCampaign",
)]
pub async fn open_campaign(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<launch_campaigns::CampaignInput>,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
    let campaign = launch_campaigns::open(&state.db, enterprise.id, auth.user_id, input).await?;

    let total = &campaign.reward_pool + &campaign.campaign_fee;
    let _ = sqlx::query(
        "INSERT INTO enterprise_products
            (enterprise_id, product_type, source_table, source_id,
             contract_value, currency, created_by)
         VALUES ($1, 'product_launch_campaign', 'product_launch_campaigns', $2, $3, $4, $5)",
    )
    .bind(enterprise.id)
    .bind(campaign.id)
    .bind(&total)
    .bind(campaign.currency.as_str())
    .bind(auth.user_id)
    .execute(&state.db)
    .await;

    Ok(Json(build_response(
        json!({ "campaign": campaign, "committed_maximum": total }),
    )))
}

#[utoipa::path(
    get, path = "/api/enterprise/launch-campaigns", tag = "enterprise",
    responses(
        (status = 200, body = serde_json::Value),
        (status = 403, description = "Not acting for an enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
    operation_id = "brandMyCampaigns",
)]
pub async fn my_campaigns(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
    let list = launch_campaigns::for_enterprise(&state.db, enterprise.id).await?;
    Ok(Json(build_response(json!({ "campaigns": list }))))
}

/// What a contributor can write for.
#[utoipa::path(
    get, path = "/api/launch-campaigns/open", tag = "work",
    responses((status = 200, body = serde_json::Value)),
)]
pub async fn open_campaigns(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let list = launch_campaigns::open_campaigns(&state.db).await?;
    Ok(Json(build_response(json!({ "campaigns": list }))))
}

#[utoipa::path(
    post, path = "/api/launch-campaigns/{id}/pieces", tag = "work",
    params(("id" = Uuid, Path, description = "Campaign id")),
    request_body(content = serde_json::Value, description = "PieceInput"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "The campaign is closed, the type is not wanted, or the pot is spent", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn submit_piece(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<launch_campaigns::PieceInput>,
) -> Result<Json<Value>, AppError> {
    let piece_id = launch_campaigns::submit(&state.db, id, auth.user_id, input).await?;
    let (left, affordable) = launch_campaigns::budget_left(&state.db, id).await?;
    Ok(Json(build_response(json!({
        "piece_id": piece_id,
        "pot_remaining": left,
        "pieces_still_payable": affordable,
    }))))
}

/// The campaign, if the caller is the company that opened it.
async fn campaign_owned_by_caller(
    state: &AppState,
    auth: &AuthUser,
    id: Uuid,
) -> Result<launch_campaigns::Campaign, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(state, auth).await?;
    let campaign = launch_campaigns::by_id(&state.db, id).await?;
    if campaign.enterprise_id != enterprise.id {
        return Err(AppError::NotFound("campaign not found".into()));
    }
    Ok(campaign)
}

/// What the sponsor sees: only what has passed Skilluv's check.
#[utoipa::path(
    get, path = "/api/enterprise/launch-campaigns/{id}/pieces", tag = "enterprise",
    params(("id" = Uuid, Path, description = "Campaign id")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "Not your campaign", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn read_pieces_as_sponsor(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    campaign_owned_by_caller(&state, &auth, id).await?;
    let pieces = launch_campaigns::pieces_for_sponsor(&state.db, id).await?;
    let (left, affordable) = launch_campaigns::budget_left(&state.db, id).await?;
    Ok(Json(build_response(json!({
        "pieces": pieces,
        "pot_remaining": left,
        "pieces_still_payable": affordable,
    }))))
}

#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = BrandDecisionBody)]
pub struct DecisionBody {
    pub accept: bool,
    #[serde(default)]
    pub reason: Option<String>,
}

#[utoipa::path(
    post, path = "/api/enterprise/launch-pieces/{id}/decide", tag = "enterprise",
    params(("id" = Uuid, Path, description = "Piece id")),
    request_body = DecisionBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Not yet through Skilluv's check, a refusal with no reason, or a spent pot", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn decide_piece(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<DecisionBody>,
) -> Result<Json<Value>, AppError> {
    // The piece belongs to a campaign; the campaign belongs to a company.
    let campaign_id: Option<Uuid> =
        sqlx::query_scalar("SELECT campaign_id FROM launch_campaign_pieces WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await?;
    let campaign_id = campaign_id.ok_or_else(|| AppError::NotFound("piece not found".into()))?;
    campaign_owned_by_caller(&state, &auth, campaign_id).await?;

    let paid = launch_campaigns::decide(&state.db, id, body.accept, body.reason.as_deref()).await?;
    Ok(Json(build_response(
        json!({ "accepted": body.accept, "reward_paid": paid }),
    )))
}

#[utoipa::path(
    post, path = "/api/admin/launch-campaigns/{id}/open", tag = "admin",
    params(("id" = Uuid, Path, description = "Campaign id")),
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn open_submissions(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let campaign = launch_campaigns::open_for_submissions(&state.db, id).await?;
    Ok(Json(build_response(json!({ "campaign": campaign }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct QualityBody {
    pub passed: bool,
    pub notes: String,
}

/// Skilluv's gate, before the sponsor sees anything.
#[utoipa::path(
    post, path = "/api/admin/launch-pieces/{id}/quality", tag = "admin",
    params(("id" = Uuid, Path, description = "Piece id")),
    request_body = QualityBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "A verdict with no notes", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn review_quality(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<QualityBody>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    launch_campaigns::review_quality(&state.db, id, auth.user_id, body.passed, &body.notes).await?;
    Ok(Json(build_response(json!({ "passed": body.passed }))))
}

#[utoipa::path(
    post, path = "/api/admin/launch-campaigns/{id}/close", tag = "admin",
    params(("id" = Uuid, Path, description = "Campaign id")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Pieces still have no verdict", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn close_campaign(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let fee = launch_campaigns::close(&state.db, id).await?;
    Ok(Json(build_response(json!({ "campaign_fee_booked": fee }))))
}

// ═══════════════════════════════════════════════════════════════════
// Ambassadors
// ═══════════════════════════════════════════════════════════════════

#[utoipa::path(
    post, path = "/api/enterprise/ambassador-programs", tag = "enterprise",
    request_body(content = serde_json::Value, description = "ProgramInput"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "An empty brief, an unpaid programme, or a rank floor too low", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not acting for an enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn open_ambassador_program(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<ambassadors::ProgramInput>,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
    let program = ambassadors::open(&state.db, enterprise.id, auth.user_id, input).await?;
    Ok(Json(build_response(json!({ "program": program }))))
}

#[utoipa::path(
    get, path = "/api/enterprise/ambassador-programs", tag = "enterprise",
    responses(
        (status = 200, body = serde_json::Value),
        (status = 403, description = "Not acting for an enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn my_ambassador_programs(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
    let list = ambassadors::for_enterprise(&state.db, enterprise.id).await?;
    Ok(Json(build_response(json!({ "programs": list }))))
}

#[utoipa::path(
    get, path = "/api/enterprise/ambassador-programs/{id}/ambassadors", tag = "enterprise",
    params(("id" = Uuid, Path, description = "Programme id")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "Not your programme", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn read_ambassadors(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
    let program = ambassadors::by_id(&state.db, id).await?;
    if program.enterprise_id != enterprise.id {
        return Err(AppError::NotFound("programme not found".into()));
    }
    let people = ambassadors::ambassadors(&state.db, id).await?;
    Ok(Json(build_response(json!({ "ambassadors": people }))))
}

/// Programmes looking for ambassadors.
#[utoipa::path(
    get, path = "/api/ambassador-programs/open", tag = "work",
    responses((status = 200, body = serde_json::Value)),
)]
pub async fn open_ambassador_programs(
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    let list = ambassadors::recruiting(&state.db).await?;
    Ok(Json(build_response(json!({ "programs": list }))))
}

#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = BrandRespondBody)]
pub struct RespondBody {
    pub accept: bool,
}

/// The ambassador's own answer, and nobody else's.
#[utoipa::path(
    post, path = "/api/ambassador-programs/{id}/respond", tag = "work",
    params(("id" = Uuid, Path, description = "Programme id")),
    request_body = RespondBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "No open invitation", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn respond_to_invite(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<RespondBody>,
) -> Result<Json<Value>, AppError> {
    ambassadors::respond(&state.db, id, auth.user_id, body.accept).await?;
    Ok(Json(build_response(json!({ "accepted": body.accept }))))
}

#[utoipa::path(
    post, path = "/api/ambassador-programs/{id}/deliverables", tag = "work",
    params(("id" = Uuid, Path, description = "Programme id")),
    request_body(content = serde_json::Value, description = "DeliverableInput"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Not an ambassador on this programme, or a non-https link", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn record_deliverable(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<ambassadors::DeliverableInput>,
) -> Result<Json<Value>, AppError> {
    let deliverable_id =
        ambassadors::record_deliverable(&state.db, id, auth.user_id, input).await?;
    Ok(Json(build_response(
        json!({ "deliverable_id": deliverable_id }),
    )))
}

#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = BrandInviteBody)]
pub struct InviteBody {
    pub user_id: Uuid,
}

#[utoipa::path(
    post, path = "/api/admin/ambassador-programs/{id}/invite", tag = "admin",
    params(("id" = Uuid, Path, description = "Programme id")),
    request_body = InviteBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Below the rank floor, or the cohort is full", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn invite_ambassador(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<InviteBody>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    ambassadors::invite(&state.db, id, body.user_id).await?;
    Ok(Json(build_response(json!({ "invited": true }))))
}

#[utoipa::path(
    post, path = "/api/admin/ambassador-programs/{id}/activate", tag = "admin",
    params(("id" = Uuid, Path, description = "Programme id")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Nobody has accepted yet", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn activate_program(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let fee = ambassadors::activate(&state.db, id).await?;
    Ok(Json(build_response(
        json!({ "activation_fee_booked": fee }),
    )))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct StipendBody {
    pub user_id: Uuid,
    /// Any day in the month being paid; the first of it is what is recorded.
    pub month: chrono::NaiveDate,
}

/// Pay one ambassador for one month.
#[utoipa::path(
    post, path = "/api/admin/ambassador-programs/{id}/pay", tag = "admin",
    params(("id" = Uuid, Path, description = "Programme id")),
    request_body = StipendBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Nothing delivered that month, or it is already paid", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn pay_stipend(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<StipendBody>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;

    use chrono::Datelike;
    let month = chrono::NaiveDate::from_ymd_opt(body.month.year(), body.month.month(), 1)
        .unwrap_or(body.month);

    let paid = ambassadors::pay_month(&state.db, id, body.user_id, month).await?;
    Ok(Json(build_response(
        json!({ "paid": paid, "month": month }),
    )))
}

// ═══════════════════════════════════════════════════════════════════
// The audience
// ═══════════════════════════════════════════════════════════════════

/// What an individual can pay Skilluv for. Deliberately short.
#[utoipa::path(
    get, path = "/api/audience/plans", tag = "profile",
    responses((status = 200, body = serde_json::Value)),
)]
pub async fn list_audience_plans(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let plans = sponsorship::audience_plans(&state.db).await?;
    Ok(Json(build_response(json!({ "plans": plans }))))
}

#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = BrandSubscribeBody)]
pub struct SubscribeBody {
    pub plan: String,
    #[serde(default)]
    pub payment_reference: Option<String>,
}

#[utoipa::path(
    post, path = "/api/audience/subscribe", tag = "profile",
    request_body = SubscribeBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "No such plan", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
    operation_id = "brandSubscribe",
)]
pub async fn subscribe(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<SubscribeBody>,
) -> Result<Json<Value>, AppError> {
    let expires = sponsorship::subscribe(
        &state.db,
        auth.user_id,
        &body.plan,
        body.payment_reference.as_deref(),
    )
    .await?;
    Ok(Json(build_response(json!({ "expires_at": expires }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CancelBody {
    pub plan: String,
}

#[utoipa::path(
    post, path = "/api/audience/cancel", tag = "profile",
    request_body = CancelBody,
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
    operation_id = "brandCancelSubscription",
)]
pub async fn cancel_subscription(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CancelBody>,
) -> Result<Json<Value>, AppError> {
    sponsorship::cancel_subscription(&state.db, auth.user_id, &body.plan).await?;
    // Cancelling stops the renewal; what was paid for is kept to its end.
    Ok(Json(build_response(
        json!({ "auto_renew": false, "access_until_expiry": true }),
    )))
}

#[utoipa::path(
    get, path = "/api/users/me/audience", tag = "profile",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn my_audience_access(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let premium = sponsorship::has_premium_access(&state.db, auth.user_id).await?;
    Ok(Json(build_response(json!({ "premium": premium }))))
}

/// Kept as a named type so the OpenAPI document has something to point at
/// for the money fields the brand line returns.
#[derive(Debug, serde::Serialize, ToSchema)]
pub struct Money {
    #[schema(value_type = String)]
    pub amount: BigDecimal,
    pub currency: String,
}

// ═══════════════════════════════════════════════════════════════════
// Commissioned content — the list that was nowhere
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SponsoredContentQuery {
    pub status: Option<String>,
}

type SponsoredRow = (
    Uuid,
    Option<Uuid>,
    Option<String>,
    String,
    String,
    Option<String>,
    Option<BigDecimal>,
    Option<String>,
    String,
    Option<chrono::DateTime<chrono::Utc>>,
    chrono::DateTime<chrono::Utc>,
);

/// Every commissioned piece, drafts included.
///
/// The only one of the twelve product lines listed **nowhere** — not under
/// admin, not under enterprise, not publicly. `POST /admin/sponsored-content`
/// returns the id, so publishing straight after creating works; coming back
/// the next day did not.
///
/// Unpublished first, because publishing is the action this list exists to
/// enable and a published piece is finished business.
#[utoipa::path(
    get, path = "/api/admin/sponsored-content", tag = "admin",
    params(SponsoredContentQuery),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 403, description = "Not an administrator", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
    operation_id = "adminSponsoredContentList",
)]
pub async fn list_sponsored_content(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<SponsoredContentQuery>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let rows: Vec<Value> = sqlx::query_as::<_, SponsoredRow>(
        "SELECT c.id, c.sponsor_enterprise_id, e.company_name, c.content_type,
                c.title, c.content_url, c.fee, c.currency, c.status,
                c.published_at, c.created_at
           FROM event_sponsored_content c
           LEFT JOIN enterprises e ON e.id = c.sponsor_enterprise_id
          WHERE ($1::VARCHAR IS NULL OR c.status = $1)
          ORDER BY (c.published_at IS NULL) DESC, c.created_at DESC",
    )
    .bind(q.status.as_deref())
    .fetch_all(&state.db)
    .await?
    .into_iter()
    .map(
        |(
            id,
            sponsor_enterprise_id,
            company_name,
            content_type,
            title,
            content_url,
            fee,
            currency,
            status,
            published_at,
            created_at,
        )| {
            json!({
                "id": id,
                "sponsor_enterprise_id": sponsor_enterprise_id,
                "company_name": company_name,
                "content_type": content_type,
                "title": title,
                "content_url": content_url,
                "fee": fee,
                "currency": currency,
                "status": status,
                "published_at": published_at,
                "created_at": created_at,
            })
        },
    )
    .collect();
    Ok(Json(build_response(json!({ "content": rows }))))
}
