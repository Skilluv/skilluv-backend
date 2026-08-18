//! Onboarding guides, toolkits and writeup templates.
//!
//! ## Why these are not under `/api/code`
//!
//! `content_guides` carries a `skill_domain` and always did — but the
//! endpoint that served it was `/api/code/guides` and its query ignored the
//! column. That was invisible while one domain had rows. The moment a second
//! one did, an AI onboarding guide answered under the code path, and a
//! reader filtering by family got two domains mixed in one list.
//!
//! Guides are not a code feature. One endpoint, one query, a domain filter
//! that is honoured.
//!
//! ## Locale
//!
//! French is the base locale for this content, unlike the orientation
//! catalogue: these are written here first and translated after. A guide with
//! no translation in the requested locale is served in French rather than
//! 404ing, because a half-translated catalogue should show the untranslated
//! page instead of claiming the guide does not exist.

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;

pub fn guide_routes() -> Router<AppState> {
    Router::new()
        .route("/guides", get(list_guides))
        .route("/guides/{slug}", get(get_guide))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct GuideQuery {
    /// `code`, `ai`, … Absent means every domain, which is rarely what a
    /// reader wants and always what an operator does.
    #[param(max_length = 30)]
    pub domain: Option<String>,
    /// `onboarding`, `toolkit`, `writeup_template` or `brief_template`.
    /// Absent means all of them.
    ///
    /// The last is written by whoever commissions the work rather than by
    /// whoever does it (migration 0419), which is why a listing meant for
    /// contributors usually asks for the other three.
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
    pub skill_domain: String,
    pub reviewer_group: Option<String>,
    pub locale: String,
    pub title: String,
    pub summary: String,
}

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct Guide {
    pub slug: String,
    pub kind: String,
    pub skill_domain: String,
    pub reviewer_group: Option<String>,
    pub locale: String,
    pub title: String,
    pub summary: String,
    /// Markdown. Rendered by the reader, not here.
    pub body_md: String,
}

/// The guides, toolkits and templates on offer, without their bodies.
#[utoipa::path(
    get,
    path = "/api/guides",
    tag = "code",
    params(GuideQuery),
    responses((status = 200, description = "Published guides", body = ApiResponse<Vec<GuideSummary>>)),
)]
pub async fn list_guides(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Query(q): Query<GuideQuery>,
) -> Result<Json<ApiResponse<Vec<GuideSummary>>>, AppError> {
    crate::validators::check_max_len_opt(&q.domain, "domain", 30)?;
    crate::validators::check_max_len_opt(&q.kind, "kind", 30)?;
    crate::validators::check_max_len_opt(&q.reviewer_group, "reviewer_group", 30)?;
    let locale = guide_locale(&headers);

    let guides = sqlx::query_as::<_, GuideSummary>(
        r#"
        SELECT slug, kind, skill_domain, reviewer_group, locale, title, summary
          FROM content_guides
         WHERE is_published = TRUE
           AND locale = $1
           AND ($2::TEXT IS NULL OR skill_domain = $2)
           AND ($3::TEXT IS NULL OR kind = $3)
           AND ($4::TEXT IS NULL OR reviewer_group = $4)
         ORDER BY skill_domain, sort_order, slug
        "#,
    )
    .bind(&locale)
    .bind(q.domain.as_deref())
    .bind(q.kind.as_deref())
    .bind(q.reviewer_group.as_deref())
    .fetch_all(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(guides)))
}

/// One guide, with its body.
#[utoipa::path(
    get,
    path = "/api/guides/{slug}",
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
    Path(slug): Path<String>,
) -> Result<Json<ApiResponse<Guide>>, AppError> {
    let locale = guide_locale(&headers);

    let guide = sqlx::query_as::<_, Guide>(
        r#"
        SELECT slug, kind, skill_domain, reviewer_group, locale, title, summary,
               body_md
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

fn guide_locale(headers: &axum::http::HeaderMap) -> String {
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
