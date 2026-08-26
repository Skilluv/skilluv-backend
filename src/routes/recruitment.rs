//! Recruitment campaigns — briefing, sourcing, shortlist, hire.
//!
//! Three audiences, and the split between them is the point:
//!
//!   * the **client** briefs, reads their own shortlist, and confirms a hire;
//!   * **Skilluv** assigns a recruiter, sources, and delivers;
//!   * the **talent** answers for themselves, and nobody answers for them.
//!
//! That last one is why the response endpoint exists at all. An admin could
//! technically flip the status, and a trigger stops it: presenting somebody
//! who has not agreed is how a platform burns the trust it runs on.

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
use crate::services::recruitment;

pub fn recruitment_routes() -> Router<AppState> {
    Router::new()
        // Client.
        .route(
            "/enterprise/recruitment/campaigns",
            get(my_campaigns).post(open_campaign),
        )
        .route(
            "/enterprise/recruitment/campaigns/{id}/shortlist",
            get(read_shortlist),
        )
        .route(
            "/enterprise/recruitment/campaigns/{id}/hired",
            post(confirm_hire),
        )
        // Talent — their own answer, through their own session.
        .route("/recruitment/campaigns/{id}/respond", post(talent_response))
        .route("/users/me/recruitment-invitations", get(my_invitations))
}

