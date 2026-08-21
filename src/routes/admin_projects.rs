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

/// Payload of `POST /admin/projects`.
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct CreatedProject {
    pub id: Uuid,
    pub slug: String,
}

// ═══════════════════════════════════════════════════════════════════
// SKI-111 — response schemas
// ═══════════════════════════════════════════════════════════════════

/// One row of `GET /admin/projects`. Narrower than the detail view: the
/// listing deliberately omits the ingestion wiring.
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct AdminProjectRow {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub repo_url: Option<String>,
    pub is_flagship: bool,
    pub curated_by_admin: bool,
    pub skilluv_partnership_level: Option<i16>,
    pub flagship_steward_user_id: Option<Uuid>,
    /// RFC 3339.
    pub created_at: String,
    pub archived_at: Option<String>,
}

/// Response of `GET /admin/projects`.
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct AdminProjectListResponse {
    pub data: Vec<AdminProjectRow>,
    pub pagination: crate::api_response::Pagination,
    pub meta: crate::api_response::MetaInfo,
}

/// Payload of `GET /admin/projects/{slug}` — the full record, including
/// the GitHub ingestion wiring an operator needs to debug a project.
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct AdminProjectDetail {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub repo_url: Option<String>,
    pub demo_url: Option<String>,
    pub tech_stack: Vec<String>,
    pub is_oss: bool,
    pub looking_for_contributors: bool,
    pub owner_type: String,
    pub owner_id: Uuid,
    pub curated_by_admin: bool,
    pub is_flagship: bool,
    pub flagship_steward_user_id: Option<Uuid>,
    pub skilluv_partnership_level: Option<i16>,
    pub skilluv_editorial_notes: Option<String>,
    /// `None` once the repo is detached (SKI-269).
    pub github_repo_owner: Option<String>,
    pub github_repo_name: Option<String>,
    pub curated_labels: Vec<String>,
    pub slice_ingestion_mode: String,
    pub skill_domains: Vec<String>,
    /// RFC 3339.
    pub created_at: String,
    pub updated_at: String,
    pub archived_at: Option<String>,
}

/// Payload of `POST /admin/projects/{slug}/ingest` — one manual pass of
/// the GitHub ingestor.
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct IngestRunReport {
    pub issues_seen: u32,
    pub slices_created: u32,
    /// Issues that already had a slice, so nothing was created.
    pub slices_skipped_existing: u32,
    pub errors: u32,
    /// `auto` or `curator_review` — `manual_only` is refused upstream.
    pub mode: String,
    /// Curated labels that actually matched during this run. Empty means
    /// the label filter let nothing through.
    pub labels_matched: Vec<String>,
}

/// Create a new project (admin curated OSS/enterprise project).
// SKI-111 — the annotation claimed 201; the handler returns `Json<Value>`,
// which axum serves as 200. Corrected to describe what actually happens
// rather than changing a live status code as a side effect of a typing
// pass.
#[utoipa::path(
    post, path = "/api/admin/projects", tag = "admin",
    request_body(content = serde_json::Value),
    responses(
        (status = 200, description = "Project created", body = crate::api_response::ApiResponse<CreatedProject>),
        (status = 400, body = crate::api_response::ErrorResponse),
        (status = 403, body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
    operation_id = "adminProjectsCreateProject",
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
    //
    // SKI-269 — the GitHub pair is a double Option so that `null` can mean
    // "unwire this repo". With a plain Option, serde maps both an absent
    // field and an explicit `null` to `None`, `COALESCE` treats that as
    // "leave alone", and there is no value an admin can send to detach a
    // repo — the PATCH answered 200 while changing nothing. The arrays
    // escape this because `[]` is not `null`.
    //
    //   absent      -> leave unchanged
    //   null        -> set to NULL (both fields together)
    //   "value"     -> write
    #[serde(default, deserialize_with = "deserialize_double_option")]
    github_repo_owner: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    github_repo_name: Option<Option<String>>,
    #[serde(default)]
    curated_labels: Option<Vec<String>>,
    #[serde(default)]
    slice_ingestion_mode: Option<String>,
    #[serde(default)]
    skill_domains: Option<Vec<String>>,
}

/// Serde helper: missing field → `None`, JSON `null` → `Some(None)`,
/// value → `Some(Some(v))`. Same trick as `routes::admin_slices`.
fn deserialize_double_option<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: serde::Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    Ok(Some(Option::<T>::deserialize(deserializer)?))
}

