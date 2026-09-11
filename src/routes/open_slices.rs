//! The open pool: work nobody has taken yet, in any trade.
//!
//! `GET /api/code/first-issues` answered one question - "what could I work on
//! right now?" - for one domain. The query behind it was never about code: it
//! reads `project_slices`, filtered to `slice_type = 'github_issue'`. Every
//! other trade has slices of exactly the same shape (`design_artifact`,
//! `audio_artifact`, `ops_artifact`, and eight more), sitting open and
//! unclaimed with no listing that shows them.
//!
//! So the filter moves into the query string. One endpoint answers for the
//! twelve domains, and code becomes `?domain=code` rather than a route of its
//! own.
//!
//! ## Why `slice_types` is joined rather than matched in Rust
//!
//! Migration 0408 made the surfaces a table, and it carries the domain each
//! one belongs to - `design_artifact` is design, `audio_artifact` is audio.
//! Three of them carry no domain at all (`github_issue`, `documentation`,
//! `other`): a ticket in an upstream repository exists in every trade, and
//! what decides its domain is the slice's own `primary_domain`. That is the
//! whole of the domain filter, and putting the mapping in Rust would be the
//! twelfth copy of the domain list - the failure migration 0400 exists to
//! remove.

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;

/// One hour. The ingestion pollers run on a longer cycle than that, so the
/// cache is never the reason a slice is missing from the pool.
const POOL_TTL_SECS: u64 = 3600;

/// Long enough to be a real listing, short enough that nobody paginates
/// through a feed whose whole point is "the best ones right now".
const MAX_POOL_LIMIT: i64 = 100;

/// The default ceiling. This is an entry pool, not the whole backlog.
pub const DEFAULT_MAX_DIFFICULTY: i16 = 3;

pub fn open_slice_routes() -> Router<AppState> {
    Router::new().route("/open-slices", get(list_open_slices))
}

// ═══════════════════════════════════════════════════════════════════
// GET /open-slices
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct OpenSlicesQuery {
    /// One of the platform's skill domains. Absent means every trade.
    ///
    /// Declared as the enum rather than as a bounded string, because the
    /// handler checks it against `validators::SKILL_DOMAINS` and a contract
    /// that only says "a short string" promises something the server refuses.
    /// The fuzzer is what noticed: it generated a schema-compliant value and
    /// got a 400 back.
    #[param(value_type = Option<crate::validators::SkillDomain>)]
    pub domain: Option<String>,
    /// One surface, by its `slice_types` slug - narrower than a domain, which
    /// usually holds several.
    #[param(max_length = 30)]
    pub slice_type: Option<String>,
    /// Orientation slug. Follows a rename, so an old slug still answers.
    #[param(max_length = 100)]
    pub orientation: Option<String>,
    /// Matched against whatever the domain tags its work with: languages for
    /// code, tools for design, frameworks for AI and security, platforms for
    /// game and ops.
    #[param(max_length = 40)]
    pub tag: Option<String>,
    /// Hardest difficulty to include, 1..5. Defaults to 3.
    #[param(minimum = 1, maximum = 5)]
    pub max_difficulty: Option<i16>,
    #[serde(default = "default_pool_limit")]
    #[param(minimum = 1, maximum = 100)]
    pub limit: i64,
}

fn default_pool_limit() -> i64 {
    30
}

