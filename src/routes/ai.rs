//! Public entry points into AI work.
//!
//! Four questions somebody arriving with no account should be able to answer
//! without one:
//!
//!   * "what do I install, and what can I reach without a credit card?"
//!     — `GET /api/ai/toolkit`
//!   * "what is worth entering right now, outside Skilluv?"
//!     — `GET /api/ai/competitions`
//!   * "what have people here actually published?"
//!     — `GET /api/ai/artifacts`
//!   * "what has this person done in AI?"
//!     — `GET /api/users/{username}/ai-profile`
//!
//! The prefix means the domain of work, not the assistant — that moved to
//! `/api/assistant` for exactly this reason.

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::services::ai_profile::{self, AiProfile};

/// Ten minutes. The underlying rows change when an operator edits them or
/// when the weekly stats sweep runs, so a fresher answer would be precision
/// the data does not have.
const AI_CACHE_TTL_SECS: u64 = 600;

/// Long enough to be a real listing, short enough that nobody paginates
/// through a feed whose whole point is "the best ones right now".
const MAX_AI_LIMIT: i64 = 100;

pub fn ai_routes() -> Router<AppState> {
    Router::new()
        .route("/ai/toolkit", get(toolkit))
        .route("/ai/competitions", get(competitions))
        .route("/ai/artifacts", get(artifacts))
        .route("/users/{username}/ai-profile", get(user_ai_profile))
}

fn check_limit(limit: i64) -> Result<i64, AppError> {
    if !(1..=MAX_AI_LIMIT).contains(&limit) {
        return Err(AppError::Validation(format!(
            "limit must be between 1 and {MAX_AI_LIMIT}"
        )));
    }
    Ok(limit)
}

fn default_limit() -> i64 {
    50
}

// ═══════════════════════════════════════════════════════════════════
// GET /ai/toolkit
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct ToolkitQuery {
    /// `framework`, `llm_tooling`, `mlops`, `data_stack`, `compute`,
    /// `safety`, `hub`, `community`, `learning`.
    #[param(max_length = 20)]
    pub category: Option<String>,
    /// Restrict to resources tagged for one trade. Resources tagged for none
    /// serve the whole domain and are always included.
    #[param(max_length = 60)]
    pub orientation: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, sqlx::FromRow, ToSchema)]
