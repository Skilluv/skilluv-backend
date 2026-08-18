//! Who the platform puts forward, and who decides.
//!
//! Four endpoints. Three are public — a featuring nobody can read is a
//! distinction nobody can check — and one is the editorial act itself.
//!
//! Nothing is posted to a social network from here. [`card`] returns
//! everything a post needs; who presses send is a person. Publishing
//! somebody's name and work to a third-party platform on a schedule, with no
//! human between the decision and the post, is not a feature.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::NaiveDate;
use serde::Deserialize;
use serde_json::json;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::featured;

pub fn featured_routes() -> Router<AppState> {
    Router::new()
        .route("/featured/{domain}", get(this_week))
        .route("/featured/{domain}/recent", get(recent))
}

pub fn admin_featured_routes() -> Router<AppState> {
    Router::new()
        .route("/admin/featured", post(feature))
        .route("/admin/featured/{domain}/{week}/card", get(card))
}

fn wrap(data: serde_json::Value) -> serde_json::Value {
    json!({
        "data": data,
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })
}

// ═══════════════════════════════════════════════════════════════════
// GET /featured/{domain}
// ═══════════════════════════════════════════════════════════════════

/// Who is featured in this domain this week.
///
/// Returns `null` rather than 404 when nobody is: a week with nobody featured
/// is a normal week, and a 404 would make a quiet week look like a broken
/// page.
#[utoipa::path(
    get, path = "/api/featured/{domain}", tag = "profile",
    params(("domain" = String, Path, description = "skill domain")),
    responses(
        (status = 200, description = "this week's featured talent, or null"),
        (status = 400, description = "unknown domain", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn this_week(
    State(state): State<AppState>,
    Path(domain): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    crate::validators::validate_skill_domain(&domain, "domain")?;
    let featured = featured::of_week(&state.db, &domain, featured::current_week()).await?;
    Ok(Json(wrap(json!({ "featured": featured }))))
}

// ═══════════════════════════════════════════════════════════════════
// GET /featured/{domain}/recent
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct RecentQuery {
    /// How many weeks back, 1..52.
    #[param(minimum = 1, maximum = 52)]
    pub limit: Option<i64>,
}

/// The last weeks of a domain, newest first.
#[utoipa::path(
    get, path = "/api/featured/{domain}/recent", tag = "profile",
    params(("domain" = String, Path, description = "skill domain"), RecentQuery),
    responses(
        (status = 200, description = "past featurings, newest first"),
        (status = 400, description = "unknown domain", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn recent(
    State(state): State<AppState>,
    Path(domain): Path<String>,
    Query(q): Query<RecentQuery>,
) -> Result<impl IntoResponse, AppError> {
    crate::validators::validate_skill_domain(&domain, "domain")?;
    let rows = featured::recent(&state.db, &domain, q.limit.unwrap_or(12)).await?;
    Ok(Json(wrap(json!({ "featured": rows }))))
}

// ═══════════════════════════════════════════════════════════════════
// POST /admin/featured
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct FeatureBody {
    pub skill_domain: String,
    /// The Monday of the week being awarded. Refused if it is not a Monday,
    /// rather than silently rounded: rounding somebody's intent is how a
    /// featuring lands on the wrong week.
    pub week_of: NaiveDate,
    pub user_id: Uuid,
    /// Why this person, this week. Published as written, minimum forty
    /// characters.
    pub reason_md: String,
    /// The work being pointed at. Optional: somebody can be put forward for a
    /// body of work rather than one piece.
    #[serde(default)]
    pub deliverable_id: Option<Uuid>,
}

/// Put somebody forward for a week.
///
/// One per domain per week — two people featured in one week means neither
/// was, and the scarcity is the whole value.
#[utoipa::path(
    post, path = "/api/admin/featured", tag = "admin",
    request_body = FeatureBody,
    responses(
        (status = 201, description = "featured"),
        (status = 400, description = "no reason, not a Monday, or nothing proven in the domain",
         body = crate::api_response::ErrorResponse),
        (status = 409, description = "the week is taken, or this person was featured recently",
         body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn feature(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<FeatureBody>,
) -> Result<impl IntoResponse, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;

    let featured = featured::feature(
        &state.db,
        &body.skill_domain,
        body.week_of,
        body.user_id,
        &body.reason_md,
        body.deliverable_id,
        auth.user_id,
        &state.config.frontend_url,
    )
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(wrap(json!({ "featured": featured }))),
    ))
}

// ═══════════════════════════════════════════════════════════════════
// GET /admin/featured/{domain}/{week}/card
// ═══════════════════════════════════════════════════════════════════

/// The post a person will send.
///
/// Composed here rather than in a client so that whoever posts it does not
/// rebuild the profile URL, and so the same words go to every network.
#[utoipa::path(
    get, path = "/api/admin/featured/{domain}/{week}/card", tag = "admin",
    params(
        ("domain" = String, Path, description = "skill domain"),
        ("week" = String, Path, description = "the Monday of the week, YYYY-MM-DD"),
    ),
    responses(
        (status = 200, description = "headline, body and links"),
        (status = 404, description = "nobody was featured that week",
         body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn card(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((domain, week)): Path<(String, NaiveDate)>,
) -> Result<impl IntoResponse, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    crate::validators::validate_skill_domain(&domain, "domain")?;

    let featured = featured::of_week(&state.db, &domain, week)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("nobody was featured in {domain} that week")))?;

    let card = featured::card(&state.db, &featured, &state.config.frontend_url).await?;
    Ok(Json(wrap(json!({ "card": card }))))
}
