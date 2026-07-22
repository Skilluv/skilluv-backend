//! Admin CRUD on projects — flagships, curated OSS partners, and internal
//! administrative moderation of user projects.
//!
//! Endpoints (all gated by admin_gate = origin + 2FA + capability admin):
//!
//! - POST   /admin/projects                    — create curated / flagship / OSS partner
//! - PATCH  /admin/projects/{slug}             — edit
//! - DELETE /admin/projects/{slug}             — soft archive (sets archived_at)
//! - GET    /admin/projects                    — list with filters
//! - GET    /admin/projects/{slug}             — get by slug
//!
//! See content-strategy-2027-2028.md §4, annexes E and F.

use axum::extract::{Path, Query, State};
use axum::routing::{delete, get, patch, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;

pub fn admin_project_routes() -> Router<AppState> {
    Router::new()
        .route("/admin/projects", post(create_project))
        .route("/admin/projects", get(list_projects))
        .route("/admin/projects/{slug}", get(get_project))
        .route("/admin/projects/{slug}", patch(patch_project))
        .route("/admin/projects/{slug}", delete(archive_project))
}

fn wrap(data: Value) -> Value {
    json!({
        "data": data,
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })
}

// ═══════════════════════════════════════════════════════════════════
// POST /admin/projects
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct CreateProjectBody {
    slug: String,
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    repo_url: Option<String>,
    #[serde(default)]
    demo_url: Option<String>,
    #[serde(default)]
    tech_stack: Vec<String>,
    #[serde(default = "default_true")]
    is_oss: bool,
    #[serde(default)]
    looking_for_contributors: bool,
    /// "user" or "guild". For OSS partners and flagships, use "user" with the admin's id.
    owner_type: String,
    owner_id: Uuid,
    #[serde(default = "default_true")]
    curated_by_admin: bool,

    // Flagship-specific
    #[serde(default)]
    is_flagship: bool,
    #[serde(default)]
    flagship_steward_user_id: Option<Uuid>,

    // OSS partner-specific
    #[serde(default)]
    skilluv_partnership_level: Option<i16>,

    #[serde(default)]
    skilluv_editorial_notes: Option<String>,
}

fn default_true() -> bool {
    true
}

async fn create_project(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateProjectBody>,
) -> Result<Json<Value>, AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await?;

    validate_slug(&body.slug)?;
    validate_owner_type(&body.owner_type)?;
    validate_flagship(&body)?;
    validate_partnership_level(body.skilluv_partnership_level)?;

    let inserted: (Uuid, String) = sqlx::query_as(
        r#"
        INSERT INTO projects (
            slug, name, description, repo_url, demo_url, tech_stack,
            is_oss, looking_for_contributors, owner_type, owner_id, curated_by_admin,
            is_flagship, flagship_steward_user_id, skilluv_partnership_level,
            skilluv_editorial_notes
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
        RETURNING id, slug
        "#,
    )
    .bind(&body.slug)
    .bind(&body.name)
    .bind(&body.description)
    .bind(&body.repo_url)
    .bind(&body.demo_url)
    .bind(&body.tech_stack)
    .bind(body.is_oss)
    .bind(body.looking_for_contributors)
    .bind(&body.owner_type)
    .bind(body.owner_id)
    .bind(body.curated_by_admin)
    .bind(body.is_flagship)
    .bind(body.flagship_steward_user_id)
    .bind(body.skilluv_partnership_level)
    .bind(&body.skilluv_editorial_notes)
    .fetch_one(&state.db)
    .await?;

    crate::services::audit::record(
        &state.db,
        crate::services::audit::AuditEntry {
            actor_type: crate::services::audit::ActorType::Admin,
            actor_id: Some(auth.user_id),
            action: "project.create",
            target_type: Some("project"),
            target_id: Some(inserted.0),
            metadata: Some(json!({
                "slug": body.slug,
                "is_flagship": body.is_flagship,
                "partnership_level": body.skilluv_partnership_level,
            })),
            headers: None,
        },
    )
    .await;

    Ok(Json(wrap(json!({
        "id": inserted.0,
        "slug": inserted.1,
    }))))
}

// ═══════════════════════════════════════════════════════════════════
// PATCH /admin/projects/{slug}
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct PatchProjectBody {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    repo_url: Option<String>,
    #[serde(default)]
    demo_url: Option<String>,
    #[serde(default)]
    tech_stack: Option<Vec<String>>,
    #[serde(default)]
    is_oss: Option<bool>,
    #[serde(default)]
    looking_for_contributors: Option<bool>,
    #[serde(default)]
    curated_by_admin: Option<bool>,
    #[serde(default)]
    is_flagship: Option<bool>,
    #[serde(default)]
    flagship_steward_user_id: Option<Uuid>,
    #[serde(default)]
    skilluv_partnership_level: Option<i16>,
    #[serde(default)]
    skilluv_editorial_notes: Option<String>,
}

async fn patch_project(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(body): Json<PatchProjectBody>,
) -> Result<Json<Value>, AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await?;
    validate_slug(&slug)?;
    validate_partnership_level(body.skilluv_partnership_level)?;

    let updated: Option<(Uuid,)> = sqlx::query_as(
        r#"
        UPDATE projects SET
            name = COALESCE($1, name),
            description = COALESCE($2, description),
            repo_url = COALESCE($3, repo_url),
            demo_url = COALESCE($4, demo_url),
            tech_stack = COALESCE($5, tech_stack),
            is_oss = COALESCE($6, is_oss),
            looking_for_contributors = COALESCE($7, looking_for_contributors),
            curated_by_admin = COALESCE($8, curated_by_admin),
            is_flagship = COALESCE($9, is_flagship),
            flagship_steward_user_id = COALESCE($10, flagship_steward_user_id),
            skilluv_partnership_level = COALESCE($11, skilluv_partnership_level),
            skilluv_editorial_notes = COALESCE($12, skilluv_editorial_notes),
            updated_at = NOW()
        WHERE slug = $13 AND archived_at IS NULL
        RETURNING id
        "#,
    )
    .bind(&body.name)
    .bind(&body.description)
    .bind(&body.repo_url)
    .bind(&body.demo_url)
    .bind(&body.tech_stack)
    .bind(body.is_oss)
    .bind(body.looking_for_contributors)
    .bind(body.curated_by_admin)
    .bind(body.is_flagship)
    .bind(body.flagship_steward_user_id)
    .bind(body.skilluv_partnership_level)
    .bind(&body.skilluv_editorial_notes)
    .bind(&slug)
    .fetch_optional(&state.db)
    .await?;

    let project_id = updated
        .ok_or_else(|| AppError::NotFound(format!("project {slug} not found or archived")))?
        .0;

    crate::services::audit::record(
        &state.db,
        crate::services::audit::AuditEntry {
            actor_type: crate::services::audit::ActorType::Admin,
            actor_id: Some(auth.user_id),
            action: "project.patch",
            target_type: Some("project"),
            target_id: Some(project_id),
            metadata: Some(json!({ "slug": slug })),
            headers: None,
        },
    )
    .await;

    Ok(Json(wrap(json!({ "slug": slug, "updated": true }))))
}

