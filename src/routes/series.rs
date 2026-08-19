//! Contests read as one event, over HTTP.
//!
//! Reading is public — an awards edition nobody can read is a press release —
//! and composing one is an editorial act, so it needs `admin`.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::series;

pub fn series_routes() -> Router<AppState> {
    Router::new()
        .route("/series", get(list))
        .route("/series/{slug}", get(one))
        .route("/series/{slug}/standings", get(standings))
}

pub fn admin_series_routes() -> Router<AppState> {
    Router::new()
        .route("/admin/series", post(create))
        .route("/admin/series/{slug}/tournaments", post(attach))
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

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct ListQuery {
    /// `awards_edition`, `sprint` or `programme`.
    #[param(max_length = 30)]
    pub kind: Option<String>,
    #[param(minimum = 1, maximum = 100)]
    pub limit: Option<i64>,
}

/// Every series, newest first.
#[utoipa::path(
    get, path = "/api/series", tag = "challenges",
    params(ListQuery),
    responses((status = 200, description = "series, newest first")),
    operation_id = "seriesList",
)]
pub async fn list(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<impl IntoResponse, AppError> {
    crate::validators::check_max_len_opt(&q.kind, "kind", 30)?;
    let rows = series::list(&state.db, q.kind.as_deref(), q.limit.unwrap_or(25)).await?;
    Ok(Json(wrap(json!({ "series": rows }))))
}

/// One series.
#[utoipa::path(
    get, path = "/api/series/{slug}", tag = "challenges",
    params(("slug" = String, Path)),
    responses(
        (status = 200, description = "the series"),
        (status = 404, description = "no such series", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn one(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let found = series::by_slug(&state.db, &slug).await?;
    Ok(Json(wrap(json!({ "series": found }))))
}

/// Every category of a series and its podium.
///
/// No overall winner, and that is not an omission. Summing places across
/// thirteen categories would rank somebody who entered all of them above
/// somebody who won the only category they work in — the opposite of what an
/// awards edition is for.
#[utoipa::path(
    get, path = "/api/series/{slug}/standings", tag = "challenges",
    params(("slug" = String, Path)),
    responses(
        (status = 200, description = "each category and its podium"),
        (status = 404, description = "no such series", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn standings(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let found = series::by_slug(&state.db, &slug).await?;
    let rows = series::standings(&state.db, found.id).await?;
    Ok(Json(wrap(json!({ "series": found, "categories": rows }))))
}

/// Open a series.
#[utoipa::path(
    post, path = "/api/admin/series", tag = "admin",
    request_body = series::CreateSeries,
    responses(
        (status = 201, description = "created"),
        (status = 400, description = "unknown kind or domain, or it ends before it starts",
         body = crate::api_response::ErrorResponse),
        (status = 409, description = "the slug is taken", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
    operation_id = "seriesCreate",
)]
pub async fn create(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<series::CreateSeries>,
) -> Result<impl IntoResponse, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let created = series::create(&state.db, body, auth.user_id).await?;
    Ok((
        StatusCode::CREATED,
        Json(wrap(json!({ "series": created }))),
    ))
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AttachBody {
    pub tournament_id: Uuid,
    /// What this contest is *for* inside the series — a family for an awards
    /// edition, an editorial axis for a programme. Absent for a sprint, whose
    /// contest is the whole of its series.
    #[serde(default)]
    pub category: Option<String>,
}

/// Put a contest in a series.
#[utoipa::path(
    post, path = "/api/admin/series/{slug}/tournaments", tag = "admin",
    params(("slug" = String, Path)),
    request_body = AttachBody,
    responses(
        (status = 200, description = "attached"),
        (status = 404, description = "no such series or contest", body = crate::api_response::ErrorResponse),
        (status = 409, description = "the series already has a contest in that category",
         body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn attach(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(body): Json<AttachBody>,
) -> Result<impl IntoResponse, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let found = series::by_slug(&state.db, &slug).await?;
    series::attach(
        &state.db,
        found.id,
        body.tournament_id,
        body.category.as_deref(),
    )
    .await?;
    Ok(Json(wrap(json!({ "attached": true }))))
}
