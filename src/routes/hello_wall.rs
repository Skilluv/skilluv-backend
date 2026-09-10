//! The wall of HELLOs, served publicly.
//!
//! Public because the thing on it is a self introduction, and because the
//! rite says so before anybody uploads (migration 0627). Signed out on
//! purpose: the first screen somebody sees of Skilluv should be the people
//! already on it, not a login form.
//!
//! The entitlement is not in this file. `services::hello_wall` decides what
//! may be shown and signs the URLs, so there is one place to read rather than
//! a handler and a service that could drift.

use axum::Router;
use axum::extract::{Query, State};
use axum::routing::get;
use serde::Deserialize;
use serde_json::{Value, json};
use utoipa::IntoParams;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::services::hello_wall;

/// Enough to fill a screen and stop.
///
/// A wall is looked at, not paged through. Somebody who wants everybody looks
/// at the talent search, which is built for that and has the filters for it.
const DEFAULT_LIMIT: i64 = 24;
const MAX_LIMIT: i64 = 60;

pub fn hello_wall_routes() -> Router<AppState> {
    Router::new().route("/hello-wall", get(wall))
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct WallQuery {
    /// Which trade's wall. One of the twelve domains.
    #[param(value_type = crate::validators::SkillDomain)]
    domain: String,
    #[param(minimum = 1, maximum = 60)]
    limit: Option<i64>,
}

/// GET /api/hello-wall?domain=design
#[utoipa::path(
    get, path = "/api/hello-wall", tag = "onboarding",
    operation_id = "helloWall",
    params(WallQuery),
    responses(
        (status = 200, description = "The HELLOs of one domain, newest first", body = serde_json::Value),
        (status = 400, description = "No such domain", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn wall(
    State(state): State<AppState>,
    Query(query): Query<WallQuery>,
) -> Result<axum::Json<Value>, AppError> {
    crate::validators::check_skill_domain(&query.domain, "domain")?;
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);

    let entries = hello_wall::entries(&state.db, &state.storage, &query.domain, limit).await?;

    Ok(axum::Json(json!({
        "data": {
            "domain": query.domain,
            "entries": entries,
        },
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })))
}
