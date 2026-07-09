//! Projects routes — Phase 2 Sprint 5.

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::routes::analytics_consent;
use crate::services::analytics::{events, props};
use crate::services::projects;

pub fn project_routes() -> Router<AppState> {
    Router::new()
        .route("/projects", post(create_project))
        .route("/projects/looking-for-contributors", get(list_looking))
        .route("/projects/curated", get(list_curated))
        .route("/projects/{slug}", get(by_slug))
        .route(
            "/projects/{slug}/contributors",
            get(list_contributors).post(add_contributor),
        )
        .route(
            "/projects/{slug}/contributors/{user_id}",
            delete(remove_contributor),
        )
        .route("/projects/{slug}/archive", post(archive))
        .route("/u/{username}/projects", get(by_user))
        .route("/guilds/{slug}/projects", get(by_guild_slug))
        .route("/admin/projects/{slug}/curated", post(admin_set_curated))
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

async fn create_project(
    State(state): State<AppState>,
    auth: AuthUser,
    headers: HeaderMap,
    Json(body): Json<projects::CreateProjectInput>,
) -> Result<Json<Value>, AppError> {
    let project = projects::create(&state.db, auth.user_id, &auth.role, body).await?;
    if analytics_consent(&headers) {
        state.analytics.track(
            auth.user_id,
            events::PROJECT_CREATED,
            props(&[
                ("owner_type", json!(project.owner_type)),
                ("is_oss", json!(project.is_oss)),
            ]),
        );
    }
    metrics::counter!(
        "skilluv_projects_created_total",
        "owner_type" => project.owner_type.clone()
    )
    .increment(1);
    Ok(Json(build_response(json!({ "project": project }))))
}

#[derive(Deserialize)]
struct LimitQuery {
    limit: Option<i64>,
}

async fn list_looking(
    State(state): State<AppState>,
    Query(q): Query<LimitQuery>,
) -> Result<Json<Value>, AppError> {
    let rows = projects::list_looking_for_contributors(&state.db, q.limit.unwrap_or(50)).await?;
    Ok(Json(build_response(json!({ "projects": rows }))))
}

async fn list_curated(
    State(state): State<AppState>,
    Query(q): Query<LimitQuery>,
) -> Result<Json<Value>, AppError> {
    let rows = projects::list_curated(&state.db, q.limit.unwrap_or(50)).await?;
    Ok(Json(build_response(json!({ "projects": rows }))))
}

async fn by_slug(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<Value>, AppError> {
    let p = projects::by_slug(&state.db, &slug).await?;
    Ok(Json(build_response(json!({ "project": p }))))
}

async fn by_user(
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> Result<Json<Value>, AppError> {
    let user: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM users WHERE username = $1 AND profile_active = TRUE AND is_banned = FALSE",
    )
    .bind(&username)
    .fetch_optional(&state.db)
    .await?;
    let (uid,) = user.ok_or(AppError::NotFound("user not found".into()))?;
    let rows = projects::list_for_owner(&state.db, "user", uid).await?;
    Ok(Json(build_response(json!({ "projects": rows }))))
}

async fn by_guild_slug(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<Value>, AppError> {
    let guild: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM guilds WHERE slug = $1 AND disbanded_at IS NULL")
            .bind(&slug)
            .fetch_optional(&state.db)
            .await?;
    let (gid,) = guild.ok_or(AppError::NotFound("guild not found".into()))?;
    let rows = projects::list_for_owner(&state.db, "guild", gid).await?;
    Ok(Json(build_response(json!({ "projects": rows }))))
}

async fn list_contributors(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<Value>, AppError> {
    let project = projects::by_slug(&state.db, &slug).await?;
    let rows = projects::list_contributors(&state.db, project.id).await?;
    Ok(Json(build_response(json!({ "contributors": rows }))))
}

#[derive(Deserialize)]
struct AddContributorBody {
    user_id: Uuid,
    role: Option<String>,
}

async fn add_contributor(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(body): Json<AddContributorBody>,
) -> Result<Json<Value>, AppError> {
    let project = projects::by_slug(&state.db, &slug).await?;
    projects::add_contributor(
        &state.db,
        project.id,
        auth.user_id,
        &auth.role,
        body.user_id,
        body.role.as_deref().unwrap_or("contributor"),
    )
    .await?;
    Ok(Json(build_response(json!({ "added": true }))))
}

async fn remove_contributor(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((slug, user_id)): Path<(String, Uuid)>,
) -> Result<Json<Value>, AppError> {
    let project = projects::by_slug(&state.db, &slug).await?;
    projects::remove_contributor(&state.db, project.id, auth.user_id, &auth.role, user_id).await?;
    Ok(Json(build_response(json!({ "removed": true }))))
}

async fn archive(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> Result<Json<Value>, AppError> {
    let project = projects::by_slug(&state.db, &slug).await?;
    projects::archive(&state.db, project.id, auth.user_id, &auth.role).await?;
    Ok(Json(build_response(json!({ "archived": true }))))
}

#[derive(Deserialize)]
struct SetCuratedBody {
    curated: bool,
}

async fn admin_set_curated(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(body): Json<SetCuratedBody>,
) -> Result<Json<Value>, AppError> {
    if auth.role != "admin" {
        return Err(AppError::Forbidden);
    }
    let project = projects::by_slug(&state.db, &slug).await?;
    projects::admin_set_curated(&state.db, project.id, body.curated).await?;
    Ok(Json(build_response(json!({ "curated": body.curated }))))
}
