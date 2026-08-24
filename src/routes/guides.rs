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
//! These are written here first and translated after, unlike the orientation
//! catalogue. A guide with no row in the requested locale is served in the
//! next best one rather than disappearing, because a half-translated
//! catalogue should show the untranslated page instead of claiming the guide
//! does not exist.
//!
//! The chain is: the locale asked for, then English, then French. English is
//! in the middle rather than last because it is now the locale this content
//! is authored in — the guides seeded from migration 0514 onwards have no
//! French row until somebody writes one, and the older ones have no English
//! row. Either way a reader gets the page.

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
    /// whoever does it (migration 0519), which is why a listing meant for
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

    // One row per guide, in the best locale available for it: the one asked
    // for, then English, then French.
    //
    // This used to be `locale = $1`, which silently hid every guide that had
    // no row in the requested locale. It was invisible while the whole table
    // was French and every reader defaulted to French; the first English-only
    // domain — communication, migration 0514 — would have been missing from
    // a French reader's list entirely, and the list would have looked
    // complete.
    let guides = sqlx::query_as::<_, GuideSummary>(
        r#"
        SELECT DISTINCT ON (slug)
               slug, kind, skill_domain, reviewer_group, locale, title, summary
          FROM content_guides
         WHERE is_published = TRUE
           AND ($2::TEXT IS NULL OR skill_domain = $2)
           AND ($3::TEXT IS NULL OR kind = $3)
           AND ($4::TEXT IS NULL OR reviewer_group = $4)
         ORDER BY slug, (locale = $1) DESC, (locale = 'en') DESC, (locale = 'fr') DESC
        "#,
    )
    .bind(&locale)
    .bind(q.domain.as_deref())
    .bind(q.kind.as_deref())
    .bind(q.reviewer_group.as_deref())
    .fetch_all(&state.db)
    .await?;

    // `DISTINCT ON` dictates the SQL ordering, so the display ordering is
    // restored here. Sorting in Rust rather than wrapping the query in a
    // subselect: the whole table is a few hundred rows and always will be.
    let mut guides = guides;
    guides.sort_by(|a, b| {
        a.skill_domain
            .cmp(&b.skill_domain)
            .then_with(|| a.slug.cmp(&b.slug))
    });

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
         ORDER BY (locale = $2) DESC, (locale = 'en') DESC, (locale = 'fr') DESC
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
        // English when the caller says nothing. It was French, from when the
        // whole table was; the repository's default is English now, and a
        // reader who expressed no preference should get the locale the
        // content is authored in rather than the one it is translated into.
        "en".into()
    }
}
