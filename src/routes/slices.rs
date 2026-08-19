//! Routes HTTP pour les `project_slices` (Phase P1).
//!
//! Endpoints publics :
//!   GET   /api/slices                    — liste des slices open (filtres domain/difficulty/project)
//!   GET   /api/slices/{id}               — détail d'une slice
//!   POST  /api/slices/{id}/claim         — claim une slice (soft-lock 7j)
//!   POST  /api/slices/{id}/unclaim       — relâche sa slice
//!   GET   /api/users/me/slices           — mes slices actives (claimed / in_review)
//!
//! Voir docs/challenges-target-model-and-roadmap.md partie G.1 et H pour
//! les workflows amont/aval (vérification via webhook, review humaine).

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::{SlicesListFilter, SlicesService};

pub fn slice_routes() -> Router<AppState> {
    Router::new()
        .route("/slices", get(list_open))
        .route("/slices/{id}", get(get_slice))
        .route("/slices/{id}/claim", post(claim_slice))
        .route("/slices/{id}/unclaim", post(unclaim_slice))
        .route("/slices/{id}/submit-pr", post(submit_pr))
        .route("/slices/{id}/claim-as-team", post(claim_slice_as_team))
        .route("/slices/{id}/unclaim-team", post(unclaim_slice_by_team))
        .route("/users/me/slices", get(my_slices))
        .route("/teams/{team_id}/slices", get(team_slices))
        // P11.4 — steward inbox : validation des drafts ingérés
        .route("/stewards/{project_id}/inbox", get(steward_inbox))
        .route("/slices/{id}/publish", post(publish_slice))
        .route("/slices/{id}/reject", post(reject_slice))
}

// ═══════════════════════════════════════════════════════════════════
// Query / body types
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct ListQuery {
    #[param(pattern = r"^(code|design|game|security|ops|ai|soft_skills)$")]
    domain: Option<String>,
    #[param(minimum = 1, maximum = 5)]
    difficulty: Option<i16>,
    project_id: Option<Uuid>,
    #[param(minimum = 1, maximum = 100000)]
    page: Option<i64>,
    #[param(minimum = 1, maximum = 100)]
    per_page: Option<i64>,
}

impl From<ListQuery> for SlicesListFilter {
    fn from(q: ListQuery) -> Self {
        Self {
            domain: q.domain,
            difficulty: q.difficulty,
            project_id: q.project_id,
            page: q.page.unwrap_or(1),
            per_page: q.per_page.unwrap_or(20),
        }
    }
}

fn build_response(data: serde_json::Value) -> serde_json::Value {
    json!({
        "data": data,
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })
}

// ═══════════════════════════════════════════════════════════════════
// Handlers
// ═══════════════════════════════════════════════════════════════════

/// GET /api/slices
///
/// Liste paginée des slices `status='open'`. Public (pas d'auth requise) pour que
/// les visiteurs découvrent l'offre. Trié par difficulty ASC, created_at DESC.
/// Paginated open slices. Filter on domain / difficulty / project.
/// Public — no auth required.
#[utoipa::path(
    get,
    path = "/api/slices",
    tag = "projects",
    params(ListQuery),
    responses((status = 200, body = serde_json::Value)),
    operation_id = "slicesListOpen",
)]
pub async fn list_open(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    crate::validators::check_skill_domain_opt(&query.domain, "domain")?;
    crate::validators::check_range_opt(query.difficulty.map(i64::from), "difficulty", 1, 5)?;
    crate::validators::check_range_opt(query.page, "page", 1, 100_000)?;
    crate::validators::check_range_opt(query.per_page, "per_page", 1, 100)?;
    let filter: SlicesListFilter = query.into();
    let per_page = filter.per_page.clamp(1, 100);
    let page = filter.page.max(1);

    let (slices, total) = SlicesService::list_open(&state.db, &filter).await?;

    let total_pages = if per_page > 0 {
        (total as f64 / per_page as f64).ceil() as i64
    } else {
        0
    };

    Ok(Json(json!({
        "data": slices,
        "pagination": {
            "page": page,
            "per_page": per_page,
            "total": total,
            "total_pages": total_pages,
        },
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })))
}

