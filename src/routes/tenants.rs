//! White-label tenants — Phase 5.9.
//!
//! Endpoints d'administration (création/config tenant) + résolution du tenant
//! courant depuis le sous-domaine ou l'en-tête `X-Skilluv-Tenant`. La stratégie
//! d'isolation reste souple : les challenges portent un `tenant_id` optionnel
//! (NULL = public), les users un `primary_tenant_id`, et la table
//! `tenant_memberships` gère l'appartenance multi-tenant.

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;

pub const ROOT_TENANT_ID: Uuid = Uuid::from_bytes([
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x01,
]);

/// True si le tenant courant est le tenant racine (`skilluv`).
pub fn is_root_tenant(id: Uuid) -> bool {
    id == ROOT_TENANT_ID
}

pub fn tenant_routes() -> Router<AppState> {
    Router::new()
        .route("/tenants/current", get(get_current_tenant))
        .route("/admin/tenants", get(list_tenants).post(create_tenant))
        .route(
            "/admin/tenants/{id}",
            get(get_tenant).put(update_tenant),
        )
        .route(
            "/admin/tenants/{id}/members",
            get(list_members).post(add_member),
        )
        .route(
            "/admin/tenants/{id}/cohorts",
            get(list_cohorts).post(create_cohort),
        )
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
    {
        if let Some(id) = tenant_id_by_slug(db, slug).await? {
            return Ok(id);
        }
    }
    // 2. Sous-domaine du Host
    if let Some(host) = headers.get("host").and_then(|v| v.to_str().ok()) {
        let base = host.split(':').next().unwrap_or(host);
        let parts: Vec<&str> = base.split('.').collect();
        if parts.len() >= 3 {
            let sub = parts[0];
            if sub != "www" && sub != "app" {
                if let Some(id) = tenant_id_by_subdomain(db, sub).await? {
                    return Ok(id);
                }
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
    let row: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM tenants WHERE subdomain = $1 AND active = TRUE",
    )
    .bind(subdomain)
    .fetch_optional(db)
    .await?;
    Ok(row.map(|(id,)| id))
}

// ─── Endpoints ───────────────────────────────────────────────────

async fn get_current_tenant(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
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
    Ok(Json(build_response(json!({
        "id": row.get::<Uuid, _>("id"),
        "slug": row.get::<String, _>("slug"),
        "name": row.get::<String, _>("name"),
        "subdomain": row.get::<Option<String>, _>("subdomain"),
        "custom_domain": row.get::<Option<String>, _>("custom_domain"),
        "logo_url": row.get::<Option<String>, _>("logo_url"),
        "primary_color": row.get::<Option<String>, _>("primary_color"),
        "secondary_color": row.get::<Option<String>, _>("secondary_color"),
        "plan": row.get::<String, _>("plan"),
    }))))
}

async fn list_tenants(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    if auth.role != "admin" {
        return Err(AppError::Forbidden);
    }
    let rows = sqlx::query(
        r#"
        SELECT t.id, t.slug, t.name, t.subdomain, t.plan, t.max_users, t.active,
               t.created_at,
               (SELECT COUNT(*)::BIGINT FROM tenant_memberships m WHERE m.tenant_id = t.id) AS members_count
        FROM tenants t ORDER BY t.created_at DESC
        "#,
    )
    .fetch_all(&state.db)
    .await?;
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.get::<Uuid, _>("id"),
                "slug": r.get::<String, _>("slug"),
                "name": r.get::<String, _>("name"),
                "subdomain": r.get::<Option<String>, _>("subdomain"),
                "plan": r.get::<String, _>("plan"),
                "max_users": r.get::<i32, _>("max_users"),
                "active": r.get::<bool, _>("active"),
                "members_count": r.get::<i64, _>("members_count"),
                "created_at": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
            })
        })
        .collect();
    Ok(Json(build_response(json!({ "tenants": items }))))
}

