//! SKI-38 (Post-MVP T1-03) — personal goals CRUD.
//!
//! Endpoints:
//!   POST   /api/users/me/goals        (auth) — create
//!   GET    /api/users/me/goals        (auth) — list with live progress
//!   GET    /api/users/me/goals/{id}   (auth) — one goal with progress
//!   PATCH  /api/users/me/goals/{id}   (auth) — move or clear the deadline
//!   DELETE /api/users/me/goals/{id}   (auth) — abandon
//!
//! Progress is never stored, only computed — see `services::goals`. That
//! means GET is the expensive verb here and POST is trivial, which is the
//! opposite of most of the API, and deliberate: a stored percentage would
//! be wrong the moment a deliverable is verified elsewhere.
//!
//! A goal's `kind` and target are immutable. Changing what you are aiming
//! at is a different goal, and letting it mutate would make `achieved_at`
//! meaningless (achieved... at what?). Only the deadline moves.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::goals;

/// Cap on live goals per user. Goal-setting stops being useful well before
/// this; the limit exists so the derived-progress listing stays bounded.
const MAX_LIVE_GOALS: i64 = 20;

pub fn goal_routes() -> Router<AppState> {
    Router::new()
        .route("/users/me/goals", post(create).get(list_mine))
        .route(
            "/users/me/goals/{id}",
            get(fetch).patch(update).delete(remove),
        )
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

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateGoalBody {
    /// `rank` | `skill_level` | `capability` | `artifact_count`
    pub kind: String,
    /// Interpretation depends on `kind` — see `services::goals`.
    pub target_value: String,
    /// Required for `skill_level`, rejected for every other kind.
    #[serde(default)]
    pub target_skill_id: Option<Uuid>,
    /// Optional self-imposed deadline. Must be in the future.
    #[serde(default)]
    pub deadline: Option<chrono::NaiveDate>,
}

/// Set a goal. Progress is recomputed on every read, never stored.
#[utoipa::path(
    post, path = "/api/users/me/goals", tag = "profile",
    request_body = CreateGoalBody,
    responses(
        (status = 201, description = "The goal was set"),
        (status = 409, description = "A live goal of that kind already exists", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn create(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateGoalBody>,
) -> Result<impl IntoResponse, AppError> {
    goals::validate_target(
        &state.db,
        &body.kind,
        &body.target_value,
        body.target_skill_id,
    )
    .await?;

    if let Some(d) = body.deadline
        && d <= chrono::Utc::now().date_naive()
    {
        return Err(AppError::Validation(
            "deadline must be a future date".into(),
        ));
    }

    let live: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_goals WHERE user_id = $1 AND archived_at IS NULL",
    )
    .bind(auth.user_id)
    .fetch_one(&state.db)
    .await?;
    if live >= MAX_LIVE_GOALS {
        return Err(AppError::Validation(format!(
            "at most {MAX_LIVE_GOALS} live goals — archive or delete one first"
        )));
    }

    // The partial unique index rejects a duplicate live goal. Translate it
    // to a 409 instead of letting a raw constraint violation become a 500.
    let inserted: Result<goals::Goal, sqlx::Error> = sqlx::query_as(
        r#"
        INSERT INTO user_goals (user_id, kind, target_value, target_skill_id, deadline)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING *
        "#,
    )
    .bind(auth.user_id)
    .bind(&body.kind)
    .bind(&body.target_value)
    .bind(body.target_skill_id)
    .bind(body.deadline)
    .fetch_one(&state.db)
    .await;

    let goal = match inserted {
        Ok(g) => g,
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            return Err(AppError::Conflict(
                "you already have a live goal for this target".into(),
            ));
        }
        Err(e) => return Err(e.into()),
    };

    // Echo the freshly computed progress: a goal can be created already
    // met (targeting a rank you just reached), and the client should see
    // that immediately rather than after a refresh.
    let progress = goals::compute_progress(&state.db, auth.user_id, goal.id).await?;

    Ok((StatusCode::CREATED, Json(wrap(json!({ "goal": progress })))))
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct ListGoalsQuery {
    /// Include archived goals (achieved or expired). Off by default.
    #[serde(default)]
    pub include_archived: bool,
}

/// The caller's goals with their progress recomputed on read.
#[utoipa::path(
    get, path = "/api/users/me/goals", tag = "profile",
    params(ListGoalsQuery),
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn list_mine(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<ListGoalsQuery>,
) -> Result<impl IntoResponse, AppError> {
    let goals = goals::list_with_progress(&state.db, auth.user_id, q.include_archived).await?;
    Ok(Json(wrap(json!({ "goals": goals }))))
}

/// One of the caller's goals, with its progress.
#[utoipa::path(
    get, path = "/api/users/me/goals/{id}", tag = "profile",
    params(("id" = uuid::Uuid, Path)),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "No goal of yours with that id", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn fetch(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let progress = goals::compute_progress(&state.db, auth.user_id, id).await?;
    Ok(Json(wrap(json!({ "goal": progress }))))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateGoalBody {
    /// New deadline. Explicit `null` clears it; omitting the field leaves
    /// it untouched — hence the double Option.
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub deadline: Option<Option<chrono::NaiveDate>>,
}

/// Serde helper: missing field → `None`, JSON `null` → `Some(None)`,
/// value → `Some(Some(v))`. Same trick as `routes::admin_slices`; kept
/// local rather than shared because it is three lines and exporting it
/// would couple two unrelated route modules.
fn deserialize_double_option<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: serde::Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    Ok(Some(Option::<T>::deserialize(deserializer)?))
}

/// Change a goal's target or its deadline.
#[utoipa::path(
    patch, path = "/api/users/me/goals/{id}", tag = "profile",
    params(("id" = uuid::Uuid, Path)),
    request_body = UpdateGoalBody,
    responses(
        (status = 200, description = "Updated"),
        (status = 404, description = "No goal of yours with that id", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn update(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateGoalBody>,
) -> Result<impl IntoResponse, AppError> {
    let Some(deadline) = body.deadline else {
        // Nothing to change — return current state rather than a no-op 204,
        // so the client always gets fresh progress from this verb.
        let progress = goals::compute_progress(&state.db, auth.user_id, id).await?;
        return Ok(Json(wrap(json!({ "goal": progress }))));
    };

    if let Some(d) = deadline
        && d <= chrono::Utc::now().date_naive()
    {
        return Err(AppError::Validation(
            "deadline must be a future date".into(),
        ));
    }

    // Archived goals are settled history — moving their deadline would
    // resurrect them outside the archival job's control.
    let affected = sqlx::query(
        "UPDATE user_goals SET deadline = $3
          WHERE id = $1 AND user_id = $2 AND archived_at IS NULL",
    )
    .bind(id)
    .bind(auth.user_id)
    .bind(deadline)
    .execute(&state.db)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(AppError::NotFound(format!("live goal {id} not found")));
    }

    let progress = goals::compute_progress(&state.db, auth.user_id, id).await?;
    Ok(Json(wrap(json!({ "goal": progress }))))
}

/// Drop a goal.
#[utoipa::path(
    delete, path = "/api/users/me/goals/{id}", tag = "profile",
    params(("id" = uuid::Uuid, Path)),
    responses(
        (status = 204, description = "Deleted"),
        (status = 404, description = "No goal of yours with that id", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn remove(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    // Hard delete: an abandoned goal is not history worth keeping, and
    // `achieved_at`/`archived_at` already cover the "kept" outcomes.
    let affected = sqlx::query("DELETE FROM user_goals WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(auth.user_id)
        .execute(&state.db)
        .await?
        .rows_affected();

    if affected == 0 {
        return Err(AppError::NotFound(format!("goal {id} not found")));
    }
    Ok(StatusCode::NO_CONTENT)
}
