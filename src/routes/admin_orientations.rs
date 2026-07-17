//! ADM-M3.1 — Admin CRUD sur orientations + orientation_skill_map.
//!
//! Endpoints exposés (tous montés dans `admin_routes()` → passent par le
//! middleware admin_gate = origin + 2FA + capability admin) :
//!
//! - POST   /admin/orientations                              — crée (curated=true si admin veut)
//! - PATCH  /admin/orientations/{slug}                       — édite (slug immutable)
//! - POST   /admin/orientations/{slug}/skills                — attache skill (upsert)
//! - DELETE /admin/orientations/{slug}/skills/{skill_id}     — détache skill (idempotent)

use axum::extract::{Path, Query, State};
use axum::routing::{delete, patch, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;

pub fn admin_orientation_routes() -> Router<AppState> {
    Router::new()
        .route("/admin/orientations", post(create_orientation))
        .route("/admin/orientations/{slug}", patch(patch_orientation))
        .route("/admin/orientations/{slug}/skills", post(attach_skill))
        .route(
            "/admin/orientations/{slug}/skills/{skill_id}",
            delete(detach_skill),
        )
}

const ALLOWED_DOMAINS: &[&str] = &[
    "code", "design", "game", "security", "soft_skills", "ai", "ops",
];

fn build_response(data: Value) -> Value {
    json!({
        "data": data,
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })
}

fn build_response_with(data: Value, extra_meta: Value) -> Value {
    let mut meta = json!({
        "request_id": Uuid::new_v4().to_string(),
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });
    if let (Some(m), Some(e)) = (meta.as_object_mut(), extra_meta.as_object()) {
        for (k, v) in e {
            m.insert(k.clone(), v.clone());
        }
    }
    json!({ "data": data, "meta": meta })
}

#[derive(Debug, Deserialize)]
pub struct DryRunQuery {
    #[serde(default)]
    dry_run: bool,
}

// ═══════════════════════════════════════════════════════════════════
// POST /admin/orientations
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct CreateOrientationBody {
    slug: String,
    name: String,
    #[serde(default)]
    description: Option<String>,
    primary_domain: String,
    #[serde(default)]
    secondary_domains: Vec<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    is_curated: Option<bool>,
}

async fn create_orientation(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateOrientationBody>,
) -> Result<Json<Value>, AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await?;
    crate::middleware::admin_destructive::enforce_admin_destructive(&state, auth.user_id).await?;

    validate_slug(&body.slug)?;
    if body.name.trim().is_empty() || body.name.len() > 120 {
        return Err(AppError::Validation("name must be 1..=120 chars".into()));
    }
    if !ALLOWED_DOMAINS.contains(&body.primary_domain.as_str()) {
        return Err(AppError::Validation(format!(
            "primary_domain invalid; allowed: {ALLOWED_DOMAINS:?}"
        )));
    }

    if crate::middleware::admin_destructive::is_admin_dry_run() {
        return Ok(Json(build_response(json!({
            "dry_run": true,
            "would_create": {
                "slug": body.slug, "name": body.name, "primary_domain": body.primary_domain,
            }
        }))));
    }

    let row: (Uuid, String, String, String, String, Vec<String>, Vec<String>, bool, bool) = sqlx::query_as(
        r#"
        INSERT INTO orientations
            (slug, name, description, primary_domain, secondary_domains, tags, is_curated, created_by)
        VALUES ($1, $2, $3, $4, $5, $6, COALESCE($7, FALSE), $8)
        RETURNING id, slug, name, description, primary_domain, secondary_domains, tags, is_curated, is_archived
        "#,
    )
    .bind(&body.slug)
    .bind(&body.name)
    .bind(body.description.clone().unwrap_or_default())
    .bind(&body.primary_domain)
    .bind(&body.secondary_domains)
    .bind(&body.tags)
    .bind(body.is_curated)
    .bind(auth.user_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(ref db_err) if db_err.constraint() == Some("orientations_slug_key") => {
            AppError::Validation(format!("slug '{}' already exists", body.slug))
        }
        _ => AppError::Database(e),
    })?;

    let orientation = json!({
        "id": row.0, "slug": row.1, "name": row.2, "description": row.3,
        "primary_domain": row.4, "secondary_domains": row.5, "tags": row.6,
        "is_curated": row.7, "is_archived": row.8,
    });

    crate::services::audit::record(
        &state.db,
        crate::services::audit::AuditEntry {
            actor_type: crate::services::audit::ActorType::Admin,
            actor_id: Some(auth.user_id),
            action: "orientation.create",
            target_type: Some("orientation"),
            target_id: Some(row.0),
            metadata: Some(json!({"after": orientation})),
            headers: None,
        },
    )
    .await;

    Ok(Json(build_response(json!({ "orientation": orientation }))))
}