/// GET /api/slices/{id}
///
/// Détail public d'une slice (peu importe son status — le status est dans la réponse).
/// Public slice detail.
#[utoipa::path(
    get,
    path = "/api/slices/{id}",
    tag = "projects",
    params(("id" = Uuid, Path)),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn get_slice(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let slice = SlicesService::get(&state.db, id).await?;
    Ok(Json(build_response(json!({ "slice": slice }))))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SubmitPrBody {
    /// Canonical GitHub PR URL — validated by the service layer.
    pub pr_url: String,
    /// P26 v2 SKI-119 — when true and the user has connected GitHub OAuth,
    /// posts a Skilluv attribution comment on the PR (as the user, not
    /// the bot). Best-effort: a POST failure does not roll back the
    /// submission itself. Default false — opt-in on purpose.
    #[serde(default)]
    pub announce_publicly: bool,
}

/// POST /api/slices/{id}/submit-pr
///
/// Challenger declares the PR they've opened against the target repo.
/// Transitions the slice from `claimed`/`in_progress` to `submitted`.
#[utoipa::path(
    post,
    path = "/api/slices/{id}/submit-pr",
    tag = "projects",
    request_body = SubmitPrBody,
    responses(
        (status = 200, description = "PR recorded, status advanced to submitted"),
        (status = 400, description = "Malformed PR URL or slice not in a submittable state"),
        (status = 401, description = "Unauthenticated"),
    ),
    operation_id = "slicesSubmitPr",
)]
pub async fn submit_pr(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<SubmitPrBody>,
) -> Result<impl IntoResponse, AppError> {
    let slice = SlicesService::submit_pr(
        &state.db,
        &state.config.jwt_secret,
        id,
        auth.user_id,
        &body.pr_url,
        body.announce_publicly,
    )
    .await?;
    Ok((
        StatusCode::OK,
        Json(build_response(json!({
            "slice": slice,
            "message": "PR recorded. Waiting for CI signal and validator pickup."
        }))),
    ))
}

