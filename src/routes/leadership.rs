//! The leadership domain: redaction, retrospectives, coordination, cohorts.
//!
//! ## Who is allowed to do what
//!
//! Three different answers, and the difference is the design.
//!
//! **Reviewing** an artefact is guarded by the trade behind the slice —
//! `leadership_reviewer:{reviewer_group}`, derived by migration 0404's
//! trigger. Somebody who can read a delivery plan cannot necessarily read a
//! curriculum.
//!
//! **Acknowledging** a commitment is guarded by nothing at all except not
//! being the author. It is a person saying "yes, my project agreed to that",
//! and a capability gate on it would mean a plan could be agreed only by
//! people the platform had already promoted — which is not what agreement is.
//!
//! **Confirming a redaction** is guarded by any leadership review capability,
//! and it is the strictest thing here: what is being confirmed is that nobody
//! in a document is identifiable, on behalf of people who are not on this
//! platform and did not ask to be written about.

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::{leadership_practice, leadership_profile};

pub fn leadership_routes() -> Router<AppState> {
    Router::new()
        .route("/leadership/reference", get(reference))
        .route("/users/{username}/leadership-profile", get(profile))
        // Redaction and adoption, on an artefact.
        .route(
            "/leadership/slices/{id}/redaction/declare",
            post(declare_redaction),
        )
        .route(
            "/leadership/slices/{id}/redaction/confirm",
            post(confirm_redaction),
        )
        .route("/leadership/slices/{id}/adoption", post(record_adoption))
        // Coordination.
        .route(
            "/leadership/slices/{id}/links",
            get(links).post(link_project),
        )
        .route("/leadership/links/{id}/acknowledge", post(acknowledge_link))
        // Retrospectives.
        .route(
            "/leadership/retrospectives",
            get(my_retrospectives).post(record_retrospective),
        )
        .route(
            "/leadership/retrospectives/{id}/actions",
            get(actions).post(add_action),
        )
        .route("/leadership/actions/{id}/resolve", post(resolve_action))
        // Cohorts.
        .route("/leadership/cohorts/{id}/lead", post(lead_cohort))
        .route("/leadership/cohorts/{id}/graduate", post(graduate_member))
        .route("/leadership/cohorts/{id}/departure", post(record_departure))
        .route("/leadership/cohorts/{id}/conclude", post(conclude_cohort))
        .route("/leadership/cohorts/{id}/outcomes", get(cohort_outcomes))
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
    get, path = "/api/leadership/reference", tag = "work",
    responses((status = 200, body = serde_json::Value)),
)]
pub async fn reference(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let orientations: Vec<Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object(
                    'slug', slug, 'name', name, 'description', description,
                    'reviewer_group', reviewer_group, 'tags', tags,
                    'secondary_domains', secondary_domains
                )
           FROM orientations
          WHERE primary_domain = 'leadership' AND is_curated AND NOT is_archived
          ORDER BY reviewer_group, name",
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(build_response(json!({
        "orientations": orientations,
        "reviewer_groups": leadership_practice::REVIEWER_GROUPS,
        "artifact_subtypes": leadership_practice::SUBTYPES,
        "redaction_states": leadership_practice::REDACTION_STATES,
        "retrospective_formats": leadership_practice::RETRO_FORMATS,
        "link_kinds": leadership_practice::LINK_KINDS,
        "cohort_leave_reasons": leadership_practice::LEAVE_REASONS,
    }))))
}

