//! Public entry points into code work — sections T-03 and T-05.
//!
//! Two questions somebody arriving with no account should be able to answer
//! without one:
//!
//!   * "what could I work on right now, in the trade I am learning?"
//!     — `GET /api/code/first-issues`
//!   * "where do the people who write this language actually talk?"
//!     — `GET /api/code/ecosystems`
//!
//! Both are public and both are cached. The first is an aggregate over every
//! curated repository, recomputed at most once an hour: the underlying issues
//! are ingested on a polling cycle anyway, so a fresher answer would be
//! precision the data does not have.

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;

/// One hour. The ingestion poller runs on a longer cycle than that, so the
/// cache is never the reason an issue is missing from the feed.
const FEED_TTL_SECS: u64 = 3600;

/// Long enough to be a real listing, short enough that nobody paginates
/// through a feed whose whole point is "the best ones right now".
const MAX_FEED_LIMIT: i64 = 100;

pub fn code_routes() -> Router<AppState> {
    Router::new()
        .route("/code/first-issues", get(first_issues))
        .route("/code/ecosystems", get(language_ecosystems))
        .route("/code/guides", get(list_guides))
        .route("/code/guides/{slug}", get(get_guide))
}

// ═══════════════════════════════════════════════════════════════════
// GET /code/first-issues
// ═══════════════════════════════════════════════════════════════════

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
    /// Hardest difficulty to include, 1..5. Defaults to 3 — this is a
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
/// Only unclaimed, open, ingested issues appear: the feed exists to be acted
/// on, and listing something already claimed wastes the reader's time.
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
pub async fn first_issues(
    State(state): State<AppState>,
    Query(q): Query<FirstIssuesQuery>,
) -> Result<Json<ApiResponse<FirstIssuesResponse>>, AppError> {
    crate::validators::check_max_len_opt(&q.orientation, "orientation", 100)?;
    crate::validators::check_max_len_opt(&q.language, "language", 40)?;
    if !(1..=MAX_FEED_LIMIT).contains(&q.limit) {
        return Err(AppError::Validation(format!(
            "limit must be between 1 and {MAX_FEED_LIMIT}"
        )));
    }
    let max_difficulty = q.max_difficulty.unwrap_or(3);
    if !(1..=5).contains(&max_difficulty) {
        return Err(AppError::Validation(
            "max_difficulty must be between 1 and 5".into(),
        ));
    }

    // Namespaced by database. A Redis instance shared between two deployments
    // — staging and production on one managed instance is the normal cheap
    // setup — would otherwise serve one's feed to the other, and the symptom
    // would be issues from repositories the reader's deployment never seeded.
    let cache_key = format!(
        "code:first-issues:{}:{}:{}:{}:{}",
        state.db.connect_options().get_database().unwrap_or("db"),
        q.orientation.as_deref().unwrap_or("-"),
        q.language.as_deref().unwrap_or("-"),
        max_difficulty,
        q.limit
    );
    let mut redis = state.redis.clone();
    if let Some(cached) =
        crate::services::cache::get_json::<FirstIssuesResponse>(&mut redis, &cache_key).await?
    {
        return Ok(Json(ApiResponse::new(cached)));
    }

    let rows = sqlx::query_as::<_, FirstIssueRow>(
        r#"
        SELECT s.id AS slice_id,
               s.title,
               s.difficulty,
               s.fragments_reward,
               p.slug AS project_slug,
               p.name AS project_name,
               s.external_metadata ->> 'issue_url' AS issue_url,
               o.slug AS orientation_slug,
               o.name AS orientation_name,
               CASE WHEN cardinality(s.code_languages) > 0
                    THEN s.code_languages
                    ELSE p.tech_stack
               END AS languages,
               s.created_at AS ingested_at
          FROM project_slices s
          JOIN projects p ON p.id = s.project_id
          LEFT JOIN orientations o ON o.id = s.orientation_id
         WHERE s.slice_type = 'github_issue'
           AND s.status = 'open'
           AND s.claimed_by_user_id IS NULL
           AND s.claimed_by_team_id IS NULL
           AND s.closed_at IS NULL
           AND p.archived_at IS NULL
           AND s.difficulty <= $1
           AND ($2::UUID IS NULL OR s.orientation_id = $2)
           -- Same rule as the projection above: a slice that names its own
           -- languages is believed, and the repository's stack answers only
           -- for the ones that say nothing. A Zig issue in a mostly-Rust
           -- repository must not surface under `language=rust`.
           AND ($3::TEXT IS NULL
                OR $3 = ANY(CASE WHEN cardinality(s.code_languages) > 0
                                 THEN s.code_languages
                                 ELSE p.tech_stack
                            END))
         ORDER BY s.difficulty ASC, s.created_at DESC
         LIMIT $4
        "#,
    )
    .bind(max_difficulty)
    .bind(orientation_id(&state, q.orientation.as_deref()).await?)
    .bind(q.language.as_deref())
    .bind(q.limit)
    .fetch_all(&state.db)
    .await?;

    let response = FirstIssuesResponse {
        issues: rows,
        orientation: q.orientation.clone(),
        language: q.language.clone(),
        max_difficulty,
    };
    let _ =
        crate::services::cache::set_json(&mut redis, &cache_key, &response, FEED_TTL_SECS).await;

    Ok(Json(ApiResponse::new(response)))
}

