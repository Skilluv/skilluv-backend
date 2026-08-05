use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::AppState;
use crate::api_response::{ApiResponse, MetaInfo, SimpleMessage};
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::models::{Enterprise, TalentList};
use crate::routes::notifications::Pagination;

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

async fn require_enterprise(state: &AppState, auth: &AuthUser) -> Result<Enterprise, AppError> {
    crate::routes::enterprise::resolve_active_enterprise(
        &state.db,
        auth.user_id,
        auth.active_enterprise_id,
    )
    .await
}

// ─── Types de réponse ────────────────────────────────────────────

#[derive(Debug, Deserialize, IntoParams)]
pub struct PaginationQuery {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateListRequest {
    #[schema(max_length = 10000)]
    pub name: String,
    #[schema(max_length = 10000)]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateListRequest {
    #[schema(max_length = 10000)]
    pub name: Option<String>,
    #[schema(max_length = 10000)]
    pub description: Option<String>,
}

/// Bookmarked talent — minimal projection for the enterprise-side
/// bookmarks list.
#[derive(Debug, Serialize, ToSchema)]
pub struct BookmarkedTalent {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub skill_domain: String,
    pub title: String,
    pub golden_stars: i32,
    pub total_fragments: i32,
    pub country: Option<String>,
    /// RFC 3339 timestamp of when the talent was bookmarked.
    pub bookmarked_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BookmarksPageResponse {
    pub data: Vec<BookmarkedTalent>,
    pub pagination: Pagination,
    pub meta: MetaInfo,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ListResponse {
    pub list: TalentList,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TalentListSummary {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub talent_count: i64,
    /// RFC 3339 timestamp.
    pub created_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ListsResponse {
    pub lists: Vec<TalentListSummary>,
}

/// Talent projection inside a list — same shape as BookmarkedTalent
/// minus the bookmarked_at.
#[derive(Debug, Serialize, ToSchema)]
pub struct ListMemberTalent {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub skill_domain: String,
    pub title: String,
    pub golden_stars: i32,
    pub total_fragments: i32,
    pub country: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ListDetailResponse {
    pub list: TalentList,
    pub talents: Vec<ListMemberTalent>,
}

// ─── Bookmarks ──────────────────────────────────────────────────

/// Add a talent to the enterprise's bookmarks. Idempotent (ON
/// CONFLICT DO NOTHING).
#[utoipa::path(
    post,
    path = "/api/enterprise/bookmarks/{talent_id}",
    tag = "enterprise",
    params(("talent_id" = Uuid, Path, description = "Talent user UUID")),
    responses(
        (status = 201, description = "Bookmark added", body = ApiResponse<SimpleMessage>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Caller has no enterprise", body = crate::api_response::ErrorResponse),
        (status = 404, description = "Talent not found or inactive", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn add_bookmark(
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
        Json(ApiResponse::new(SimpleMessage::new("Bookmark added"))),
    ))
}

/// Remove a bookmark. No-op with 200 if the bookmark doesn't exist.
#[utoipa::path(
    delete,
    path = "/api/enterprise/bookmarks/{talent_id}",
    tag = "enterprise",
    params(("talent_id" = Uuid, Path, description = "Talent user UUID")),
    responses(
        (status = 200, description = "Bookmark removed (or was already absent)", body = ApiResponse<SimpleMessage>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Caller has no enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn remove_bookmark(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(talent_id): Path<Uuid>,
) -> Result<Json<ApiResponse<SimpleMessage>>, AppError> {
    let enterprise = require_enterprise(&state, &auth).await?;

    sqlx::query("DELETE FROM enterprise_bookmarks WHERE enterprise_id = $1 AND talent_id = $2")
        .bind(enterprise.id)
        .bind(talent_id)
        .execute(&state.db)
        .await?;

    Ok(Json(ApiResponse::new(SimpleMessage::new(
        "Bookmark removed",
    ))))
}

/// Paginated list of the enterprise's bookmarked talents.
#[utoipa::path(
    get,
    path = "/api/enterprise/bookmarks",
    tag = "enterprise",
    params(PaginationQuery),
    responses(
        (status = 200, description = "Bookmarked talents", body = BookmarksPageResponse),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Caller has no enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn list_bookmarks(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<BookmarksPageResponse>, AppError> {
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

    let results: Vec<BookmarkedTalent> = bookmarks
        .iter()
        .map(|b| BookmarkedTalent {
            id: b.0,
            username: b.1.clone(),
            display_name: b.2.clone(),
            skill_domain: b.3.clone(),
            title: b.4.clone(),
            golden_stars: b.5,
            total_fragments: b.6,
            country: b.7.clone(),
            bookmarked_at: b.8.to_rfc3339(),
        })
        .collect();

    Ok(Json(BookmarksPageResponse {
        data: results,
        pagination: Pagination {
            page,
            per_page,
            total,
            total_pages: (total as f64 / per_page as f64).ceil() as i64,
        },
        meta: MetaInfo::now(),
    }))
}

// ─── Talent Lists ───────────────────────────────────────────────

/// Create a new talent list for the enterprise.
#[utoipa::path(
    post,
    path = "/api/enterprise/lists",
    tag = "enterprise",
    request_body = CreateListRequest,
    responses(
        (status = 201, description = "List created", body = ApiResponse<ListResponse>),
        (status = 400, description = "Name empty or too long", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Caller has no enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn create_list(
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
        Json(ApiResponse::new(ListResponse { list })),
    ))
}

/// List the enterprise's talent lists with member counts pre-joined.
#[utoipa::path(
    get,
    path = "/api/enterprise/lists",
    tag = "enterprise",
    responses(
        (status = 200, description = "Talent lists (with member counts)", body = ApiResponse<ListsResponse>),
        (status = 403, description = "Caller has no enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn list_lists(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<ListsResponse>>, AppError> {
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

    let results: Vec<TalentListSummary> = lists
        .iter()
        .map(|l| TalentListSummary {
            id: l.id,
            name: l.name.clone(),
            description: l.description.clone(),
            talent_count: *count_map.get(&l.id).unwrap_or(&0),
            created_at: l.created_at.to_rfc3339(),
        })
        .collect();

    Ok(Json(ApiResponse::new(ListsResponse { lists: results })))
}

/// Get a list with its full talent roster.
#[utoipa::path(
    get,
    path = "/api/enterprise/lists/{list_id}",
    tag = "enterprise",
    params(("list_id" = Uuid, Path, description = "Talent-list UUID")),
    responses(
        (status = 200, description = "List + talents", body = ApiResponse<ListDetailResponse>),
        (status = 403, description = "Caller has no enterprise", body = crate::api_response::ErrorResponse),
        (status = 404, description = "List not found under this enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn get_list(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(list_id): Path<Uuid>,
) -> Result<Json<ApiResponse<ListDetailResponse>>, AppError> {
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

    let talent_data: Vec<ListMemberTalent> = talents
        .iter()
        .map(|t| ListMemberTalent {
            id: t.0,
            username: t.1.clone(),
            display_name: t.2.clone(),
            skill_domain: t.3.clone(),
            title: t.4.clone(),
            golden_stars: t.5,
            total_fragments: t.6,
            country: t.7.clone(),
        })
        .collect();

    Ok(Json(ApiResponse::new(ListDetailResponse {
        list,
        talents: talent_data,
    })))
}

/// Partial update on a talent list.
#[utoipa::path(
    put,
    path = "/api/enterprise/lists/{list_id}",
    tag = "enterprise",
    params(("list_id" = Uuid, Path, description = "Talent-list UUID")),
    request_body = UpdateListRequest,
    responses(
        (status = 200, description = "List updated", body = ApiResponse<ListResponse>),
        (status = 404, description = "List not found under this enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn update_list(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(list_id): Path<Uuid>,
    Json(body): Json<UpdateListRequest>,
) -> Result<Json<ApiResponse<ListResponse>>, AppError> {
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

    Ok(Json(ApiResponse::new(ListResponse { list })))
}

/// Delete a talent list.
#[utoipa::path(
    delete,
    path = "/api/enterprise/lists/{list_id}",
    tag = "enterprise",
    params(("list_id" = Uuid, Path, description = "Talent-list UUID")),
    responses(
        (status = 200, description = "List deleted", body = ApiResponse<SimpleMessage>),
        (status = 404, description = "List not found under this enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn delete_list(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(list_id): Path<Uuid>,
) -> Result<Json<ApiResponse<SimpleMessage>>, AppError> {
    let enterprise = require_enterprise(&state, &auth).await?;

    let result = sqlx::query("DELETE FROM talent_lists WHERE id = $1 AND enterprise_id = $2")
        .bind(list_id)
        .bind(enterprise.id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("List not found".to_string()));
    }

    Ok(Json(ApiResponse::new(SimpleMessage::new("List deleted"))))
}

/// Add a talent to a list. Idempotent.
#[utoipa::path(
    post,
    path = "/api/enterprise/lists/{list_id}/talents/{talent_id}",
    tag = "enterprise",
    params(
        ("list_id" = Uuid, Path, description = "Talent-list UUID"),
        ("talent_id" = Uuid, Path, description = "Talent user UUID"),
    ),
    responses(
        (status = 201, description = "Talent added", body = ApiResponse<SimpleMessage>),
        (status = 404, description = "List not found under this enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn add_to_list(
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
        Json(ApiResponse::new(SimpleMessage::new("Talent added to list"))),
    ))
}

/// Remove a talent from a list.
#[utoipa::path(
    delete,
    path = "/api/enterprise/lists/{list_id}/talents/{talent_id}",
    tag = "enterprise",
    params(
        ("list_id" = Uuid, Path, description = "Talent-list UUID"),
        ("talent_id" = Uuid, Path, description = "Talent user UUID"),
    ),
    responses(
        (status = 200, description = "Talent removed", body = ApiResponse<SimpleMessage>),
        (status = 404, description = "List not found under this enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn remove_from_list(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((list_id, talent_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<ApiResponse<SimpleMessage>>, AppError> {
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

    Ok(Json(ApiResponse::new(SimpleMessage::new(
        "Talent removed from list",
    ))))
}