/// Resolve the GitHub pair of a PATCH into a single decision.
///
/// Returns `(should_write, owner, name)`. `should_write == false` means the
/// caller omitted both fields and the columns must be left untouched.
///
/// Both fields move together: a repo is identified by owner *and* name, so
/// clearing or setting one alone would leave the project in a state that
/// cannot be acted upon. Mixing the two intents (one `null`, one value) is
/// rejected rather than guessed at.
fn resolve_github_patch(
    owner: &Option<Option<String>>,
    name: &Option<Option<String>>,
) -> Result<(bool, Option<String>, Option<String>), AppError> {
    match (owner, name) {
        // Neither mentioned: nothing to do.
        (None, None) => Ok((false, None, None)),

        // Both explicitly null: detach the repo.
        (Some(None), Some(None)) => Ok((true, None, None)),

        // Both given: validate as a pair, then write.
        (Some(Some(o)), Some(Some(n))) => {
            validate_github_pair(Some(o.as_str()), Some(n.as_str()))?;
            Ok((true, Some(o.clone()), Some(n.clone())))
        }

        // Anything else is a half-specified change: one field mentioned
        // without the other, or a null paired with a value.
        _ => Err(AppError::Validation(
            "github_repo_owner and github_repo_name must be changed together — \
             send both as strings to wire a repo, or both as null to detach it"
                .into(),
        )),
    }
}

