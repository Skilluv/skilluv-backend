use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::models::ChallengeTemplate;

pub fn challenge_tag_routes() -> Router<AppState> {
    Router::new()
        .route("/challenges/tags", get(list_tags))
        .route("/challenges/categories", get(list_categories))
        .route("/challenges/featured", get(featured_challenges))
}

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct TagWithCount {
    pub id: Uuid,
    pub name: String,
    pub category: String,
    pub challenge_count: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TagsResponse {
    pub tags: Vec<TagWithCount>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CategoryRow {
    pub category: String,
    pub tag_count: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CategoriesResponse {
    pub categories: Vec<CategoryRow>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct FeaturedChallengesResponse {
    pub challenges: Vec<ChallengeTemplate>,
}

/// List every challenge tag with the number of published challenges
/// tagged with it. Public, SSR-ready.
#[utoipa::path(
    get,
    path = "/api/challenges/tags",
    tag = "challenges",
    responses(
        (status = 200, description = "Tags with usage counts", body = ApiResponse<TagsResponse>),
    ),
)]
pub async fn list_tags(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<TagsResponse>>, AppError> {
    let tags: Vec<TagWithCount> = sqlx::query_as(
        r#"
        SELECT t.id, t.name, t.category, COUNT(ctm.challenge_id) as challenge_count
        FROM challenge_tags t
        LEFT JOIN challenge_tag_map ctm ON ctm.tag_id = t.id
        GROUP BY t.id, t.name, t.category
        ORDER BY t.category, t.name
        "#,
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(TagsResponse { tags })))
}

/// Distinct tag categories with tag counts. Public, SSR-ready.
#[utoipa::path(
    get,
    path = "/api/challenges/categories",
    tag = "challenges",
    responses(
        (status = 200, description = "Tag categories", body = ApiResponse<CategoriesResponse>),
    ),
)]
pub async fn list_categories(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<CategoriesResponse>>, AppError> {
    let categories: Vec<(String, i64)> = sqlx::query_as(
        "SELECT category, COUNT(*) as count FROM challenge_tags GROUP BY category ORDER BY category",
    )
    .fetch_all(&state.db)
    .await?;

    let result: Vec<CategoryRow> = categories
        .iter()
        .map(|(cat, count)| CategoryRow {
            category: cat.clone(),
            tag_count: *count,
        })
        .collect();

    Ok(Json(ApiResponse::new(CategoriesResponse {
        categories: result,
    })))
}

/// Featured challenges — capped at 20, ordered by vote_count desc.
/// Public, SSR-ready.
#[utoipa::path(
    get,
    path = "/api/challenges/featured",
    tag = "challenges",
    responses(
        (status = 200, description = "Featured challenges", body = ApiResponse<FeaturedChallengesResponse>),
    ),
)]
pub async fn featured_challenges(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<FeaturedChallengesResponse>>, AppError> {
    let challenges: Vec<ChallengeTemplate> = sqlx::query_as(
        "SELECT * FROM challenge_templates WHERE featured = TRUE AND status = 'published' ORDER BY vote_count DESC, created_at DESC LIMIT 20",
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(FeaturedChallengesResponse {
        challenges,
    })))
}