pub struct ToolkitRow {
    pub slug: String,
    pub display_name: String,
    pub category: String,
    pub url: String,
    pub summary: String,
    /// What it takes to actually reach this — free tier, GPU needed, course
    /// auditable without paying.
    pub access_note: String,
    pub orientation_slugs: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct ToolkitResponse {
    pub resources: Vec<ToolkitRow>,
    /// Echoed back so a cached response is self-describing.
    pub category: Option<String>,
    pub orientation: Option<String>,
}

/// The curated AI toolkit: frameworks, hubs, compute, communities, courses.
#[utoipa::path(
    get,
    path = "/api/ai/toolkit",
    tag = "ai",
    params(ToolkitQuery),
    responses(
        (status = 200, description = "Curated AI resources", body = ApiResponse<ToolkitResponse>),
        (status = 400, description = "Invalid filter", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn toolkit(
    State(state): State<AppState>,
    Query(q): Query<ToolkitQuery>,
) -> Result<Json<ApiResponse<ToolkitResponse>>, AppError> {
    crate::validators::check_max_len_opt(&q.category, "category", 20)?;
    crate::validators::check_max_len_opt(&q.orientation, "orientation", 60)?;

    // Namespaced by database, like the code feeds: a Redis instance shared
    // between staging and production would otherwise serve one's listing to
    // the other.
    let cache_key = format!(
        "ai:toolkit:{}:{}:{}",
        state.db.connect_options().get_database().unwrap_or("db"),
        q.category.as_deref().unwrap_or("-"),
        q.orientation.as_deref().unwrap_or("-"),
    );
    let mut redis = state.redis.clone();
    if let Some(cached) =
        crate::services::cache::get_json::<ToolkitResponse>(&mut redis, &cache_key).await?
    {
        return Ok(Json(ApiResponse::new(cached)));
    }

    let resources = sqlx::query_as::<_, ToolkitRow>(
        r#"
        SELECT slug, display_name, category, url, summary, access_note,
               orientation_slugs
          FROM external_ai_resources
         WHERE is_curated = TRUE
           AND ($1::TEXT IS NULL OR category = $1)
           -- A resource tagged for no trade serves every trade, so it stays
           -- in a filtered listing. Excluding it would hide HuggingFace from
           -- somebody who asked for the NLP toolkit.
           AND ($2::TEXT IS NULL
                OR cardinality(orientation_slugs) = 0
                OR $2 = ANY(orientation_slugs))
         ORDER BY category, sort_order, display_name
        "#,
    )
    .bind(q.category.as_deref())
    .bind(q.orientation.as_deref())
    .fetch_all(&state.db)
    .await?;

    let response = ToolkitResponse {
        resources,
        category: q.category.clone(),
        orientation: q.orientation.clone(),
    };
    let _ =
        crate::services::cache::set_json(&mut redis, &cache_key, &response, AI_CACHE_TTL_SECS).await;

    Ok(Json(ApiResponse::new(response)))
}

// ═══════════════════════════════════════════════════════════════════
// GET /ai/competitions
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct CompetitionQuery {
    #[param(max_length = 60)]
    pub orientation: Option<String>,
    /// Include competitions whose deadline has passed. Off by default: a
    /// listing that keeps showing closed entries teaches people to stop
    /// reading it.
    #[serde(default)]
    pub include_closed: bool,
    #[serde(default = "default_limit")]
    #[param(minimum = 1, maximum = 100)]
    pub limit: i64,
}

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct CompetitionRow {
    pub id: Uuid,
    pub platform: String,
    pub title: String,
    pub url: String,
    /// Why this one and not the forty others open right now.
    pub why_this_one: String,
    /// NULL for a rolling leaderboard, which has no deadline by nature.
    pub deadline: Option<chrono::DateTime<chrono::Utc>>,
    pub prize_note: Option<String>,
    pub orientation_slugs: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CompetitionsResponse {
    pub competitions: Vec<CompetitionRow>,
}

/// Competitions and leaderboards outside Skilluv, chosen by a curator.
#[utoipa::path(
    get,
    path = "/api/ai/competitions",
    tag = "ai",
    params(CompetitionQuery),
    responses(
        (status = 200, description = "Curated external competitions", body = ApiResponse<CompetitionsResponse>),
        (status = 400, description = "Invalid filter", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn competitions(
    State(state): State<AppState>,
    Query(q): Query<CompetitionQuery>,
) -> Result<Json<ApiResponse<CompetitionsResponse>>, AppError> {
    crate::validators::check_max_len_opt(&q.orientation, "orientation", 60)?;
    let limit = check_limit(q.limit)?;

    let competitions = sqlx::query_as::<_, CompetitionRow>(
        r#"
        SELECT id, platform, title, url, why_this_one, deadline, prize_note,
               orientation_slugs
          FROM external_ai_competitions
         WHERE is_published = TRUE
           AND ($1::TEXT IS NULL
                OR cardinality(orientation_slugs) = 0
                OR $1 = ANY(orientation_slugs))
           -- A rolling leaderboard has no deadline and is never closed.
           AND ($2::BOOLEAN OR deadline IS NULL OR deadline > NOW())
         ORDER BY deadline ASC NULLS LAST, title
         LIMIT $3
        "#,
    )
    .bind(q.orientation.as_deref())
    .bind(q.include_closed)
    .bind(limit)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(CompetitionsResponse { competitions })))
}

// ═══════════════════════════════════════════════════════════════════
// GET /ai/artifacts
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct ArtifactQuery {
    /// `ml_model`, `dataset`, `llm_agent`, `data_pipeline`,
    /// `ai_service_api`, `ai_research_paper`.
    #[param(max_length = 30)]
    pub subtype: Option<String>,
    #[param(max_length = 60)]
    pub orientation: Option<String>,
    /// `pytorch`, `jax`, `vllm` — matched against what the slice declares.
    #[param(max_length = 40)]
    pub framework: Option<String>,
    #[serde(default = "default_limit")]
    #[param(minimum = 1, maximum = 100)]
    pub limit: i64,
}

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct ArtifactRow {
    pub slice_id: Uuid,
    pub title: String,
    pub ai_subtype: String,
    pub ai_frameworks: Vec<String>,
    /// Where the artefact lives. Present for every subtype that requires it.
    pub hosting_url: Option<String>,
    pub model_size_params: Option<i64>,
    pub author_username: String,
    pub orientation_slug: Option<String>,
    /// Monthly downloads from the hub, NULL when never fetched or when the
    /// hub publishes none. Different from zero.
    pub downloads_recent: Option<i64>,
    pub likes_count: Option<i32>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ArtifactsResponse {
    pub artifacts: Vec<ArtifactRow>,
}

/// Published AI artefacts with a verified deliverable behind them.
///
/// Only verified, public, unrevoked work appears. The feed exists to be
/// browsed by somebody deciding whether this platform produces anything real,
/// and a pending submission answers that question wrongly.
#[utoipa::path(
    get,
    path = "/api/ai/artifacts",
    tag = "ai",
    params(ArtifactQuery),
    responses(
        (status = 200, description = "Verified AI artefacts", body = ApiResponse<ArtifactsResponse>),
        (status = 400, description = "Invalid filter", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn artifacts(
    State(state): State<AppState>,
    Query(q): Query<ArtifactQuery>,
) -> Result<Json<ApiResponse<ArtifactsResponse>>, AppError> {
    crate::validators::check_max_len_opt(&q.subtype, "subtype", 30)?;
    crate::validators::check_max_len_opt(&q.orientation, "orientation", 60)?;
    crate::validators::check_max_len_opt(&q.framework, "framework", 40)?;
    let limit = check_limit(q.limit)?;

    let artifacts = sqlx::query_as::<_, ArtifactRow>(
        r#"
        SELECT ps.id AS slice_id,
               ps.title,
               ps.ai_subtype,
               ps.ai_frameworks,
               ps.ai_external_hosting_url AS hosting_url,
               ps.ai_model_size_params AS model_size_params,
               u.username AS author_username,
               o.slug AS orientation_slug,
               st.downloads_recent,
               st.likes_count
          FROM project_slices ps
          JOIN deliverables d ON d.slice_id = ps.id
          JOIN users u ON u.id = d.user_id
          LEFT JOIN orientations o ON o.id = ps.orientation_id
          LEFT JOIN published_artifact_stats st ON st.slice_id = ps.id
         WHERE ps.slice_type = 'ai_artifact'
           AND d.verification_status = 'verified'
           AND d.revoked_at IS NULL
           AND d.public = TRUE
           AND ($1::TEXT IS NULL OR ps.ai_subtype = $1)
           AND ($2::TEXT IS NULL OR o.slug = $2)
           AND ($3::TEXT IS NULL OR $3 = ANY(ps.ai_frameworks))
         -- Reach first, then recency. An artefact nobody has fetched figures
         -- for sorts as zero rather than dropping out of the listing.
         ORDER BY COALESCE(st.downloads_recent, 0) DESC, ps.created_at DESC
         LIMIT $4
        "#,
    )
    .bind(q.subtype.as_deref())
    .bind(q.orientation.as_deref())
    .bind(q.framework.as_deref())
    .bind(limit)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(ArtifactsResponse { artifacts })))
}

// ═══════════════════════════════════════════════════════════════════
// GET /users/{username}/ai-profile
// ═══════════════════════════════════════════════════════════════════

/// What one person has to show in the AI trades, and a score for it.
///
/// Derived on every call from proofs that are already immutable, rather than
/// read from a stored total: a column would keep the points of a revoked
/// attestation until somebody remembered to recompute.
#[utoipa::path(
    get,
    path = "/api/users/{username}/ai-profile",
    tag = "profile",
    params(("username" = String, Path, description = "Username")),
    responses(
        (status = 200, description = "AI profile", body = ApiResponse<AiProfile>),
        (status = 404, description = "No such user", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn user_ai_profile(
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> Result<Json<ApiResponse<AiProfile>>, AppError> {
    let profile = ai_profile::build(&state.db, &username).await?;
    Ok(Json(ApiResponse::new(profile)))
}
