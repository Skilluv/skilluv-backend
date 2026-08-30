//! Onboarding as a service, living labs, and proposals that start with the
//! team rather than the client.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use bigdecimal::BigDecimal;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use chrono::{DateTime, Utc};

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::middleware::{AuthUser, OptionalAuth};
use crate::services::continuous;

pub fn continuous_routes() -> Router<AppState> {
    Router::new()
        // Onboarding.
        .route("/enterprise/onboardings", post(propose_onboarding))
        .route("/users/me/onboardings", get(my_onboardings))
        .route("/onboardings/{id}/respond", post(respond_to_onboarding))
        .route("/onboardings/{id}/check-in", post(check_in))
        // Living labs.
        .route("/enterprise/labs", post(open_lab))
        .route("/labs", get(open_labs))
        .route("/labs/{id}/join", post(join_lab))
        .route("/labs/{id}/contributions", post(contribute))
        // Team proposals.
        .route("/proposals", get(visible_proposals).post(draft_proposal))
        .route("/proposals/{id}/members", post(add_member))
        .route("/proposals/{id}/respond", post(respond_to_proposal))
        .route("/proposals/{id}/publish", post(publish_proposal))
        .route("/proposals/{id}/interest", post(express_interest))
}

pub fn admin_continuous_routes() -> Router<AppState> {
    Router::new()
        .route("/admin/onboardings/{id}/retention", post(record_retention))
        .route(
            "/admin/lab-contributions/{id}/judge",
            post(judge_contribution),
        )
        .route(
            "/admin/labs/{id}/contributions",
            get(list_lab_contributions),
        )
        .route("/admin/labs/{id}/settle", post(settle_month))
        .route("/admin/proposals/{id}/signed", post(record_signature))
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
// Onboarding
// ═══════════════════════════════════════════════════════════════════

/// Propose an onboarding. It does not start until the person agrees.
#[utoipa::path(
    post, path = "/api/enterprise/onboardings", tag = "enterprise",
    request_body(content = serde_json::Value, description = "OnboardingInput"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "The mentor is below the floor, or is the same person", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not acting for an enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn propose_onboarding(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<continuous::OnboardingInput>,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
    let onboarding =
        continuous::propose_onboarding(&state.db, enterprise.id, auth.user_id, input).await?;

    Ok(Json(build_response(json!({
        "onboarding": onboarding,
        // Said at the moment of ordering, because a client who learns it
        // later has already told the new hire it is happening.
        "note": "L'accompagnement démarre quand la personne accepte. Elle peut refuser, \
                 et rien n'est alors facturé.",
        "note_code": "onboarding_starts_on_acceptance",
    }))))
}

#[utoipa::path(
    get, path = "/api/users/me/onboardings", tag = "work",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn my_onboardings(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let onboardings = continuous::onboardings_for(&state.db, auth.user_id).await?;
    Ok(Json(build_response(json!({ "onboardings": onboardings }))))
}

#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = ContinuousRespondBody)]
pub struct RespondBody {
    pub accept: bool,
}

/// The person being onboarded answers for themselves.
#[utoipa::path(
    post, path = "/api/onboardings/{id}/respond", tag = "work",
    params(("id" = Uuid, Path, description = "Onboarding id")),
    request_body = RespondBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "Nothing waiting on your answer", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn respond_to_onboarding(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<RespondBody>,
) -> Result<Json<Value>, AppError> {
    let onboarding =
        continuous::respond_to_onboarding(&state.db, id, auth.user_id, body.accept).await?;
    Ok(Json(build_response(json!({ "onboarding": onboarding }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CheckInBody {
    pub month_number: i16,
    pub notes_md: String,
    #[serde(default)]
    pub going_well: Option<bool>,
}

/// A monthly check-in. Either side can write; both should.
#[utoipa::path(
    post, path = "/api/onboardings/{id}/check-in",
    operation_id = "continuousCheckIn",
    tag = "work",
    params(("id" = Uuid, Path, description = "Onboarding id")),
    request_body = CheckInBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "An empty note", body = crate::api_response::ErrorResponse),
        (status = 404, description = "Not your onboarding", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn check_in(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<CheckInBody>,
) -> Result<Json<Value>, AppError> {
    continuous::record_check_in(
        &state.db,
        id,
        auth.user_id,
        body.month_number,
        &body.notes_md,
        body.going_well,
    )
    .await?;
    Ok(Json(build_response(json!({ "recorded": true }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RetentionBody {
    pub months: i16,
    pub still_there: bool,
}

#[utoipa::path(
    post, path = "/api/admin/onboardings/{id}/retention", tag = "admin",
    params(("id" = Uuid, Path, description = "Onboarding id")),
    request_body = RetentionBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Not a checkpoint, or an engagement never agreed to", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn record_retention(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<RetentionBody>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    continuous::record_retention(&state.db, id, body.months, body.still_there).await?;
    Ok(Json(build_response(json!({ "recorded": true }))))
}

// ═══════════════════════════════════════════════════════════════════
// Living labs
// ═══════════════════════════════════════════════════════════════════

#[utoipa::path(
    post, path = "/api/enterprise/labs", tag = "enterprise",
    request_body(content = serde_json::Value, description = "LabInput"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "No activities, or no reward pool", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not acting for an enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn open_lab(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<continuous::LabInput>,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
    let lab = continuous::open_lab(&state.db, enterprise.id, auth.user_id, input).await?;

    let _ = sqlx::query(
        "INSERT INTO enterprise_products
            (enterprise_id, product_type, source_table, source_id,
             contract_value, currency, created_by)
         VALUES ($1, 'living_lab', 'living_lab_engagements', $2, $3, $4, $5)",
    )
    .bind(enterprise.id)
    .bind(lab.id)
    .bind(&lab.monthly_fee)
    .bind(lab.currency.as_str())
    .bind(auth.user_id)
    .execute(&state.db)
    .await;

    Ok(Json(build_response(json!({ "lab": lab }))))
}

/// Labs a contributor can join.
#[utoipa::path(
    get, path = "/api/labs", tag = "work",
    responses((status = 200, body = serde_json::Value)),
)]
pub async fn open_labs(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let labs = continuous::open_labs(&state.db).await?;
    Ok(Json(build_response(json!({ "labs": labs }))))
}

#[utoipa::path(
    post, path = "/api/labs/{id}/join", tag = "work",
    params(("id" = Uuid, Path, description = "Lab id")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Closed, or already at its target size", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn join_lab(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    continuous::join_lab(&state.db, id, auth.user_id).await?;
    Ok(Json(build_response(json!({
        "joined": true,
        // Joining a lab means seeing something unreleased. Said once, plainly.
        "note": "Vous accédez à un produit non publié : ce qui s'y dit y reste.",
        "note_code": "lab_content_is_unreleased",
    }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ContributionBody {
    pub activity_type: String,
    pub summary_md: String,
}

#[utoipa::path(
    post, path = "/api/labs/{id}/contributions", tag = "work",
    params(("id" = Uuid, Path, description = "Lab id")),
    request_body = ContributionBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Not an activity this lab asks for, or too short", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn contribute(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ContributionBody>,
) -> Result<Json<Value>, AppError> {
    let contribution_id = continuous::contribute(
        &state.db,
        id,
        auth.user_id,
        &body.activity_type,
        &body.summary_md,
    )
    .await?;
    Ok(Json(build_response(
        json!({ "contribution_id": contribution_id }),
    )))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct JudgementBody {
    pub accept: bool,
    #[serde(default)]
    pub reason: Option<String>,
}

#[utoipa::path(
    post, path = "/api/admin/lab-contributions/{id}/judge", tag = "admin",
    params(("id" = Uuid, Path, description = "Contribution id")),
    request_body = JudgementBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "A refusal with no reason", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn judge_contribution(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<JudgementBody>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    continuous::judge_contribution(&state.db, id, body.accept, body.reason.as_deref()).await?;
    Ok(Json(build_response(json!({ "accepted": body.accept }))))
}

#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = ContinuousSettleBody)]
pub struct SettleBody {
    /// Any day in the month; the first of it is what is used.
    pub month: chrono::NaiveDate,
}

/// Divide a month's pool and pay it out.
#[utoipa::path(
    post, path = "/api/admin/labs/{id}/settle", tag = "admin",
    params(("id" = Uuid, Path, description = "Lab id")),
    request_body = SettleBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Nothing accepted and unpaid that month", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn settle_month(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<SettleBody>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;

    use chrono::Datelike;
    let month = chrono::NaiveDate::from_ymd_opt(body.month.year(), body.month.month(), 1)
        .unwrap_or(body.month);

    let (paid, each) = continuous::settle_month(&state.db, id, month).await?;
    Ok(Json(build_response(
        json!({ "contributions_paid": paid, "each": each, "month": month }),
    )))
}

// ═══════════════════════════════════════════════════════════════════
// Team proposals
// ═══════════════════════════════════════════════════════════════════

#[utoipa::path(
    post, path = "/api/proposals", tag = "work",
    request_body(content = serde_json::Value, description = "ProposalInput"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "The problem or the approach is too thin, or the slug is taken", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn draft_proposal(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<continuous::ProposalInput>,
) -> Result<Json<Value>, AppError> {
    let proposal = continuous::draft_proposal(&state.db, auth.user_id, input).await?;
    Ok(Json(build_response(json!({ "proposal": proposal }))))
}

/// The proposals a reader can see: the public ones, plus any aimed at them.
#[utoipa::path(
    get, path = "/api/proposals", tag = "work",
    responses((status = 200, body = serde_json::Value)),
)]
pub async fn visible_proposals(
    State(state): State<AppState>,
    OptionalAuth(auth): OptionalAuth,
) -> Result<Json<Value>, AppError> {
    let enterprise_id = match &auth {
        Some(auth) => {
            sqlx::query_scalar::<_, Uuid>("SELECT id FROM enterprises WHERE owner_id = $1 LIMIT 1")
                .bind(auth.user_id)
                .fetch_optional(&state.db)
                .await?
        }
        None => None,
    };

    let proposals = continuous::visible_proposals(&state.db, enterprise_id).await?;
    Ok(Json(build_response(json!({ "proposals": proposals }))))
}

#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = ContinuousMemberBody)]
pub struct MemberBody {
    pub user_id: Uuid,
    pub role: String,
}

#[utoipa::path(
    post, path = "/api/proposals/{id}/members", tag = "work",
    params(("id" = Uuid, Path, description = "Proposal id")),
    request_body = MemberBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "Not your proposal", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
    operation_id = "continuousAddMember",
)]
pub async fn add_member(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<MemberBody>,
) -> Result<Json<Value>, AppError> {
    let proposal = continuous::proposal(&state.db, id).await?;
    if proposal.initiator_user_id != auth.user_id {
        return Err(AppError::NotFound("proposal not found".into()));
    }
    continuous::add_proposal_member(&state.db, id, body.user_id, &body.role).await?;
    Ok(Json(build_response(json!({ "added": true }))))
}

/// Somebody named on a proposal answers for themselves.
#[utoipa::path(
    post, path = "/api/proposals/{id}/respond", tag = "work",
    params(("id" = Uuid, Path, description = "Proposal id")),
    request_body = RespondBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "You are not on this proposal", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn respond_to_proposal(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<RespondBody>,
) -> Result<Json<Value>, AppError> {
    continuous::respond_to_proposal(&state.db, id, auth.user_id, body.accept).await?;
    Ok(Json(build_response(json!({ "accepted": body.accept }))))
}

#[utoipa::path(
    post, path = "/api/proposals/{id}/publish", tag = "work",
    params(("id" = Uuid, Path, description = "Proposal id")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Somebody named has not agreed", body = crate::api_response::ErrorResponse),
        (status = 404, description = "Not your proposal", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn publish_proposal(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let proposal = continuous::proposal(&state.db, id).await?;
    if proposal.initiator_user_id != auth.user_id {
        return Err(AppError::NotFound("proposal not found".into()));
    }
    let proposal = continuous::publish_proposal(&state.db, id).await?;
    Ok(Json(build_response(json!({ "proposal": proposal }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct InterestBody {
    #[serde(default)]
    pub note_md: Option<String>,
}

/// A company says it has the problem.
#[utoipa::path(
    post, path = "/api/proposals/{id}/interest", tag = "enterprise",
    params(("id" = Uuid, Path, description = "Proposal id")),
    request_body = InterestBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "The proposal is not open", body = crate::api_response::ErrorResponse),
        (status = 404, description = "Not aimed at you", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn express_interest(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<InterestBody>,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
    continuous::express_interest(&state.db, id, enterprise.id, body.note_md.as_deref()).await?;
    Ok(Json(build_response(json!({ "recorded": true }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SignatureBody {
    pub enterprise_id: Uuid,
    #[schema(value_type = String)]
    pub contract_value: BigDecimal,
}

#[utoipa::path(
    post, path = "/api/admin/proposals/{id}/signed", tag = "admin",
    params(("id" = Uuid, Path, description = "Proposal id")),
    request_body = SignatureBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "That company never expressed interest", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn record_signature(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<SignatureBody>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let fee = continuous::record_signature(&state.db, id, body.enterprise_id, body.contract_value)
        .await?;
    Ok(Json(build_response(json!({ "facilitation_fee": fee }))))
}

// ═══════════════════════════════════════════════════════════════════
// The list behind the judge button
// ═══════════════════════════════════════════════════════════════════
//
// `POST /admin/lab-contributions/{id}/judge` takes the id of one
// contribution, and until now no route served those ids: submission is
// write-only, `GET /labs` lists labs rather than their contributions, and
// `settle` closes a whole month without ever naming one. It was the last
// unreachable staff verb of the 308 — a button with no list, the same shape
// as SKI-337 and SKI-354.

/// One contribution, with enough context to judge it without opening
/// anything else.
#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct LabContributionRow {
    pub id: Uuid,
    pub lab_id: Uuid,
    pub contributor_user_id: Uuid,
    pub contributor_username: Option<String>,
    /// What the contribution brings, in the contributor's own words.
    pub summary_md: String,
    pub activity_type: String,
    pub counts_for_month: chrono::NaiveDate,
    pub submitted_at: DateTime<Utc>,
    /// `None` while nobody has judged it — which is the whole point of the
    /// screen this feeds, so it is what the default ordering sorts on.
    pub accepted: Option<bool>,
    pub rejection_reason: Option<String>,
    #[schema(value_type = Option<String>, example = "125.00")]
    pub reward: Option<BigDecimal>,
    pub paid_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct LabContributionsQuery {
    /// `pending`, `accepted` or `rejected`. Omitted means all of them.
    #[param(max_length = 16)]
    pub status: Option<String>,
    /// Any day in the month to filter on; the first of it is what is used.
    pub month: Option<chrono::NaiveDate>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

/// List a lab's contributions so that one of them can be judged.
#[utoipa::path(
    get, path = "/api/admin/labs/{id}/contributions", tag = "admin",
    params(("id" = Uuid, Path, description = "Lab id"), LabContributionsQuery),
    responses(
        (status = 200, body = ApiResponse<Vec<LabContributionRow>>),
        (status = 400, description = "Unknown status filter", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not staff", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
    operation_id = "adminLabContributions",
)]
pub async fn list_lab_contributions(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(lab_id): Path<Uuid>,
    Query(q): Query<LabContributionsQuery>,
) -> Result<Json<Value>, AppError> {
    // Guarded like `settle`, which pays these same rows out.
    crate::routes::admin::require_admin(&state, &auth).await?;

    crate::validators::check_range_opt(q.page, "page", 1, 100_000)?;
    crate::validators::check_range_opt(q.per_page, "per_page", 1, 200)?;
    let per_page = q.per_page.unwrap_or(50).clamp(1, 200);
    let offset = (q.page.unwrap_or(1).max(1) - 1) * per_page;

    // `accepted` is a nullable boolean, so "pending" is a third state rather
    // than a value — spelled out here instead of left to the caller to encode.
    let status = q.status.as_deref();
    if let Some(s) = status {
        if !matches!(s, "pending" | "accepted" | "rejected") {
            return Err(AppError::Validation(
                "status must be pending, accepted or rejected".into(),
            ));
        }
    }

    let rows: Vec<LabContributionRow> = sqlx::query_as(
        r#"
        SELECT c.id, c.lab_id,
               c.user_id AS contributor_user_id,
               u.username AS contributor_username,
               c.summary_md, c.activity_type, c.counts_for_month,
               c.created_at AS submitted_at,
               c.accepted, c.rejection_reason, c.reward, c.paid_at
          FROM living_lab_contributions c
          LEFT JOIN users u ON u.id = c.user_id
         WHERE c.lab_id = $1
           AND ($2::TEXT IS NULL
                OR ($2 = 'pending'  AND c.accepted IS NULL)
                OR ($2 = 'accepted' AND c.accepted IS TRUE)
                OR ($2 = 'rejected' AND c.accepted IS FALSE))
           AND ($3::DATE IS NULL
                OR c.counts_for_month = date_trunc('month', $3::DATE)::DATE)
         ORDER BY (c.accepted IS NOT NULL), c.created_at ASC
         LIMIT $4 OFFSET $5
        "#,
    )
    .bind(lab_id)
    .bind(status)
    .bind(q.month)
    .bind(per_page)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let total: i64 =
        sqlx::query_scalar("SELECT count(*) FROM living_lab_contributions WHERE lab_id = $1")
            .bind(lab_id)
            .fetch_one(&state.db)
            .await?;

    Ok(Json(build_response(json!({
        "contributions": rows,
        "page": q.page.unwrap_or(1).max(1),
        "per_page": per_page,
        "total": total,
    }))))
}