#[derive(Deserialize)]
struct CreateTenantBody {
    slug: String,
    name: String,
    subdomain: Option<String>,
    contact_email: String,
    plan: Option<String>,
    max_users: Option<i32>,
    primary_color: Option<String>,
    logo_url: Option<String>,
}

async fn create_tenant(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateTenantBody>,
) -> Result<Json<Value>, AppError> {
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
    Ok(Json(build_response(json!({ "tenant_id": inserted.0 }))))
}

async fn get_tenant(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    if auth.role != "admin" {
        return Err(AppError::Forbidden);
    }
    let row = sqlx::query("SELECT * FROM tenants WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound("tenant not found".into()))?;
    Ok(Json(build_response(json!({
        "id": row.get::<Uuid, _>("id"),
        "slug": row.get::<String, _>("slug"),
        "name": row.get::<String, _>("name"),
        "subdomain": row.get::<Option<String>, _>("subdomain"),
        "custom_domain": row.get::<Option<String>, _>("custom_domain"),
        "logo_url": row.get::<Option<String>, _>("logo_url"),
        "primary_color": row.get::<Option<String>, _>("primary_color"),
        "secondary_color": row.get::<Option<String>, _>("secondary_color"),
        "plan": row.get::<String, _>("plan"),
        "max_users": row.get::<i32, _>("max_users"),
        "contact_email": row.get::<String, _>("contact_email"),
        "active": row.get::<bool, _>("active"),
        "settings": row.get::<serde_json::Value, _>("settings"),
    }))))
}

#[derive(Deserialize)]
struct UpdateTenantBody {
    name: Option<String>,
    subdomain: Option<String>,
    custom_domain: Option<String>,
    logo_url: Option<String>,
    primary_color: Option<String>,
    secondary_color: Option<String>,
    plan: Option<String>,
    max_users: Option<i32>,
    active: Option<bool>,
    settings: Option<serde_json::Value>,
}

async fn update_tenant(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateTenantBody>,
) -> Result<Json<Value>, AppError> {
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
    Ok(Json(build_response(json!({ "updated": true }))))
}

async fn list_members(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(tenant_id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
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
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "user_id": r.get::<Uuid, _>("user_id"),
                "username": r.get::<String, _>("username"),
                "display_name": r.get::<String, _>("display_name"),
                "email": r.get::<String, _>("email"),
                "role": r.get::<String, _>("role"),
                "joined_at": r.get::<chrono::DateTime<chrono::Utc>, _>("joined_at"),
            })
        })
        .collect();
    Ok(Json(build_response(json!({ "members": items }))))
}

#[derive(Deserialize)]
struct AddMemberBody {
    user_id: Uuid,
    role: Option<String>,
}

async fn add_member(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(tenant_id): Path<Uuid>,
    Json(body): Json<AddMemberBody>,
) -> Result<Json<Value>, AppError> {
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
    Ok(Json(build_response(json!({ "added": true }))))
}

async fn list_cohorts(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(tenant_id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
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
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.get::<Uuid, _>("id"),
                "name": r.get::<String, _>("name"),
                "starts_at": r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("starts_at"),
                "ends_at": r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("ends_at"),
                "active": r.get::<bool, _>("active"),
                "members_count": r.get::<i64, _>("members_count"),
            })
        })
        .collect();
    Ok(Json(build_response(json!({ "cohorts": items }))))
}

#[derive(Deserialize)]
struct CreateCohortBody {
    name: String,
    starts_at: Option<chrono::DateTime<chrono::Utc>>,
    ends_at: Option<chrono::DateTime<chrono::Utc>>,
}

async fn create_cohort(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(tenant_id): Path<Uuid>,
    Json(body): Json<CreateCohortBody>,
) -> Result<Json<Value>, AppError> {
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
    Ok(Json(build_response(json!({ "cohort_id": inserted.0 }))))
}
