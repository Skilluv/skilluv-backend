//! Contests a company pays for, and the interviews that come out of them.
//!
//! The restraints, all of them enforced below or in the schema:
//!
//!   * an invitation-only contest returns 404 to anybody not invited — a
//!     private hiring search must not confirm its own existence to somebody
//!     who guessed the slug;
//!   * judging ranks the whole judged field, not only the shortlist;
//!   * the shortlist earns an attestation whether or not anybody is hired;
//!   * a hire needs an interview that actually happened;
//!   * the company offers times and the person picks one, never the reverse.

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
use crate::middleware::{AuthUser, OptionalAuth};
use crate::services::{contests, interviews};

pub fn contest_routes() -> Router<AppState> {
    Router::new()
        // Public.
        .route("/contests/open", get(open_contests))
        .route("/contests/{slug}", get(read_contest))
        // Client.
        .route("/enterprise/contests", get(my_contests).post(open_contest))
        .route("/enterprise/contests/{id}/status", post(set_status))
        .route(
            "/enterprise/contests/{id}/submissions",
            get(read_submissions),
        )
        .route("/enterprise/contests/{id}/invite", post(invite))
        .route("/enterprise/contests/{id}/judge", post(judge))
        .route("/enterprise/contests/{id}/hire", post(record_hire))
        .route("/enterprise/contests/{id}/outcome", post(set_outcome))
        .route("/enterprise/interviews", post(propose_interview))
        .route("/enterprise/interviews/{id}/complete", post(complete))
        // Talent.
        .route("/contests/{id}/respond", post(respond_to_invitation))
        .route("/contests/{id}/submit", post(submit))
        .route("/users/me/contest-invitations", get(my_invitations))
        .route("/users/me/interviews", get(my_interviews))
        .route("/interviews/{id}/confirm", post(confirm_interview))
        .route("/interviews/{id}/decline", post(decline_interview))
}

pub fn admin_contest_routes() -> Router<AppState> {
    Router::new().route("/admin/contests/{id}/conclude", post(conclude))
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
pub struct OpenContestsQuery {
    #[serde(default)]
    pub kind: Option<String>,
}

/// The contests anybody can enter.
#[utoipa::path(
    get, path = "/api/contests/open", tag = "work",
    params(OpenContestsQuery),
    responses((status = 200, body = serde_json::Value)),
)]
pub async fn open_contests(
    State(state): State<AppState>,
    Query(q): Query<OpenContestsQuery>,
) -> Result<Json<Value>, AppError> {
    let list = contests::open_contests(&state.db, q.kind.as_deref()).await?;
    Ok(Json(build_response(json!({ "contests": list }))))
}

/// One contest, if the caller is allowed to see it.
#[utoipa::path(
    get, path = "/api/contests/{slug}", tag = "work",
    params(("slug" = String, Path, description = "Contest slug")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "No such contest, or one you were not invited to", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn read_contest(
    State(state): State<AppState>,
    OptionalAuth(auth): OptionalAuth,
    Path(slug): Path<String>,
) -> Result<Json<Value>, AppError> {
    let contest = contests::by_slug(&state.db, &slug).await?;

    if contest.status == "draft" {
        return Err(AppError::NotFound("contest not found".into()));
    }

    if contest.visibility == "invitation_only" {
        // Not found rather than forbidden, so a private hiring search does
        // not confirm its own existence to somebody who guessed the slug.
        let invited = match &auth {
            Some(auth) => {
                sqlx::query_scalar::<_, bool>(
                    "SELECT EXISTS (
                         SELECT 1 FROM contest_invitations
                          WHERE contest_id = $1 AND talent_user_id = $2
                     )",
                )
                .bind(contest.id)
                .bind(auth.user_id)
                .fetch_one(&state.db)
                .await?
            }
            None => false,
        };
        if !invited {
            return Err(AppError::NotFound("contest not found".into()));
        }
    }

    Ok(Json(build_response(json!({ "contest": contest }))))
}

