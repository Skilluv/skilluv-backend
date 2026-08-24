//! External accounts somebody has linked, in any domain.
//!
//! ## Why this is not under `/api/audio`
//!
//! It was, and migration 0415 had already done the hard half: it renamed
//! `user_code_portfolios` to `user_external_portfolios`, turned the platform
//! list into `portfolio_platforms` rows carrying a domain, and wrote down
//! what each platform's figures mean. What stayed behind was three handlers
//! with `pf.skill_domain = 'audio'` inlined in their queries.
//!
//! Communication links ten platforms and education four. Copying the handlers
//! twice would have meant three answers to "which external accounts has this
//! person linked", and three places to get the declared-figures rule wrong.
//! One module, and the domain is a query parameter checked against the
//! catalogue.
//!
//! ## Declared figures are accepted, and marked
//!
//! Most of the platforms this serves publish nothing machine-readable:
//! SoundCloud closed its API in 2019, Bandcamp never had one, Medium stopped,
//! Apple Podcasts has none. Refusing them would erase the recorded career of
//! most musicians and most bloggers on the platform. Accepting their figures
//! as checked would make the craft score a self-assessment.
//!
//! So they are accepted, `figures_are_declared` is set, `verified_at` stays
//! NULL, and each domain's craft score counts them at a discount. The response
//! says so out loud rather than leaving it to be inferred.
//!
//! ## What this module does not do
//!
//! Fetch. A platform with an API is refreshed by the sweep in
//! `services::portfolio_sync`, which sets `figures_are_declared = FALSE` on
//! what it fetched. Declaring by hand and fetching are two different claims,
//! and the column is what separates them.

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::middleware::AuthUser;

pub fn portfolio_account_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/portfolios",
            get(my_portfolios).post(declare_portfolio),
        )
        .route("/portfolios/{id}", axum::routing::delete(drop_portfolio))
        .route("/portfolio-platforms", get(list_platforms))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct PortfolioQuery {
    /// `audio`, `communication`, `education`, … Absent means every account
    /// the caller has linked, whatever domain it belongs to.
    #[param(max_length = 30)]
    pub domain: Option<String>,
}

/// One external platform somebody can link an account from.
#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct PlatformRow {
    pub slug: String,
    pub skill_domain: Option<String>,
    pub name: String,
    pub profile_url_pattern: Option<String>,
    /// What `items_count` counts here, in the words a profile page prints.
    pub items_label: Option<String>,
    /// What `reach_count` counts here.
    pub reach_label: Option<String>,
    /// Whether anything can check a claim automatically. `false` means every
    /// figure for this platform is the person's own word until a human looks.
    pub has_public_api: bool,
}

/// The platforms a profile can be linked from, optionally for one domain.
///
/// Public and unauthenticated: it is a catalogue, and a form that has to
/// authenticate before it can render its own options is a form that renders
/// twice.
#[utoipa::path(
    get, path = "/api/portfolio-platforms", tag = "profile",
    params(PortfolioQuery),
    responses((status = 200, description = "Platforms", body = ApiResponse<Vec<PlatformRow>>)),
)]
pub async fn list_platforms(
    State(state): State<AppState>,
    Query(q): Query<PortfolioQuery>,
) -> Result<Json<ApiResponse<Vec<PlatformRow>>>, AppError> {
    crate::validators::check_skill_domain_opt(&q.domain, "domain")?;

    let rows: Vec<PlatformRow> = sqlx::query_as(
        "SELECT slug, skill_domain, name, profile_url_pattern,
                items_label, reach_label, has_public_api
           FROM portfolio_platforms
          WHERE $1::VARCHAR IS NULL OR skill_domain = $1 OR skill_domain IS NULL
          ORDER BY sort_order",
    )
    .bind(q.domain.as_deref())
    .fetch_all(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(rows)))
}

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct PortfolioRow {
    pub id: Uuid,
    pub platform: String,
    /// The domain that platform belongs to, so one listing can be grouped
    /// without a second request.
    pub skill_domain: Option<String>,
    pub handle: String,
    pub profile_url: String,
    pub items_count: Option<i32>,
    pub reach_count: Option<i64>,
    pub figures_are_declared: bool,
    pub verified_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_synced_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// The accounts the caller has linked.
#[utoipa::path(
    get, path = "/api/portfolios", tag = "profile",
    params(PortfolioQuery),
    responses((status = 200, description = "Linked accounts", body = ApiResponse<Vec<PortfolioRow>>)),
    security(("cookie_auth" = [])),
)]
pub async fn my_portfolios(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<PortfolioQuery>,
) -> Result<Json<ApiResponse<Vec<PortfolioRow>>>, AppError> {
    crate::validators::check_skill_domain_opt(&q.domain, "domain")?;

    let rows: Vec<PortfolioRow> = sqlx::query_as(
        "SELECT p.id, p.platform, pf.skill_domain, p.handle, p.profile_url,
                p.items_count, p.reach_count, p.figures_are_declared,
                p.verified_at, p.last_synced_at
           FROM user_external_portfolios p
           JOIN portfolio_platforms pf ON pf.slug = p.platform
          WHERE p.user_id = $1
            AND ($2::VARCHAR IS NULL OR pf.skill_domain = $2)
          ORDER BY pf.sort_order",
    )
    .bind(auth.user_id)
    .bind(q.domain.as_deref())
    .fetch_all(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(rows)))
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DeclarePortfolioBody {
    /// One of the rows of `portfolio_platforms` — see
    /// `GET /api/portfolio-platforms`.
    pub platform: String,
    pub handle: String,
    pub profile_url: String,
    /// Repositories, tracks, articles, videos, courses — whatever the
    /// platform's `items_label` says.
    pub items_count: Option<i32>,
    /// Stars, plays, views, enrolments, where the platform shows them.
    pub reach_count: Option<i64>,
}

