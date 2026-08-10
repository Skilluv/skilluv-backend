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
        // P26 v2 SKI-124 — per-repo challenge stats (workflow health).
        .route("/admin/projects/{slug}/stats", get(project_stats))
        // SKI-110 (M-05) — manual ingestion trigger for a single project.
        .route("/admin/projects/{slug}/ingest", post(trigger_ingest))
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

    // P26 v2 SKI-110 — GitHub ingestion wiring. All optional so the admin
    // route stays backward-compatible; enforced pairwise below.
    #[serde(default)]
    github_repo_owner: Option<String>,
    #[serde(default)]
    github_repo_name: Option<String>,
    #[serde(default)]
    curated_labels: Option<Vec<String>>,
    /// One of "auto", "curator_review", "manual_only".
    /// See migration 0055 for semantics.
    #[serde(default)]
    slice_ingestion_mode: Option<String>,
    /// Ordered list of the project's primary skill domains. First entry
    /// is the fallback for `primary_domain` when an ingested issue does
    /// not carry a `domain:*` label (SKI-101 enricher).
    #[serde(default)]
    skill_domains: Option<Vec<String>>,
}

fn default_true() -> bool {
    true
}

/// Create a new project (admin curated OSS/enterprise project).
#[utoipa::path(
    post, path = "/api/admin/projects", tag = "admin",
    request_body(content = serde_json::Value),
    responses((status = 201, body = serde_json::Value), (status = 403, body = crate::api_response::ErrorResponse)),
    security(("cookie_auth" = [])),
)]
pub async fn create_project(
    _gate: crate::middleware::admin_gate::AdminGate,
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateProjectBody>,
) -> Result<Json<Value>, AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await?;

    validate_slug(&body.slug)?;
    validate_owner_type(&body.owner_type)?;
    validate_flagship(&body)?;
    validate_partnership_level(body.skilluv_partnership_level)?;
    validate_github_pair(
        body.github_repo_owner.as_deref(),
        body.github_repo_name.as_deref(),
    )?;
    validate_ingestion_mode(body.slice_ingestion_mode.as_deref())?;
    validate_skill_domains(body.skill_domains.as_deref())?;

    // SKI-110 — warn (don't fail) when the combination would ingest nothing.
    warn_ingest_will_no_op(
        body.slice_ingestion_mode.as_deref(),
        body.curated_labels.as_deref(),
        &body.slug,
    );

    let inserted: (Uuid, String) = sqlx::query_as(
        r#"
        INSERT INTO projects (
            slug, name, description, repo_url, demo_url, tech_stack,
            is_oss, looking_for_contributors, owner_type, owner_id, curated_by_admin,
            is_flagship, flagship_steward_user_id, skilluv_partnership_level,
            skilluv_editorial_notes,
            github_repo_owner, github_repo_name, curated_labels,
            slice_ingestion_mode, skill_domains
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15,
                $16, $17, COALESCE($18, '{}'::text[]),
                COALESCE($19, 'curator_review'), COALESCE($20, '{}'::text[]))
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
    .bind(&body.github_repo_owner)
    .bind(&body.github_repo_name)
    .bind(&body.curated_labels)
    .bind(&body.slice_ingestion_mode)
    .bind(&body.skill_domains)
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

    // P26 v2 SKI-110 — same fields as create, all optional.
    #[serde(default)]
    github_repo_owner: Option<String>,
    #[serde(default)]
    github_repo_name: Option<String>,
    #[serde(default)]
    curated_labels: Option<Vec<String>>,
    #[serde(default)]
    slice_ingestion_mode: Option<String>,
    #[serde(default)]
    skill_domains: Option<Vec<String>>,
}

