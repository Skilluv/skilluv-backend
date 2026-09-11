//! Public entry points into code work - sections T-03 and T-05.
//!
//! Two questions somebody arriving with no account should be able to answer
//! without one:
//!
//!   * "what could I work on right now, in the trade I am learning?"
//!     - `GET /api/code/first-issues`, deprecated in favour of
//!       `GET /api/open-slices?domain=code&slice_type=github_issue`
//!   * "where do the people who write this language actually talk?"
//!     - `GET /api/code/ecosystems`
//!
//! Both are public and both are cached.

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::routes::open_slices::{OpenSlicesQuery, open_pool};

#[allow(deprecated)]
pub fn code_routes() -> Router<AppState> {
    Router::new()
        .route("/code/first-issues", get(first_issues))
        .route("/code/ecosystems", get(language_ecosystems))
}

// ═══════════════════════════════════════════════════════════════════
// GET /code/first-issues — deprecated
// ═══════════════════════════════════════════════════════════════════
//
// The query this used to hold was never about code. It read `project_slices`
// with `slice_type = 'github_issue'`, and the other eleven trades have slices
// of the same shape with no listing that shows them. It now lives in
// `routes::open_slices`, and this route is one call into it with the two
// filters that made it code's.
//
// Kept, rather than removed, only because the front is still calling it.
// Nothing here is a second implementation: change the pool and this changes
// with it.

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct FirstIssuesQuery {
    /// Orientation slug. Follows a rename, so an old slug still answers.
    #[param(max_length = 100)]
    pub orientation: Option<String>,
    /// Filters on the languages recorded on the slice or the repository.
    #[param(max_length = 40)]
    pub language: Option<String>,
    /// Hardest difficulty to include, 1..5. Defaults to 3 - this is a
    /// first-issue feed, not the whole backlog.
    #[param(minimum = 1, maximum = 5)]
    pub max_difficulty: Option<i16>,
    #[serde(default = "default_feed_limit")]
    #[param(minimum = 1, maximum = 100)]
    pub limit: i64,
}

fn default_feed_limit() -> i64 {
    30
}

#[derive(Debug, Serialize, Deserialize, Clone, sqlx::FromRow, ToSchema)]
pub struct FirstIssueRow {
    pub slice_id: Uuid,
    pub title: String,
    pub difficulty: i16,
    pub fragments_reward: i32,
    pub project_slug: String,
    pub project_name: String,
    /// The upstream issue, so somebody can read it before claiming anything.
    pub issue_url: Option<String>,
    /// NULL when the upstream labels said nothing we could map.
    pub orientation_slug: Option<String>,
    pub orientation_name: Option<String>,
    pub languages: Vec<String>,
    pub ingested_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct FirstIssuesResponse {
    pub issues: Vec<FirstIssueRow>,
    /// Echoed back so a cached response is self-describing.
    pub orientation: Option<String>,
    pub language: Option<String>,
    pub max_difficulty: i16,
}

/// Curated open issues across every seeded repository, filtered by trade.
///
/// Deprecated: use `GET /api/open-slices?domain=code&slice_type=github_issue`,
/// which answers the same question for the eleven other trades as well.
#[utoipa::path(
    get,
    path = "/api/code/first-issues",
    tag = "code",
    params(FirstIssuesQuery),
    responses(
        (status = 200, description = "Open first issues", body = ApiResponse<FirstIssuesResponse>),
        (status = 400, description = "Invalid filter", body = crate::api_response::ErrorResponse),
    ),
)]
#[deprecated(note = "use GET /api/open-slices?domain=code&slice_type=github_issue")]
pub async fn first_issues(
    State(state): State<AppState>,
    Query(q): Query<FirstIssuesQuery>,
) -> Result<Json<ApiResponse<FirstIssuesResponse>>, AppError> {
    let pool = open_pool(
        &state,
        OpenSlicesQuery {
            domain: Some("code".to_string()),
            slice_type: Some("github_issue".to_string()),
            orientation: q.orientation.clone(),
            tag: q.language.clone(),
            max_difficulty: q.max_difficulty,
            limit: q.limit,
        },
    )
    .await?;

    let issues = pool
        .slices
        .into_iter()
        .map(|s| FirstIssueRow {
            slice_id: s.slice_id,
            title: s.title,
            difficulty: s.difficulty,
            fragments_reward: s.fragments_reward,
            project_slug: s.project_slug,
            project_name: s.project_name,
            issue_url: s.external_url,
            orientation_slug: s.orientation_slug,
            orientation_name: s.orientation_name,
            languages: s.tags,
            ingested_at: s.opened_at,
        })
        .collect();

    Ok(Json(ApiResponse::new(FirstIssuesResponse {
        issues,
        orientation: q.orientation,
        language: q.language,
        max_difficulty: pool.max_difficulty,
    })))
}
// ═══════════════════════════════════════════════════════════════════
// GET /code/ecosystems
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct EcosystemQuery {
    /// Restrict to one language. Absent means the whole curated listing.
    #[param(max_length = 40)]
    pub language: Option<String>,
}

/// Where a language community actually talks to itself.
#[derive(Debug, Serialize, ToSchema)]
pub struct CommunityLink {
    pub name: String,
    pub url: String,
}

/// A recurring gathering, with the month it falls in so somebody can plan a
/// year around it.
#[derive(Debug, Serialize, ToSchema)]
pub struct NotableEvent {
    pub name: String,
    pub url: String,
    pub month: String,
    /// `global`, `regional` or `online`.
    pub scope: String,
}

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct EcosystemRow {
    pub language: String,
    pub display_name: String,
    pub community_url: String,
    // Stored as JSONB and served through, so the Rust type is a `Value`; the
    // schema still has to say array-of-what, or a generated client reads
    // `object` and cannot iterate the thing it was given.
    #[schema(value_type = Vec<CommunityLink>)]
    pub community_links: serde_json::Value,
    #[schema(value_type = Vec<NotableEvent>)]
    pub notable_events: serde_json::Value,
    pub summary: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EcosystemsResponse {
    pub ecosystems: Vec<EcosystemRow>,
}

/// The curated listing of language ecosystems: where each community lives
/// and which of its events are reachable.
#[utoipa::path(
    get,
    path = "/api/code/ecosystems",
    tag = "code",
    params(EcosystemQuery),
    responses(
        (status = 200, description = "Curated language ecosystems", body = ApiResponse<EcosystemsResponse>),
    ),
)]
pub async fn language_ecosystems(
    State(state): State<AppState>,
    Query(q): Query<EcosystemQuery>,
) -> Result<Json<ApiResponse<EcosystemsResponse>>, AppError> {
    crate::validators::check_max_len_opt(&q.language, "language", 40)?;

    let ecosystems = sqlx::query_as::<_, EcosystemRow>(
        r#"
        SELECT language, display_name, community_url, community_links,
               notable_events, summary
          FROM external_language_ecosystems
         WHERE is_curated = TRUE
           AND ($1::TEXT IS NULL OR language = $1)
         ORDER BY sort_order, language
        "#,
    )
    .bind(q.language.as_deref())
    .fetch_all(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(EcosystemsResponse { ecosystems })))
}
