//! Teams for hire — studios, engagements, milestones, beta programmes.
//!
//! Three audiences again, and the split is load-bearing:
//!
//!   * the **client** briefs, follows milestones, and accepts them;
//!   * **Skilluv** staffs the team, reviews before the client sees anything,
//!     and closes;
//!   * the **talent** answers for themselves — a share is an offer, and
//!     nobody is put on paid work without saying yes.
//!
//! Milestone review is the one step with no client-facing route at all. It
//! happens between the team and Skilluv, and it is what the margin buys.

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
use crate::services::{beta_programs, engagements};

pub fn engagement_routes() -> Router<AppState> {
    Router::new()
        // Public — what a client can see before signing anything.
        .route("/studios", get(list_studios))
        .route("/studios/{id}", get(read_studio))
        .route("/beta-programs/open", get(open_programs))
        // Client.
        .route(
            "/enterprise/engagements",
            get(my_engagements).post(open_engagement),
        )
        .route("/enterprise/engagements/{id}", get(read_engagement))
        .route(
            "/enterprise/engagements/{id}/milestones",
            get(read_milestones),
        )
        .route(
            "/enterprise/engagements/{id}/milestones/{milestone_id}/accept",
            post(accept_milestone),
        )
        .route(
            "/enterprise/beta-programs",
            get(my_programs).post(open_program),
        )
        .route("/enterprise/beta-programs/{id}/testers", get(read_testers))
        .route(
            "/enterprise/beta-programs/{id}/testers/{user_id}/review",
            post(review_feedback),
        )
        // Talent — their own answer, through their own session.
        .route("/engagements/{id}/respond", post(respond))
        .route("/beta-programs/{id}/join", post(join_program))
        .route("/beta-programs/{id}/feedback", post(submit_feedback))
}

/// Admin surface, mounted behind the admin gate.
pub fn admin_engagement_routes() -> Router<AppState> {
    Router::new()
        .route("/admin/studios", post(create_studio))
        .route("/admin/studios/{id}/members", post(add_studio_member))
        .route("/admin/studios/{id}/activate", post(activate_studio))
        .route("/admin/studios/{id}/disband", post(disband_studio))
        .route("/admin/engagements/{id}/members", post(add_member))
        .route("/admin/engagements/{id}/staff-from-studio", post(staff))
        .route("/admin/engagements/{id}/milestones", post(add_milestone))
        .route("/admin/engagements/{id}/start", post(start))
        .route("/admin/milestones/{id}/review", post(review_milestone))
        .route("/admin/beta-programs/{id}/close", post(close_program))
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
// Studios
// ═══════════════════════════════════════════════════════════════════

/// The studios a client can book.
#[utoipa::path(
    get, path = "/api/studios", tag = "work",
    responses((status = 200, body = serde_json::Value)),
)]
pub async fn list_studios(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let studios = engagements::bookable_studios(&state.db).await?;
    Ok(Json(build_response(json!({ "studios": studios }))))
}

