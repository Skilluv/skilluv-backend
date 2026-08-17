//! The public feed — reading it, and choosing whether to be on it.
//!
//! The read endpoint is the only one on the platform with no authentication
//! and no rate-limit exemption: it is what a landing page polls every thirty
//! seconds, from every visitor. It touches one table with one predicate,
//! which is the whole reason that table exists.

use axum::extract::{Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::public_feed;

/// Below this many artefacts a day, the honest presentation is a "latest
/// work" list rather than a live ticker. A feed whose first line says "two
/// days ago" proves the place is empty.
const LIVE_THRESHOLD_PER_DAY: f64 = 5.0;

pub fn public_feed_routes() -> Router<AppState> {
    Router::new()
        .route("/feed/public", get(read_feed))
        .route(
            "/users/me/public-feed-preferences",
            get(my_preferences).post(set_preference),
        )
        .route("/users/me/public-feed-preferences/withdraw", post(withdraw))
}

fn build_response(data: Value) -> Value {
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
pub struct FeedQuery {
    /// Opaque cursor from the previous page's `next_cursor`.
    #[param(max_length = 100)]
    pub after: Option<String>,
    #[serde(default = "default_limit")]
    #[param(minimum = 1, maximum = 100)]
    pub limit: i64,
    /// Restrict to one kind of event.
    #[param(max_length = 40)]
    pub kind: Option<String>,
}

fn default_limit() -> i64 {
    public_feed::DEFAULT_PAGE
}

/// What has come out of the forge, newest first.
///
/// Public, unauthenticated, and every line carries a URL a stranger can open.
/// `live` says whether there is enough here to be worth a pulsing dot — the
/// caller is expected to honour it, because a live badge over a two-day-old
/// first line is the fabricated social proof this replaced.
#[utoipa::path(
    get, path = "/api/feed/public", tag = "feed",
    params(FeedQuery),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Unusable cursor", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn read_feed(
    State(state): State<AppState>,
    Query(q): Query<FeedQuery>,
) -> Result<Json<Value>, AppError> {
    crate::validators::check_max_len_opt(&q.after, "after", 100)?;
    crate::validators::check_max_len_opt(&q.kind, "kind", 40)?;
    if !(1..=public_feed::MAX_PAGE).contains(&q.limit) {
        return Err(AppError::Validation(format!(
            "limit must be between 1 and {}",
            public_feed::MAX_PAGE
        )));
    }

    // A broken cursor is refused rather than treated as "from the beginning":
    // silently restarting makes a client re-read the whole feed and never
    // find out why.
    let cursor = match q.after.as_deref() {
        Some(raw) => Some(
            public_feed::Cursor::decode(raw)
                .ok_or_else(|| AppError::Validation("that cursor is unusable".into()))?,
        ),
        None => None,
    };

    let page = public_feed::page(&state.db, cursor, q.limit, q.kind.as_deref()).await?;
    let density = public_feed::density_last_days(&state.db, 7).await?;

    Ok(Json(build_response(json!({
        "items": page.items,
        "next_cursor": page.next_cursor,
        // Not a suggestion. A pulsing dot over an empty feed is the claim
        // this whole table exists to stop making.
        "live": density >= LIVE_THRESHOLD_PER_DAY,
        "artifacts_per_day": (density * 100.0).round() / 100.0,
    }))))
}

/// What can appear about you, and what you have chosen.
#[utoipa::path(
    get, path = "/api/users/me/public-feed-preferences", tag = "profile",
    responses(
        (status = 200, body = serde_json::Value),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn my_preferences(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let preferences = public_feed::preferences_for(&state.db, auth.user_id).await?;
    Ok(Json(build_response(json!({ "preferences": preferences }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PreferenceBody {
    #[schema(max_length = 40)]
    pub kind: String,
    pub visible: bool,
}

/// Choose whether one kind of event about you appears publicly.
///
/// Retroactive: turning it off takes down what is already there. Somebody
/// asking to be off the page is not asking to be off it from now on.
#[utoipa::path(
    post, path = "/api/users/me/public-feed-preferences", tag = "profile",
    request_body = PreferenceBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Not something the feed shows", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn set_preference(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<PreferenceBody>,
) -> Result<Json<Value>, AppError> {
    let changed =
        public_feed::set_preference(&state.db, auth.user_id, &body.kind, body.visible).await?;
    Ok(Json(build_response(json!({
        "kind": body.kind,
        "visible": body.visible,
        "existing_items_changed": changed,
    }))))
}

/// Take everything of yours off the feed, now.
#[utoipa::path(
    post, path = "/api/users/me/public-feed-preferences/withdraw", tag = "profile",
    responses(
        (status = 200, body = serde_json::Value),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn withdraw(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let cleared = public_feed::withdraw_entirely(&state.db, auth.user_id).await?;
    Ok(Json(build_response(json!({
        "withdrawn": true,
        "items_removed": cleared,
    }))))
}
