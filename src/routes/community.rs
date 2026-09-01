use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::models::ChallengeTemplate;

pub fn community_routes() -> Router<AppState> {
    Router::new()
        .route("/community/challenges", post(create_community_challenge))
        .route("/community/challenges/mine", get(my_challenges))
        .route(
            "/community/challenges/{id}",
            put(update_community_challenge),
        )
        .route("/community/challenges/{id}/vote", post(vote_challenge))
        .route("/community/challenges/{id}/vote", delete(unvote_challenge))
        .route("/community/challenges/popular", get(popular_challenges))
}

fn build_response(data: serde_json::Value) -> serde_json::Value {
    json!({
        "data": data,
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateCommunityChallenge {
    title: String,
    description: String,
    instructions: String,
    #[schema(schema_with = crate::validators::skill_domain_schema)]
    skill_domain: String,
    difficulty: i16,
    language: Option<String>,
    expected_output: Option<String>,
    test_cases: Option<serde_json::Value>,
    reward_fragments: Option<i32>,
    duration_minutes: Option<i32>,
    tags: Option<Vec<String>>,
    submit_for_review: Option<bool>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateCommunityChallenge {
    title: Option<String>,
    description: Option<String>,
    instructions: Option<String>,
    difficulty: Option<i16>,
    language: Option<String>,
    expected_output: Option<String>,
    test_cases: Option<serde_json::Value>,
    submit_for_review: Option<bool>,
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct PaginationQuery {
    #[param(minimum = 1, maximum = 100000)]
    pub page: Option<i64>,
    #[param(minimum = 1, maximum = 100)]
    pub per_page: Option<i64>,
}

/// Create a community-submitted challenge (draft or submit for review).
#[utoipa::path(
    post,
    path = "/api/community/challenges",
    tag = "challenges",
    request_body = CreateCommunityChallenge,
    responses(
        (status = 201, body = serde_json::Value),
        (status = 400, body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn create_community_challenge(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateCommunityChallenge>,
) -> Result<impl IntoResponse, AppError> {
    if body.title.trim().is_empty() || body.title.len() > 200 {
        return Err(AppError::Validation(
            "Title must be between 1 and 200 characters".to_string(),
        ));
    }
    if body.description.trim().is_empty() {
        return Err(AppError::Validation("Description is required".to_string()));
    }
    if body.instructions.trim().is_empty() {
        return Err(AppError::Validation(
            "Instructions are required".to_string(),
        ));
    }
    if !(1..=5).contains(&body.difficulty) {
        return Err(AppError::Validation(
            "Difficulty must be between 1 and 5".to_string(),
        ));
    }

    let community_status = if body.submit_for_review.unwrap_or(false) {
        "review"
    } else {
        "draft"
    };

    // Community challenges sont user-generated, sans project_id : on les marque
    // is_training=TRUE pour satisfaire la règle dure #1 (contrainte
    // challenges_project_or_training, migration 0061) au moment du publish.
    let challenge: ChallengeTemplate = sqlx::query_as(
        r#"
        INSERT INTO challenge_templates (
            title, description, instructions,
            title_i18n, description_i18n, instructions_i18n,
            skill_domain, difficulty,
            language, expected_output, test_cases,
            reward_fragments, duration_minutes,
            is_community, community_status, created_by, status, is_training
        ) VALUES (
            $1,$2,$3,
            jsonb_build_object('fr', $1::text),
            jsonb_build_object('fr', $2::text),
            jsonb_build_object('fr', $3::text),
            $4,$5,$6,$7,$8,$9,$10,TRUE,$11,$12,'draft',TRUE
        )
        RETURNING *
        "#,
    )
    .bind(body.title.trim())
    .bind(body.description.trim())
    .bind(body.instructions.trim())
    .bind(&body.skill_domain)
    .bind(body.difficulty)
    .bind(&body.language)
    .bind(&body.expected_output)
    .bind(&body.test_cases)
    .bind(body.reward_fragments.unwrap_or(10))
    .bind(body.duration_minutes)
    .bind(community_status)
    .bind(auth.user_id)
    .fetch_one(&state.db)
    .await?;

    // Add tags if provided
    if let Some(ref tags) = body.tags {
        for tag_name in tags {
            sqlx::query(
                r#"
                INSERT INTO challenge_tag_map (challenge_id, tag_id)
                SELECT $1, id FROM challenge_tags WHERE name = $2
                ON CONFLICT DO NOTHING
                "#,
            )
            .bind(challenge.id)
            .bind(tag_name)
            .execute(&state.db)
            .await?;
        }
    }

    Ok((
        StatusCode::CREATED,
        Json(build_response(json!({
            "challenge": challenge,
            "message": if community_status == "review" { "Challenge submitted for review" } else { "Challenge saved as draft" }
        }))),
    ))
}

/// List community challenges created by the caller (any status).
#[utoipa::path(
    get,
    path = "/api/community/challenges/mine",
    tag = "challenges",
    responses(
        (status = 200, body = serde_json::Value),
        (status = 401, body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn my_challenges(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, AppError> {
    let challenges: Vec<ChallengeTemplate> = sqlx::query_as(
        "SELECT * FROM challenge_templates WHERE created_by = $1 AND is_community = TRUE ORDER BY created_at DESC",
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(build_response(json!({ "challenges": challenges }))))
}

/// Edit a community challenge — only allowed while in draft or review.
#[utoipa::path(
    put,
    path = "/api/community/challenges/{id}",
    tag = "challenges",
    params(("id" = Uuid, Path)),
    request_body = UpdateCommunityChallenge,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, body = crate::api_response::ErrorResponse),
        (status = 404, body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn update_community_challenge(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateCommunityChallenge>,
) -> Result<Json<serde_json::Value>, AppError> {
    let existing: ChallengeTemplate = sqlx::query_as(
        "SELECT * FROM challenge_templates WHERE id = $1 AND created_by = $2 AND is_community = TRUE",
    )
    .bind(id)
    .bind(auth.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound("Challenge not found".to_string()))?;

    // Can only edit draft or review
    match existing.community_status.as_deref() {
        Some("draft") | Some("review") => {}
        _ => {
            return Err(AppError::Validation(
                "Can only edit challenges in draft or review status".to_string(),
            ));
        }
    }

    let new_status = if body.submit_for_review.unwrap_or(false) {
        "review"
    } else {
        existing.community_status.as_deref().unwrap_or("draft")
    };

    let challenge: ChallengeTemplate = sqlx::query_as(
        r#"
        UPDATE challenge_templates SET
            title = COALESCE($1, title),
            description = COALESCE($2, description),
            instructions = COALESCE($3, instructions),
            difficulty = COALESCE($4, difficulty),
            language = COALESCE($5, language),
            expected_output = COALESCE($6, expected_output),
            test_cases = COALESCE($7, test_cases),
            community_status = $8,
            updated_at = NOW()
        WHERE id = $9
        RETURNING *
        "#,
    )
    .bind(&body.title)
    .bind(&body.description)
    .bind(&body.instructions)
    .bind(body.difficulty)
    .bind(&body.language)
    .bind(&body.expected_output)
    .bind(&body.test_cases)
    .bind(new_status)
    .bind(id)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(build_response(json!({ "challenge": challenge }))))
}

/// Upvote a published community challenge. Idempotent.
#[utoipa::path(
    post,
    path = "/api/community/challenges/{id}/vote",
    tag = "challenges",
    params(("id" = Uuid, Path)),
    responses(
        (status = 201, body = serde_json::Value),
        (status = 404, body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn vote_challenge(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    // Verify challenge is published
    let exists: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM challenge_templates WHERE id = $1 AND status = 'published'")
            .bind(id)
            .fetch_optional(&state.db)
            .await?;

    if exists.is_none() {
        return Err(AppError::NotFound("Challenge not found".to_string()));
    }

    sqlx::query(
        "INSERT INTO challenge_votes (user_id, challenge_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(auth.user_id)
    .bind(id)
    .execute(&state.db)
    .await?;

    // Update vote count
    sqlx::query(
        "UPDATE challenge_templates SET vote_count = (SELECT COUNT(*) FROM challenge_votes WHERE challenge_id = $1) WHERE id = $1",
    )
    .bind(id)
    .execute(&state.db)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(build_response(json!({ "message": "Vote recorded" }))),
    ))
}

/// Remove the caller's vote from a challenge.
#[utoipa::path(
    delete,
    path = "/api/community/challenges/{id}/vote",
    tag = "challenges",
    params(("id" = Uuid, Path)),
    responses(
        (status = 200, body = serde_json::Value),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn unvote_challenge(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    sqlx::query("DELETE FROM challenge_votes WHERE user_id = $1 AND challenge_id = $2")
        .bind(auth.user_id)
        .bind(id)
        .execute(&state.db)
        .await?;

    // Update vote count
    sqlx::query(
        "UPDATE challenge_templates SET vote_count = (SELECT COUNT(*) FROM challenge_votes WHERE challenge_id = $1) WHERE id = $1",
    )
    .bind(id)
    .execute(&state.db)
    .await?;

    Ok(Json(build_response(json!({ "message": "Vote removed" }))))
}

/// Popular community challenges paginated by vote count. Public.
#[utoipa::path(
    get,
    path = "/api/community/challenges/popular",
    tag = "challenges",
    params(PaginationQuery),
    responses((status = 200, body = serde_json::Value)),
)]
pub async fn popular_challenges(
    State(state): State<AppState>,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    crate::validators::check_range_opt(query.page, "page", 1, 100_000)?;
    crate::validators::check_range_opt(query.per_page, "per_page", 1, 100)?;

    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(20).clamp(1, 50);
    let offset = (page - 1) * per_page;

    let challenges: Vec<ChallengeTemplate> = sqlx::query_as(
        "SELECT * FROM challenge_templates WHERE status = 'published' AND is_community = TRUE ORDER BY vote_count DESC, created_at DESC LIMIT $1 OFFSET $2",
    )
    .bind(per_page)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM challenge_templates WHERE status = 'published' AND is_community = TRUE",
    )
    .fetch_one(&state.db)
    .await?;

    Ok(Json(json!({
        "data": challenges,
        "pagination": {
            "page": page,
            "per_page": per_page,
            "total": total,
            "total_pages": (total as f64 / per_page as f64).ceil() as i64,
        },
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })))
}