#[derive(Debug, Serialize, Deserialize, Clone, sqlx::FromRow, ToSchema)]
pub struct OpenSliceRow {
    pub slice_id: Uuid,
    pub title: String,
    /// The surface the work lives on, as a `slice_types` slug.
    pub slice_type: String,
    /// That surface's display name, as the catalogue holds it.
    pub slice_type_name: String,
    /// The trade this is in: the surface's domain, or the slice's own for the
    /// three surfaces that belong to no single trade.
    pub domain: String,
    /// What comes out of it, when the domain distinguishes kinds. NULL for a
    /// surface that does not.
    pub subtype: Option<String>,
    pub difficulty: i16,
    pub fragments_reward: i32,
    pub project_slug: String,
    pub project_name: String,
    /// Where to read the work before claiming it - the upstream issue, the
    /// repository, the design file, the hosted track. NULL when the brief on
    /// the slice is all there is.
    pub external_url: Option<String>,
    /// NULL when nothing mapped the slice to a trade.
    pub orientation_slug: Option<String>,
    pub orientation_name: Option<String>,
    /// What this domain tags its work with. Empty rather than invented when
    /// the domain has no such notion.
    pub tags: Vec<String>,
    pub opened_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct OpenSlicesResponse {
    pub slices: Vec<OpenSliceRow>,
    /// Echoed back so a cached response is self-describing.
    pub domain: Option<String>,
    pub slice_type: Option<String>,
    pub orientation: Option<String>,
    pub tag: Option<String>,
    pub max_difficulty: i16,
}

/// Open, unclaimed work across every curated project, in any trade.
///
/// Only unclaimed, open slices on live projects appear: the pool exists to be
/// acted on, and listing something already taken wastes the reader's time.
#[utoipa::path(
    get,
    path = "/api/open-slices",
    tag = "slices",
    params(OpenSlicesQuery),
    responses(
        (status = 200, description = "Open, unclaimed slices", body = ApiResponse<OpenSlicesResponse>),
        (status = 400, description = "Invalid filter", body = crate::api_response::ErrorResponse),
        (status = 404, description = "Unknown trade or surface", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn list_open_slices(
    State(state): State<AppState>,
    Query(q): Query<OpenSlicesQuery>,
) -> Result<Json<ApiResponse<OpenSlicesResponse>>, AppError> {
    let response = open_pool(&state, q).await?;
    Ok(Json(ApiResponse::new(response)))
}

/// The pool query itself, shared with the surfaces that expose a slice of it.
pub async fn open_pool(
    state: &AppState,
    q: OpenSlicesQuery,
) -> Result<OpenSlicesResponse, AppError> {
    crate::validators::check_skill_domain_opt(&q.domain, "domain")?;
    crate::validators::check_max_len_opt(&q.slice_type, "slice_type", 30)?;
    crate::validators::check_max_len_opt(&q.orientation, "orientation", 100)?;
    crate::validators::check_max_len_opt(&q.tag, "tag", 40)?;
    if !(1..=MAX_POOL_LIMIT).contains(&q.limit) {
        return Err(AppError::Validation(format!(
            "limit must be between 1 and {MAX_POOL_LIMIT}"
        )));
    }
    let max_difficulty = q.max_difficulty.unwrap_or(DEFAULT_MAX_DIFFICULTY);
    if !(1..=5).contains(&max_difficulty) {
        return Err(AppError::Validation(
            "max_difficulty must be between 1 and 5".into(),
        ));
    }

    // An unknown surface is a 404 rather than an empty pool, for the same
    // reason an unknown trade is: "there is nothing open in design" and
    // "`design_artefact` is not how that is spelled" are different answers,
    // and only one of them tells the caller to fix the request.
    if let Some(slice_type) = q.slice_type.as_deref() {
        let known: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM slice_types WHERE slug = $1)")
                .bind(slice_type)
                .fetch_one(&state.db)
                .await?;
        if !known {
            return Err(AppError::NotFound(format!(
                "slice type '{slice_type}' not found"
            )));
        }
    }

    // Namespaced by database. A Redis instance shared between two deployments
    // - staging and production on one managed instance is the normal cheap
    // setup - would otherwise serve one's pool to the other, and the symptom
    // would be slices from projects the reader's deployment never seeded.
    let cache_key = format!(
        "open-slices:{}:{}:{}:{}:{}:{}:{}",
        state.db.connect_options().get_database().unwrap_or("db"),
        q.domain.as_deref().unwrap_or("-"),
        q.slice_type.as_deref().unwrap_or("-"),
        q.orientation.as_deref().unwrap_or("-"),
        q.tag.as_deref().unwrap_or("-"),
        max_difficulty,
        q.limit
    );
    let mut redis = state.redis.clone();
    if let Some(cached) =
        crate::services::cache::get_json::<OpenSlicesResponse>(&mut redis, &cache_key).await?
    {
        return Ok(cached);
    }

    let slices = sqlx::query_as::<_, OpenSliceRow>(
        r#"
        SELECT s.id AS slice_id,
               s.title,
               s.slice_type,
               st.name AS slice_type_name,
               d.domain,
               x.subtype,
               s.difficulty,
               s.fragments_reward,
               p.slug AS project_slug,
               p.name AS project_name,
               -- Where the work actually is, in the order that answers it.
               -- Migration 0435 folded the three domain-specific published
               -- columns into `published_artifact_url`, so design no longer
               -- has one of its own and this list is shorter than the number
               -- of trades it serves.
               COALESCE(
                   s.external_metadata ->> 'issue_url',
                   s.code_external_repo_url,
                   s.published_artifact_url,
                   s.audio_external_hosting_url
               ) AS external_url,
               o.slug AS orientation_slug,
               o.name AS orientation_name,
               x.tags,
               s.created_at AS opened_at
          FROM project_slices s
          JOIN projects p ON p.id = s.project_id
          JOIN slice_types st ON st.slug = s.slice_type
          LEFT JOIN orientations o ON o.id = s.orientation_id
          -- The trade, resolved once: the surface's own domain, and the
          -- slice's own for the three surfaces that belong to every trade.
          CROSS JOIN LATERAL (
              SELECT COALESCE(st.skill_domain, s.primary_domain) AS domain
          ) d
          CROSS JOIN LATERAL (
              SELECT
                  CASE d.domain
                      WHEN 'code'          THEN s.code_languages
                      WHEN 'ai'            THEN s.ai_frameworks
                      WHEN 'design'        THEN s.design_tools
                      WHEN 'communication' THEN s.communication_target_languages
                      WHEN 'ops'           THEN s.ops_tooling
                      WHEN 'quality'       THEN s.qa_tooling
                      WHEN 'security'      THEN s.security_frameworks
                      WHEN 'game'          THEN s.game_target_platforms
                  END AS own_tags
          ) t
          CROSS JOIN LATERAL (
              SELECT
                  CASE d.domain
                      WHEN 'code'          THEN s.code_subtype
                      WHEN 'ai'            THEN s.ai_subtype
                      WHEN 'design'        THEN s.design_subtype
                      WHEN 'audio'         THEN s.audio_subtype
                      WHEN 'game'          THEN s.game_artifact_subtype
                      WHEN 'ops'           THEN s.ops_subtype
                      WHEN 'quality'       THEN s.qa_subtype
                      WHEN 'security'      THEN s.security_subtype
                      WHEN 'leadership'    THEN s.leadership_subtype
                      WHEN 'communication' THEN s.communication_subtype
                      WHEN 'education'     THEN s.education_subtype
                  END AS subtype,
                  -- A slice that names its own tags is believed. The
                  -- repository's stack answers only for the ones that say
                  -- nothing, and only where that stack is about the work: a
                  -- design artefact delivered against a Rust repository is
                  -- not tagged `rust`.
                  CASE
                      WHEN cardinality(t.own_tags) > 0 THEN t.own_tags
                      WHEN st.skill_domain IS NULL OR d.domain = 'code'
                          THEN p.tech_stack
                      ELSE '{}'::TEXT[]
                  END AS tags
          ) x
         WHERE s.status = 'open'
           AND s.claimed_by_user_id IS NULL
           AND s.claimed_by_team_id IS NULL
           AND s.closed_at IS NULL
           AND p.archived_at IS NULL
           AND s.difficulty <= $1
           AND ($2::TEXT IS NULL OR d.domain = $2)
           AND ($3::TEXT IS NULL OR s.slice_type = $3)
           AND ($4::UUID IS NULL OR s.orientation_id = $4)
           AND ($5::TEXT IS NULL OR $5 = ANY(x.tags))
         ORDER BY s.difficulty ASC, s.created_at DESC
         LIMIT $6
        "#,
    )
    .bind(max_difficulty)
    .bind(q.domain.as_deref())
    .bind(q.slice_type.as_deref())
    .bind(orientation_id(state, q.orientation.as_deref()).await?)
    .bind(q.tag.as_deref())
    .bind(q.limit)
    .fetch_all(&state.db)
    .await?;

    let response = OpenSlicesResponse {
        slices,
        domain: q.domain,
        slice_type: q.slice_type,
        orientation: q.orientation,
        tag: q.tag,
        max_difficulty,
    };
    let _ =
        crate::services::cache::set_json(&mut redis, &cache_key, &response, POOL_TTL_SECS).await;
    Ok(response)
}

/// A slug the caller gave us, resolved to a live orientation.
///
/// An unknown slug is a 404 rather than an unfiltered pool: silently
/// answering "here is everything" to a typo is how somebody ends up claiming
/// kernel work believing it is frontend.
pub(crate) async fn orientation_id(
    state: &AppState,
    slug: Option<&str>,
) -> Result<Option<Uuid>, AppError> {
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