// ═══════════════════════════════════════════════════════════════════
// DELETE /admin/projects/{slug} — soft archive
// ═══════════════════════════════════════════════════════════════════

async fn archive_project(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> Result<Json<Value>, AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await?;
    validate_slug(&slug)?;

    let updated: Option<(Uuid,)> = sqlx::query_as(
        r#"
        UPDATE projects
        SET archived_at = NOW()
        WHERE slug = $1 AND archived_at IS NULL
        RETURNING id
        "#,
    )
    .bind(&slug)
    .fetch_optional(&state.db)
    .await?;

    let project_id = updated
        .ok_or_else(|| AppError::NotFound(format!("project {slug} not found or already archived")))?
        .0;

    crate::services::audit::record(
        &state.db,
        crate::services::audit::AuditEntry {
            actor_type: crate::services::audit::ActorType::Admin,
            actor_id: Some(auth.user_id),
            action: "project.archive",
            target_type: Some("project"),
            target_id: Some(project_id),
            metadata: Some(json!({ "slug": slug })),
            headers: None,
        },
    )
    .await;

    Ok(Json(wrap(json!({ "slug": slug, "archived": true }))))
}

// ═══════════════════════════════════════════════════════════════════
// GET /admin/projects — list with filters
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct ListQuery {
    #[serde(default)]
    is_flagship: Option<bool>,
    #[serde(default)]
    curated_by_admin: Option<bool>,
    #[serde(default)]
    partnership_level: Option<i16>,
    #[serde(default)]
    include_archived: bool,
    #[serde(default)]
    page: Option<i64>,
    #[serde(default)]
    per_page: Option<i64>,
}

