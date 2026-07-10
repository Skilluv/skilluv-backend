use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::models::Challenge;

pub fn challenge_tag_routes() -> Router<AppState> {
    Router::new()
        .route("/challenges/tags", get(list_tags))
        .route("/challenges/categories", get(list_categories))
        .route("/challenges/featured", get(featured_challenges))
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

#[derive(Debug, serde::Serialize, sqlx::FromRow)]
struct TagWithCount {
    id: Uuid,
    name: String,
    category: String,
    challenge_count: i64,
}

// GET /api/challenges/tags — all tags with usage count (no auth, SSR)
async fn list_tags(State(state): State<AppState>) -> Result<Json<serde_json::Value>, AppError> {
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

    Ok(Json(build_response(json!({ "tags": tags }))))
}

// GET /api/challenges/categories — distinct categories (no auth, SSR)
async fn list_categories(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    let categories: Vec<(String, i64)> = sqlx::query_as(
        "SELECT category, COUNT(*) as count FROM challenge_tags GROUP BY category ORDER BY category",
    )
    .fetch_all(&state.db)
    .await?;

    let result: Vec<serde_json::Value> = categories
        .iter()
        .map(|(cat, count)| json!({ "category": cat, "tag_count": count }))
        .collect();

    Ok(Json(build_response(json!({ "categories": result }))))
}

// GET /api/challenges/featured — featured challenges (no auth, SSR)
async fn featured_challenges(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    let challenges: Vec<Challenge> = sqlx::query_as(
        "SELECT * FROM challenge_templates WHERE featured = TRUE AND status = 'published' ORDER BY vote_count DESC, created_at DESC LIMIT 20",
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(build_response(json!({ "challenges": challenges }))))
}
