//! One-off consultation, and audits of a client's own team.
//!
//! The audit routes have a shape worth noticing: the person assessed reaches
//! their own assessment through their own session, not through their
//! employer. They are the subject and not the customer, and the only way to
//! make that mean something is to give them a door of their own.

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
use crate::services::consultations;

pub fn consultation_routes() -> Router<AppState> {
    Router::new()
        // Client.
        .route(
            "/enterprise/consultations",
            get(my_consultations).post(request_consultation),
        )
        .route("/enterprise/consultations/{id}", get(read_consultation))
        .route("/enterprise/consultations/{id}/rate", post(rate))
        .route("/enterprise/skill-audits", post(open_audit))
        .route("/enterprise/skill-audits/{id}/readiness", get(readiness))
        // Experts.
        .route("/consultations/{id}/respond", post(respond))
        .route("/consultations/{id}/opinion", post(submit_opinion))
        // The people assessed, through their own door.
        .route("/users/me/assessments", get(my_assessments))
        .route("/assessments/{id}/response", post(respond_to_assessment))
}

pub fn admin_consultation_routes() -> Router<AppState> {
    Router::new()
        .route("/admin/consultations/{id}/invite", post(invite_expert))
        .route("/admin/consultations/{id}/deliver", post(deliver))
        .route("/admin/skill-audits/{id}/inform", post(inform_employee))
        .route("/admin/assessments/{id}", post(assess))
        .route("/admin/assessments/{id}/share", post(share_assessment))
        .route("/admin/skill-audits/{id}/deliver", post(deliver_audit))
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
// Consultations
// ═══════════════════════════════════════════════════════════════════

#[utoipa::path(
    post, path = "/api/enterprise/consultations", tag = "enterprise",
    request_body(content = serde_json::Value, description = "ConsultationInput"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "No stated question, or a shape the kind forbids", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not acting for an enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn request_consultation(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<consultations::ConsultationInput>,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
    let consultation =
        consultations::request(&state.db, enterprise.id, auth.user_id, input).await?;

    let product = if consultation.kind == "advisory" {
        "advisory_call"
    } else {
        "architecture_review"
    };
    let _ = sqlx::query(
        "INSERT INTO enterprise_products
            (enterprise_id, product_type, source_table, source_id,
             contract_value, currency, created_by)
         VALUES ($1, $2, 'consultations', $3, $4, $5, $6)",
    )
    .bind(enterprise.id)
    .bind(product)
    .bind(consultation.id)
    .bind(&consultation.fee)
    .bind(consultation.currency.as_str())
    .bind(auth.user_id)
    .execute(&state.db)
    .await;

    Ok(Json(build_response(
        json!({ "consultation": consultation }),
    )))
}

#[utoipa::path(
    get, path = "/api/enterprise/consultations", tag = "enterprise",
    responses(
        (status = 200, body = serde_json::Value),
        (status = 403, description = "Not acting for an enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn my_consultations(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
    let list = consultations::for_enterprise(&state.db, enterprise.id).await?;
    Ok(Json(build_response(json!({ "consultations": list }))))
}

#[utoipa::path(
    get, path = "/api/enterprise/consultations/{id}", tag = "enterprise",
    params(("id" = Uuid, Path, description = "Consultation id")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "Not yours", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn read_consultation(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
    let consultation = consultations::owned_by(&state.db, id, enterprise.id).await?;
    let experts = consultations::experts(&state.db, id).await?;
    Ok(Json(build_response(
        json!({ "consultation": consultation, "experts": experts }),
    )))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RatingBody {
    pub rating: i16,
}

#[utoipa::path(
    post, path = "/api/enterprise/consultations/{id}/rate", tag = "enterprise",
    params(("id" = Uuid, Path, description = "Consultation id")),
    request_body = RatingBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Nothing delivered yet, or a rating outside 1 to 5", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn rate(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<RatingBody>,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
    consultations::owned_by(&state.db, id, enterprise.id).await?;
    consultations::rate(&state.db, id, body.rating).await?;
    Ok(Json(build_response(json!({ "rated": body.rating }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct InviteBody {
    pub expert_user_id: Uuid,
}

#[utoipa::path(
    post, path = "/api/admin/consultations/{id}/invite", tag = "admin",
    params(("id" = Uuid, Path, description = "Consultation id")),
    request_body = InviteBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Below the expert rank floor", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn invite_expert(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<InviteBody>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    consultations::invite_expert(&state.db, id, body.expert_user_id).await?;
    let experts = consultations::experts(&state.db, id).await?;
    Ok(Json(build_response(json!({ "experts": experts }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RespondBody {
    pub accept: bool,
    #[serde(default)]
    pub reason: Option<String>,
}

/// The expert answers for themselves.
#[utoipa::path(
    post, path = "/api/consultations/{id}/respond", tag = "work",
    params(("id" = Uuid, Path, description = "Consultation id")),
    request_body = RespondBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "Not invited", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn respond(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<RespondBody>,
) -> Result<Json<Value>, AppError> {
    consultations::respond(
        &state.db,
        id,
        auth.user_id,
        body.accept,
        body.reason.as_deref(),
    )
    .await?;

    // What they would receive, shown after accepting so it is on the record
    // rather than in a message somewhere.
    let consultation = consultations::consultation(&state.db, id).await?;
    let expected = consultations::expected_share(
        &consultation.fee,
        &consultation.commission_percent,
        consultation.reviewers_wanted.unwrap_or(1).max(1) as usize,
    );

    Ok(Json(build_response(json!({
        "accepted": body.accept,
        "expected_share": expected,
        "note": "La part réelle se répartit entre les personnes qui rendent \
                 effectivement un avis.",
    }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct OpinionBody {
    pub comment_md: String,
    #[serde(default)]
    pub verdict: Option<String>,
}

#[utoipa::path(
    post, path = "/api/consultations/{id}/opinion", tag = "work",
    params(("id" = Uuid, Path, description = "Consultation id")),
    request_body = OpinionBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Too short, or not a verdict", body = crate::api_response::ErrorResponse),
        (status = 404, description = "You have not accepted this one", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn submit_opinion(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<OpinionBody>,
) -> Result<Json<Value>, AppError> {
    consultations::submit_opinion(
        &state.db,
        id,
        auth.user_id,
        &body.comment_md,
        body.verdict.as_deref(),
    )
    .await?;
    Ok(Json(build_response(json!({ "submitted": true }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct DeliverBody {
    #[serde(default)]
    pub synthesis_md: Option<String>,
}

#[utoipa::path(
    post, path = "/api/admin/consultations/{id}/deliver", tag = "admin",
    params(("id" = Uuid, Path, description = "Consultation id")),
    request_body = DeliverBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Nobody has written anything, or a review with no synthesis", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn deliver(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<DeliverBody>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let (commission, paid) =
        consultations::deliver(&state.db, id, body.synthesis_md.as_deref()).await?;
    Ok(Json(build_response(
        json!({ "commission": commission, "experts_paid": paid }),
    )))
}

// ═══════════════════════════════════════════════════════════════════
// Skill audits
// ═══════════════════════════════════════════════════════════════════

#[utoipa::path(
    post, path = "/api/enterprise/skill-audits", tag = "enterprise",
    request_body(content = serde_json::Value, description = "AuditInput"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "No stated purpose, or no domains", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not acting for an enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn open_audit(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<consultations::AuditInput>,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
    let audit = consultations::open_audit(&state.db, enterprise.id, auth.user_id, input).await?;

    Ok(Json(build_response(json!({
        "audit": audit,
        // Said at the moment of ordering, because a client who learns it at
        // delivery has already made plans.
        "note": "Chaque personne évaluée est informée avant l'évaluation et reçoit ce \
                 qui a été écrit sur elle. L'audit ne peut pas être livré tant que ce \
                 n'est pas le cas.",
    }))))
}

/// How far an audit is from being deliverable.
#[utoipa::path(
    get, path = "/api/enterprise/skill-audits/{id}/readiness", tag = "enterprise",
    params(("id" = Uuid, Path, description = "Audit id")),
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn readiness(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
    let audit = consultations::audit(&state.db, id).await?;
    if audit.enterprise_id != enterprise.id {
        return Err(AppError::NotFound("audit not found".into()));
    }

    let (informed, assessed, shared) = consultations::audit_readiness(&state.db, id).await?;
    Ok(Json(build_response(json!({
        "informed": informed,
        "assessed": assessed,
        "shared_with_the_person": shared,
        "deliverable": assessed > 0 && shared == assessed,
    }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct InformBody {
    pub employee_email: String,
    pub orientation_slug: String,
    #[serde(default)]
    pub employee_name: Option<String>,
}

/// Tell somebody they are being assessed, before assessing them.
#[utoipa::path(
    post, path = "/api/admin/skill-audits/{id}/inform", tag = "admin",
    params(("id" = Uuid, Path, description = "Audit id")),
    request_body = InformBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Not an email", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn inform_employee(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<InformBody>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let assessment_id = consultations::inform_employee(
        &state.db,
        id,
        &body.employee_email,
        &body.orientation_slug,
        body.employee_name.as_deref(),
    )
    .await?;
    Ok(Json(build_response(
        json!({ "assessment_id": assessment_id }),
    )))
}

#[utoipa::path(
    post, path = "/api/admin/assessments/{id}", tag = "admin",
    params(("id" = Uuid, Path, description = "Assessment id")),
    request_body(content = serde_json::Value, description = "AssessmentInput"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Not a level, or the person has not been told", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn assess(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<consultations::AssessmentInput>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    consultations::assess(&state.db, id, auth.user_id, input).await?;
    Ok(Json(build_response(json!({ "assessed": true }))))
}

#[utoipa::path(
    post, path = "/api/admin/assessments/{id}/share", tag = "admin",
    params(("id" = Uuid, Path, description = "Assessment id")),
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn share_assessment(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    consultations::share_with_employee(&state.db, id).await?;
    Ok(Json(build_response(json!({ "shared": true }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AuditDeliveryBody {
    pub matrix_url: String,
    #[serde(default)]
    pub recommendations_md: Option<String>,
}

#[utoipa::path(
    post, path = "/api/admin/skill-audits/{id}/deliver", tag = "admin",
    params(("id" = Uuid, Path, description = "Audit id")),
    request_body = AuditDeliveryBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Somebody assessed has not seen their assessment", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn deliver_audit(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<AuditDeliveryBody>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let booked = consultations::deliver_audit(
        &state.db,
        id,
        &body.matrix_url,
        body.recommendations_md.as_deref(),
    )
    .await?;
    Ok(Json(build_response(json!({ "revenue_booked": booked }))))
}

/// What was written about you, through your own session.
#[utoipa::path(
    get, path = "/api/users/me/assessments", tag = "profile",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn my_assessments(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let assessments = consultations::assessments_for_user(&state.db, auth.user_id).await?;
    Ok(Json(build_response(json!({ "assessments": assessments }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AssessmentResponseBody {
    pub response_md: String,
}

/// The right of reply. A conclusion with none is a verdict.
#[utoipa::path(
    post, path = "/api/assessments/{id}/response", tag = "profile",
    params(("id" = Uuid, Path, description = "Assessment id")),
    request_body = AssessmentResponseBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "No assessment of yours has been shared here", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn respond_to_assessment(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<AssessmentResponseBody>,
) -> Result<Json<Value>, AppError> {
    consultations::respond_to_assessment(&state.db, id, auth.user_id, &body.response_md).await?;
    Ok(Json(build_response(json!({ "recorded": true }))))
}
