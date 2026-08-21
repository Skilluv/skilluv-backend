//! What languages someone actually works in, and what the community works in.
//!
//! Both answers come from `project_slices.code_languages` on slices that
//! produced a verified deliverable. Nothing here reads a self-declared
//! profile field: the question is what a person has shipped, and a list they
//! typed answers a different one.

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;

pub fn code_stats_routes() -> Router<AppState> {
    Router::new()
        .route("/users/{username}/code-languages", get(user_languages))
        .route("/code/languages/top", get(top_languages))
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
pub struct LanguageCount {
    pub language: String,
    /// Verified artefacts touching this language. A slice spanning two
    /// languages counts once for each — it is one piece of work, and both
    /// statements about it are true.
    pub artifacts: i64,
}

/// The languages one person has verified work in.
#[utoipa::path(
    get, path = "/api/users/{username}/code-languages", tag = "profile",
    params(("username" = String, Path, description = "Username")),
    responses(
        (status = 200, description = "Languages, most-used first", body = ApiResponse<Vec<LanguageCount>>),
        (status = 404, description = "No such user", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn user_languages(
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> Result<Json<ApiResponse<Vec<LanguageCount>>>, AppError> {
    let user_id: Option<uuid::Uuid> =
        sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
            .bind(&username)
            .fetch_optional(&state.db)
            .await?;

    let Some(user_id) = user_id else {
        return Err(AppError::NotFound(format!("user '{username}' not found")));
    };

    let rows: Vec<LanguageCount> = sqlx::query_as(
        r#"
        SELECT language, count(DISTINCT ps.id) AS artifacts
          FROM project_slices ps
          JOIN deliverables d ON d.slice_id = ps.id
          CROSS JOIN LATERAL unnest(ps.code_languages) AS language
         WHERE d.user_id = $1
           AND d.verification_status = 'verified'
         GROUP BY language
         ORDER BY artifacts DESC, language
        "#,
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(rows)))
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct TopQuery {
    /// How many to return. Capped, because this is a public endpoint and the
    /// list is a leaderboard, not an export.
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    20
}

/// What the community works in.
#[utoipa::path(
    get, path = "/api/code/languages/top", tag = "profile",
    params(TopQuery),
    responses(
        (status = 200, description = "Languages, most-used first", body = ApiResponse<Vec<LanguageCount>>),
    ),
)]
pub async fn top_languages(
    State(state): State<AppState>,
    Query(q): Query<TopQuery>,
) -> Result<Json<ApiResponse<Vec<LanguageCount>>>, AppError> {
    let limit = q.limit.clamp(1, 100);

    let rows: Vec<LanguageCount> = sqlx::query_as(
        r#"
        SELECT language, count(DISTINCT ps.id) AS artifacts
          FROM project_slices ps
          JOIN deliverables d ON d.slice_id = ps.id
          CROSS JOIN LATERAL unnest(ps.code_languages) AS language
         WHERE d.verification_status = 'verified'
         GROUP BY language
         ORDER BY artifacts DESC, language
         LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(rows)))
}