/// The public leadership profile.
#[utoipa::path(
    get, path = "/api/users/{username}/leadership-profile", tag = "profile",
    params(("username" = String, Path, description = "Username")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "No such person", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn profile(
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> Result<Json<Value>, AppError> {
    let profile = leadership_profile::build(&state.db, &username).await?;
    Ok(Json(build_response(json!({ "profile": profile }))))
}

/// Holding any one of the leadership review capabilities.
///
/// Built from `REVIEWER_GROUPS` rather than written out, so a sixth family
/// added to the catalogue reaches this guard without anybody remembering to
/// edit it.
async fn require_any_leadership_reviewer(
    state: &AppState,
    auth: &AuthUser,
) -> Result<(), AppError> {
    let mut caps: Vec<String> = leadership_practice::REVIEWER_GROUPS
        .iter()
        .map(|g| format!("leadership_reviewer:{g}"))
        .collect();
    caps.push("leadership_reviewer:all".to_string());
    let refs: Vec<&str> = caps.iter().map(String::as_str).collect();

    crate::middleware::capabilities::require_any_capability(&state.db, auth.user_id, &refs).await
}

// ═══════════════════════════════════════════════════════════════════
// Redaction and adoption
// ═══════════════════════════════════════════════════════════════════

/// The author saying the document has been rewritten so nobody is
/// identifiable.
#[utoipa::path(
    post, path = "/api/leadership/slices/{id}/redaction/declare", tag = "work",
    params(("id" = Uuid, Path, description = "Slice id")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Not an anonymised artefact of yours", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn declare_redaction(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    leadership_practice::declare_redaction(&state.db, auth.user_id, id).await?;
    Ok(Json(build_response(json!({ "declared": true }))))
}

/// A reviewer saying they have read it and nobody in it is identifiable.
///
/// The strictest confirmation on the platform, and the one thing that blocks
/// publication outright. It is done on behalf of people who are not here.
#[utoipa::path(
    post, path = "/api/leadership/slices/{id}/redaction/confirm", tag = "work",
    params(("id" = Uuid, Path, description = "Slice id")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 403, description = "No leadership review capability", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn confirm_redaction(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    require_any_leadership_reviewer(&state, &auth).await?;
    leadership_practice::confirm_redaction(&state.db, auth.user_id, id).await?;

    // The confirmation is what the attestation was waiting for. Best-effort:
    // the confirmation stands either way, and the sweep picks up the rest.
    if let Some(author) = slice_author(&state, id).await
        && let Err(e) =
            crate::services::proof_hooks::recompute_all_for_user(&state.db, author).await
    {
        tracing::warn!(
            user_id = %author, error = %e,
            "redaction confirmed but the proof recompute did not run"
        );
    }

    Ok(Json(build_response(json!({ "confirmed": true }))))
}

/// Who holds a slice, for recomputing their proof after somebody else's act.
async fn slice_author(state: &AppState, slice_id: Uuid) -> Option<Uuid> {
    sqlx::query_scalar(
        "SELECT COALESCE(claimed_by_user_id, created_by_user_id)
           FROM project_slices WHERE id = $1",
    )
    .bind(slice_id)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten()
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct AdoptionBody {
    /// Where the adoption can be seen. Required unless the artefact is
    /// confidential, in which case there is nothing public to point at.
    #[serde(default)]
    pub evidence_url: Option<String>,
}

/// Record that an organisation took the proposal up.
#[utoipa::path(
    post, path = "/api/leadership/slices/{id}/adoption", tag = "work",
    params(("id" = Uuid, Path, description = "Slice id")),
    request_body = AdoptionBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Not a written decision of yours, or no evidence", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn record_adoption(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<AdoptionBody>,
) -> Result<Json<Value>, AppError> {
    leadership_practice::record_adoption(&state.db, auth.user_id, id, body.evidence_url.as_deref())
        .await?;

    if let Err(e) =
        crate::services::proof_hooks::recompute_all_for_user(&state.db, auth.user_id).await
    {
        tracing::warn!(
            user_id = %auth.user_id, error = %e,
            "adoption recorded but the proof recompute did not run"
        );
    }

    Ok(Json(build_response(json!({ "adopted": true }))))
}

// ═══════════════════════════════════════════════════════════════════
// Coordination
// ═══════════════════════════════════════════════════════════════════

#[utoipa::path(
    post, path = "/api/leadership/slices/{id}/links", tag = "work",
    params(("id" = Uuid, Path, description = "Slice id")),
    request_body = leadership_practice::LinkInput,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "A commitment with nothing written down", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn link_project(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<leadership_practice::LinkInput>,
) -> Result<Json<Value>, AppError> {
    let link = leadership_practice::link_project(&state.db, auth.user_id, id, input).await?;
    Ok(Json(build_response(json!({ "link": link }))))
}

#[utoipa::path(
    get, path = "/api/leadership/slices/{id}/links", tag = "work",
    params(("id" = Uuid, Path, description = "Slice id")),
    responses((status = 200, body = serde_json::Value)),
)]
pub async fn links(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let reach = leadership_practice::coordination_reach(&state.db, id).await?;
    Ok(Json(build_response(json!({ "reach": reach }))))
}

/// A project's steward accepting what a document commits them to.
///
/// Guarded by nothing but not being the author. Agreement is not a capability.
#[utoipa::path(
    post, path = "/api/leadership/links/{id}/acknowledge", tag = "work",
    params(("id" = Uuid, Path, description = "Link id")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Not a commitment somebody else's document makes", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn acknowledge_link(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let link = leadership_practice::acknowledge_link(&state.db, auth.user_id, id).await?;

    // The acknowledgement is a term in the author's score, not in the
    // steward's — so the recompute is for them.
    if let Some(author) = slice_author(&state, link.leadership_slice_id).await
        && let Err(e) =
            crate::services::proof_hooks::recompute_all_for_user(&state.db, author).await
    {
        tracing::warn!(
            user_id = %author, error = %e,
            "commitment acknowledged but the proof recompute did not run"
        );
    }

    Ok(Json(build_response(json!({ "link": link }))))
}

// ═══════════════════════════════════════════════════════════════════
// Retrospectives
// ═══════════════════════════════════════════════════════════════════

#[utoipa::path(
    post, path = "/api/leadership/retrospectives", tag = "work",
    request_body = leadership_practice::RetrospectiveInput,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "A meeting rather than a retrospective", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn record_retrospective(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<leadership_practice::RetrospectiveInput>,
) -> Result<Json<Value>, AppError> {
    let retro = leadership_practice::record_retrospective(&state.db, auth.user_id, input).await?;
    Ok(Json(build_response(json!({ "retrospective": retro }))))
}

#[utoipa::path(
    get, path = "/api/leadership/retrospectives", tag = "work",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn my_retrospectives(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let retros = leadership_practice::retrospectives_for(&state.db, auth.user_id).await?;
    Ok(Json(build_response(json!({ "retrospectives": retros }))))
}

#[utoipa::path(
    post, path = "/api/leadership/retrospectives/{id}/actions", tag = "work",
    params(("id" = Uuid, Path, description = "Retrospective id")),
    request_body = leadership_practice::ActionInput,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "An action with nobody on it", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn add_action(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<leadership_practice::ActionInput>,
) -> Result<Json<Value>, AppError> {
    let action = leadership_practice::add_action(&state.db, auth.user_id, id, input).await?;
    Ok(Json(build_response(json!({ "action": action }))))
}

#[utoipa::path(
    get, path = "/api/leadership/retrospectives/{id}/actions", tag = "work",
    params(("id" = Uuid, Path, description = "Retrospective id")),
    responses((status = 200, body = serde_json::Value)),
)]
pub async fn actions(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let actions = leadership_practice::actions_for(&state.db, id).await?;
    let followthrough = leadership_practice::followthrough_for(&state.db, id).await?;
    Ok(Json(build_response(json!({
        "actions": actions,
        "followthrough": followthrough,
    }))))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ResolveBody {
    /// Present to drop the action rather than close it. Dropping is not a
    /// lesser outcome — the follow-through counts it as resolved — but it
    /// says why.
    #[serde(default)]
    pub abandoned_reason: Option<String>,
}

#[utoipa::path(
    post, path = "/api/leadership/actions/{id}/resolve", tag = "work",
    params(("id" = Uuid, Path, description = "Action id")),
    request_body = ResolveBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "Not an action on a retrospective of yours", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn resolve_action(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ResolveBody>,
) -> Result<Json<Value>, AppError> {
    let action = leadership_practice::resolve_action(
        &state.db,
        auth.user_id,
        id,
        body.abandoned_reason.as_deref(),
    )
    .await?;

    // Closing the last action can be what turns a facilitated hour into an
    // attestation.
    if let Err(e) =
        crate::services::proof_hooks::recompute_all_for_user(&state.db, auth.user_id).await
    {
        tracing::warn!(
            user_id = %auth.user_id, error = %e,
            "action resolved but the proof recompute did not run"
        );
    }

    Ok(Json(build_response(json!({ "action": action }))))
}

// ═══════════════════════════════════════════════════════════════════
// Cohorts
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct LeadBody {
    #[serde(default)]
    pub curriculum_slice_id: Option<Uuid>,
    #[serde(default)]
    pub target_domain: Option<String>,
}

#[utoipa::path(
    post, path = "/api/leadership/cohorts/{id}/lead", tag = "work",
    params(("id" = Uuid, Path, description = "Cohort id")),
    request_body = LeadBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Not a cohort you can lead", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn lead_cohort(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<LeadBody>,
) -> Result<Json<Value>, AppError> {
    leadership_practice::lead_cohort(
        &state.db,
        auth.user_id,
        id,
        body.curriculum_slice_id,
        body.target_domain.as_deref(),
    )
    .await?;
    Ok(Json(build_response(json!({ "leading": true }))))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct MemberBody {
    pub member_id: Uuid,
}

#[utoipa::path(
    post, path = "/api/leadership/cohorts/{id}/graduate", tag = "work",
    params(("id" = Uuid, Path, description = "Cohort id")),
    request_body = MemberBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Not a member of a cohort you lead", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn graduate_member(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<MemberBody>,
) -> Result<Json<Value>, AppError> {
    leadership_practice::graduate_member(&state.db, auth.user_id, id, body.member_id).await?;
    Ok(Json(build_response(json!({ "graduated": true }))))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct DepartureBody {
    pub member_id: Uuid,
    pub reason: String,
    #[serde(default)]
    pub note: Option<String>,
}

#[utoipa::path(
    post, path = "/api/leadership/cohorts/{id}/departure", tag = "work",
    params(("id" = Uuid, Path, description = "Cohort id")),
    request_body = DepartureBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "A departure with no reason", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn record_departure(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<DepartureBody>,
) -> Result<Json<Value>, AppError> {
    leadership_practice::record_departure(
        &state.db,
        auth.user_id,
        id,
        body.member_id,
        &body.reason,
        body.note.as_deref(),
    )
    .await?;
    Ok(Json(build_response(json!({ "recorded": true }))))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ConcludeBody {
    #[serde(default)]
    pub note: Option<String>,
}

/// Bring a cohort to its end, and get the numbers back.
#[utoipa::path(
    post, path = "/api/leadership/cohorts/{id}/conclude", tag = "work",
    params(("id" = Uuid, Path, description = "Cohort id")),
    request_body = ConcludeBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Not a cohort you lead", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn conclude_cohort(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ConcludeBody>,
) -> Result<Json<Value>, AppError> {
    let outcomes =
        leadership_practice::conclude_cohort(&state.db, auth.user_id, id, body.note.as_deref())
            .await?;

    if let Err(e) =
        crate::services::proof_hooks::recompute_all_for_user(&state.db, auth.user_id).await
    {
        tracing::warn!(
            user_id = %auth.user_id, error = %e,
            "cohort concluded but the proof recompute did not run"
        );
    }

    Ok(Json(build_response(json!({ "outcomes": outcomes }))))
}

/// The numbers behind a cohort, with the denominator.
#[utoipa::path(
    get, path = "/api/leadership/cohorts/{id}/outcomes", tag = "work",
    params(("id" = Uuid, Path, description = "Cohort id")),
    responses((status = 200, body = serde_json::Value)),
)]
pub async fn cohort_outcomes(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let outcomes = leadership_practice::outcomes_for_cohort(&state.db, id).await?;
    Ok(Json(build_response(json!({ "outcomes": outcomes }))))
}