/// Link an account, with the figures the person reads on it.
///
/// Whether a sync will ever replace those figures depends on the platform,
/// and the answer is in `portfolio_platforms.has_public_api`. The response
/// repeats it, because it changes what the numbers are worth.
#[utoipa::path(
    post, path = "/api/portfolios", tag = "profile",
    request_body = DeclarePortfolioBody,
    responses(
        (status = 200, description = "Linked", body = ApiResponse<serde_json::Value>),
        (status = 400, description = "Unknown platform", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn declare_portfolio(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<DeclarePortfolioBody>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    crate::validators::check_max_len(&body.handle, "handle", 120)?;
    crate::validators::validate_url(&body.profile_url, "profile_url", 500)?;
    if !body.profile_url.starts_with("https://") {
        return Err(AppError::Validation(
            "profile_url must start with https://".into(),
        ));
    }
    if body.handle.trim().is_empty() {
        return Err(AppError::Validation("handle cannot be empty".into()));
    }
    if body.items_count.is_some_and(|n| n < 0) || body.reach_count.is_some_and(|n| n < 0) {
        return Err(AppError::Validation("a count cannot be negative".into()));
    }

    let known: Option<bool> =
        sqlx::query_scalar("SELECT has_public_api FROM portfolio_platforms WHERE slug = $1")
            .bind(&body.platform)
            .fetch_optional(&state.db)
            .await?;

    let Some(has_public_api) = known else {
        // The list rather than a bare 400: the caller is filling in a form and
        // needs to know what it accepts.
        let options: Vec<String> =
            sqlx::query_scalar("SELECT slug FROM portfolio_platforms ORDER BY sort_order")
                .fetch_all(&state.db)
                .await?;
        return Err(AppError::Validation(format!(
            "platform must be one of: {}",
            options.join(", ")
        )));
    };

    // Sync is enabled only where something can actually fetch. Enabling it on
    // a platform with no API would put the row in the staleness queue forever,
    // failing every pass.
    let id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO user_external_portfolios
            (user_id, platform, handle, profile_url, items_count, reach_count,
             figures_are_declared, sync_enabled)
        VALUES ($1, $2, $3, $4, $5, $6, TRUE, $7)
        ON CONFLICT (user_id, platform, handle) DO UPDATE
            SET profile_url = EXCLUDED.profile_url,
                items_count = EXCLUDED.items_count,
                reach_count = EXCLUDED.reach_count,
                updated_at = NOW()
        RETURNING id
        "#,
    )
    .bind(auth.user_id)
    .bind(&body.platform)
    .bind(body.handle.trim())
    .bind(body.profile_url.trim())
    .bind(body.items_count)
    .bind(body.reach_count)
    .bind(has_public_api)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(json!({
        "id": id,
        // Said out loud, because it changes what the figures are worth: on a
        // platform with no API nothing here can ever be checked.
        "figures_are_declared": true,
        "will_be_refreshed": has_public_api,
    }))))
}

/// Unlink an account.
#[utoipa::path(
    delete, path = "/api/portfolios/{id}", tag = "profile",
    params(("id" = Uuid, Path, description = "Portfolio row")),
    responses(
        (status = 200, description = "Removed", body = ApiResponse<serde_json::Value>),
        (status = 404, description = "Not the caller's", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn drop_portfolio(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    let done = sqlx::query("DELETE FROM user_external_portfolios WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(auth.user_id)
        .execute(&state.db)
        .await?;
    if done.rows_affected() == 0 {
        return Err(AppError::NotFound("portfolio not found".into()));
    }
    Ok(Json(ApiResponse::new(json!({ "removed": true }))))
}
