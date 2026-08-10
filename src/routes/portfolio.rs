//! Routes HTTP portfolio export (Phase P7).
//!
//! Endpoints publics par username (pas UUID — URL partageables) :
//!   GET /api/users/{username}/portfolio.json    — JSON-LD schema.org Person
//!   GET /api/users/{username}/badge.svg         — SVG dynamique pour README

use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use serde_json::Value;

use crate::AppState;
use crate::errors::AppError;
use crate::services::PortfolioService;

pub fn portfolio_routes() -> Router<AppState> {
    Router::new()
        .route("/users/{username}/portfolio.json", get(portfolio_json))
        .route("/users/{username}/badge.svg", get(badge_svg))
}

/// Public portfolio as JSON-LD (schema.org Person). The exact shape is
/// defined by schema.org, not by Skilluv, so it is documented here as a
/// free-form JSON object — the front consumes it verbatim for SSR meta.
#[utoipa::path(
    get,
    path = "/api/users/{username}/portfolio.json",
    tag = "profile",
    params(("username" = String, Path, description = "Public username")),
    responses(
        (status = 200, description = "JSON-LD schema.org Person", body = serde_json::Value),
        (status = 404, description = "User not found", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn portfolio_json(
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> Result<Json<Value>, AppError> {
    let portfolio =
        PortfolioService::build_portfolio_json(&state.db, &username, &state.config.frontend_url)
            .await?;
    Ok(Json(portfolio))
}

/// SVG badge for embedding in a README. Not JSON — served as
/// `image/svg+xml` with 15-min public cache.
#[utoipa::path(
    get,
    path = "/api/users/{username}/badge.svg",
    tag = "profile",
    params(("username" = String, Path, description = "Public username")),
    responses(
        (status = 200, description = "SVG badge", content_type = "image/svg+xml"),
        (status = 404, description = "User not found", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn badge_svg(
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let svg = PortfolioService::build_badge_svg(&state.db, &username).await?;

    let mut headers = HeaderMap::new();
    headers.insert(
        "Content-Type",
        HeaderValue::from_static("image/svg+xml; charset=utf-8"),
    );
    // Cache 15 minutes — le badge n'a pas besoin d'être temps réel
    headers.insert(
        "Cache-Control",
        HeaderValue::from_static("public, max-age=900"),
    );

    Ok((StatusCode::OK, headers, svg))
}
