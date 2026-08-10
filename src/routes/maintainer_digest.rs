//! P26 v2 SKI-120 — public routes for maintainer digest subscribe /
//! confirm / unsubscribe. All unauthenticated; mounted OUTSIDE `/api`.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;

use crate::AppState;
use crate::errors::AppError;
use crate::services::maintainer_digest::{self, SubscribeInput};

pub fn maintainer_digest_routes() -> Router<AppState> {
    Router::new()
        .route("/maintainer-digest/subscribe", post(subscribe))
        .route("/maintainer-digest/confirm/{token}", get(confirm))
        .route("/maintainer-digest/unsubscribe/{token}", get(unsubscribe))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubscribeBody {
    pub github_login: String,
    pub email: String,
    /// e.g. ["launchbadge/sqlx", "launchbadge/sqlxmigrator"]
    pub repos: Vec<String>,
}

async fn subscribe(
    State(state): State<AppState>,
    Json(body): Json<SubscribeBody>,
) -> Result<impl IntoResponse, AppError> {
    if body.email.trim().is_empty() || body.github_login.trim().is_empty() || body.repos.is_empty()
    {
        return Err(AppError::Validation(
            "email, github_login and repos are required".into(),
        ));
    }
    for repo in &body.repos {
        if !repo.contains('/') {
            return Err(AppError::Validation(format!(
                "repo must be owner/name (got: {repo})"
            )));
        }
    }
    let sub = maintainer_digest::subscribe(
        &state.db,
        &state.email,
        &state.config.base_url,
        SubscribeInput {
            github_login: body.github_login,
            email: body.email,
            repos: body.repos,
        },
    )
    .await?;

    // Don't echo the tokens in the response — those are for the email.
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({
            "message": "confirmation email sent",
            "email": sub.email,
        })),
    ))
}

async fn confirm(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let sub = maintainer_digest::confirm(&state.db, &token).await?;
    Ok(Json(json!({
        "confirmed": true,
        "repos": sub.repos,
    })))
}

async fn unsubscribe(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    maintainer_digest::unsubscribe(&state.db, &token).await?;
    Ok(Json(json!({ "unsubscribed": true })))
}