/// Patch a project by slug.
#[utoipa::path(
    patch, path = "/api/admin/projects/{slug}", tag = "admin",
    params(("slug" = String, Path)),
    request_body(content = serde_json::Value),
    responses((status = 200, body = serde_json::Value), (status = 403, body = crate::api_response::ErrorResponse), (status = 404, body = crate::api_response::ErrorResponse)),
    security(("cookie_auth" = [])),
)]
pub async fn patch_project(
    _gate: crate::middleware::admin_gate::AdminGate,
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(body): Json<PatchProjectBody>,
) -> Result<Json<Value>, AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await?;
    validate_slug(&slug)?;
    validate_partnership_level(body.skilluv_partnership_level)?;
    // Pairwise github validation only when at least one field is present:
    // PATCH can legitimately update only owner if the other was already set,
    // so we only reject explicit half-clears (one=Some("") means "clear").
    validate_github_pair(
        body.github_repo_owner.as_deref(),
        body.github_repo_name.as_deref(),
    )?;
    validate_ingestion_mode(body.slice_ingestion_mode.as_deref())?;
    validate_skill_domains(body.skill_domains.as_deref())?;
    warn_ingest_will_no_op(
        body.slice_ingestion_mode.as_deref(),
        body.curated_labels.as_deref(),
        &slug,
    );

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
            github_repo_owner = COALESCE($14, github_repo_owner),
            github_repo_name = COALESCE($15, github_repo_name),
            curated_labels = COALESCE($16, curated_labels),
            slice_ingestion_mode = COALESCE($17, slice_ingestion_mode),
            skill_domains = COALESCE($18, skill_domains),
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
    .bind(&body.github_repo_owner)
    .bind(&body.github_repo_name)
    .bind(&body.curated_labels)
    .bind(&body.slice_ingestion_mode)
    .bind(&body.skill_domains)
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

