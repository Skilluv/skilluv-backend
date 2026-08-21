//! The ops domain: service objectives, incidents, cost work.

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
use crate::services::ops_practice;

pub fn ops_routes() -> Router<AppState> {
    Router::new()
        .route("/ops/reference", get(reference))
        .route("/users/{username}/ops-profile", get(ops_profile))
        .route("/ops/toolkit", get(toolkit))
        .route("/ops/mentors/for-me", get(ops_mentor_matches))
        .route("/ops/onboarding", post(complete_onboarding))
        .route("/ops/onboarding/skip", post(skip_onboarding))
        // Service objectives.
        .route(
            "/ops/objectives",
            get(my_objectives).post(declare_objective),
        )
        .route("/ops/objectives/{id}/close", post(close_objective))
        // Incidents.
        .route("/ops/incidents", get(my_incidents).post(open_incident))
        .route("/ops/incidents/{id}/resolve", post(resolve_incident))
        .route("/ops/incidents/{id}/actions", post(add_action))
        .route("/ops/incidents/{id}/postmortem", post(publish_postmortem))
        // Cost work.
        .route("/ops/cost-work", post(record_cost_work))
}

pub fn admin_ops_practice_routes() -> Router<AppState> {
    Router::new()
        .route("/admin/ops/objectives/{id}/verify", post(verify_objective))
        .route("/admin/ops/cost-work/{id}/verify", post(verify_cost_work))
        .route("/admin/ops/overdue-actions", get(overdue_actions))
        .route("/admin/ops/attestations/artefact", post(attest_artefact))
        .route("/admin/ops/attestations/featured", post(attest_featured))
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

/// The vocabulary of the domain, so a client does not hard-code it.
#[utoipa::path(
    get, path = "/api/ops/reference", tag = "work",
    responses((status = 200, body = serde_json::Value)),
)]
pub async fn reference(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let orientations: Vec<Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object(
                    'slug', slug, 'name', name, 'description', description,
                    'reviewer_group', reviewer_group, 'tags', tags
                )
           FROM orientations
          WHERE primary_domain = 'ops' AND is_curated AND NOT is_archived
          ORDER BY reviewer_group, name",
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(build_response(json!({
        "orientations": orientations,
        "reviewer_groups": ops_practice::REVIEWER_GROUPS,
        "artifact_subtypes": ops_practice::SUBTYPES,
        "severities": ops_practice::SEVERITIES,
    }))))
}

// ═══════════════════════════════════════════════════════════════════
// Service objectives
// ═══════════════════════════════════════════════════════════════════