#[utoipa::path(
    post, path = "/api/enterprise/contests", tag = "enterprise",
    request_body(content = serde_json::Value, description = "ContestInput"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "A shape the chosen kind forbids, an unknown trade, or a deadline in the past", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not acting for an enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn open_contest(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<contests::ContestInput>,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
    let kind = input.kind.clone();
    let contest = contests::open(&state.db, enterprise.id, auth.user_id, input).await?;

    let product = match contest.kind.as_str() {
        "recruiting" => "recruiting_contest",
        "award" => "innovation_award",
        "product_led" => "product_led_hackathon",
        "corporate_internal" => "corporate_hackathon",
        _ => "migration_contest",
    };
    let _ = sqlx::query(
        "INSERT INTO enterprise_products
            (enterprise_id, product_type, source_table, source_id,
             contract_value, currency, created_by)
         VALUES ($1, $2, 'enterprise_contests', $3, $4, $5, $6)",
    )
    .bind(enterprise.id)
    .bind(product)
    .bind(contest.id)
    .bind(&contest.orchestration_fee)
    .bind(contest.currency.as_str())
    .bind(auth.user_id)
    .execute(&state.db)
    .await;

    metrics::counter!("skilluv_enterprise_contests_total", "kind" => kind).increment(1);
    Ok(Json(build_response(json!({ "contest": contest }))))
}

#[utoipa::path(
    get, path = "/api/enterprise/contests", tag = "enterprise",
    responses(
        (status = 200, body = serde_json::Value),
        (status = 403, description = "Not acting for an enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn my_contests(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
    let list = contests::for_enterprise(&state.db, enterprise.id).await?;
    Ok(Json(build_response(json!({ "contests": list }))))
}

/// The contest, if the caller is the company that opened it.
async fn owned_by_caller(
    state: &AppState,
    auth: &AuthUser,
    id: Uuid,
) -> Result<contests::Contest, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(state, auth).await?;
    let contest = contests::by_id(&state.db, id).await?;
    if contest.enterprise_id != enterprise.id {
        return Err(AppError::NotFound("contest not found".into()));
    }
    Ok(contest)
}

#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = ContestsStatusBody)]
pub struct StatusBody {
    pub status: String,
}

#[utoipa::path(
    post, path = "/api/enterprise/contests/{id}/status", tag = "enterprise",
    params(("id" = Uuid, Path, description = "Contest id")),
    request_body = StatusBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Not a status", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
    operation_id = "contestsSetStatus",
)]
pub async fn set_status(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<StatusBody>,
) -> Result<Json<Value>, AppError> {
    owned_by_caller(&state, &auth, id).await?;
    let contest = contests::set_status(&state.db, id, &body.status).await?;
    Ok(Json(build_response(json!({ "contest": contest }))))
}

