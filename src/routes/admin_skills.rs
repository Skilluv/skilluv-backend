//! MVP.md §2.2 #14 — Admin CRUD sur skill_nodes.
//!
//! Endpoints :
//!   - GET   /admin/skills                — liste paginée (filtres domain, parent_id)
//!   - POST  /admin/skills                — crée un skill node
//!   - PUT   /admin/skills/{id}           — édite (slug + domain + parent + is_skilluv_specific)

use axum::extract::{Path, Query, State};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;

// Type aliases pour clippy::type_complexity (rangées sqlx::query_as).
type AdminSkillsRow99 = (
    Uuid,
    String,
    String,
    Option<String>,
    String,
    Option<Uuid>,
    bool,
);

pub fn admin_skill_routes() -> Router<AppState> {
    Router::new()
        .route("/admin/skills", get(list_skills).post(create_skill))
        .route("/admin/skills/{id}", put(update_skill))
}

const ALLOWED_DOMAINS: &[&str] = &[
    "code",
    "design",
    "game",
    "security",
    "soft_skills",
    "ai",
    "ops",
];

fn wrap(data: Value) -> Value {
    json!({
        "data": data,
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })
}

fn validate_slug(s: &str) -> Result<(), AppError> {
    let len = s.len();
    if !(2..=80).contains(&len) {
        return Err(AppError::Validation("slug length must be 2..=80".into()));
    }
    if !s
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        return Err(AppError::Validation("slug must match ^[a-z0-9_-]+$".into()));
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════
// GET /admin/skills?domain=code&parent_id=UUID&page=1&per_page=50
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct ListQuery {
    #[serde(default)]
    domain: Option<String>,
    #[serde(default)]
    parent_id: Option<Uuid>,
    #[serde(default)]
    is_skilluv_specific: Option<bool>,
    #[serde(default)]
    q: Option<String>,
    #[serde(default)]
    page: Option<i64>,
    #[serde(default)]
    per_page: Option<i64>,
}

async fn list_skills(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<ListQuery>,
) -> Result<Json<Value>, AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await?;

    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(50).clamp(1, 200);
    let offset = (page - 1) * per_page;

    if let Some(d) = q.domain.as_ref()
        && !ALLOWED_DOMAINS.contains(&d.as_str())
    {
        return Err(AppError::Validation(format!(
            "domain invalid; allowed: {ALLOWED_DOMAINS:?}"
        )));
    }

    let search = q.q.as_ref().map(|s| format!("%{}%", s.to_lowercase()));

    let rows: Vec<AdminSkillsRow99> = sqlx::query_as(
        r#"
            SELECT id, slug, display_name, description, domain, parent_id, is_skilluv_specific
            FROM skill_nodes
            WHERE ($1::text IS NULL OR domain = $1)
              AND ($2::uuid IS NULL OR parent_id = $2)
              AND ($3::bool IS NULL OR is_skilluv_specific = $3)
              AND ($4::text IS NULL
                   OR LOWER(slug) LIKE $4
                   OR LOWER(display_name) LIKE $4)
            ORDER BY domain, display_name
            LIMIT $5 OFFSET $6
            "#,
    )
    .bind(q.domain.as_ref())
    .bind(q.parent_id)
    .bind(q.is_skilluv_specific)
    .bind(search.as_ref())
    .bind(per_page)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let total: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM skill_nodes
        WHERE ($1::text IS NULL OR domain = $1)
          AND ($2::uuid IS NULL OR parent_id = $2)
          AND ($3::bool IS NULL OR is_skilluv_specific = $3)
        "#,
    )
    .bind(q.domain.as_ref())
    .bind(q.parent_id)
    .bind(q.is_skilluv_specific)
    .fetch_one(&state.db)
    .await?;

    let items: Vec<Value> = rows
        .into_iter()
        .map(|(id, slug, name, desc, domain, parent, specific)| {
            json!({
                "id": id, "slug": slug, "display_name": name, "description": desc,
                "domain": domain, "parent_id": parent, "is_skilluv_specific": specific,
            })
        })
        .collect();

    Ok(Json(json!({
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

// ═══════════════════════════════════════════════════════════════════
// POST /admin/skills
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct CreateSkillBody {
    slug: String,
    display_name: String,
    #[serde(default)]
    description: Option<String>,
    domain: String,
    #[serde(default)]
    parent_id: Option<Uuid>,
    #[serde(default)]
    aliases: Option<Vec<String>>,
    #[serde(default)]
    external_refs: Option<Value>,
    #[serde(default)]
    is_skilluv_specific: Option<bool>,
}

async fn create_skill(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateSkillBody>,
) -> Result<Json<Value>, AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await?;
    crate::middleware::admin_destructive::enforce_admin_destructive(&state, auth.user_id).await?;

    validate_slug(&body.slug)?;
    if body.display_name.trim().is_empty() || body.display_name.len() > 150 {
        return Err(AppError::Validation(
            "display_name must be 1..=150 chars".into(),
        ));
    }
    if !ALLOWED_DOMAINS.contains(&body.domain.as_str()) {
        return Err(AppError::Validation(format!(
            "domain invalid; allowed: {ALLOWED_DOMAINS:?}"
        )));
    }

    if crate::middleware::admin_destructive::is_admin_dry_run() {
        return Ok(Json(wrap(json!({
            "dry_run": true,
            "would_create": { "slug": body.slug, "domain": body.domain },
        }))));
    }

    let aliases = body.aliases.clone().unwrap_or_default();
    let refs = body.external_refs.clone().unwrap_or_else(|| json!({}));

    let (id,): (Uuid,) = sqlx::query_as(
        r#"INSERT INTO skill_nodes
                (slug, display_name, description, domain, parent_id, aliases, external_refs, is_skilluv_specific)
           VALUES ($1, $2, $3, $4, $5, $6, $7, COALESCE($8, FALSE))
           RETURNING id"#,
    )
    .bind(&body.slug)
    .bind(&body.display_name)
    .bind(body.description.clone())
    .bind(&body.domain)
    .bind(body.parent_id)
    .bind(&aliases)
    .bind(&refs)
    .bind(body.is_skilluv_specific)
    .fetch_one(&state.db)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(ref db_err) if db_err.constraint() == Some("skill_nodes_slug_key") => {
            AppError::Validation(format!("skill slug '{}' already exists", body.slug))
        }
        _ => AppError::Database(e),
    })?;

    crate::services::audit::record(
        &state.db,
        crate::services::audit::AuditEntry {
            actor_type: crate::services::audit::ActorType::Admin,
            actor_id: Some(auth.user_id),
            action: "skill_node.create",
            target_type: Some("skill_node"),
            target_id: Some(id),
            metadata: Some(json!({
                "slug": body.slug, "domain": body.domain,
                "parent_id": body.parent_id,
            })),
            headers: None,
        },
    )
    .await;

    Ok(Json(wrap(json!({
        "skill": {
            "id": id, "slug": body.slug, "display_name": body.display_name,
            "domain": body.domain, "parent_id": body.parent_id,
            "aliases": aliases, "external_refs": refs,
            "is_skilluv_specific": body.is_skilluv_specific.unwrap_or(false),
        }
    }))))
}

// ═══════════════════════════════════════════════════════════════════
// PUT /admin/skills/{id}
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct UpdateSkillBody {
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    domain: Option<String>,
    #[serde(default)]
    parent_id: Option<Option<Uuid>>, // Some(None) → unset parent
    #[serde(default)]
    aliases: Option<Vec<String>>,
    #[serde(default)]
    external_refs: Option<Value>,
    #[serde(default)]
    is_skilluv_specific: Option<bool>,
}

async fn update_skill(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateSkillBody>,
) -> Result<Json<Value>, AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await?;
    crate::middleware::admin_destructive::enforce_admin_destructive(&state, auth.user_id).await?;

    if let Some(d) = body.domain.as_ref()
        && !ALLOWED_DOMAINS.contains(&d.as_str())
    {
        return Err(AppError::Validation(format!(
            "domain invalid; allowed: {ALLOWED_DOMAINS:?}"
        )));
    }
    // Anti-self-parent (contrainte DB aussi).
    if let Some(Some(pid)) = body.parent_id
        && pid == id
    {
        return Err(AppError::Validation(
            "skill cannot be its own parent".into(),
        ));
    }

    // COALESCE côté SQL pour patch partiel. parent_id est Option<Option<Uuid>>
    // pour distinguer "absent" (garde valeur actuelle) et "None explicite" (unset).
    let parent_present = body.parent_id.is_some();
    let parent_value = body.parent_id.and_then(|inner| inner);

    let affected = sqlx::query(
        r#"
        UPDATE skill_nodes SET
            display_name        = COALESCE($2, display_name),
            description         = COALESCE($3, description),
            domain              = COALESCE($4, domain),
            parent_id           = CASE WHEN $5::bool THEN $6 ELSE parent_id END,
            aliases             = COALESCE($7, aliases),
            external_refs       = COALESCE($8, external_refs),
            is_skilluv_specific = COALESCE($9, is_skilluv_specific),
            updated_at          = NOW()
        WHERE id = $1
        "#,
    )
    .bind(id)
    .bind(&body.display_name)
    .bind(&body.description)
    .bind(&body.domain)
    .bind(parent_present)
    .bind(parent_value)
    .bind(body.aliases.as_ref())
    .bind(&body.external_refs)
    .bind(body.is_skilluv_specific)
    .execute(&state.db)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(AppError::NotFound(format!("skill_node {id} not found")));
    }

    crate::services::audit::record(
        &state.db,
        crate::services::audit::AuditEntry {
            actor_type: crate::services::audit::ActorType::Admin,
            actor_id: Some(auth.user_id),
            action: "skill_node.update",
            target_type: Some("skill_node"),
            target_id: Some(id),
            metadata: Some(json!({
                "display_name": body.display_name, "domain": body.domain,
                "parent_id_touched": parent_present,
            })),
            headers: None,
        },
    )
    .await;

    Ok(Json(wrap(json!({ "updated": true, "id": id }))))
}