/// A slug the caller gave us, resolved to a live orientation.
///
/// An unknown slug is a 404 rather than an unfiltered feed: silently
/// answering "here is everything" to a typo is how somebody ends up claiming
/// kernel work believing it is frontend.
async fn orientation_id(state: &AppState, slug: Option<&str>) -> Result<Option<Uuid>, AppError> {
    let Some(slug) = slug else {
        return Ok(None);
    };
    let resolved: Option<Uuid> = sqlx::query_scalar("SELECT resolve_orientation($1)")
        .bind(slug)
        .fetch_one(&state.db)
        .await?;
    resolved
        .ok_or_else(|| AppError::NotFound(format!("orientation '{slug}' not found")))
        .map(Some)
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

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct EcosystemRow {
    pub language: String,
    pub display_name: String,
    pub community_url: String,
    /// `[{"name", "url"}]`.
    #[schema(value_type = Object)]
    pub community_links: serde_json::Value,
    /// `[{"name", "url", "month", "scope"}]` where scope is `global`,
    /// `regional` or `online`.
    #[schema(value_type = Object)]
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

// ═══════════════════════════════════════════════════════════════════
// GET /code/guides
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct GuideQuery {
    /// `onboarding`, `toolkit` or `writeup_template`. Absent means all three.
    #[param(max_length = 30)]
    pub kind: Option<String>,
    /// Restrict onboarding guides to one family of trades.
    #[param(max_length = 30)]
    pub reviewer_group: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct GuideSummary {
    pub slug: String,
    pub kind: String,
    pub reviewer_group: Option<String>,
    pub locale: String,
    pub title: String,
    pub summary: String,
}

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct Guide {
    pub slug: String,
    pub kind: String,
    pub reviewer_group: Option<String>,
    pub locale: String,
    pub title: String,
    pub summary: String,
    /// Markdown. Rendered by the reader, not here.
    pub body_md: String,
}

/// The guides, toolkits and templates on offer, without their bodies.
///
/// The locale follows `Accept-Language`, and falls back to French — the base
/// locale everything else on the platform falls back to.
#[utoipa::path(
    get,
    path = "/api/code/guides",
    tag = "code",
    params(GuideQuery),
    responses((status = 200, description = "Published guides", body = ApiResponse<Vec<GuideSummary>>)),
)]
pub async fn list_guides(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Query(q): Query<GuideQuery>,
) -> Result<Json<ApiResponse<Vec<GuideSummary>>>, AppError> {
    crate::validators::check_max_len_opt(&q.kind, "kind", 30)?;
    crate::validators::check_max_len_opt(&q.reviewer_group, "reviewer_group", 30)?;
    let locale = guide_locale(&headers);

    let guides = sqlx::query_as::<_, GuideSummary>(
        r#"
        SELECT slug, kind, reviewer_group, locale, title, summary
          FROM content_guides
         WHERE is_published = TRUE
           -- This route is the code catalogue. Without the domain the ops
           -- guides would appear here the day they were written, under a
           -- path that says code.
           AND skill_domain = 'code'
           AND locale = $1
           AND ($2::TEXT IS NULL OR kind = $2)
           AND ($3::TEXT IS NULL OR reviewer_group = $3)
         ORDER BY sort_order, slug
        "#,
    )
    .bind(&locale)
    .bind(q.kind.as_deref())
    .bind(q.reviewer_group.as_deref())
    .fetch_all(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(guides)))
}

/// One guide, with its body.
///
/// Falls back to the French version when the requested locale has none: a
/// half-translated catalogue should show the untranslated page rather than a
/// 404 that reads as "this guide does not exist".
#[utoipa::path(
    get,
    path = "/api/code/guides/{slug}",
    tag = "code",
    params(("slug" = String, Path, description = "Guide slug")),
    responses(
        (status = 200, description = "The guide", body = ApiResponse<Guide>),
        (status = 404, description = "No such guide", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn get_guide(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    axum::extract::Path(slug): axum::extract::Path<String>,
) -> Result<Json<ApiResponse<Guide>>, AppError> {
    let locale = guide_locale(&headers);

    let guide = sqlx::query_as::<_, Guide>(
        r#"
        SELECT slug, kind, reviewer_group, locale, title, summary, body_md
          FROM content_guides
         WHERE slug = $1 AND is_published = TRUE
         ORDER BY (locale = $2) DESC, (locale = 'fr') DESC
         LIMIT 1
        "#,
    )
    .bind(&slug)
    .bind(&locale)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("no guide '{slug}'")))?;

    Ok(Json(ApiResponse::new(guide)))
}

/// French is the base locale for this content, unlike the orientation
/// catalogue: these are written here first and translated after.
pub fn guide_locale(headers: &axum::http::HeaderMap) -> String {
    let resolved = crate::routes::resolve_from_accept_language(
        headers
            .get(axum::http::header::ACCEPT_LANGUAGE)
            .and_then(|value| value.to_str().ok()),
    );
    if headers.contains_key(axum::http::header::ACCEPT_LANGUAGE) {
        resolved
    } else {
        "fr".into()
    }
}