/// Patch a project by slug.
#[utoipa::path(
    patch, path = "/api/admin/projects/{slug}", tag = "admin",
    params(("slug" = String, Path)),
    request_body(content = serde_json::Value),
    responses((status = 200, body = crate::api_response::ApiResponse<crate::api_response::AdminActionResult>), (status = 403, body = crate::api_response::ErrorResponse), (status = 404, body = crate::api_response::ErrorResponse)),
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
    // SKI-269 — resolve the GitHub pair into "touch or not, and to what".
    let (write_github, github_owner, github_name) =
        resolve_github_patch(&body.github_repo_owner, &body.github_repo_name)?;
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
            -- SKI-269 — COALESCE cannot express "set to NULL", so the
            -- GitHub pair is gated on an explicit flag instead.
            github_repo_owner = CASE WHEN $19 THEN $14 ELSE github_repo_owner END,
            github_repo_name  = CASE WHEN $19 THEN $15 ELSE github_repo_name END,
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
    .bind(&github_owner)
    .bind(&github_name)
    .bind(&body.curated_labels)
    .bind(&body.slice_ingestion_mode)
    .bind(&body.skill_domains)
    .bind(write_github)
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
    responses((status = 200, body = crate::api_response::ApiResponse<crate::api_response::AdminActionResult>), (status = 403, body = crate::api_response::ErrorResponse), (status = 404, body = crate::api_response::ErrorResponse)),
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
    responses((status = 200, body = AdminProjectListResponse), (status = 403, body = crate::api_response::ErrorResponse)),
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
    responses((status = 200, body = crate::api_response::ApiResponse<AdminProjectDetail>), (status = 403, body = crate::api_response::ErrorResponse), (status = 404, body = crate::api_response::ErrorResponse)),
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
        if !crate::validators::SKILL_DOMAINS.contains(&d.as_str()) {
            return Err(AppError::Validation(format!(
                "unknown skill_domain: {d}; allowed: {:?}",
                crate::validators::SKILL_DOMAINS
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

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct StatsQuery {
    /// Rolling window in days for time-to-* averages. Default 90,
    /// clamped 7..365 to keep the query cheap.
    #[serde(default)]
    window_days: Option<i32>,
}

/// Slice counts by workflow status.
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct SliceStatusCounts {
    pub draft: i64,
    pub open: i64,
    pub claimed: i64,
    pub in_progress: i64,
    pub submitted: i64,
    pub ci_green: i64,
    pub pending_validation: i64,
    pub validated: i64,
    pub merged: i64,
    pub closed: i64,
}

/// How slices got their domain: from a curated label, or from the
/// project's default (SKI-101 enrichment adoption).
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct DomainSourceDistribution {
    pub label: i64,
    pub project_default: i64,
}

/// Workflow-health metrics for one project.
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct ProjectStats {
    pub window_days: i32,
    pub slices: SliceStatusCounts,
    /// Averages in hours over the window. `None` when no slice reached
    /// the corresponding state, which is different from zero.
    pub avg_time_to_submit_hours: Option<f64>,
    pub avg_time_to_validate_hours: Option<f64>,
    pub avg_time_to_merge_hours: Option<f64>,
    /// Share of validated-or-merged slices that actually merged, 0..=1.
    pub validated_to_merged_ratio: f64,
    pub domain_source_distribution: DomainSourceDistribution,
}

/// GET /api/admin/projects/{slug}/stats?window_days=90
///
/// Returns aggregate workflow-health metrics on a project. Distinct
/// from the P17 badges dashboard (per-user) and the SKI-122 public
/// endpoint (community pulse): this is the admin operator's view of
/// how the workflow is performing on THIS repo.
// SKI-111 — this was the one admin handler with no utoipa annotation at
// all, so it did not appear in the spec and schemathesis never exercised
// it.
#[utoipa::path(
    get,
    path = "/api/admin/projects/{slug}/stats",
    tag = "admin",
    params(
        ("slug" = String, Path, description = "Project slug"),
        StatsQuery,
    ),
    responses(
        (status = 200, description = "Workflow-health metrics", body = crate::api_response::ApiResponse<ProjectStats>),
        (status = 403, description = "Not an admin", body = crate::api_response::ErrorResponse),
        (status = 404, description = "Unknown project", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
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
        (status = 200, body = crate::api_response::ApiResponse<IngestRunReport>),
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
    let mode = row
        .slice_ingestion_mode
        .as_deref()
        .unwrap_or("curator_review");
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
            "project.ingest.manual rate-limited to 1 call / minute / project — retry shortly"
                .into(),
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

    // ─── SKI-269 — GitHub pair patch semantics ─────────────────────

    fn some(v: &str) -> Option<Option<String>> {
        Some(Some(v.to_string()))
    }
    const NULLED: Option<Option<String>> = Some(None);
    const ABSENT: Option<Option<String>> = None;

    #[test]
    fn omitting_both_github_fields_leaves_them_untouched() {
        let (write, owner, name) = resolve_github_patch(&ABSENT, &ABSENT).unwrap();
        assert!(!write, "the columns must not be written at all");
        assert!(owner.is_none() && name.is_none());
    }

    #[test]
    fn explicit_null_on_both_detaches_the_repo() {
        let (write, owner, name) = resolve_github_patch(&NULLED, &NULLED).unwrap();
        assert!(write, "this is the case the old COALESCE could not express");
        assert!(
            owner.is_none() && name.is_none(),
            "both columns are written as NULL"
        );
    }

    #[test]
    fn both_values_wire_the_repo() {
        let (write, owner, name) =
            resolve_github_patch(&some("launchbadge"), &some("sqlx")).unwrap();
        assert!(write);
        assert_eq!(owner.as_deref(), Some("launchbadge"));
        assert_eq!(name.as_deref(), Some("sqlx"));
    }

    #[test]
    fn half_specified_changes_are_refused_rather_than_guessed() {
        // One field mentioned without the other.
        assert!(resolve_github_patch(&some("launchbadge"), &ABSENT).is_err());
        assert!(resolve_github_patch(&ABSENT, &some("sqlx")).is_err());
        assert!(resolve_github_patch(&NULLED, &ABSENT).is_err());
        assert!(resolve_github_patch(&ABSENT, &NULLED).is_err());
        // Mixed intent: detach one side, set the other.
        assert!(resolve_github_patch(&NULLED, &some("sqlx")).is_err());
        assert!(resolve_github_patch(&some("launchbadge"), &NULLED).is_err());
    }

    #[test]
    fn empty_strings_are_still_refused() {
        // Clearing is `null`, not `""` — otherwise a form submitting blank
        // inputs would silently detach a repo.
        assert!(resolve_github_patch(&some(""), &some("")).is_err());
        assert!(resolve_github_patch(&some("launchbadge"), &some("")).is_err());
    }
}