async fn list_projects(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<ListQuery>,
) -> Result<Json<Value>, AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await?;

    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(20).clamp(1, 100);
    let offset = (page - 1) * per_page;

    type ProjectListRow = (
        Uuid,
        String,
        String,
        Option<String>,
        Option<String>,
        bool,
        bool,
        Option<i16>,
        Option<Uuid>,
        chrono::DateTime<chrono::Utc>,
        Option<chrono::DateTime<chrono::Utc>>,
    );
    let rows: Vec<ProjectListRow> = sqlx::query_as(
        r#"
        SELECT id, slug, name, description, repo_url,
               is_flagship, curated_by_admin, skilluv_partnership_level,
               flagship_steward_user_id, created_at, archived_at
        FROM projects
        WHERE ($1::bool IS NULL OR is_flagship = $1)
          AND ($2::bool IS NULL OR curated_by_admin = $2)
          AND ($3::int2 IS NULL OR skilluv_partnership_level = $3)
          AND ($4::bool = TRUE OR archived_at IS NULL)
        ORDER BY is_flagship DESC, curated_by_admin DESC, created_at DESC
        LIMIT $5 OFFSET $6
        "#,
    )
    .bind(q.is_flagship)
    .bind(q.curated_by_admin)
    .bind(q.partnership_level)
    .bind(q.include_archived)
    .bind(per_page)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let total: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM projects
        WHERE ($1::bool IS NULL OR is_flagship = $1)
          AND ($2::bool IS NULL OR curated_by_admin = $2)
          AND ($3::int2 IS NULL OR skilluv_partnership_level = $3)
          AND ($4::bool = TRUE OR archived_at IS NULL)
        "#,
    )
    .bind(q.is_flagship)
    .bind(q.curated_by_admin)
    .bind(q.partnership_level)
    .bind(q.include_archived)
    .fetch_one(&state.db)
    .await?;

    let items: Vec<Value> = rows
        .into_iter()
        .map(
            |(
                id,
                slug,
                name,
                description,
                repo_url,
                is_flagship,
                curated,
                plevel,
                steward,
                created,
                archived,
            )| {
                json!({
                    "id": id,
                    "slug": slug,
                    "name": name,
                    "description": description,
                    "repo_url": repo_url,
                    "is_flagship": is_flagship,
                    "curated_by_admin": curated,
                    "skilluv_partnership_level": plevel,
                    "flagship_steward_user_id": steward,
                    "created_at": created.to_rfc3339(),
                    "archived_at": archived.map(|d| d.to_rfc3339()),
                })
            },
        )
        .collect();

    let total_pages = if per_page > 0 {
        (total + per_page - 1) / per_page
    } else {
        0
    };

    Ok(Json(json!({
        "data": items,
        "pagination": {
            "page": page,
            "per_page": per_page,
            "total": total,
            "total_pages": total_pages,
        },
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })))
}

// ═══════════════════════════════════════════════════════════════════
// GET /admin/projects/{slug}
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, sqlx::FromRow)]
struct ProjectFullRow {
    id: Uuid,
    slug: String,
    name: String,
    description: Option<String>,
    repo_url: Option<String>,
    demo_url: Option<String>,
    tech_stack: Vec<String>,
    is_oss: bool,
    looking_for_contributors: bool,
    owner_type: String,
    owner_id: Uuid,
    curated_by_admin: bool,
    is_flagship: bool,
    flagship_steward_user_id: Option<Uuid>,
    skilluv_partnership_level: Option<i16>,
    skilluv_editorial_notes: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    archived_at: Option<chrono::DateTime<chrono::Utc>>,
}

