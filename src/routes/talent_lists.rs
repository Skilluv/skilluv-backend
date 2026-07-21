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
use crate::models::{Enterprise, TalentList};

// Type aliases pour clippy::type_complexity (rangées sqlx::query_as).
type TalentListsRow142 = (
    Uuid,
    String,
    String,
    String,
    String,
    i32,
    i32,
    Option<String>,
    chrono::DateTime<chrono::Utc>,
);
type TalentListsRow286 = (
    Uuid,
    String,
    String,
    String,
    String,
    i32,
    i32,
    Option<String>,
);

pub fn talent_list_routes() -> Router<AppState> {
    Router::new()
        // Bookmarks
        .route("/enterprise/bookmarks/{talent_id}", post(add_bookmark))
        .route("/enterprise/bookmarks/{talent_id}", delete(remove_bookmark))
        .route("/enterprise/bookmarks", get(list_bookmarks))
        // Talent lists
        .route("/enterprise/lists", post(create_list))
        .route("/enterprise/lists", get(list_lists))
        .route("/enterprise/lists/{list_id}", get(get_list))
        .route("/enterprise/lists/{list_id}", put(update_list))
        .route("/enterprise/lists/{list_id}", delete(delete_list))
        .route(
            "/enterprise/lists/{list_id}/talents/{talent_id}",
            post(add_to_list),
        )
        .route(
            "/enterprise/lists/{list_id}/talents/{talent_id}",
            delete(remove_from_list),
        )
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

async fn require_enterprise(state: &AppState, auth: &AuthUser) -> Result<Enterprise, AppError> {
    crate::routes::enterprise::resolve_active_enterprise(
        &state.db,
        auth.user_id,
        auth.active_enterprise_id,
    )
    .await
}

#[derive(Debug, Deserialize)]
struct PaginationQuery {
    page: Option<i64>,
    per_page: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct CreateListRequest {
    name: String,
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UpdateListRequest {
    name: Option<String>,
    description: Option<String>,
}

// ─── Bookmarks ──────────────────────────────────────────────────

// POST /api/enterprise/bookmarks/:talent_id
async fn add_bookmark(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(talent_id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let enterprise = require_enterprise(&state, &auth).await?;

    // Verify talent exists and is active
    let exists: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM users WHERE id = $1 AND profile_active = TRUE AND is_banned = FALSE",
    )
    .bind(talent_id)
    .fetch_optional(&state.db)
    .await?;

    if exists.is_none() {
        return Err(AppError::NotFound("Talent not found".to_string()));
    }

    sqlx::query(
        "INSERT INTO enterprise_bookmarks (enterprise_id, talent_id, created_by) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
    )
    .bind(enterprise.id)
    .bind(talent_id)
    .bind(auth.user_id)
    .execute(&state.db)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(build_response(json!({ "message": "Bookmark added" }))),
    ))
}

// DELETE /api/enterprise/bookmarks/:talent_id
async fn remove_bookmark(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(talent_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let enterprise = require_enterprise(&state, &auth).await?;

    sqlx::query("DELETE FROM enterprise_bookmarks WHERE enterprise_id = $1 AND talent_id = $2")
        .bind(enterprise.id)
        .bind(talent_id)
        .execute(&state.db)
        .await?;

    Ok(Json(build_response(json!({
        "message": "Bookmark removed"
    }))))
}

// GET /api/enterprise/bookmarks
async fn list_bookmarks(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let enterprise = require_enterprise(&state, &auth).await?;

    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(20).clamp(1, 50);
    let offset = (page - 1) * per_page;

    let bookmarks: Vec<TalentListsRow142> = sqlx::query_as(
        r#"
        SELECT u.id, u.username, u.display_name, u.skill_domain, u.title, u.golden_stars, u.total_fragments, u.country, eb.created_at
        FROM enterprise_bookmarks eb
        JOIN users u ON u.id = eb.talent_id
        WHERE eb.enterprise_id = $1 AND u.profile_active = TRUE AND u.is_banned = FALSE
        ORDER BY eb.created_at DESC
        LIMIT $2 OFFSET $3
        "#,
    )
    .bind(enterprise.id)
    .bind(per_page)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM enterprise_bookmarks eb JOIN users u ON u.id = eb.talent_id WHERE eb.enterprise_id = $1 AND u.profile_active = TRUE AND u.is_banned = FALSE",
    )
    .bind(enterprise.id)
    .fetch_one(&state.db)
    .await?;

    let results: Vec<serde_json::Value> = bookmarks
        .iter()
        .map(|b| {
            json!({
                "id": b.0,
                "username": b.1,
                "display_name": b.2,
                "skill_domain": b.3,
                "title": b.4,
                "golden_stars": b.5,
                "total_fragments": b.6,
                "country": b.7,
                "bookmarked_at": b.8.to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(json!({
        "data": results,
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

// ─── Talent Lists ───────────────────────────────────────────────

// POST /api/enterprise/lists
async fn create_list(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateListRequest>,
) -> Result<impl IntoResponse, AppError> {
    let enterprise = require_enterprise(&state, &auth).await?;

    if body.name.trim().is_empty() || body.name.len() > 200 {
        return Err(AppError::Validation(
            "name must be between 1 and 200 characters".to_string(),
        ));
    }

    let list: TalentList = sqlx::query_as(
        "INSERT INTO talent_lists (enterprise_id, name, description, created_by) VALUES ($1, $2, $3, $4) RETURNING *",
    )
    .bind(enterprise.id)
    .bind(body.name.trim())
    .bind(&body.description)
    .bind(auth.user_id)
    .fetch_one(&state.db)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(build_response(json!({ "list": list }))),
    ))
}

// GET /api/enterprise/lists
async fn list_lists(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, AppError> {
    let enterprise = require_enterprise(&state, &auth).await?;

    let lists: Vec<TalentList> = sqlx::query_as(
        "SELECT * FROM talent_lists WHERE enterprise_id = $1 ORDER BY created_at DESC",
    )
    .bind(enterprise.id)
    .fetch_all(&state.db)
    .await?;

    // Get member counts
    let list_ids: Vec<Uuid> = lists.iter().map(|l| l.id).collect();
    let counts: Vec<(Uuid, i64)> = sqlx::query_as(
        "SELECT list_id, COUNT(*) FROM talent_list_members WHERE list_id = ANY($1) GROUP BY list_id",
    )
    .bind(&list_ids)
    .fetch_all(&state.db)
    .await?;

    let count_map: std::collections::HashMap<Uuid, i64> = counts.into_iter().collect();

    let results: Vec<serde_json::Value> = lists
        .iter()
        .map(|l| {
            json!({
                "id": l.id,
                "name": l.name,
                "description": l.description,
                "talent_count": count_map.get(&l.id).unwrap_or(&0),
                "created_at": l.created_at.to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(build_response(json!({ "lists": results }))))
}

// GET /api/enterprise/lists/:list_id
async fn get_list(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(list_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let enterprise = require_enterprise(&state, &auth).await?;

    let list: TalentList =
        sqlx::query_as("SELECT * FROM talent_lists WHERE id = $1 AND enterprise_id = $2")
            .bind(list_id)
            .bind(enterprise.id)
            .fetch_optional(&state.db)
            .await?
            .ok_or(AppError::NotFound("List not found".to_string()))?;

    let talents: Vec<TalentListsRow286> = sqlx::query_as(
        r#"
        SELECT u.id, u.username, u.display_name, u.skill_domain, u.title, u.golden_stars, u.total_fragments, u.country
        FROM talent_list_members tlm
        JOIN users u ON u.id = tlm.talent_id
        WHERE tlm.list_id = $1
        ORDER BY tlm.added_at DESC
        "#,
    )
    .bind(list_id)
    .fetch_all(&state.db)
    .await?;

    let talent_data: Vec<serde_json::Value> = talents
        .iter()
        .map(|t| {
            json!({
                "id": t.0,
                "username": t.1,
                "display_name": t.2,
                "skill_domain": t.3,
                "title": t.4,
                "golden_stars": t.5,
                "total_fragments": t.6,
                "country": t.7,
            })
        })
        .collect();

    Ok(Json(build_response(json!({
        "list": list,
        "talents": talent_data,
    }))))
}

// PUT /api/enterprise/lists/:list_id
async fn update_list(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(list_id): Path<Uuid>,
    Json(body): Json<UpdateListRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let enterprise = require_enterprise(&state, &auth).await?;

    let list: TalentList = sqlx::query_as(
        r#"
        UPDATE talent_lists SET
            name = COALESCE($1, name),
            description = COALESCE($2, description),
            updated_at = NOW()
        WHERE id = $3 AND enterprise_id = $4
        RETURNING *
        "#,
    )
    .bind(&body.name)
    .bind(&body.description)
    .bind(list_id)
    .bind(enterprise.id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound("List not found".to_string()))?;

    Ok(Json(build_response(json!({ "list": list }))))
}

// DELETE /api/enterprise/lists/:list_id
async fn delete_list(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(list_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let enterprise = require_enterprise(&state, &auth).await?;

    let result = sqlx::query("DELETE FROM talent_lists WHERE id = $1 AND enterprise_id = $2")
        .bind(list_id)
        .bind(enterprise.id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("List not found".to_string()));
    }

    Ok(Json(build_response(json!({
        "message": "List deleted"
    }))))
}

// POST /api/enterprise/lists/:list_id/talents/:talent_id
async fn add_to_list(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((list_id, talent_id)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, AppError> {
    let enterprise = require_enterprise(&state, &auth).await?;

    // Verify list belongs to enterprise
    let exists: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM talent_lists WHERE id = $1 AND enterprise_id = $2")
            .bind(list_id)
            .bind(enterprise.id)
            .fetch_optional(&state.db)
            .await?;

    if exists.is_none() {
        return Err(AppError::NotFound("List not found".to_string()));
    }

    sqlx::query(
        "INSERT INTO talent_list_members (list_id, talent_id, added_by) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
    )
    .bind(list_id)
    .bind(talent_id)
    .bind(auth.user_id)
    .execute(&state.db)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(build_response(json!({
            "message": "Talent added to list"
        }))),
    ))
}

// DELETE /api/enterprise/lists/:list_id/talents/:talent_id
async fn remove_from_list(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((list_id, talent_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, AppError> {
    let enterprise = require_enterprise(&state, &auth).await?;

    // Verify list belongs to enterprise
    let exists: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM talent_lists WHERE id = $1 AND enterprise_id = $2")
            .bind(list_id)
            .bind(enterprise.id)
            .fetch_optional(&state.db)
            .await?;

    if exists.is_none() {
        return Err(AppError::NotFound("List not found".to_string()));
    }

    sqlx::query("DELETE FROM talent_list_members WHERE list_id = $1 AND talent_id = $2")
        .bind(list_id)
        .bind(talent_id)
        .execute(&state.db)
        .await?;

    Ok(Json(build_response(json!({
        "message": "Talent removed from list"
    }))))
}
