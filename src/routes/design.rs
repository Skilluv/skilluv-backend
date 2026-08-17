//! The design critique loop, over HTTP.
//!
//! Four endpoints, and the shape of the trade is visible in them: a designer
//! hands in a version, a reviewer answers with one of three verdicts, anybody
//! can read the whole trail, and a reviewer sees the queue of what they are
//! competent to judge.
//!
//! Contests are not here. A design contest is a `brief_contest` on the
//! tournament routes, because a contest is the same event whatever the
//! subject — see `services::contest`.
//!
//! Authorisation lives in the service (`design_reviewer:{group}` resolved
//! from the slice's trade) and in the database (the five-round ceiling, the
//! blocking-reason rules). This layer validates payload shape and nothing
//! else, so there is one place per rule.

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
use crate::services::design_reviews::{self, ReviewInput, Verdict};

/// Long enough for a reviewer's morning, short enough that nobody paginates
/// through a queue whose whole point is "the oldest wait first".
const MAX_QUEUE_LIMIT: i64 = 100;

pub fn design_routes() -> Router<AppState> {
    Router::new()
        .route("/design/slices/{id}/versions", post(submit_version))
        .route("/design/slices/{id}/reviews", get(history).post(review))
        .route("/design/reviews/queue", get(reviewer_queue))
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
// POST /design/slices/{id}/versions
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SubmitVersionBody {
    /// Where this version lives: a versioned Figma node, a hosted board, a
    /// published project, or a stored object.
    pub artifact_url: String,
    /// What changed since the previous version. Optional on the first one,
    /// and the single most useful thing to write on any later one.
    #[serde(default)]
    pub notes_md: Option<String>,
}

/// Hand in a version and ask for a critique.
#[utoipa::path(
    post, path = "/api/design/slices/{id}/versions", tag = "design",
    params(("id" = Uuid, Path, description = "design slice id")),
    request_body = SubmitVersionBody,
    responses(
        (status = 201, description = "version recorded, the slice now waits for a critique"),
        (status = 403, description = "not the designer who claimed this challenge"),
        (status = 409, description = "the slice is not in a state a version can be handed in from"),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn submit_version(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<SubmitVersionBody>,
) -> Result<impl IntoResponse, AppError> {
    let slice = design_reviews::submit_version(
        &state.db,
        id,
        auth.user_id,
        &body.artifact_url,
        body.notes_md.as_deref(),
    )
    .await?;
    Ok((StatusCode::CREATED, Json(wrap(json!({ "slice": slice })))))
}

// ═══════════════════════════════════════════════════════════════════
// GET /design/slices/{id}/reviews
// ═══════════════════════════════════════════════════════════════════

/// The whole critique trail, oldest round first.
///
/// Public, and deliberately so: the sequence of rounds is the most convincing
/// thing a designer can show, far more than the final image on its own. Hiding
/// it would leave the profile saying "validated" and nothing about how.
#[utoipa::path(
    get, path = "/api/design/slices/{id}/reviews", tag = "design",
    params(("id" = Uuid, Path, description = "design slice id")),
    responses((status = 200, description = "every round: what was read, what was found")),
)]
pub async fn history(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let rounds = design_reviews::history(&state.db, id).await?;
    Ok(Json(wrap(json!({ "rounds": rounds }))))
}

// ═══════════════════════════════════════════════════════════════════
// POST /design/slices/{id}/reviews
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ReviewBody {
    /// `approve`, `iterate` or `reject`.
    pub verdict: String,
    /// What kind of problem. Required for `iterate` and `reject`.
    #[serde(default)]
    pub blocking_reason: Option<String>,
    /// The written critique. At least 40 characters when the verdict is not
    /// an approval.
    #[serde(default)]
    pub feedback_md: Option<String>,
    /// The family review grid, filled in. Free-form JSON so a grid can be
    /// revised without a migration; the criteria are in `review_grids`.
    #[serde(default)]
    pub grid_scores: Option<serde_json::Value>,
}

/// Answer a version.
///
/// Requires review rights for the slice's trade, and refuses a reviewer who
/// claimed the challenge themselves.
#[utoipa::path(
    post, path = "/api/design/slices/{id}/reviews", tag = "design",
    params(("id" = Uuid, Path, description = "design slice id")),
    request_body = ReviewBody,
    responses(
        (status = 200, description = "critique recorded and the slice moved"),
        (status = 400, description = "unknown verdict, missing reason, or feedback too short"),
        (status = 403, description = "no review rights for this trade, or own challenge"),
        (status = 409, description = "no version is waiting, or somebody answered first"),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn review(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ReviewBody>,
) -> Result<impl IntoResponse, AppError> {
    let verdict = Verdict::parse(&body.verdict).ok_or_else(|| {
        AppError::Validation(format!(
            "unknown verdict '{}': expected approve, iterate or reject",
            body.verdict
        ))
    })?;

    let slice = design_reviews::review(
        &state.db,
        id,
        auth.user_id,
        ReviewInput {
            verdict,
            blocking_reason: body.blocking_reason.as_deref(),
            feedback_md: body.feedback_md.as_deref(),
            grid_scores: body.grid_scores,
        },
    )
    .await?;
    Ok(Json(wrap(json!({ "slice": slice }))))
}

// ═══════════════════════════════════════════════════════════════════
// GET /design/reviews/queue
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct QueueQuery {
    /// How many to return, 1..100. Defaults to 25.
    #[param(minimum = 1, maximum = 100)]
    pub limit: Option<i64>,
}

/// Versions waiting for a critique, in the trades this reviewer is competent
/// in, oldest first.
///
/// Returns an empty list rather than a refusal for somebody holding no review
/// capability: "nothing for you to do" is the honest answer, and a 403 on a
/// queue reads as a bug.
#[utoipa::path(
    get, path = "/api/design/reviews/queue", tag = "design",
    params(QueueQuery),
    responses((status = 200, description = "slices awaiting a critique, oldest first")),
    security(("cookie_auth" = [])),
)]
pub async fn reviewer_queue(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<QueueQuery>,
) -> Result<impl IntoResponse, AppError> {
    let limit = q.limit.unwrap_or(25).clamp(1, MAX_QUEUE_LIMIT);
    let slices = design_reviews::reviewer_queue(&state.db, auth.user_id, limit).await?;
    Ok(Json(wrap(json!({ "slices": slices }))))
}