/// One studio and who is on it.
#[utoipa::path(
    get, path = "/api/studios/{id}", tag = "work",
    params(("id" = Uuid, Path, description = "Studio id")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "No such studio", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn read_studio(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let studio = engagements::studio_by_id(&state.db, id).await?;
    let members = engagements::studio_members(&state.db, id).await?;
    Ok(Json(build_response(
        json!({ "studio": studio, "members": members }),
    )))
}

#[utoipa::path(
    post, path = "/api/admin/studios", tag = "admin",
    request_body(content = serde_json::Value, description = "StudioInput"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Bad slug, empty specialization, or a taken name", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn create_studio(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<engagements::StudioInput>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let studio = engagements::create_studio(&state.db, input, None).await?;
    Ok(Json(build_response(json!({ "studio": studio }))))
}

#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = EngagementsMemberBody)]
pub struct MemberBody {
    pub user_id: Uuid,
    pub role: String,
    /// What this person takes of the team's side of the money, as a
    /// percentage. Every member's shares have to total 100 before the work
    /// can start.
    #[schema(value_type = String)]
    pub share_percent: BigDecimal,
}

#[utoipa::path(
    post, path = "/api/admin/studios/{id}/members", tag = "admin",
    params(("id" = Uuid, Path, description = "Studio id")),
    request_body = MemberBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "The studio is full, or the role is empty", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn add_studio_member(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<MemberBody>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    engagements::add_studio_member(&state.db, id, body.user_id, &body.role, body.share_percent)
        .await?;
    let members = engagements::studio_members(&state.db, id).await?;
    Ok(Json(build_response(json!({ "members": members }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ActivateBody {
    pub lead_user_id: Uuid,
}

#[utoipa::path(
    post, path = "/api/admin/studios/{id}/activate", tag = "admin",
    params(("id" = Uuid, Path, description = "Studio id")),
    request_body = ActivateBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Fewer than two members, shares that do not total 100%, or a lead who is not on the team", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn activate_studio(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ActivateBody>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let studio = engagements::activate_studio(&state.db, id, body.lead_user_id).await?;
    metrics::counter!("skilluv_studios_activated_total").increment(1);
    Ok(Json(build_response(json!({ "studio": studio }))))
}

#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = EngagementsReasonBody)]
pub struct ReasonBody {
    pub reason: String,
}

#[utoipa::path(
    post, path = "/api/admin/studios/{id}/disband", tag = "admin",
    params(("id" = Uuid, Path, description = "Studio id")),
    request_body = ReasonBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "No reason, or engagements still running", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn disband_studio(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ReasonBody>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    engagements::disband_studio(&state.db, id, &body.reason).await?;
    Ok(Json(build_response(json!({ "disbanded": true }))))
}

// ═══════════════════════════════════════════════════════════════════
// Engagements
// ═══════════════════════════════════════════════════════════════════

/// Brief a piece of work.
#[utoipa::path(
    post, path = "/api/enterprise/engagements", tag = "enterprise",
    request_body(content = serde_json::Value, description = "BriefInput"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Unknown kind, a pricing contradiction, or a shape the chosen kind forbids", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not acting for an enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn open_engagement(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<engagements::BriefInput>,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
    let kind = input.kind.clone();
    let engagement = engagements::open(&state.db, enterprise.id, auth.user_id, input).await?;

    // The engagement register, so this appears next to everything else the
    // company has with us rather than only in its own table.
    let product = match engagement.kind.as_str() {
        "discovery" => "discovery_phase",
        "sprint" => "group_sprint",
        "fractional" => "fractional_placement",
        _ => "outsourcing_project",
    };
    let _ = sqlx::query(
        "INSERT INTO enterprise_products
            (enterprise_id, product_type, source_table, source_id,
             contract_value, currency, created_by)
         VALUES ($1, $2, 'team_engagements', $3, $4, $5, $6)",
    )
    .bind(enterprise.id)
    .bind(product)
    .bind(engagement.id)
    .bind(engagements::contract_value(&engagement))
    .bind(engagement.currency.as_str())
    .bind(auth.user_id)
    .execute(&state.db)
    .await;

    metrics::counter!("skilluv_engagements_total", "kind" => kind).increment(1);
    Ok(Json(build_response(json!({ "engagement": engagement }))))
}

/// The client's own engagements.
#[utoipa::path(
    get, path = "/api/enterprise/engagements", tag = "enterprise",
    responses(
        (status = 200, body = serde_json::Value),
        (status = 403, description = "Not acting for an enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn my_engagements(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
    let list = engagements::for_enterprise(&state.db, enterprise.id).await?;
    Ok(Json(build_response(json!({ "engagements": list }))))
}

/// One engagement, with the team on it.
#[utoipa::path(
    get, path = "/api/enterprise/engagements/{id}", tag = "enterprise",
    params(("id" = Uuid, Path, description = "Engagement id")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 403, description = "Not the client on this engagement", body = crate::api_response::ErrorResponse),
        (status = 404, description = "No such engagement", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn read_engagement(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let engagement = owned_by_caller(&state, &auth, id).await?;
    let team = engagements::members(&state.db, id).await?;
    Ok(Json(build_response(
        json!({ "engagement": engagement, "members": team }),
    )))
}

/// The engagement, if the caller is the client who booked it.
async fn owned_by_caller(
    state: &AppState,
    auth: &AuthUser,
    id: Uuid,
) -> Result<engagements::Engagement, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(state, auth).await?;
    let engagement = engagements::by_id(&state.db, id).await?;
    if engagement.enterprise_id != enterprise.id {
        // Not found rather than forbidden: confirming an id exists tells a
        // competitor which of their guesses is a real engagement.
        return Err(AppError::NotFound("engagement not found".into()));
    }
    Ok(engagement)
}

#[utoipa::path(
    get, path = "/api/enterprise/engagements/{id}/milestones", tag = "enterprise",
    params(("id" = Uuid, Path, description = "Engagement id")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "No such engagement", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn read_milestones(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    owned_by_caller(&state, &auth, id).await?;
    let list = engagements::milestones(&state.db, id).await?;
    Ok(Json(build_response(json!({ "milestones": list }))))
}

/// The client accepts a milestone, and the money moves.
#[utoipa::path(
    post, path = "/api/enterprise/engagements/{id}/milestones/{milestone_id}/accept",
    tag = "enterprise",
    params(
        ("id" = Uuid, Path, description = "Engagement id"),
        ("milestone_id" = Uuid, Path, description = "Milestone id"),
    ),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "The milestone has not passed review", body = crate::api_response::ErrorResponse),
        (status = 404, description = "No such engagement or milestone", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn accept_milestone(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, milestone_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>, AppError> {
    owned_by_caller(&state, &auth, id).await?;
    let paid = engagements::accept_milestone(&state.db, milestone_id, auth.user_id).await?;

    metrics::counter!("skilluv_engagement_milestones_accepted_total").increment(1);
    Ok(Json(build_response(json!({
        "paid": paid
            .into_iter()
            .map(|(user_id, amount)| json!({ "user_id": user_id, "amount": amount }))
            .collect::<Vec<_>>(),
    }))))
}

#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = EngagementsRespondBody)]
pub struct RespondBody {
    pub accept: bool,
}

/// A talent answers for themselves.
#[utoipa::path(
    post, path = "/api/engagements/{id}/respond", tag = "work",
    params(("id" = Uuid, Path, description = "Engagement id")),
    request_body = RespondBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "Not on this engagement", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
    operation_id = "engagementsRespond",
)]
pub async fn respond(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<RespondBody>,
) -> Result<Json<Value>, AppError> {
    engagements::respond(&state.db, id, auth.user_id, body.accept).await?;
    Ok(Json(build_response(json!({ "accepted": body.accept }))))
}

#[utoipa::path(
    post, path = "/api/admin/engagements/{id}/members", tag = "admin",
    params(("id" = Uuid, Path, description = "Engagement id")),
    request_body = MemberBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Empty role or an impossible share", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
    operation_id = "engagementsAddMember",
)]
pub async fn add_member(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<MemberBody>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    engagements::add_member(&state.db, id, body.user_id, &body.role, body.share_percent).await?;
    let team = engagements::members(&state.db, id).await?;
    Ok(Json(build_response(json!({ "members": team }))))
}

/// Staff an engagement from the studio that is taking it.
#[utoipa::path(
    post, path = "/api/admin/engagements/{id}/staff-from-studio", tag = "admin",
    params(("id" = Uuid, Path, description = "Engagement id")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "No studio is attached to this engagement", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn staff(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let engagement = engagements::by_id(&state.db, id).await?;
    let studio_id = engagement.studio_id.ok_or_else(|| {
        AppError::Validation(
            "no studio is attached to this engagement — add the members individually, or \
             book a studio"
                .into(),
        )
    })?;

    let added = engagements::staff_from_studio(&state.db, id, studio_id).await?;
    let team = engagements::members(&state.db, id).await?;
    Ok(Json(build_response(
        json!({ "added": added, "members": team }),
    )))
}

#[utoipa::path(
    post, path = "/api/admin/engagements/{id}/milestones", tag = "admin",
    params(("id" = Uuid, Path, description = "Engagement id")),
    request_body(content = serde_json::Value, description = "MilestoneInput"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "No acceptance criteria, or a share outside 0-100", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn add_milestone(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<engagements::MilestoneInput>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let milestone_id = engagements::add_milestone(&state.db, id, input).await?;
    let list = engagements::milestones(&state.db, id).await?;
    Ok(Json(build_response(
        json!({ "milestone_id": milestone_id, "milestones": list }),
    )))
}

#[utoipa::path(
    post, path = "/api/admin/engagements/{id}/start", tag = "admin",
    params(("id" = Uuid, Path, description = "Engagement id")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Somebody has not agreed, or the shares or milestones do not total 100%", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
    operation_id = "engagementsStart",
)]
pub async fn start(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let engagement = engagements::start(&state.db, id).await?;
    Ok(Json(build_response(json!({ "engagement": engagement }))))
}

#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = EngagementsReviewBody)]
pub struct ReviewBody {
    pub passed: bool,
    pub notes: String,
}

/// Skilluv reviews a milestone before the client sees it.
#[utoipa::path(
    post, path = "/api/admin/milestones/{id}/review", tag = "admin",
    params(("id" = Uuid, Path, description = "Milestone id")),
    request_body = ReviewBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "A review with no notes", body = crate::api_response::ErrorResponse),
        (status = 404, description = "That milestone is not waiting for review", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn review_milestone(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ReviewBody>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    engagements::review(&state.db, id, auth.user_id, body.passed, &body.notes).await?;
    metrics::counter!(
        "skilluv_milestone_reviews_total",
        "passed" => body.passed.to_string()
    )
    .increment(1);
    Ok(Json(build_response(json!({ "passed": body.passed }))))
}

// ═══════════════════════════════════════════════════════════════════
// Beta programmes
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
pub struct OpenProgramsQuery {
    #[serde(default)]
    pub test_type: Option<String>,
}

/// What a tester can join.
#[utoipa::path(
    get, path = "/api/beta-programs/open", tag = "work",
    params(("test_type" = Option<String>, Query, description = "Narrow to one kind of test")),
    responses((status = 200, body = serde_json::Value)),
    operation_id = "engagementsOpenPrograms",
)]
pub async fn open_programs(
    State(state): State<AppState>,
    Query(q): Query<OpenProgramsQuery>,
) -> Result<Json<Value>, AppError> {
    let list = beta_programs::recruiting(&state.db, q.test_type.as_deref()).await?;
    Ok(Json(build_response(json!({ "programs": list }))))
}

#[utoipa::path(
    post, path = "/api/enterprise/beta-programs", tag = "enterprise",
    request_body(content = serde_json::Value, description = "ProgramInput"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Unknown test type, an empty brief, or an unpaid beta", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not acting for an enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
    operation_id = "engagementsOpenProgram",
)]
pub async fn open_program(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<beta_programs::ProgramInput>,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
    let program = beta_programs::open(&state.db, enterprise.id, auth.user_id, input).await?;

    let quoted = beta_programs::quote(
        program.testers_wanted as i32,
        &program.tester_reward,
        &program.program_fee,
    );
    let _ = sqlx::query(
        "INSERT INTO enterprise_products
            (enterprise_id, product_type, source_table, source_id,
             contract_value, currency, created_by)
         VALUES ($1, 'beta_program', 'beta_programs', $2, $3, $4, $5)",
    )
    .bind(enterprise.id)
    .bind(program.id)
    .bind(&quoted)
    .bind(program.currency.as_str())
    .bind(auth.user_id)
    .execute(&state.db)
    .await;

    metrics::counter!(
        "skilluv_beta_programs_total",
        "test_type" => program.test_type.clone()
    )
    .increment(1);
    Ok(Json(build_response(
        json!({ "program": program, "quoted_maximum": quoted }),
    )))
}

#[utoipa::path(
    get, path = "/api/enterprise/beta-programs", tag = "enterprise",
    responses(
        (status = 200, body = serde_json::Value),
        (status = 403, description = "Not acting for an enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn my_programs(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
    let list = beta_programs::for_enterprise(&state.db, enterprise.id).await?;
    Ok(Json(build_response(json!({ "programs": list }))))
}

/// The programme, if the caller is the client who opened it.
async fn program_owned_by_caller(
    state: &AppState,
    auth: &AuthUser,
    id: Uuid,
) -> Result<beta_programs::BetaProgram, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(state, auth).await?;
    let program = beta_programs::by_id(&state.db, id).await?;
    if program.enterprise_id != enterprise.id {
        return Err(AppError::NotFound("programme not found".into()));
    }
    Ok(program)
}

#[utoipa::path(
    get, path = "/api/enterprise/beta-programs/{id}/testers", tag = "enterprise",
    params(("id" = Uuid, Path, description = "Programme id")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "No such programme", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn read_testers(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    program_owned_by_caller(&state, &auth, id).await?;
    let list = beta_programs::testers(&state.db, id).await?;
    Ok(Json(build_response(json!({ "testers": list }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct FeedbackVerdictBody {
    pub accept: bool,
    #[serde(default)]
    pub reason: Option<String>,
}

/// The client judges one tester's feedback. Accepting pays the reward.
#[utoipa::path(
    post, path = "/api/enterprise/beta-programs/{id}/testers/{user_id}/review",
    tag = "enterprise",
    params(
        ("id" = Uuid, Path, description = "Programme id"),
        ("user_id" = Uuid, Path, description = "Tester id"),
    ),
    request_body = FeedbackVerdictBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "A refusal with no reason", body = crate::api_response::ErrorResponse),
        (status = 404, description = "Nothing from that tester is waiting for review", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn review_feedback(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, user_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<FeedbackVerdictBody>,
) -> Result<Json<Value>, AppError> {
    program_owned_by_caller(&state, &auth, id).await?;
    let paid =
        beta_programs::review_feedback(&state.db, id, user_id, body.accept, body.reason.as_deref())
            .await?;
    Ok(Json(build_response(
        json!({ "accepted": body.accept, "reward_paid": paid }),
    )))
}

/// A tester signs up.
#[utoipa::path(
    post, path = "/api/beta-programs/{id}/join", tag = "work",
    params(("id" = Uuid, Path, description = "Programme id")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "The programme is full, closed, or already joined", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn join_program(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    beta_programs::join(&state.db, id, auth.user_id).await?;
    Ok(Json(build_response(json!({ "joined": true }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct FeedbackBody {
    pub feedback_md: String,
}

#[utoipa::path(
    post, path = "/api/beta-programs/{id}/feedback", tag = "work",
    params(("id" = Uuid, Path, description = "Programme id")),
    request_body = FeedbackBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Too short to build a report from", body = crate::api_response::ErrorResponse),
        (status = 404, description = "Not on this programme, or already reviewed", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn submit_feedback(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<FeedbackBody>,
) -> Result<Json<Value>, AppError> {
    beta_programs::submit_feedback(&state.db, id, auth.user_id, &body.feedback_md).await?;
    Ok(Json(build_response(json!({ "submitted": true }))))
}

#[utoipa::path(
    post, path = "/api/admin/beta-programs/{id}/close", tag = "admin",
    params(("id" = Uuid, Path, description = "Programme id")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Testers are still waiting on a verdict", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn close_program(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let fee = beta_programs::close(&state.db, id).await?;
    Ok(Json(build_response(json!({ "program_fee_booked": fee }))))
}