/// Archive a project (soft-delete).
#[utoipa::path(
    delete, path = "/api/admin/projects/{slug}", tag = "admin",
    params(("slug" = String, Path)),
    responses((status = 200, body = serde_json::Value), (status = 403, body = crate::api_response::ErrorResponse), (status = 404, body = crate::api_response::ErrorResponse)),
    security(("cookie_auth" = [])),
)]
pub async fn archive_project(
    _gate: crate::middleware::admin_gate::AdminGate,
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

/// List projects (admin view).
#[utoipa::path(
    get, path = "/api/admin/projects", tag = "admin",
    responses((status = 200, body = serde_json::Value), (status = 403, body = crate::api_response::ErrorResponse)),
    security(("cookie_auth" = [])),
)]
pub async fn list_projects(
    _gate: crate::middleware::admin_gate::AdminGate,
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
    // SKI-109 (M-04): P26 v2 fields, needed by admin UI so the edit
    // form can pre-fill the ingestion config it was allowed to write via
    // POST/PATCH (SKI-110). Arrays default to [] via COALESCE so the
    // front never sees null.
    github_repo_owner: Option<String>,
    github_repo_name: Option<String>,
    curated_labels: Vec<String>,
    slice_ingestion_mode: Option<String>,
    skill_domains: Vec<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    archived_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Get a project by slug (admin view).
#[utoipa::path(
    get, path = "/api/admin/projects/{slug}", tag = "admin",
    params(("slug" = String, Path)),
    responses((status = 200, body = serde_json::Value), (status = 403, body = crate::api_response::ErrorResponse), (status = 404, body = crate::api_response::ErrorResponse)),
    security(("cookie_auth" = [])),
)]
pub async fn get_project(
    _gate: crate::middleware::admin_gate::AdminGate,
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
               skilluv_editorial_notes,
               github_repo_owner, github_repo_name,
               COALESCE(curated_labels, ARRAY[]::text[]) AS curated_labels,
               slice_ingestion_mode,
               COALESCE(skill_domains, ARRAY[]::text[]) AS skill_domains,
               created_at, updated_at, archived_at
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
        "github_repo_owner": r.github_repo_owner,
        "github_repo_name": r.github_repo_name,
        "curated_labels": r.curated_labels,
        "slice_ingestion_mode": r.slice_ingestion_mode,
        "skill_domains": r.skill_domains,
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

// ─── P26 v2 SKI-110 validators ───────────────────────────────────

/// Reject when exactly one of (owner, name) is set — always a mistake.
/// Also refuse empty strings on either side (would insert broken data).
fn validate_github_pair(owner: Option<&str>, name: Option<&str>) -> Result<(), AppError> {
    let owner_present = owner.is_some_and(|s| !s.is_empty());
    let name_present = name.is_some_and(|s| !s.is_empty());
    if owner_present != name_present {
        return Err(AppError::Validation(
            "github_repo_owner and github_repo_name must be set together (or both omitted)".into(),
        ));
    }
    // Neither empty-string counts as "clear" from a PATCH; a real caller
    // wanting to clear should send explicit NULL (which serde translates
    // as Option::None → we do nothing, keeping the existing value).
    for (label, value) in [("github_repo_owner", owner), ("github_repo_name", name)] {
        if let Some(v) = value
            && v.is_empty()
        {
            return Err(AppError::Validation(format!(
                "{label} cannot be an empty string"
            )));
        }
    }
    Ok(())
}

fn validate_ingestion_mode(mode: Option<&str>) -> Result<(), AppError> {
    if let Some(m) = mode
        && !matches!(m, "auto" | "curator_review" | "manual_only")
    {
        return Err(AppError::Validation(
            "slice_ingestion_mode must be one of: auto, curator_review, manual_only".into(),
        ));
    }
    Ok(())
}

fn validate_skill_domains(domains: Option<&[String]>) -> Result<(), AppError> {
    let Some(list) = domains else {
        return Ok(());
    };
    for d in list {
        if !crate::services::validator_applications::VALID_DOMAINS.contains(&d.as_str()) {
            return Err(AppError::Validation(format!(
                "unknown skill_domain: {d}; allowed: {:?}",
                crate::services::validator_applications::VALID_DOMAINS
            )));
        }
    }
    Ok(())
}

/// Log a warning (no failure) when the combination of mode + labels
/// would silently no-op the ingest — operators usually don't intend this.
fn warn_ingest_will_no_op(mode: Option<&str>, curated_labels: Option<&[String]>, slug: &str) {
    if mode == Some("auto") && curated_labels.is_some_and(|l| l.is_empty()) {
        tracing::warn!(
            slug,
            "project set to slice_ingestion_mode='auto' but curated_labels is empty — \
             the ingestor will not pick up any issues"
        );
    }
}

// ═══════════════════════════════════════════════════════════════════
// P26 v2 SKI-124 — per-repo challenge stats
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct StatsQuery {
    /// Rolling window in days for time-to-* averages. Default 90,
    /// clamped 7..365 to keep the query cheap.
    #[serde(default)]
    window_days: Option<i32>,
}

/// GET /api/admin/projects/{slug}/stats?window_days=90
///
/// Returns aggregate workflow-health metrics on a project. Distinct
/// from the P17 badges dashboard (per-user) and the SKI-122 public
/// endpoint (community pulse): this is the admin operator's view of
/// how the workflow is performing on THIS repo.
pub async fn project_stats(
    _gate: crate::middleware::admin_gate::AdminGate,
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Query(q): Query<StatsQuery>,
) -> Result<Json<Value>, AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await?;
    validate_slug(&slug)?;
    let window_days = q.window_days.unwrap_or(90).clamp(7, 365);

    let project_id: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM projects WHERE slug = $1")
        .bind(&slug)
        .fetch_optional(&state.db)
        .await?;
    let project_id = project_id
        .ok_or_else(|| AppError::NotFound(format!("project {slug} not found")))?
        .0;

    // Slice count breakdown by status — single aggregate for cheapness.
    type StatusCounts = (i64, i64, i64, i64, i64, i64, i64, i64, i64, i64);
    let counts: StatusCounts = sqlx::query_as(
        r#"
        SELECT
          COUNT(*) FILTER (WHERE status = 'draft')::bigint,
          COUNT(*) FILTER (WHERE status = 'open')::bigint,
          COUNT(*) FILTER (WHERE status = 'claimed')::bigint,
          COUNT(*) FILTER (WHERE status = 'in_progress')::bigint,
          COUNT(*) FILTER (WHERE status = 'submitted')::bigint,
          COUNT(*) FILTER (WHERE status = 'ci_green')::bigint,
          COUNT(*) FILTER (WHERE status = 'pending_validation')::bigint,
          COUNT(*) FILTER (WHERE status = 'validated')::bigint,
          COUNT(*) FILTER (WHERE status = 'merged')::bigint,
          COUNT(*) FILTER (WHERE status = 'closed')::bigint
        FROM project_slices WHERE project_id = $1
        "#,
    )
    .bind(project_id)
    .fetch_one(&state.db)
    .await?;

    // Time-to-* averages (hours), restricted to slices that HIT the target
    // state within the window. Postgres EXTRACT(EPOCH FROM interval)/3600.
    type Averages = (Option<f64>, Option<f64>, Option<f64>);
    let (avg_submit_h, avg_validate_h, avg_merge_h): Averages = sqlx::query_as(
        r#"
        SELECT
          AVG(EXTRACT(EPOCH FROM (submitted_at - claimed_at)) / 3600.0)
            FILTER (WHERE submitted_at IS NOT NULL AND claimed_at IS NOT NULL
                      AND submitted_at > NOW() - ($2 || ' days')::interval),
          AVG(EXTRACT(EPOCH FROM (validated_at - submitted_at)) / 3600.0)
            FILTER (WHERE validated_at IS NOT NULL AND submitted_at IS NOT NULL
                      AND validated_at > NOW() - ($2 || ' days')::interval),
          AVG(EXTRACT(EPOCH FROM (updated_at - validated_at)) / 3600.0)
            FILTER (WHERE status = 'merged' AND validated_at IS NOT NULL
                      AND updated_at > NOW() - ($2 || ' days')::interval)
          FROM project_slices WHERE project_id = $1
        "#,
    )
    .bind(project_id)
    .bind(window_days.to_string())
    .fetch_one(&state.db)
    .await?;

    // "How aligned are our validations with upstream merges?"
    // Numerator = merged (bonus tier). Denominator = validated + merged
    // (both are Skilluv successes; only merged also got the upstream nod).
    let (validated_count, merged_count): (i64, i64) = sqlx::query_as(
        r#"
        SELECT
          COUNT(*) FILTER (WHERE status = 'validated' AND validated_at > NOW() - ($2 || ' days')::interval)::bigint,
          COUNT(*) FILTER (WHERE status = 'merged' AND updated_at > NOW() - ($2 || ' days')::interval)::bigint
          FROM project_slices WHERE project_id = $1
        "#,
    )
    .bind(project_id)
    .bind(window_days.to_string())
    .fetch_one(&state.db)
    .await?;
    let validated_to_merged_ratio = if validated_count + merged_count > 0 {
        merged_count as f64 / (validated_count + merged_count) as f64
    } else {
        0.0
    };

    // Adoption of the SKI-101 label-based enrichment. Reads the
    // JSONB field written by the ingestor.
    let (from_label, from_default): (i64, i64) = sqlx::query_as(
        r#"
        SELECT
          COUNT(*) FILTER (WHERE external_metadata->'enrichment'->>'domain_source' = 'label')::bigint,
          COUNT(*) FILTER (WHERE external_metadata->'enrichment'->>'domain_source' = 'project_default')::bigint
          FROM project_slices WHERE project_id = $1
        "#,
    )
    .bind(project_id)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(wrap(json!({
        "window_days": window_days,
        "slices": {
            "draft":              counts.0,
            "open":               counts.1,
            "claimed":            counts.2,
            "in_progress":        counts.3,
            "submitted":          counts.4,
            "ci_green":           counts.5,
            "pending_validation": counts.6,
            "validated":          counts.7,
            "merged":             counts.8,
            "closed":             counts.9,
        },
        "avg_time_to_submit_hours":   avg_submit_h,
        "avg_time_to_validate_hours": avg_validate_h,
        "avg_time_to_merge_hours":    avg_merge_h,
        "validated_to_merged_ratio":  validated_to_merged_ratio,
        "domain_source_distribution": {
            "label":           from_label,
            "project_default": from_default,
        },
    }))))
}