#[utoipa::path(
    get, path = "/api/enterprise/contests/{id}/submissions", tag = "enterprise",
    params(("id" = Uuid, Path, description = "Contest id")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "Not your contest", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn read_submissions(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    owned_by_caller(&state, &auth, id).await?;
    let list = contests::submissions(&state.db, id).await?;
    Ok(Json(build_response(json!({ "submissions": list }))))
}

#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = ContestsInviteBody)]
pub struct InviteBody {
    pub user_id: Uuid,
}

#[utoipa::path(
    post, path = "/api/enterprise/contests/{id}/invite", tag = "enterprise",
    params(("id" = Uuid, Path, description = "Contest id")),
    request_body = InviteBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "Not your contest", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn invite(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<InviteBody>,
) -> Result<Json<Value>, AppError> {
    owned_by_caller(&state, &auth, id).await?;
    contests::invite(&state.db, id, body.user_id).await?;
    Ok(Json(build_response(json!({ "invited": true }))))
}

#[derive(Debug, Deserialize)]
pub struct JudgeBody {
    /// Every judged entry with its rank. Sent in one go because ranks are
    /// unique per contest: applied one at a time, a re-rank would fail
    /// halfway and leave a shortlist that is neither the old nor the new.
    pub verdicts: Vec<contests::Verdict>,
}

#[utoipa::path(
    post, path = "/api/enterprise/contests/{id}/judge", tag = "enterprise",
    params(("id" = Uuid, Path, description = "Contest id")),
    request_body(content = serde_json::Value, description = "JudgeBody"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Two entries share a rank, or no verdicts given", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn judge(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<JudgeBody>,
) -> Result<Json<Value>, AppError> {
    owned_by_caller(&state, &auth, id).await?;
    let ranked = contests::judge(&state.db, id, body.verdicts).await?;
    metrics::counter!("skilluv_contests_judged_total").increment(1);
    Ok(Json(build_response(json!({ "submissions": ranked }))))
}

#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = ContestsHireBody)]
pub struct HireBody {
    pub talent_user_id: Uuid,
    /// As agreed with the person hired. Declared, not verified.
    #[schema(value_type = String)]
    pub annual_salary: BigDecimal,
    /// How long the placement is guaranteed. Defaults to six months.
    #[serde(default)]
    pub guarantee_days: Option<i64>,
}

#[utoipa::path(
    post, path = "/api/enterprise/contests/{id}/hire", tag = "enterprise",
    params(("id" = Uuid, Path, description = "Contest id")),
    request_body = HireBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Not a recruiting contest, or the person was never interviewed", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn record_hire(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<HireBody>,
) -> Result<Json<Value>, AppError> {
    owned_by_caller(&state, &auth, id).await?;
    let fee_id = contests::record_hire(
        &state.db,
        id,
        body.talent_user_id,
        body.annual_salary,
        body.guarantee_days.unwrap_or(180),
    )
    .await?;
    Ok(Json(build_response(json!({ "success_fee_id": fee_id }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct OutcomeBody {
    pub engagement_id: Uuid,
}

/// Point a contest at the work it turned into.
#[utoipa::path(
    post, path = "/api/enterprise/contests/{id}/outcome", tag = "enterprise",
    params(("id" = Uuid, Path, description = "Contest id")),
    request_body = OutcomeBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "A recruiting contest ends in a hire, not an engagement", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn set_outcome(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<OutcomeBody>,
) -> Result<Json<Value>, AppError> {
    owned_by_caller(&state, &auth, id).await?;
    contests::set_outcome(&state.db, id, body.engagement_id).await?;
    Ok(Json(build_response(json!({ "linked": true }))))
}

#[utoipa::path(
    post, path = "/api/admin/contests/{id}/conclude", tag = "admin",
    params(("id" = Uuid, Path, description = "Contest id")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Entries still have no verdict", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn conclude(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let booked = contests::conclude(&state.db, id).await?;
    Ok(Json(build_response(json!({ "revenue_booked": booked }))))
}

// ═══════════════════════════════════════════════════════════════════
// Entrants
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = ContestsRespondBody)]
pub struct RespondBody {
    pub accept: bool,
}

#[utoipa::path(
    post, path = "/api/contests/{id}/respond", tag = "work",
    params(("id" = Uuid, Path, description = "Contest id")),
    request_body = RespondBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "No invitation", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn respond_to_invitation(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<RespondBody>,
) -> Result<Json<Value>, AppError> {
    contests::respond_to_invitation(&state.db, id, auth.user_id, body.accept).await?;
    Ok(Json(build_response(json!({ "accepted": body.accept }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SubmitBody {
    pub deliverable_url: String,
    #[serde(default)]
    pub notes_md: Option<String>,
}

#[utoipa::path(
    post, path = "/api/contests/{id}/submit", tag = "work",
    params(("id" = Uuid, Path, description = "Contest id")),
    request_body = SubmitBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Closed, past the deadline, or a link that is not https", body = crate::api_response::ErrorResponse),
        (status = 404, description = "A private contest you were not invited to", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn submit(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<SubmitBody>,
) -> Result<Json<Value>, AppError> {
    let submission_id = contests::submit(
        &state.db,
        id,
        auth.user_id,
        &body.deliverable_url,
        body.notes_md.as_deref(),
    )
    .await?;
    Ok(Json(build_response(
        json!({ "submission_id": submission_id }),
    )))
}

#[utoipa::path(
    get, path = "/api/users/me/contest-invitations", tag = "work",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
    operation_id = "contestsMyInvitations",
)]
pub async fn my_invitations(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let rows: Vec<Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object(
                    'contest_id', c.id, 'slug', c.slug, 'title', c.title,
                    'kind', c.kind, 'deadline', c.submissions_deadline,
                    'invited_at', i.invited_at, 'accepted_at', i.accepted_at
                )
           FROM contest_invitations i
           JOIN enterprise_contests c ON c.id = i.contest_id
          WHERE i.talent_user_id = $1 AND i.declined_at IS NULL
          ORDER BY i.invited_at DESC",
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(build_response(json!({ "invitations": rows }))))
}

// ═══════════════════════════════════════════════════════════════════
// Interviews
// ═══════════════════════════════════════════════════════════════════

#[utoipa::path(
    post, path = "/api/enterprise/interviews", tag = "enterprise",
    request_body(content = serde_json::Value, description = "ProposalInput"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "No usable slot, or an unknown source", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not acting for an enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn propose_interview(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<interviews::ProposalInput>,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
    let interview = interviews::propose(&state.db, enterprise.id, input).await?;
    Ok(Json(build_response(json!({ "interview": interview }))))
}

#[utoipa::path(
    post, path = "/api/enterprise/interviews/{id}/complete", tag = "enterprise",
    params(("id" = Uuid, Path, description = "Interview id")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Not a confirmed interview", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn complete(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
    let interview = interviews::by_id(&state.db, id).await?;
    if interview.enterprise_id != enterprise.id {
        return Err(AppError::NotFound("interview not found".into()));
    }
    interviews::complete(&state.db, id).await?;
    Ok(Json(build_response(json!({ "completed": true }))))
}

#[utoipa::path(
    get, path = "/api/users/me/interviews", tag = "work",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn my_interviews(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let list = interviews::for_talent(&state.db, auth.user_id).await?;
    Ok(Json(build_response(json!({ "interviews": list }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ConfirmBody {
    #[schema(value_type = Object)]
    pub slot: interviews::Slot,
}

/// The person picks a time.
#[utoipa::path(
    post, path = "/api/interviews/{id}/confirm", tag = "work",
    params(("id" = Uuid, Path, description = "Interview id")),
    request_body = ConfirmBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "A time that was not offered", body = crate::api_response::ErrorResponse),
        (status = 404, description = "Not your interview", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn confirm_interview(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ConfirmBody>,
) -> Result<Json<Value>, AppError> {
    let interview = interviews::confirm(&state.db, id, auth.user_id, body.slot).await?;
    Ok(Json(build_response(json!({ "interview": interview }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct DeclineBody {
    #[serde(default)]
    pub reason: Option<String>,
}

/// Declining is a first-class answer, not a silence.
#[utoipa::path(
    post, path = "/api/interviews/{id}/decline", tag = "work",
    params(("id" = Uuid, Path, description = "Interview id")),
    request_body = DeclineBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "No open invitation", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn decline_interview(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<DeclineBody>,
) -> Result<Json<Value>, AppError> {
    interviews::decline(&state.db, id, auth.user_id, body.reason.as_deref()).await?;
    Ok(Json(build_response(json!({ "declined": true }))))
}
