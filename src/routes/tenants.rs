//! White-label tenants — Phase 5.9.
//!
//! Endpoints d'administration (création/config tenant) + résolution du tenant
//! courant depuis le sous-domaine ou l'en-tête `X-Skilluv-Tenant`. La stratégie
//! d'isolation reste souple : les challenges portent un `tenant_id` optionnel
//! (NULL = public), les users un `primary_tenant_id`, et la table
//! `tenant_memberships` gère l'appartenance multi-tenant.

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::middleware::AuthUser;

pub const ROOT_TENANT_ID: Uuid =
    Uuid::from_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x01]);

/// True si le tenant courant est le tenant racine (`skilluv`).
pub fn is_root_tenant(id: Uuid) -> bool {
    id == ROOT_TENANT_ID
}

pub fn tenant_routes() -> Router<AppState> {
    Router::new()
        .route("/tenants/current", get(get_current_tenant))
        .route("/admin/tenants", get(list_tenants).post(create_tenant))
        .route("/admin/tenants/{id}", get(get_tenant).put(update_tenant))
        .route(
            "/admin/tenants/{id}/members",
            get(list_members).post(add_member),
        )
        .route(
            "/admin/tenants/{id}/cohorts",
            get(list_cohorts).post(create_cohort),
        )
}

/// Résout le tenant courant à partir des headers de la requête.
///
/// Ordre de priorité :
///   1. En-tête `X-Skilluv-Tenant` (utilisé par le front en dev + preview)
///   2. `Host` header : extrait le sous-domaine (`acme.skilluv.com` → `acme`)
///   3. Fallback : le tenant racine
pub async fn resolve_tenant_from_headers(
    db: &sqlx::PgPool,
    headers: &HeaderMap,
) -> Result<Uuid, AppError> {
    // 1. Header explicite (slug)
    if let Some(slug) = headers
        .get("x-skilluv-tenant")
        .and_then(|v| v.to_str().ok())
        && let Some(id) = tenant_id_by_slug(db, slug).await?
    {
        return Ok(id);
    }
    // 2. Sous-domaine du Host
    if let Some(host) = headers.get("host").and_then(|v| v.to_str().ok()) {
        let base = host.split(':').next().unwrap_or(host);
        let parts: Vec<&str> = base.split('.').collect();
        if parts.len() >= 3 {
            let sub = parts[0];
            if sub != "www"
                && sub != "app"
                && let Some(id) = tenant_id_by_subdomain(db, sub).await?
            {
                return Ok(id);
            }
        }
    }
    Ok(ROOT_TENANT_ID)
}

async fn tenant_id_by_slug(db: &sqlx::PgPool, slug: &str) -> Result<Option<Uuid>, AppError> {
    let row: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM tenants WHERE slug = $1 AND active = TRUE")
            .bind(slug)
            .fetch_optional(db)
            .await?;
    Ok(row.map(|(id,)| id))
}

async fn tenant_id_by_subdomain(
    db: &sqlx::PgPool,
    subdomain: &str,
) -> Result<Option<Uuid>, AppError> {
    let row: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM tenants WHERE subdomain = $1 AND active = TRUE")
            .bind(subdomain)
            .fetch_optional(db)
            .await?;
    Ok(row.map(|(id,)| id))
}

// ─── Types de réponse ────────────────────────────────────────────

/// Public projection of a tenant (theming + branding only — no admin
/// fields like max_users or contact_email).
#[derive(Debug, Serialize, ToSchema)]
pub struct PublicTenant {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub subdomain: Option<String>,
    pub custom_domain: Option<String>,
    pub logo_url: Option<String>,
    pub primary_color: Option<String>,
    pub secondary_color: Option<String>,
    /// `starter`, `pro`, `enterprise`.
    pub plan: String,
}