// ═══════════════════════════════════════════════════════════════════
// PATCH /admin/orientations/{slug}
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct PatchOrientationBody {
    #[serde(default)]
    slug: Option<String>, // any attempt → 400
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    primary_domain: Option<String>,
    #[serde(default)]
    secondary_domains: Option<Vec<String>>,
    #[serde(default)]
    tags: Option<Vec<String>>,
    #[serde(default)]
    is_curated: Option<bool>,
    #[serde(default)]
    is_archived: Option<bool>,
}

async fn patch_orientation(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Query(q): Query<DryRunQuery>,
    Json(body): Json<PatchOrientationBody>,
) -> Result<Json<Value>, AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await?;
    crate::middleware::admin_destructive::enforce_admin_destructive(&state, auth.user_id).await?;

    if body.slug.is_some() {
        return Err(AppError::Validation(
            "slug is immutable — create a new orientation instead".into(),
        ));
    }
    if let Some(pd) = body.primary_domain.as_ref() {
        if !ALLOWED_DOMAINS.contains(&pd.as_str()) {
            return Err(AppError::Validation(format!(
                "primary_domain invalid; allowed: {ALLOWED_DOMAINS:?}"
            )));
        }
    }

    let before: (Uuid, String, String, String, String, Vec<String>, Vec<String>, bool, bool) = sqlx::query_as(
        "SELECT id, slug, name, description, primary_domain, secondary_domains, tags, is_curated, is_archived
         FROM orientations WHERE slug = $1",
    )
    .bind(&slug)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("orientation slug '{slug}' not found")))?;

    let name = body.name.clone().unwrap_or(before.2.clone());
    let description = body.description.clone().unwrap_or(before.3.clone());
    let primary_domain = body.primary_domain.clone().unwrap_or(before.4.clone());
    let secondary = body.secondary_domains.clone().unwrap_or(before.5.clone());
    let tags = body.tags.clone().unwrap_or(before.6.clone());
    let curated = body.is_curated.unwrap_or(before.7);
    let archived = body.is_archived.unwrap_or(before.8);

    let before_json = json!({
        "id": before.0, "slug": before.1, "name": before.2, "description": before.3,
        "primary_domain": before.4, "secondary_domains": before.5, "tags": before.6,
        "is_curated": before.7, "is_archived": before.8,
    });
    let after_json = json!({
        "id": before.0, "slug": before.1.clone(), "name": name, "description": description,
        "primary_domain": primary_domain, "secondary_domains": secondary, "tags": tags,
        "is_curated": curated, "is_archived": archived,
    });

    let dry = q.dry_run || crate::middleware::admin_destructive::is_admin_dry_run();
    if dry {
        return Ok(Json(build_response_with(
            json!({ "orientation": before_json.clone() }),
            json!({ "dry_run_preview": { "before": before_json, "after": after_json } }),
        )));
    }

    let updated: (Uuid, String, String, String, String, Vec<String>, Vec<String>, bool, bool) = sqlx::query_as(
        r#"
        UPDATE orientations SET
            name = $2, description = $3, primary_domain = $4,
            secondary_domains = $5, tags = $6, is_curated = $7, is_archived = $8,
            updated_at = NOW()
        WHERE slug = $1
        RETURNING id, slug, name, description, primary_domain, secondary_domains, tags, is_curated, is_archived
        "#,
    )
    .bind(&slug)
    .bind(&after_json["name"].as_str().unwrap_or(""))
    .bind(&after_json["description"].as_str().unwrap_or(""))
    .bind(&after_json["primary_domain"].as_str().unwrap_or(""))
    .bind(&after_json["secondary_domains"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect::<Vec<_>>()).unwrap_or_default())
    .bind(&after_json["tags"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect::<Vec<_>>()).unwrap_or_default())
    .bind(after_json["is_curated"].as_bool().unwrap_or(false))
    .bind(after_json["is_archived"].as_bool().unwrap_or(false))
    .fetch_one(&state.db)
    .await?;

    let orientation = json!({
        "id": updated.0, "slug": updated.1, "name": updated.2, "description": updated.3,
        "primary_domain": updated.4, "secondary_domains": updated.5, "tags": updated.6,
        "is_curated": updated.7, "is_archived": updated.8,
    });

    crate::services::audit::record(
        &state.db,
        crate::services::audit::AuditEntry {
            actor_type: crate::services::audit::ActorType::Admin,
            actor_id: Some(auth.user_id),
            action: "orientation.update",
            target_type: Some("orientation"),
            target_id: Some(updated.0),
            metadata: Some(json!({"before": before_json, "after": orientation})),
            headers: None,
        },
    )
    .await;

    Ok(Json(build_response(json!({ "orientation": orientation }))))
}

