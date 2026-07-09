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
use crate::models::Challenge;

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

#[derive(Debug, Deserialize)]
struct CreateCommunityChallenge {
    title: String,
    description: String,
    instructions: String,
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

#[derive(Debug, Deserialize)]
struct UpdateCommunityChallenge {
    title: Option<String>,
    description: Option<String>,
    instructions: Option<String>,
    difficulty: Option<i16>,
    language: Option<String>,
    expected_output: Option<String>,
    test_cases: Option<serde_json::Value>,
    submit_for_review: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct PaginationQuery {
    page: Option<i64>,
    per_page: Option<i64>,
}

// POST /api/community/challenges
async fn create_community_challenge(
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

    let challenge: Challenge = sqlx::query_as(
        r#"
        INSERT INTO challenges (
            title, description, instructions, skill_domain, difficulty,
            language, expected_output, test_cases,
            reward_fragments, duration_minutes,
            is_community, community_status, created_by, status
        ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,TRUE,$11,$12,'draft')
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

// GET /api/community/challenges/mine
async fn my_challenges(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, AppError> {
    let challenges: Vec<Challenge> = sqlx::query_as(
        "SELECT * FROM challenges WHERE created_by = $1 AND is_community = TRUE ORDER BY created_at DESC",
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(build_response(json!({ "challenges": challenges }))))
}

// PUT /api/community/challenges/:id
async fn update_community_challenge(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateCommunityChallenge>,
) -> Result<Json<serde_json::Value>, AppError> {
    let existing: Challenge = sqlx::query_as(
        "SELECT * FROM challenges WHERE id = $1 AND created_by = $2 AND is_community = TRUE",
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

    let challenge: Challenge = sqlx::query_as(
        r#"
        UPDATE challenges SET
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

// POST /api/community/challenges/:id/vote
async fn vote_challenge(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    // Verify challenge is published
    let exists: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM challenges WHERE id = $1 AND status = 'published'")
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
        "UPDATE challenges SET vote_count = (SELECT COUNT(*) FROM challenge_votes WHERE challenge_id = $1) WHERE id = $1",
    )
    .bind(id)
    .execute(&state.db)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(build_response(json!({ "message": "Vote recorded" }))),
    ))
}

// DELETE /api/community/challenges/:id/vote
async fn unvote_challenge(
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
        "UPDATE challenges SET vote_count = (SELECT COUNT(*) FROM challenge_votes WHERE challenge_id = $1) WHERE id = $1",
    )
    .bind(id)
    .execute(&state.db)
    .await?;

    Ok(Json(build_response(json!({ "message": "Vote removed" }))))
}

// GET /api/community/challenges/popular — top by votes (no auth, SSR)
async fn popular_challenges(
    State(state): State<AppState>,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(20).clamp(1, 50);
    let offset = (page - 1) * per_page;

    let challenges: Vec<Challenge> = sqlx::query_as(
        "SELECT * FROM challenges WHERE status = 'published' AND is_community = TRUE ORDER BY vote_count DESC, created_at DESC LIMIT $1 OFFSET $2",
    )
    .bind(per_page)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM challenges WHERE status = 'published' AND is_community = TRUE",
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