async fn get_project(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> Result<Json<Value>, AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await?;
    validate_slug(&slug)?;

    let row: Option<ProjectFullRow> = sqlx::query_as(
        r#"
        SELECT id, slug, name, description, repo_url, demo_url, tech_stack,
               is_oss, looking_for_contributors, owner_type, owner_id, curated_by_admin,
               is_flagship, flagship_steward_user_id, skilluv_partnership_level,
               skilluv_editorial_notes, created_at, updated_at, archived_at
        FROM projects WHERE slug = $1
        "#,
    )
    .bind(&slug)
    .fetch_optional(&state.db)
    .await?;

    let Some(r) = row else {
        return Err(AppError::NotFound(format!("project {slug} not found")));
    };

    Ok(Json(wrap(json!({
        "id": r.id,
        "slug": r.slug,
        "name": r.name,
        "description": r.description,
        "repo_url": r.repo_url,
        "demo_url": r.demo_url,
        "tech_stack": r.tech_stack,
        "is_oss": r.is_oss,
        "looking_for_contributors": r.looking_for_contributors,
        "owner_type": r.owner_type,
        "owner_id": r.owner_id,
        "curated_by_admin": r.curated_by_admin,
        "is_flagship": r.is_flagship,
        "flagship_steward_user_id": r.flagship_steward_user_id,
        "skilluv_partnership_level": r.skilluv_partnership_level,
        "skilluv_editorial_notes": r.skilluv_editorial_notes,
        "created_at": r.created_at.to_rfc3339(),
        "updated_at": r.updated_at.to_rfc3339(),
        "archived_at": r.archived_at.map(|d| d.to_rfc3339()),
    }))))
}

// ═══════════════════════════════════════════════════════════════════
// Validators
// ═══════════════════════════════════════════════════════════════════

fn validate_slug(slug: &str) -> Result<(), AppError> {
    if slug.is_empty()
        || slug.len() > 80
        || !slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(AppError::Validation(
            "slug must be 1-80 lowercase ASCII alphanumerics + dashes".into(),
        ));
    }
    Ok(())
}

fn validate_owner_type(owner_type: &str) -> Result<(), AppError> {
    if !matches!(owner_type, "user" | "guild") {
        return Err(AppError::Validation(
            "owner_type must be 'user' or 'guild'".into(),
        ));
    }
    Ok(())
}

fn validate_flagship(body: &CreateProjectBody) -> Result<(), AppError> {
    if body.is_flagship && body.flagship_steward_user_id.is_none() {
        return Err(AppError::Validation(
            "flagship projects must have a flagship_steward_user_id".into(),
        ));
    }
    Ok(())
}

fn validate_partnership_level(level: Option<i16>) -> Result<(), AppError> {
    if let Some(l) = level
        && !(1..=3).contains(&l)
    {
        return Err(AppError::Validation(
            "skilluv_partnership_level must be 1, 2, or 3".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_accepts_valid() {
        assert!(validate_slug("sqlx").is_ok());
        assert!(validate_slug("hello-africa").is_ok());
        assert!(validate_slug("wax-icons-2027").is_ok());
    }

    #[test]
    fn slug_rejects_invalid() {
        assert!(validate_slug("").is_err());
        assert!(validate_slug("Hello").is_err()); // uppercase
        assert!(validate_slug("under_score").is_err());
        assert!(validate_slug(&"x".repeat(81)).is_err());
    }

    #[test]
    fn owner_type_accepts_valid() {
        assert!(validate_owner_type("user").is_ok());
        assert!(validate_owner_type("guild").is_ok());
        assert!(validate_owner_type("enterprise").is_err());
    }

    #[test]
    fn partnership_level_range() {
        assert!(validate_partnership_level(None).is_ok());
        assert!(validate_partnership_level(Some(1)).is_ok());
        assert!(validate_partnership_level(Some(2)).is_ok());
        assert!(validate_partnership_level(Some(3)).is_ok());
        assert!(validate_partnership_level(Some(0)).is_err());
        assert!(validate_partnership_level(Some(4)).is_err());
    }
}