/// Admin-only projection with quotas + contact.
#[derive(Debug, Serialize, ToSchema)]
pub struct AdminTenant {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub subdomain: Option<String>,
    pub custom_domain: Option<String>,
    pub logo_url: Option<String>,
    pub primary_color: Option<String>,
    pub secondary_color: Option<String>,
    pub plan: String,
    pub max_users: i32,
    pub contact_email: String,
    pub active: bool,
    pub settings: serde_json::Value,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminTenantSummary {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub subdomain: Option<String>,
    pub plan: String,
    pub max_users: i32,
    pub active: bool,
    pub members_count: i64,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct TenantsListQuery {
    #[param(minimum = 1, maximum = 100000)]
    pub page: Option<i64>,
    #[param(minimum = 1, maximum = 200)]
    pub per_page: Option<i64>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateTenantBody {
    /// Lowercase alphanumeric with dashes, >= 2 chars. Unique.
    #[schema(max_length = 10000)]
    pub slug: String,
    #[schema(max_length = 10000)]
    pub name: String,
    #[schema(max_length = 10000)]
    pub subdomain: Option<String>,
    #[schema(max_length = 10000)]
    pub contact_email: String,
    /// `starter` (default), `pro`, `enterprise`.
    #[schema(max_length = 10000)]
    pub plan: Option<String>,
    pub max_users: Option<i32>,
    #[schema(max_length = 10000)]
    pub primary_color: Option<String>,
    #[schema(max_length = 10000)]
    pub logo_url: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TenantCreatedResponse {
    pub tenant_id: Uuid,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateTenantBody {
    #[schema(max_length = 10000)]
    pub name: Option<String>,
    #[schema(max_length = 10000)]
    pub subdomain: Option<String>,
    #[schema(max_length = 10000)]
    pub custom_domain: Option<String>,
    #[schema(max_length = 10000)]
    pub logo_url: Option<String>,
    #[schema(max_length = 10000)]
    pub primary_color: Option<String>,
    #[schema(max_length = 10000)]
    pub secondary_color: Option<String>,
    #[schema(max_length = 10000)]
    pub plan: Option<String>,
    pub max_users: Option<i32>,
    pub active: Option<bool>,
    pub settings: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TenantUpdatedResponse {
    pub updated: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TenantMemberRow {
    pub user_id: Uuid,
    pub username: String,
    pub display_name: String,
    pub email: String,
    /// `member`, `instructor`, `admin`, `owner`.
    pub role: String,
    pub joined_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MembersResponse {
    pub members: Vec<TenantMemberRow>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AddMemberBody {
    pub user_id: Uuid,
    /// Defaults to `member`. One of `member`, `instructor`, `admin`,
    /// `owner`.
    #[schema(max_length = 10000)]
    pub role: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MemberAddedResponse {
    pub added: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CohortRow {
    pub id: Uuid,
    pub name: String,
    pub starts_at: Option<chrono::DateTime<chrono::Utc>>,
    pub ends_at: Option<chrono::DateTime<chrono::Utc>>,
    pub active: bool,
    pub members_count: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CohortsResponse {
    pub cohorts: Vec<CohortRow>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateCohortBody {
    #[schema(max_length = 10000)]
    pub name: String,
    pub starts_at: Option<chrono::DateTime<chrono::Utc>>,
    pub ends_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CohortCreatedResponse {
    pub cohort_id: Uuid,
}

// ─── Endpoints ───────────────────────────────────────────────────

/// Return the tenant resolved from the current request (via
/// `X-Skilluv-Tenant` header or subdomain, else root).
#[utoipa::path(
    get,
    path = "/api/tenants/current",
    tag = "enterprise",
    responses(
        (status = 200, description = "Current tenant (public projection)", body = ApiResponse<PublicTenant>),
        (status = 404, description = "Tenant not found (should never happen — root is a fallback)", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn get_current_tenant(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<PublicTenant>>, AppError> {
    let tenant_id = resolve_tenant_from_headers(&state.db, &headers).await?;
    let row = sqlx::query(
        r#"
        SELECT id, slug, name, subdomain, custom_domain, logo_url,
               primary_color, secondary_color, plan
        FROM tenants WHERE id = $1
        "#,
    )
    .bind(tenant_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound("tenant not found".into()))?;
    Ok(Json(ApiResponse::new(PublicTenant {
        id: row.get("id"),
        slug: row.get("slug"),
        name: row.get("name"),
        subdomain: row.get("subdomain"),
        custom_domain: row.get("custom_domain"),
        logo_url: row.get("logo_url"),
        primary_color: row.get("primary_color"),
        secondary_color: row.get("secondary_color"),
        plan: row.get("plan"),
    })))
}

/// Admin only: list every tenant with member counts.
///
/// **Payload shape**: standard admin listing convention
/// `{data: [AdminTenantSummary], pagination: {...}, meta: {...}}`.
#[utoipa::path(
    get,
    path = "/api/admin/tenants",
    tag = "admin",
    params(TenantsListQuery),
    responses(
        (status = 200, description = "Tenants list (paginated)", body = serde_json::Value),
        (status = 403, description = "Not an admin", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn list_tenants(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<TenantsListQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    if auth.role != "admin" {
        return Err(AppError::Forbidden);
    }
    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(50).clamp(1, 200);
    let offset = (page - 1) * per_page;

    let rows = sqlx::query(
        r#"
        SELECT t.id, t.slug, t.name, t.subdomain, t.plan, t.max_users, t.active,
               t.created_at,
               (SELECT COUNT(*)::BIGINT FROM tenant_memberships m WHERE m.tenant_id = t.id) AS members_count
        FROM tenants t ORDER BY t.created_at DESC
        LIMIT $1 OFFSET $2
        "#,
    )
    .bind(per_page)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tenants")
        .fetch_one(&state.db)
        .await?;
    let items: Vec<AdminTenantSummary> = rows
        .iter()
        .map(|r| AdminTenantSummary {
            id: r.get("id"),
            slug: r.get("slug"),
            name: r.get("name"),
            subdomain: r.get("subdomain"),
            plan: r.get("plan"),
            max_users: r.get("max_users"),
            active: r.get("active"),
            members_count: r.get("members_count"),
            created_at: r.get("created_at"),
        })
        .collect();
    Ok(Json(serde_json::json!({
        "data": items,
        "pagination": {
            "page": page, "per_page": per_page, "total": total,
            "total_pages": if per_page > 0 { (total + per_page - 1) / per_page } else { 0 },
        },
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })))
}

/// Admin only: create a new tenant.
#[utoipa::path(
    post,
    path = "/api/admin/tenants",
    tag = "admin",
    request_body = CreateTenantBody,
    responses(
        (status = 200, description = "Tenant created", body = ApiResponse<TenantCreatedResponse>),
        (status = 400, description = "Invalid slug or plan", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not an admin", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn create_tenant(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateTenantBody>,
) -> Result<Json<ApiResponse<TenantCreatedResponse>>, AppError> {
    if auth.role != "admin" {
        return Err(AppError::Forbidden);
    }
    let slug = body.slug.trim().to_lowercase();
    if !slug.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') || slug.len() < 2 {
        return Err(AppError::Validation(
            "slug must be lowercase alphanumeric with dashes, >= 2 chars".into(),
        ));
    }
    let plan = body.plan.clone().unwrap_or_else(|| "starter".into());
    if !matches!(plan.as_str(), "starter" | "pro" | "enterprise") {
        return Err(AppError::Validation("invalid plan".into()));
    }
    let inserted: (Uuid,) = sqlx::query_as(
        r#"
        INSERT INTO tenants
            (slug, name, subdomain, contact_email, plan, max_users, primary_color, logo_url)
        VALUES ($1, $2, $3, $4, $5, $6, COALESCE($7, '#6C5CE7'), $8)
        RETURNING id
        "#,
    )
    .bind(&slug)
    .bind(&body.name)
    .bind(&body.subdomain)
    .bind(&body.contact_email)
    .bind(&plan)
    .bind(body.max_users.unwrap_or(100))
    .bind(&body.primary_color)
    .bind(&body.logo_url)
    .fetch_one(&state.db)
    .await?;
    metrics::counter!("skilluv_tenants_created_total").increment(1);
    Ok(Json(ApiResponse::new(TenantCreatedResponse {
        tenant_id: inserted.0,
    })))
}

/// Admin only: full tenant detail (with quotas + contact + settings).
#[utoipa::path(
    get,
    path = "/api/admin/tenants/{id}",
    tag = "admin",
    params(("id" = Uuid, Path, description = "Tenant UUID")),
    responses(
        (status = 200, description = "Tenant detail", body = ApiResponse<AdminTenant>),
        (status = 403, description = "Not an admin", body = crate::api_response::ErrorResponse),
        (status = 404, description = "Tenant not found", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn get_tenant(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<AdminTenant>>, AppError> {
    if auth.role != "admin" {
        return Err(AppError::Forbidden);
    }
    let row = sqlx::query("SELECT * FROM tenants WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound("tenant not found".into()))?;
    Ok(Json(ApiResponse::new(AdminTenant {
        id: row.get("id"),
        slug: row.get("slug"),
        name: row.get("name"),
        subdomain: row.get("subdomain"),
        custom_domain: row.get("custom_domain"),
        logo_url: row.get("logo_url"),
        primary_color: row.get("primary_color"),
        secondary_color: row.get("secondary_color"),
        plan: row.get("plan"),
        max_users: row.get("max_users"),
        contact_email: row.get("contact_email"),
        active: row.get("active"),
        settings: row.get("settings"),
    })))
}

/// Admin only: partial update on a tenant.
#[utoipa::path(
    put,
    path = "/api/admin/tenants/{id}",
    tag = "admin",
    params(("id" = Uuid, Path, description = "Tenant UUID")),
    request_body = UpdateTenantBody,
    responses(
        (status = 200, description = "Updated", body = ApiResponse<TenantUpdatedResponse>),
        (status = 403, description = "Not an admin", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn update_tenant(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateTenantBody>,
) -> Result<Json<ApiResponse<TenantUpdatedResponse>>, AppError> {
    if auth.role != "admin" {
        return Err(AppError::Forbidden);
    }
    sqlx::query(
        r#"
        UPDATE tenants SET
            name = COALESCE($1, name),
            subdomain = COALESCE($2, subdomain),
            custom_domain = COALESCE($3, custom_domain),
            logo_url = COALESCE($4, logo_url),
            primary_color = COALESCE($5, primary_color),
            secondary_color = COALESCE($6, secondary_color),
            plan = COALESCE($7, plan),
            max_users = COALESCE($8, max_users),
            active = COALESCE($9, active),
            settings = COALESCE($10, settings),
            updated_at = NOW()
        WHERE id = $11
        "#,
    )
    .bind(&body.name)
    .bind(&body.subdomain)
    .bind(&body.custom_domain)
    .bind(&body.logo_url)
    .bind(&body.primary_color)
    .bind(&body.secondary_color)
    .bind(&body.plan)
    .bind(body.max_users)
    .bind(body.active)
    .bind(&body.settings)
    .bind(id)
    .execute(&state.db)
    .await?;
    Ok(Json(ApiResponse::new(TenantUpdatedResponse {
        updated: true,
    })))
}

/// Admin only: list a tenant's members (cap 500).
#[utoipa::path(
    get,
    path = "/api/admin/tenants/{id}/members",
    tag = "admin",
    params(("id" = Uuid, Path, description = "Tenant UUID")),
    responses(
        (status = 200, description = "Members", body = ApiResponse<MembersResponse>),
        (status = 403, description = "Not an admin", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn list_members(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(tenant_id): Path<Uuid>,
) -> Result<Json<ApiResponse<MembersResponse>>, AppError> {
    if auth.role != "admin" {
        return Err(AppError::Forbidden);
    }
    let rows = sqlx::query(
        r#"
        SELECT m.user_id, m.role, m.joined_at,
               u.username, u.display_name, u.email
        FROM tenant_memberships m
        JOIN users u ON u.id = m.user_id
        WHERE m.tenant_id = $1
        ORDER BY m.joined_at DESC
        LIMIT 500
        "#,
    )
    .bind(tenant_id)
    .fetch_all(&state.db)
    .await?;
    let items: Vec<TenantMemberRow> = rows
        .iter()
        .map(|r| TenantMemberRow {
            user_id: r.get("user_id"),
            username: r.get("username"),
            display_name: r.get("display_name"),
            email: r.get("email"),
            role: r.get("role"),
            joined_at: r.get("joined_at"),
        })
        .collect();
    Ok(Json(ApiResponse::new(MembersResponse { members: items })))
}

/// Admin only: enroll a user in a tenant. Idempotent (upserts role).
/// Enforces `max_users` quota.
#[utoipa::path(
    post,
    path = "/api/admin/tenants/{id}/members",
    tag = "admin",
    params(("id" = Uuid, Path, description = "Tenant UUID")),
    request_body = AddMemberBody,
    responses(
        (status = 200, description = "Member added", body = ApiResponse<MemberAddedResponse>),
        (status = 400, description = "Invalid role or quota reached", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not an admin", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn add_member(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(tenant_id): Path<Uuid>,
    Json(body): Json<AddMemberBody>,
) -> Result<Json<ApiResponse<MemberAddedResponse>>, AppError> {
    if auth.role != "admin" {
        return Err(AppError::Forbidden);
    }
    let role = body.role.unwrap_or_else(|| "member".into());
    if !matches!(role.as_str(), "member" | "instructor" | "admin" | "owner") {
        return Err(AppError::Validation("invalid role".into()));
    }
    // Vérifier quota
    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*)::BIGINT FROM tenant_memberships WHERE tenant_id = $1")
            .bind(tenant_id)
            .fetch_one(&state.db)
            .await?;
    let max_users: (i32,) = sqlx::query_as("SELECT max_users FROM tenants WHERE id = $1")
        .bind(tenant_id)
        .fetch_one(&state.db)
        .await?;
    if count.0 >= max_users.0 as i64 {
        return Err(AppError::Validation(format!(
            "tenant reached max_users cap ({})",
            max_users.0
        )));
    }
    sqlx::query(
        r#"
        INSERT INTO tenant_memberships (tenant_id, user_id, role)
        VALUES ($1, $2, $3)
        ON CONFLICT (tenant_id, user_id) DO UPDATE SET role = EXCLUDED.role
        "#,
    )
    .bind(tenant_id)
    .bind(body.user_id)
    .bind(&role)
    .execute(&state.db)
    .await?;
    Ok(Json(ApiResponse::new(MemberAddedResponse { added: true })))
}

/// Admin only: list the tenant's cohorts (learning groups).
#[utoipa::path(
    get,
    path = "/api/admin/tenants/{id}/cohorts",
    tag = "admin",
    params(("id" = Uuid, Path, description = "Tenant UUID")),
    responses(
        (status = 200, description = "Cohorts", body = ApiResponse<CohortsResponse>),
        (status = 403, description = "Not an admin", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn list_cohorts(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(tenant_id): Path<Uuid>,
) -> Result<Json<ApiResponse<CohortsResponse>>, AppError> {
    if auth.role != "admin" {
        return Err(AppError::Forbidden);
    }
    let rows = sqlx::query(
        r#"
        SELECT c.id, c.name, c.starts_at, c.ends_at, c.active,
               (SELECT COUNT(*)::BIGINT FROM tenant_cohort_members m WHERE m.cohort_id = c.id) AS members_count
        FROM tenant_cohorts c WHERE c.tenant_id = $1
        ORDER BY c.starts_at DESC NULLS LAST
        "#,
    )
    .bind(tenant_id)
    .fetch_all(&state.db)
    .await?;
    let items: Vec<CohortRow> = rows
        .iter()
        .map(|r| CohortRow {
            id: r.get("id"),
            name: r.get("name"),
            starts_at: r.get("starts_at"),
            ends_at: r.get("ends_at"),
            active: r.get("active"),
            members_count: r.get("members_count"),
        })
        .collect();
    Ok(Json(ApiResponse::new(CohortsResponse { cohorts: items })))
}

/// Admin only: create a new cohort under a tenant.
#[utoipa::path(
    post,
    path = "/api/admin/tenants/{id}/cohorts",
    tag = "admin",
    params(("id" = Uuid, Path, description = "Tenant UUID")),
    request_body = CreateCohortBody,
    responses(
        (status = 200, description = "Cohort created", body = ApiResponse<CohortCreatedResponse>),
        (status = 403, description = "Not an admin", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn create_cohort(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(tenant_id): Path<Uuid>,
    Json(body): Json<CreateCohortBody>,
) -> Result<Json<ApiResponse<CohortCreatedResponse>>, AppError> {
    if auth.role != "admin" {
        return Err(AppError::Forbidden);
    }
    let inserted: (Uuid,) = sqlx::query_as(
        r#"
        INSERT INTO tenant_cohorts (tenant_id, name, starts_at, ends_at)
        VALUES ($1, $2, $3, $4) RETURNING id
        "#,
    )
    .bind(tenant_id)
    .bind(&body.name)
    .bind(body.starts_at)
    .bind(body.ends_at)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(ApiResponse::new(CohortCreatedResponse {
        cohort_id: inserted.0,
    })))
}