/// POST /api/slices/{id}/claim
///
/// Auth requis. Le user claim la slice pour 7 jours.
/// Claim a slice for 7 days (individual claim).
#[utoipa::path(
    post,
    path = "/api/slices/{id}/claim",
    tag = "projects",
    params(("id" = Uuid, Path)),
    responses(
        (status = 201, body = serde_json::Value),
        (status = 400, body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn claim_slice(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let slice = SlicesService::claim(&state.db, id, auth.user_id).await?;

    // P26 v2 SKI-75 — best-effort auto-fork. Runs after the claim so a fork
    // failure never blocks the claim itself. Any error is logged as warn
    // and the slice is returned with `fork_repo_url = NULL`; the user can
    // then declare their fork manually via submit-pr (SKI-76).
    let slice = try_auto_fork(&state, &slice, auth.user_id)
        .await
        .unwrap_or(slice);

    Ok((
        StatusCode::CREATED,
        Json(build_response(json!({
            "slice": slice,
            "message": "Slice claimed. You have 7 days to complete it."
        }))),
    ))
}

/// Attempt to fork the target GitHub repo to the user's account and record
/// the URL on the slice. Returns `None` on any failure (missing GH
/// connection, unknown target repo, upstream error) — the caller keeps the
/// original slice and the user completes the flow manually.
async fn try_auto_fork(
    state: &AppState,
    slice: &crate::models::ProjectSlice,
    user_id: Uuid,
) -> Option<crate::models::ProjectSlice> {
    if slice.slice_type != "github_issue" {
        return None;
    }
    let target: Option<(Option<String>, Option<String>)> =
        sqlx::query_as("SELECT github_repo_owner, github_repo_name FROM projects WHERE id = $1")
            .bind(slice.project_id)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten();
    let (Some(owner), Some(repo)) = target.unwrap_or((None, None)) else {
        return None;
    };

    let token =
        match crate::services::github::load_token(&state.db, &state.config.jwt_secret, user_id)
            .await
        {
            Ok(Some(t)) => t,
            _ => return None, // user hasn't connected GitHub — silent no-op
        };

    match crate::services::github::fork_repo_for_user(&token, &owner, &repo).await {
        Ok(fork_url) => sqlx::query_as::<_, crate::models::ProjectSlice>(
            r#"
                UPDATE project_slices
                   SET fork_repo_url = $2, fork_created_at = NOW(), updated_at = NOW()
                 WHERE id = $1
             RETURNING *
                "#,
        )
        .bind(slice.id)
        .bind(&fork_url)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten(),
        Err(e) => {
            tracing::warn!(
                slice_id = %slice.id, user_id = %user_id, error = %e,
                "SKI-75 auto-fork failed — user will need to fork manually"
            );
            None
        }
    }
}

/// POST /api/slices/{id}/unclaim
///
/// Auth requis. Le user relâche sa slice (retour au pool `open`).
/// Release a slice back to the open pool.
#[utoipa::path(
    post,
    path = "/api/slices/{id}/unclaim",
    tag = "projects",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn unclaim_slice(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let slice = SlicesService::unclaim(&state.db, id, auth.user_id).await?;

    Ok(Json(build_response(json!({
        "slice": slice,
        "message": "Slice released. Others can now claim it."
    }))))
}

/// GET /api/users/me/slices
///
/// Auth requis. Liste des slices claimed/in_review par le user courant.
/// List slices claimed / in-review by the caller.
#[utoipa::path(
    get,
    path = "/api/users/me/slices",
    tag = "projects",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn my_slices(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, AppError> {
    let slices = SlicesService::list_claimed_by(&state.db, auth.user_id).await?;

    Ok(Json(build_response(json!({ "slices": slices }))))
}

// ═══════════════════════════════════════════════════════════════════
// P10.1 : claim collectif par une team persistente
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
pub struct ClaimAsTeamBody {
    team_id: Uuid,
}

/// Vérifie que le user est membre de la team (best-effort validation).
pub async fn require_team_member(
    db: &sqlx::PgPool,
    team_id: Uuid,
    user_id: Uuid,
) -> Result<(), AppError> {
    let is_member: Option<(Uuid,)> =
        sqlx::query_as("SELECT user_id FROM team_members WHERE team_id = $1 AND user_id = $2")
            .bind(team_id)
            .bind(user_id)
            .fetch_optional(db)
            .await?;
    if is_member.is_none() {
        return Err(AppError::Forbidden);
    }
    Ok(())
}

/// POST /api/slices/{id}/claim-as-team
///
/// Auth requis + être membre de la team. Claim collectif 7 jours.
/// Team claim (7-day collective lock).
#[utoipa::path(
    post,
    path = "/api/slices/{id}/claim-as-team",
    tag = "projects",
    params(("id" = Uuid, Path)),
    request_body = ClaimAsTeamBody,
    responses(
        (status = 201, body = serde_json::Value),
        (status = 403, body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn claim_slice_as_team(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ClaimAsTeamBody>,
) -> Result<impl IntoResponse, AppError> {
    require_team_member(&state.db, body.team_id, auth.user_id).await?;
    // P26 v2 SKI-79 / SKI-78: the requester (as team member) must clear
    // the orientation and rank gates on the slice — same rule as solo claim.
    SlicesService::assert_orientation_access(&state.db, id, auth.user_id).await?;
    SlicesService::assert_rank_access(&state.db, id, auth.user_id).await?;
    let slice = SlicesService::claim_as_team(&state.db, id, body.team_id).await?;
    Ok((
        StatusCode::CREATED,
        Json(build_response(json!({
            "slice": slice,
            "message": "Slice claimed by team. 7 days to submit a deliverable."
        }))),
    ))
}

/// POST /api/slices/{id}/unclaim-team
/// Team release.
#[utoipa::path(
    post,
    path = "/api/slices/{id}/unclaim-team",
    tag = "projects",
    params(("id" = Uuid, Path)),
    request_body = ClaimAsTeamBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 403, body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn unclaim_slice_by_team(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ClaimAsTeamBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_team_member(&state.db, body.team_id, auth.user_id).await?;
    let slice = SlicesService::unclaim_by_team(&state.db, id, body.team_id).await?;
    Ok(Json(build_response(json!({
        "slice": slice,
        "message": "Team released the slice. Others can now claim it."
    }))))
}

/// GET /api/teams/{team_id}/slices
///
/// Auth requis + membre de la team. Slices actives de la team.
/// Active slices claimed by a team. Requires membership.
#[utoipa::path(
    get,
    path = "/api/teams/{team_id}/slices",
    tag = "projects",
    params(("team_id" = Uuid, Path)),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 403, body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn team_slices(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(team_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_team_member(&state.db, team_id, auth.user_id).await?;
    let slices = SlicesService::list_claimed_by_team(&state.db, team_id).await?;
    Ok(Json(build_response(json!({ "slices": slices }))))
}

// ═══════════════════════════════════════════════════════════════════
// P11.4 — Steward inbox : validation des drafts ingérés
// ═══════════════════════════════════════════════════════════════════

/// Vérifie que l'user est admin OU steward actif du project.
pub async fn require_admin_or_steward(
    db: &sqlx::PgPool,
    project_id: Uuid,
    user_id: Uuid,
    role: &str,
) -> Result<(), AppError> {
    if role == "admin" {
        return Ok(());
    }
    let is_steward = crate::services::StewardsService::is_steward(db, project_id, user_id).await?;
    if is_steward {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

/// GET /api/stewards/{project_id}/inbox
///
/// Liste des slices `status='draft'` du project qui attendent validation.
/// Steward inbox: draft slices awaiting validation for a project.
#[utoipa::path(
    get,
    path = "/api/stewards/{project_id}/inbox",
    tag = "projects",
    params(("project_id" = Uuid, Path)),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 403, body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn steward_inbox(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(project_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin_or_steward(&state.db, project_id, auth.user_id, &auth.role).await?;
    let drafts = SlicesService::list_drafts_for_project(&state.db, project_id).await?;
    Ok(Json(build_response(json!({
        "drafts": drafts,
        "count": drafts.len(),
    }))))
}

/// POST /api/slices/{id}/publish
///
/// Steward (ou admin) valide la slice draft → status='open'.
/// Steward validates a draft slice → status=open.
#[utoipa::path(
    post,
    path = "/api/slices/{id}/publish",
    tag = "projects",
    params(("id" = Uuid, Path)),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 403, body = crate::api_response::ErrorResponse),
        (status = 404, body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn publish_slice(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    // On récupère le project_id AVANT le publish pour valider les droits.
    let project_id: Option<Uuid> =
        sqlx::query_scalar("SELECT project_id FROM project_slices WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await?;
    let project_id = project_id.ok_or_else(|| AppError::NotFound("Slice not found".into()))?;
    require_admin_or_steward(&state.db, project_id, auth.user_id, &auth.role).await?;

    let slice = SlicesService::publish_draft(&state.db, id).await?;
    metrics::counter!(
        "skilluv_steward_slices_published_total",
        "project" => project_id.to_string()
    )
    .increment(1);
    Ok(Json(build_response(json!({
        "slice": slice,
        "message": "Slice published — now open for claim."
    }))))
}

/// POST /api/slices/{id}/reject
///
/// Steward refuse une slice draft (pas pertinente / hors scope) → status='closed'.
/// Steward rejects a draft slice → status=closed.
#[utoipa::path(
    post,
    path = "/api/slices/{id}/reject",
    tag = "projects",
    params(("id" = Uuid, Path)),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 403, body = crate::api_response::ErrorResponse),
        (status = 404, body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn reject_slice(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let project_id: Option<Uuid> =
        sqlx::query_scalar("SELECT project_id FROM project_slices WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await?;
    let project_id = project_id.ok_or_else(|| AppError::NotFound("Slice not found".into()))?;
    require_admin_or_steward(&state.db, project_id, auth.user_id, &auth.role).await?;

    let slice = SlicesService::reject_draft(&state.db, id).await?;
    Ok(Json(build_response(json!({
        "slice": slice,
        "message": "Slice rejected — moved to closed."
    }))))
}