// ═══════════════════════════════════════════════════════════════════
// POST /admin/projects/{slug}/ingest — SKI-110 (M-05)
// ═══════════════════════════════════════════════════════════════════

/// Manually trigger a single ingestion pass on one project.
///
/// The P11 GitHubIngestor otherwise runs on an hourly cron; when tuning
/// a freshly-wired project (repo + curated labels + ingestion mode), the
/// hour-long feedback loop is what an admin hits first. This endpoint
/// runs a synchronous pass on ONE project and returns a detailed report
/// so the admin can tell "config wrong, 0 issues match" apart from
/// "config right, nothing new to ingest".
///
/// Rate-limited to 1 call / minute / project via a recent-audit-log
/// check — cheap protection against a spammed button burning the
/// unauthenticated GitHub API quota (60/h/IP).
///
/// Returns 400 when the project has no GitHub repo pair configured OR
/// runs `slice_ingestion_mode = 'manual_only'` (both would be silent
/// no-ops in the ingestor loop, which is exactly the confusion this
/// endpoint exists to avoid).
#[utoipa::path(
    post, path = "/api/admin/projects/{slug}/ingest", tag = "admin",
    params(("slug" = String, Path)),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, body = crate::api_response::ErrorResponse),
        (status = 403, body = crate::api_response::ErrorResponse),
        (status = 404, body = crate::api_response::ErrorResponse),
        (status = 429, body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn trigger_ingest(
    _gate: crate::middleware::admin_gate::AdminGate,
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> Result<Json<Value>, AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await?;
    validate_slug(&slug)?;

    // Load the project + ingestion config in a single query so 404 fires
    // before we do any validation work.
    #[derive(sqlx::FromRow)]
    struct Row {
        id: Uuid,
        github_repo_owner: Option<String>,
        github_repo_name: Option<String>,
        curated_labels: Option<Vec<String>>,
        slice_ingestion_mode: Option<String>,
    }
    let row: Option<Row> = sqlx::query_as(
        r#"
        SELECT id, github_repo_owner, github_repo_name,
               curated_labels, slice_ingestion_mode
        FROM projects WHERE slug = $1
        "#,
    )
    .bind(&slug)
    .fetch_optional(&state.db)
    .await?;
    let row = row.ok_or_else(|| AppError::NotFound(format!("project {slug} not found")))?;

    // Fail loudly when the caller's about to trigger a no-op — the whole
    // point of this endpoint is to give the admin a real signal.
    let (Some(owner), Some(name)) = (
        row.github_repo_owner.as_deref(),
        row.github_repo_name.as_deref(),
    ) else {
        return Err(AppError::Validation(
            "project has no github_repo_owner / github_repo_name — set them via PATCH /admin/projects/{slug} first".into(),
        ));
    };
    let mode = row.slice_ingestion_mode.as_deref().unwrap_or("curator_review");
    if mode == "manual_only" {
        return Err(AppError::Validation(
            "project runs in slice_ingestion_mode='manual_only' — the ingestor is intentionally disabled for it".into(),
        ));
    }
    let labels_matched: Vec<String> = row.curated_labels.clone().unwrap_or_default();
    if labels_matched.is_empty() {
        return Err(AppError::Validation(
            "project has no curated_labels — set at least one label to filter GitHub issues".into(),
        ));
    }

    // 1/minute/project via audit_log lookback. Durable across restarts,
    // no extra table, no in-process state.
    let recent: Option<(i64,)> = sqlx::query_as(
        r#"
        SELECT 1::bigint
          FROM audit_log
         WHERE action = 'project.ingest.manual'
           AND target_id = $1
           AND created_at > NOW() - INTERVAL '60 seconds'
         LIMIT 1
        "#,
    )
    .bind(row.id)
    .fetch_optional(&state.db)
    .await?;
    if recent.is_some() {
        return Err(AppError::ServiceUnavailable(
            "project.ingest.manual rate-limited to 1 call / minute / project — retry shortly".into(),
        ));
    }

    // Actually run the ingestion pass. Errors bubble as 500.
    let ingestor = crate::services::slice_ingestion::GitHubIngestor;
    let report = crate::services::slice_ingestion::SliceIngestor::ingest_for_project(
        &ingestor, &state.db, row.id,
    )
    .await?;

    tracing::info!(
        slug = %slug, owner = %owner, name = %name, mode = %mode,
        issues_seen = report.issues_seen,
        slices_created = report.slices_created,
        slices_skipped = report.slices_skipped_duplicate,
        "manual ingest completed"
    );

    crate::services::audit::record(
        &state.db,
        crate::services::audit::AuditEntry {
            actor_type: crate::services::audit::ActorType::Admin,
            actor_id: Some(auth.user_id),
            action: "project.ingest.manual",
            target_type: Some("project"),
            target_id: Some(row.id),
            metadata: Some(json!({
                "slug": slug,
                "mode": mode,
                "issues_seen": report.issues_seen,
                "slices_created": report.slices_created,
                "slices_skipped_existing": report.slices_skipped_duplicate,
                "errors": report.errors,
            })),
            headers: None,
        },
    )
    .await;

    Ok(Json(wrap(json!({
        "issues_seen": report.issues_seen,
        "slices_created": report.slices_created,
        "slices_skipped_existing": report.slices_skipped_duplicate,
        "errors": report.errors,
        "mode": mode,
        "labels_matched": labels_matched,
    }))))
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

    // ─── SKI-110 tests ─────────────────────────────────────────────

    #[test]
    fn github_pair_both_or_neither() {
        assert!(validate_github_pair(None, None).is_ok());
        assert!(validate_github_pair(Some("launchbadge"), Some("sqlx")).is_ok());
        assert!(validate_github_pair(Some("launchbadge"), None).is_err());
        assert!(validate_github_pair(None, Some("sqlx")).is_err());
    }

    #[test]
    fn github_pair_rejects_empty_strings() {
        // Empty is not the same as absent — refuse to insert broken data.
        assert!(validate_github_pair(Some(""), Some("sqlx")).is_err());
        assert!(validate_github_pair(Some("launchbadge"), Some("")).is_err());
    }

    #[test]
    fn ingestion_mode_allowed_values() {
        assert!(validate_ingestion_mode(None).is_ok());
        assert!(validate_ingestion_mode(Some("auto")).is_ok());
        assert!(validate_ingestion_mode(Some("curator_review")).is_ok());
        assert!(validate_ingestion_mode(Some("manual_only")).is_ok());
        assert!(validate_ingestion_mode(Some("automatic")).is_err());
        assert!(validate_ingestion_mode(Some("")).is_err());
    }

    #[test]
    fn skill_domains_all_must_be_valid() {
        let ok: Vec<String> = ["code", "ops"].iter().map(|s| s.to_string()).collect();
        assert!(validate_skill_domains(Some(&ok)).is_ok());
        let empty: Vec<String> = vec![];
        assert!(validate_skill_domains(Some(&empty)).is_ok());
        assert!(validate_skill_domains(None).is_ok());

        let bad: Vec<String> = ["code", "blockchain"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert!(validate_skill_domains(Some(&bad)).is_err());
    }
}