// ═══════════════════════════════════════════════════════════════════
// POST /admin/orientations/{slug}/skills   (upsert)
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct AttachSkillBody {
    skill_id: Uuid,
    #[serde(default)]
    is_core: Option<bool>,
    #[serde(default)]
    is_recommended: Option<bool>,
    #[serde(default)]
    weight: Option<f32>,
}

async fn attach_skill(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(body): Json<AttachSkillBody>,
) -> Result<Json<Value>, AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await?;
    crate::middleware::admin_destructive::enforce_admin_destructive(&state, auth.user_id).await?;

    if let Some(w) = body.weight {
        if !w.is_finite() || w <= 0.0 {
            return Err(AppError::Validation("weight must be > 0".into()));
        }
    }

    let orientation_id: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM orientations WHERE slug = $1")
            .bind(&slug)
            .fetch_optional(&state.db)
            .await?;
    let orientation_id = orientation_id
        .ok_or_else(|| AppError::NotFound(format!("orientation slug '{slug}' not found")))?
        .0;

    let is_core = body.is_core.unwrap_or(false);
    let is_recommended = body.is_recommended.unwrap_or(true);
    let weight = body.weight.unwrap_or(1.0);

    sqlx::query(
        r#"
        INSERT INTO orientation_skill_map (orientation_id, skill_id, is_core, is_recommended, weight)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (orientation_id, skill_id) DO UPDATE SET
            is_core = EXCLUDED.is_core,
            is_recommended = EXCLUDED.is_recommended,
            weight = EXCLUDED.weight
        "#,
    )
    .bind(orientation_id)
    .bind(body.skill_id)
    .bind(is_core)
    .bind(is_recommended)
    .bind(weight)
    .execute(&state.db)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(ref db_err)
            if db_err.constraint() == Some("orientation_skill_map_skill_id_fkey") =>
        {
            AppError::NotFound(format!("skill_id {} not found in skill_nodes", body.skill_id))
        }
        _ => AppError::Database(e),
    })?;

    crate::services::audit::record(
        &state.db,
        crate::services::audit::AuditEntry {
            actor_type: crate::services::audit::ActorType::Admin,
            actor_id: Some(auth.user_id),
            action: "orientation.skill_attach",
            target_type: Some("orientation"),
            target_id: Some(orientation_id),
            metadata: Some(json!({
                "skill_id": body.skill_id, "is_core": is_core,
                "is_recommended": is_recommended, "weight": weight,
            })),
            headers: None,
        },
    )
    .await;

    Ok(Json(build_response(json!({
        "attached": true,
        "orientation_slug": slug,
        "skill_id": body.skill_id,
    }))))
}

// ═══════════════════════════════════════════════════════════════════
// DELETE /admin/orientations/{slug}/skills/{skill_id}   (idempotent)
// ═══════════════════════════════════════════════════════════════════

async fn detach_skill(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((slug, skill_id)): Path<(String, Uuid)>,
) -> Result<Json<Value>, AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await?;
    crate::middleware::admin_destructive::enforce_admin_destructive(&state, auth.user_id).await?;

    let orientation_id: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM orientations WHERE slug = $1")
            .bind(&slug)
            .fetch_optional(&state.db)
            .await?;
    let orientation_id = orientation_id
        .ok_or_else(|| AppError::NotFound(format!("orientation slug '{slug}' not found")))?
        .0;

    // Idempotent : execute renvoie 0 si déjà détaché → on renvoie 200 quand même.
    sqlx::query("DELETE FROM orientation_skill_map WHERE orientation_id = $1 AND skill_id = $2")
        .bind(orientation_id)
        .bind(skill_id)
        .execute(&state.db)
        .await?;

    crate::services::audit::record(
        &state.db,
        crate::services::audit::AuditEntry {
            actor_type: crate::services::audit::ActorType::Admin,
            actor_id: Some(auth.user_id),
            action: "orientation.skill_detach",
            target_type: Some("orientation"),
            target_id: Some(orientation_id),
            metadata: Some(json!({"skill_id": skill_id})),
            headers: None,
        },
    )
    .await;

    Ok(Json(build_response(json!({ "detached": true }))))
}

fn validate_slug(s: &str) -> Result<(), AppError> {
    let len = s.len();
    if !(3..=60).contains(&len) {
        return Err(AppError::Validation("slug length must be 3..=60".into()));
    }
    if !s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        return Err(AppError::Validation(
            "slug must match ^[a-z0-9-]+$".into(),
        ));
    }
    Ok(())
}