#[utoipa::path(
    post, path = "/api/ops/objectives", tag = "work",
    request_body(content = serde_json::Value, description = "ObjectiveInput"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "An objective attached to nothing", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn declare_objective(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<ops_practice::ObjectiveInput>,
) -> Result<Json<Value>, AppError> {
    let objective = ops_practice::declare_objective(&state.db, auth.user_id, input).await?;
    Ok(Json(build_response(json!({ "objective": objective }))))
}

#[utoipa::path(
    get, path = "/api/ops/objectives", tag = "work",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn my_objectives(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let objectives = ops_practice::objectives_for(&state.db, auth.user_id).await?;
    Ok(Json(build_response(json!({ "objectives": objectives }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CloseBody {
    #[schema(value_type = String)]
    pub achieved_percent: BigDecimal,
    pub evidence_url: String,
}

/// Close a window with what actually happened.
#[utoipa::path(
    post, path = "/api/ops/objectives/{id}/close", tag = "work",
    params(("id" = Uuid, Path, description = "Objective id")),
    request_body = CloseBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "No source for the figure", body = crate::api_response::ErrorResponse),
        (status = 404, description = "Not an open objective of yours", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn close_objective(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<CloseBody>,
) -> Result<Json<Value>, AppError> {
    let (objective, met) = ops_practice::close_objective(
        &state.db,
        id,
        auth.user_id,
        body.achieved_percent,
        &body.evidence_url,
    )
    .await?;

    let consumed = objective
        .achieved_percent
        .as_ref()
        .and_then(|a| ops_practice::error_budget_consumed(&objective.target_percent, a));

    Ok(Json(build_response(json!({
        "objective": objective,
        "met": met,
        // How close a pass was. Uptime alone hides it.
        "error_budget_consumed_percent": consumed,
    }))))
}

#[utoipa::path(
    post, path = "/api/admin/ops/objectives/{id}/verify", tag = "admin",
    params(("id" = Uuid, Path, description = "Objective id")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "The window is not closed", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn verify_objective(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let attested = ops_practice::verify_objective(&state.db, id, auth.user_id).await?;
    Ok(Json(build_response(json!({
        "verified": true,
        "attestation_issued": attested,
    }))))
}

// ═══════════════════════════════════════════════════════════════════
// Incidents
// ═══════════════════════════════════════════════════════════════════

#[utoipa::path(
    post, path = "/api/ops/incidents", tag = "work",
    request_body(content = serde_json::Value, description = "IncidentInput"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Not a severity we record", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn open_incident(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<ops_practice::IncidentInput>,
) -> Result<Json<Value>, AppError> {
    let incident = ops_practice::open_incident(&state.db, auth.user_id, input).await?;
    Ok(Json(build_response(json!({ "incident": incident }))))
}

#[utoipa::path(
    get, path = "/api/ops/incidents", tag = "work",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn my_incidents(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let incidents = ops_practice::incidents_for(&state.db, auth.user_id).await?;
    Ok(Json(build_response(json!({ "incidents": incidents }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ResolveBody {
    #[serde(default)]
    pub time_to_detect_minutes: Option<i32>,
    #[serde(default)]
    pub time_to_resolve_minutes: Option<i32>,
}

#[utoipa::path(
    post, path = "/api/ops/incidents/{id}/resolve", tag = "work",
    params(("id" = Uuid, Path, description = "Incident id")),
    request_body = ResolveBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "Not an open incident of yours", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn resolve_incident(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ResolveBody>,
) -> Result<Json<Value>, AppError> {
    let incident = ops_practice::resolve_incident(
        &state.db,
        id,
        auth.user_id,
        body.time_to_detect_minutes,
        body.time_to_resolve_minutes,
    )
    .await?;
    Ok(Json(build_response(json!({ "incident": incident }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ActionBody {
    pub description: String,
    #[serde(default)]
    pub owner_user_id: Option<Uuid>,
    #[serde(default)]
    pub due_on: Option<chrono::NaiveDate>,
}

#[utoipa::path(
    post, path = "/api/ops/incidents/{id}/actions", tag = "work",
    params(("id" = Uuid, Path, description = "Incident id")),
    request_body = ActionBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "An empty action", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn add_action(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ActionBody>,
) -> Result<Json<Value>, AppError> {
    let action_id = ops_practice::add_action(
        &state.db,
        id,
        &body.description,
        body.owner_user_id,
        body.due_on,
    )
    .await?;
    Ok(Json(build_response(json!({ "action_id": action_id }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PostmortemBody {
    pub postmortem_md: String,
    #[serde(default)]
    pub url: Option<String>,
}

/// Publish the post-mortem.
#[utoipa::path(
    post, path = "/api/ops/incidents/{id}/postmortem", tag = "work",
    params(("id" = Uuid, Path, description = "Incident id")),
    request_body = PostmortemBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Too short, or no action items", body = crate::api_response::ErrorResponse),
        (status = 404, description = "Not a resolved incident of yours", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn publish_postmortem(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<PostmortemBody>,
) -> Result<Json<Value>, AppError> {
    ops_practice::publish_postmortem(
        &state.db,
        id,
        auth.user_id,
        &body.postmortem_md,
        body.url.as_deref(),
    )
    .await?;
    Ok(Json(build_response(json!({ "published": true }))))
}

/// What was promised in a post-mortem and is late.
#[utoipa::path(
    get, path = "/api/admin/ops/overdue-actions", tag = "admin",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn overdue_actions(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let overdue = ops_practice::overdue_actions(&state.db).await?;
    Ok(Json(build_response(json!({ "overdue": overdue }))))
}

// ═══════════════════════════════════════════════════════════════════
// Cost work
// ═══════════════════════════════════════════════════════════════════

#[utoipa::path(
    post, path = "/api/ops/cost-work", tag = "work",
    request_body(content = serde_json::Value, description = "CostInput"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "No explanation, nothing to attach to, or not a reduction", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn record_cost_work(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<ops_practice::CostInput>,
) -> Result<Json<Value>, AppError> {
    let work = ops_practice::record_cost_work(&state.db, auth.user_id, input).await?;

    Ok(Json(build_response(json!({
        "cost_work": work,
        "annual_saving": ops_practice::annual_saving(&work.monthly_before, &work.monthly_after),
        "reduction_percent":
            ops_practice::reduction_percent(&work.monthly_before, &work.monthly_after),
        "note": "La réduction n'est attestée qu'une fois vérifié que le service tient \
                 toujours son objectif.",
    }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CostVerdictBody {
    /// Whether the service still meets its objective. Verifying the saving
    /// alone would certify an outage with a spreadsheet.
    pub service_still_meets_slo: bool,
}

#[utoipa::path(
    post, path = "/api/admin/ops/cost-work/{id}/verify", tag = "admin",
    params(("id" = Uuid, Path, description = "Cost record id")),
    request_body = CostVerdictBody,
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn verify_cost_work(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<CostVerdictBody>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let attested =
        ops_practice::verify_cost_work(&state.db, id, auth.user_id, body.service_still_meets_slo)
            .await?;
    Ok(Json(build_response(json!({
        "verified": true,
        "attestation_issued": attested,
    }))))
}

#[derive(Deserialize, ToSchema)]
pub struct ArtefactAttestationBody {
    pub user_id: Uuid,
    /// `ops_infra_shipped`, `ops_observability_stack_shipped` or
    /// `ops_migration_completed`.
    pub basis: String,
    pub deliverable_id: Uuid,
    pub title: String,
    pub evidence_url: String,
}

/// Attest an ops artefact somebody delivered.
///
/// The basis and the artefact's subtype have to agree — a migration
/// attestation cannot be issued from a dashboard — which is checked in the
/// service rather than here, because it is a statement about the domain and
/// not about the request.
#[utoipa::path(
    post, path = "/api/admin/ops/attestations/artefact", tag = "admin",
    request_body = ArtefactAttestationBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Unknown basis, or an artefact the basis does not accept", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn attest_artefact(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<ArtefactAttestationBody>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    ops_practice::attest_artefact(
        &state.db,
        body.user_id,
        &body.basis,
        body.deliverable_id,
        &body.title,
        &body.evidence_url,
    )
    .await?;
    Ok(Json(build_response(json!({ "issued": true }))))
}

#[derive(Deserialize, ToSchema)]
pub struct FeaturedBody {
    pub user_id: Uuid,
    pub reason: String,
}

/// The community attestation, which rests on a decision rather than a file.
#[utoipa::path(
    post, path = "/api/admin/ops/attestations/featured", tag = "admin",
    request_body = FeaturedBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "No reason given", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn attest_featured(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<FeaturedBody>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    ops_practice::attest_featured(&state.db, body.user_id, &body.reason).await?;
    Ok(Json(build_response(json!({ "issued": true }))))
}

// ═══════════════════════════════════════════════════════════════════
// GET /users/{username}/ops-profile
// ═══════════════════════════════════════════════════════════════════

/// What one person has to show in the ops trades, and a score for it.
///
/// Derived on every call rather than read from a stored total. The backlog
/// asked for a `craft_score_ops` column; a column keeps the points of a
/// revoked attestation until somebody remembers to recompute, and this is a
/// platform that sells the opposite of that.
#[utoipa::path(
    get, path = "/api/users/{username}/ops-profile", tag = "profile",
    params(("username" = String, Path, description = "Username")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "No such person", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn ops_profile(
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> Result<Json<Value>, AppError> {
    let profile = crate::services::ops_profile::build(&state.db, &username).await?;
    Ok(Json(build_response(json!({ "profile": profile }))))
}

/// Ops mentors worth suggesting to the person asking, best first.
///
/// Each match carries the reasoning that produced it, so somebody who was
/// suggested a bad fit can say so — and so that "matched by an algorithm"
/// never has to be taken on trust.
#[utoipa::path(
    get, path = "/api/ops/mentors/for-me", tag = "work",
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "The ops onboarding has not been answered", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn ops_mentor_matches(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let matches = crate::services::ops_mentorship::matches_for(&state.db, auth.user_id, 10).await?;
    Ok(Json(build_response(json!({ "matches": matches }))))
}

// ═══════════════════════════════════════════════════════════════════
// Onboarding
// ═══════════════════════════════════════════════════════════════════

/// Six answers, and what to do first.
///
/// The recommendation is computed rather than stored: it is a function of the
/// answers, and storing it would produce a row that disagrees with the wizard
/// the day the advice improves.
#[utoipa::path(
    post, path = "/api/ops/onboarding", tag = "work",
    request_body = crate::services::ops_onboarding::WizardAnswers,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "An answer outside the vocabulary, or more than two trades", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
    operation_id = "opsPracticeCompleteOnboarding",
)]
pub async fn complete_onboarding(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(answers): Json<crate::services::ops_onboarding::WizardAnswers>,
) -> Result<Json<Value>, AppError> {
    let recommendation =
        crate::services::ops_onboarding::complete(&state.db, auth.user_id, &answers).await?;
    Ok(Json(build_response(
        json!({ "recommendation": recommendation }),
    )))
}

/// Stop asking.
#[utoipa::path(
    post, path = "/api/ops/onboarding/skip", tag = "work",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
    operation_id = "opsPracticeSkipOnboarding",
)]
pub async fn skip_onboarding(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    crate::services::ops_onboarding::skip(&state.db, auth.user_id).await?;
    Ok(Json(build_response(json!({ "skipped": true }))))
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct ToolkitQuery {
    pub category: Option<String>,
    pub orientation: Option<String>,
}

/// The curated ops toolkit, including where to practise without a budget.
///
/// The `access_note` on every row is the point of the list. A page that names
/// Terraform, Kubernetes and Datadog without saying what each one costs to
/// reach is a page that tells somebody in Cotonou the trade is not for them.
#[utoipa::path(
    get, path = "/api/ops/toolkit", tag = "work",
    params(ToolkitQuery),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Invalid filter", body = crate::api_response::ErrorResponse),
    ),
    operation_id = "opsPracticeToolkit",
)]
pub async fn toolkit(
    State(state): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<ToolkitQuery>,
) -> Result<Json<Value>, AppError> {
    crate::validators::check_max_len_opt(&q.category, "category", 20)?;
    crate::validators::check_max_len_opt(&q.orientation, "orientation", 60)?;

    let resources: Vec<Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object(
                    'slug', slug, 'display_name', display_name,
                    'category', category, 'url', url, 'summary', summary,
                    'access_note', access_note,
                    'orientation_slugs', orientation_slugs)
           FROM external_resources
          WHERE is_curated = TRUE AND domain = 'ops'
            AND ($1::TEXT IS NULL OR category = $1)
            -- A resource tagged for no trade serves every trade: excluding it
            -- would hide Docker from somebody who asked for the SRE toolkit.
            AND ($2::TEXT IS NULL
                 OR cardinality(orientation_slugs) = 0
                 OR $2 = ANY(orientation_slugs))
          ORDER BY category, sort_order, display_name",
    )
    .bind(q.category.as_deref())
    .bind(q.orientation.as_deref())
    .fetch_all(&state.db)
    .await?;

    Ok(Json(build_response(json!({ "resources": resources }))))
}