/// Admin surface, mounted behind the admin gate.
pub fn admin_recruitment_routes() -> Router<AppState> {
    Router::new()
        .route("/admin/recruitment/campaigns", get(all_campaigns))
        .route("/admin/recruitment/campaigns/{id}/assign", post(assign))
        .route(
            "/admin/recruitment/campaigns/{id}/shortlist",
            post(add_to_shortlist),
        )
        .route(
            "/admin/recruitment/fees/{id}/departure",
            post(record_departure),
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

/// Brief a campaign.
#[utoipa::path(
    post, path = "/api/enterprise/recruitment/campaigns", tag = "enterprise",
    request_body(content = serde_json::Value, description = "BriefInput"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Empty brief, unknown trade, or a pricing contradiction", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not acting for an enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
    operation_id = "recruitmentOpenCampaign",
)]
pub async fn open_campaign(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<recruitment::BriefInput>,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
    let campaign =
        recruitment::open_campaign(&state.db, enterprise.id, auth.user_id, input).await?;

    // The engagement register, so this shows up next to everything else the
    // company has with us rather than only in its own table.
    // Every kind of campaign registers as the same product: what the client
    // bought is managed recruitment, whatever shape it took.
    let product = "raas_campaign";
    let _ = sqlx::query(
        "INSERT INTO enterprise_products
            (enterprise_id, product_type, source_table, source_id,
             contract_value, currency, created_by)
         VALUES ($1, $2, 'recruitment_campaigns', $3, $4, $5, $6)",
    )
    .bind(enterprise.id)
    .bind(product)
    .bind(campaign.id)
    .bind(campaign.setup_fee.as_ref())
    .bind(
        campaign
            .setup_fee
            .as_ref()
            .map(|_| campaign.currency.as_str()),
    )
    .bind(auth.user_id)
    .execute(&state.db)
    .await;

    metrics::counter!("skilluv_recruitment_campaigns_total", "kind" => campaign.kind.clone())
        .increment(1);
    Ok(Json(build_response(json!({ "campaign": campaign }))))
}

/// The client's own campaigns.
#[utoipa::path(
    get, path = "/api/enterprise/recruitment/campaigns", tag = "enterprise",
    responses(
        (status = 200, body = serde_json::Value),
        (status = 403, description = "Not acting for an enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
    operation_id = "recruitmentMyCampaigns",
)]
pub async fn my_campaigns(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
    let campaigns = recruitment::for_enterprise(&state.db, enterprise.id).await?;
    Ok(Json(build_response(json!({ "campaigns": campaigns }))))
}

/// The shortlist, for the client who paid for it.
///
/// Only people who have agreed to be put forward. A client reading names of
/// people who have not answered would be reading a search result, and the
/// people in it never chose to be there.
#[utoipa::path(
    get, path = "/api/enterprise/recruitment/campaigns/{id}/shortlist", tag = "enterprise",
    params(("id" = Uuid, Path, description = "Campaign id")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 403, description = "Not this campaign's client", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn read_shortlist(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
    let campaign = recruitment::by_id(&state.db, id).await?;
    if campaign.enterprise_id != enterprise.id {
        return Err(AppError::Forbidden);
    }

    let entries = recruitment::shortlist_of(&state.db, id).await?;
    let visible: Vec<_> = entries
        .into_iter()
        .filter(|e| e.status != "proposed" && e.status != "declined")
        .collect();

    Ok(Json(build_response(json!({
        "shortlist": visible,
        "campaign_status": campaign.status,
    }))))
}

/// One campaign somebody has been put forward for.
#[derive(sqlx::FromRow)]
struct Invitation {
    campaign_id: Uuid,
    title: String,
    target_role: String,
    company_name: String,
    brief_md: String,
    my_status: String,
    salary_range: Option<serde_json::Value>,
}

/// One line of the recruiter queue.
#[derive(sqlx::FromRow)]
struct CampaignQueueRow {
    id: Uuid,
    title: String,
    kind: String,
    status: String,
    company_name: String,
    assigned_to: Option<Uuid>,
    shortlisted: i64,
}

#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = RecruitmentHireBody)]
pub struct HireBody {
    pub talent_user_id: Uuid,
    /// As agreed with the person hired. Declared, not verified.
    #[schema(value_type = String)]
    pub annual_salary: BigDecimal,
    #[schema(max_length = 3)]
    pub currency: Option<String>,
    /// How long the placement is guaranteed. Defaults to six months.
    pub guarantee_days: Option<i64>,
}

/// Confirm a hire, which is what makes the fee due.
#[utoipa::path(
    post, path = "/api/enterprise/recruitment/campaigns/{id}/hired", tag = "enterprise",
    params(("id" = Uuid, Path, description = "Campaign id")),
    request_body = HireBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "No success fee on this campaign, or no salary", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not this campaign's client", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn confirm_hire(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<HireBody>,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
    let campaign = recruitment::by_id(&state.db, id).await?;
    if campaign.enterprise_id != enterprise.id {
        return Err(AppError::Forbidden);
    }

    let fee_id = recruitment::record_hire(
        &state.db,
        id,
        body.talent_user_id,
        body.annual_salary,
        body.currency.as_deref().unwrap_or(&campaign.currency),
        body.guarantee_days
            .unwrap_or(recruitment::DEFAULT_GUARANTEE_DAYS),
    )
    .await?;

    Ok(Json(build_response(json!({ "success_fee_id": fee_id }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ResponseBody {
    pub interested: bool,
}

/// The talent's own answer.
///
/// Their session, their decision. There is deliberately no admin equivalent.
#[utoipa::path(
    post, path = "/api/recruitment/campaigns/{id}/respond", tag = "profile",
    params(("id" = Uuid, Path, description = "Campaign id")),
    request_body = ResponseBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "Not on this shortlist", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn talent_response(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ResponseBody>,
) -> Result<Json<Value>, AppError> {
    recruitment::talent_responds(&state.db, id, auth.user_id, body.interested).await?;
    Ok(Json(build_response(json!({
        "recorded": true,
        "interested": body.interested,
    }))))
}

/// The campaigns somebody has been put forward for.
///
/// What the brief says and who the client is, so the answer is informed. The
/// alternative — asking somebody to agree to "an opportunity" — is how people
/// stop answering.
#[utoipa::path(
    get, path = "/api/users/me/recruitment-invitations", tag = "profile",
    responses(
        (status = 200, body = serde_json::Value),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
    operation_id = "recruitmentMyInvitations",
)]
pub async fn my_invitations(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let rows = sqlx::query_as::<_, Invitation>(
        "SELECT c.id AS campaign_id, c.title, c.target_role,
                e.company_name, c.brief_md, s.status AS my_status, c.salary_range
           FROM recruitment_shortlist s
           JOIN recruitment_campaigns c ON c.id = s.campaign_id
           JOIN enterprises e ON e.id = c.enterprise_id
          WHERE s.talent_user_id = $1
            AND c.status NOT IN ('closed', 'cancelled')
          ORDER BY s.created_at DESC",
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await?;

    let invitations: Vec<Value> = rows
        .into_iter()
        .map(|r| {
            json!({
                "campaign_id": r.campaign_id,
                "title": r.title,
                "target_role": r.target_role,
                "company_name": r.company_name,
                "brief_md": r.brief_md,
                "salary_range": r.salary_range,
                "my_status": r.my_status,
            })
        })
        .collect();

    Ok(Json(build_response(json!({ "invitations": invitations }))))
}

// ─── Admin ───────────────────────────────────────────────────────

/// Admin: every campaign, whoever it belongs to.
#[utoipa::path(
    get, path = "/api/admin/recruitment/campaigns", tag = "admin",
    responses(
        (status = 200, body = serde_json::Value),
        (status = 403, description = "Not an administrator", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn all_campaigns(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;

    let rows = sqlx::query_as::<_, CampaignQueueRow>(
        "SELECT c.id, c.title, c.kind, c.status, e.company_name, c.assigned_to,
                (SELECT count(*) FROM recruitment_shortlist s WHERE s.campaign_id = c.id)
                    AS shortlisted
           FROM recruitment_campaigns c
           JOIN enterprises e ON e.id = c.enterprise_id
          WHERE c.status NOT IN ('closed', 'cancelled')
          ORDER BY c.assigned_to NULLS FIRST, c.created_at ASC",
    )
    .fetch_all(&state.db)
    .await?;

    let campaigns: Vec<Value> = rows
        .into_iter()
        .map(|r| {
            json!({
                "id": r.id,
                "title": r.title,
                "kind": r.kind,
                "status": r.status,
                "company_name": r.company_name,
                "assigned_to": r.assigned_to,
                "shortlisted": r.shortlisted,
                // A campaign with nobody on it is a campaign nobody is doing,
                // which is why they sort first.
                "unassigned": r.assigned_to.is_none(),
            })
        })
        .collect();

    Ok(Json(build_response(json!({ "campaigns": campaigns }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AssignBody {
    pub recruiter_user_id: Uuid,
}

/// Admin: put a recruiter on a campaign.
#[utoipa::path(
    post, path = "/api/admin/recruitment/campaigns/{id}/assign", tag = "admin",
    params(("id" = Uuid, Path, description = "Campaign id")),
    request_body = AssignBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 403, description = "Not an administrator", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn assign(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<AssignBody>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;

    let done = sqlx::query(
        "UPDATE recruitment_campaigns
            SET assigned_to = $2,
                status = CASE WHEN status = 'briefing' THEN 'sourcing' ELSE status END
          WHERE id = $1",
    )
    .bind(id)
    .bind(body.recruiter_user_id)
    .execute(&state.db)
    .await?;

    if done.rows_affected() == 0 {
        return Err(AppError::NotFound("campaign not found".into()));
    }
    Ok(Json(build_response(json!({ "assigned": true }))))
}

#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = RecruitmentShortlistBody)]
pub struct ShortlistBody {
    pub talent_user_id: Uuid,
    /// Why this person, with links. Required.
    #[schema(max_length = 8000)]
    pub match_reason_md: String,
}

/// Admin: put somebody forward, and ask them.
#[utoipa::path(
    post, path = "/api/admin/recruitment/campaigns/{id}/shortlist", tag = "admin",
    params(("id" = Uuid, Path, description = "Campaign id")),
    request_body = ShortlistBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "No argument given for the match", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not an administrator", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn add_to_shortlist(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ShortlistBody>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;

    recruitment::add_to_shortlist(&state.db, id, body.talent_user_id, &body.match_reason_md)
        .await?;

    let campaign = recruitment::by_id(&state.db, id).await?;
    // Asked, not assumed. The notification is the consent request, so it is
    // sent before anybody is shown to a client.
    let _ = crate::services::notify::send(
        &state,
        crate::services::notify::Recipient::User(body.talent_user_id),
        "recruitment.shortlisted",
    )
    .arg("role", campaign.target_role.clone())
    .payload(json!({
        "campaign_id": id,
        "title": campaign.title,
    }))
    .execute()
    .await;

    sqlx::query(
        "UPDATE recruitment_shortlist SET talent_notified_at = NOW()
          WHERE campaign_id = $1 AND talent_user_id = $2 AND talent_notified_at IS NULL",
    )
    .bind(id)
    .bind(body.talent_user_id)
    .execute(&state.db)
    .await?;

    Ok(Json(build_response(json!({ "shortlisted": true }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct DepartureBody {
    pub left_at: chrono::DateTime<chrono::Utc>,
    #[schema(max_length = 2000)]
    pub reason: String,
}

/// Admin: somebody left inside the guarantee.
#[utoipa::path(
    post, path = "/api/admin/recruitment/fees/{id}/departure",
    operation_id = "recruitmentRecordDeparture",
    tag = "admin",
    params(("id" = Uuid, Path, description = "Success fee id")),
    request_body = DepartureBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Already refunded, or no reason given", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not an administrator", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn record_departure(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<DepartureBody>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let refund = recruitment::record_departure(&state.db, id, body.left_at, &body.reason).await?;
    Ok(Json(build_response(json!({ "refund_amount": refund }))))
}
