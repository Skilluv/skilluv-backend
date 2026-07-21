//! ADM-M3.2 — Admin CRUD sur badge_rules (proof engine editor).
//!
//! - POST  /admin/badge-rules                    — crée
//! - PATCH /admin/badge-rules/{slug}             — édite (rejet si admin_editable=false ou déprécié)
//! - POST  /admin/badge-rules/{slug}/deprecate   — soft delete (deprecated_at = NOW)

use axum::extract::{Path, Query, State};
use axum::routing::{patch, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;

pub fn admin_badge_rule_routes() -> Router<AppState> {
    Router::new()
        .route("/admin/badge-rules", post(create_rule))
        .route("/admin/badge-rules/{slug}", patch(patch_rule))
        .route("/admin/badge-rules/{slug}/deprecate", post(deprecate_rule))
}

const ALLOWED_OUTPUT_TYPES: &[&str] = &[
    "skill_patch",
    "rank",
    "guild_crest",
    "challenge_seal",
    "event_stamp",
    "medal",
];
const ALLOWED_RARITIES: &[&str] = &["auto", "common", "rare", "epic", "legendary"];

fn build_response(data: Value) -> Value {
    json!({
        "data": data,
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })
}

fn build_response_with(data: Value, extra: Value) -> Value {
    let mut meta = json!({
        "request_id": Uuid::new_v4().to_string(),
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });
    if let (Some(m), Some(e)) = (meta.as_object_mut(), extra.as_object()) {
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

fn validate_slug(s: &str) -> Result<(), AppError> {
    let len = s.len();
    if !(3..=80).contains(&len) {
        return Err(AppError::Validation("slug length must be 3..=80".into()));
    }
    if !s
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        return Err(AppError::Validation("slug must match ^[a-z0-9_-]+$".into()));
    }
    Ok(())
}

fn validate_conditions(v: &Value) -> Result<(), AppError> {
    let obj = v
        .as_object()
        .ok_or_else(|| AppError::Validation("conditions must be a JSON object".into()))?;
    // Best-effort type validation on known keys.
    if let Some(pt) = obj.get("proof_types") {
        if !pt.is_array() {
            return Err(AppError::Validation(
                "conditions.proof_types must be array".into(),
            ));
        }
    }
    if let Some(mc) = obj.get("min_count") {
        if !mc.is_number() {
            return Err(AppError::Validation(
                "conditions.min_count must be number".into(),
            ));
        }
    }
    if let Some(st) = obj.get("skill_tag") {
        if !st.is_string() {
            return Err(AppError::Validation(
                "conditions.skill_tag must be string".into(),
            ));
        }
    }
    if let Some(vb) = obj.get("verified_by") {
        if !vb.is_array() {
            return Err(AppError::Validation(
                "conditions.verified_by must be array".into(),
            ));
        }
    }
    if let Some(wd) = obj.get("within_days") {
        if !wd.is_number() {
            return Err(AppError::Validation(
                "conditions.within_days must be number".into(),
            ));
        }
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════
// POST /admin/badge-rules
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct CreateRuleBody {
    slug: String,
    output_type: String,
    #[serde(default)]
    output_variant: Option<String>,
    display_name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    icon_key: Option<String>,
    conditions: Value,
    #[serde(default)]
    rarity: Option<String>,
    #[serde(default)]
    admin_editable: Option<bool>,
    #[serde(default)]
    ui_metadata: Option<Value>,
}

async fn create_rule(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateRuleBody>,
) -> Result<Json<Value>, AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await?;
    crate::middleware::admin_destructive::enforce_admin_destructive(&state, auth.user_id).await?;

    validate_slug(&body.slug)?;
    if body.display_name.trim().is_empty() || body.display_name.len() > 120 {
        return Err(AppError::Validation(
            "display_name must be 1..=120 chars".into(),
        ));
    }
    if !ALLOWED_OUTPUT_TYPES.contains(&body.output_type.as_str()) {
        return Err(AppError::Validation(format!(
            "output_type invalid; allowed: {ALLOWED_OUTPUT_TYPES:?}"
        )));
    }
    let rarity = body.rarity.clone().unwrap_or_else(|| "auto".into());
    if !ALLOWED_RARITIES.contains(&rarity.as_str()) {
        return Err(AppError::Validation(format!(
            "rarity invalid; allowed: {ALLOWED_RARITIES:?}"
        )));
    }
    validate_conditions(&body.conditions)?;

    if crate::middleware::admin_destructive::is_admin_dry_run() {
        return Ok(Json(build_response(json!({
            "dry_run": true,
            "would_create": { "slug": body.slug, "output_type": body.output_type },
        }))));
    }

    let ui_meta = body.ui_metadata.clone().unwrap_or_else(|| json!({}));
    let admin_editable = body.admin_editable.unwrap_or(true);

    let row: (Uuid,) = sqlx::query_as(
        r#"
        INSERT INTO badge_rules
            (slug, output_type, output_variant, display_name, description,
             icon_key, conditions, rarity, admin_editable, ui_metadata, created_by)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
        RETURNING id
        "#,
    )
    .bind(&body.slug)
    .bind(&body.output_type)
    .bind(&body.output_variant)
    .bind(&body.display_name)
    .bind(body.description.clone().unwrap_or_default())
    .bind(&body.icon_key)
    .bind(&body.conditions)
    .bind(&rarity)
    .bind(admin_editable)
    .bind(&ui_meta)
    .bind(auth.user_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(ref db_err)
            if db_err.constraint() == Some("badge_rules_slug_key") =>
        {
            AppError::Validation(format!("slug '{}' already exists", body.slug))
        }
        _ => AppError::Database(e),
    })?;

    let rule = json!({
        "id": row.0, "slug": body.slug, "output_type": body.output_type,
        "output_variant": body.output_variant, "display_name": body.display_name,
        "description": body.description.unwrap_or_default(), "icon_key": body.icon_key,
        "conditions": body.conditions, "rarity": rarity,
        "admin_editable": admin_editable, "ui_metadata": ui_meta,
    });

    crate::services::audit::record(
        &state.db,
        crate::services::audit::AuditEntry {
            actor_type: crate::services::audit::ActorType::Admin,
            actor_id: Some(auth.user_id),
            action: "badge_rule.create",
            target_type: Some("badge_rule"),
            target_id: Some(row.0),
            metadata: Some(json!({ "after": rule })),
            headers: None,
        },
    )
    .await;

    Ok(Json(build_response(json!({ "rule": rule }))))
}

// ═══════════════════════════════════════════════════════════════════
// PATCH /admin/badge-rules/{slug}
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct PatchRuleBody {
    #[serde(default)]
    output_type: Option<String>,
    #[serde(default)]
    output_variant: Option<String>,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    icon_key: Option<String>,
    #[serde(default)]
    conditions: Option<Value>,
    #[serde(default)]
    rarity: Option<String>,
    #[serde(default)]
    admin_editable: Option<bool>,
    #[serde(default)]
    ui_metadata: Option<Value>,
}

async fn patch_rule(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Query(q): Query<DryRunQuery>,
    Json(body): Json<PatchRuleBody>,
) -> Result<Json<Value>, AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await?;
    crate::middleware::admin_destructive::enforce_admin_destructive(&state, auth.user_id).await?;

    // Fetch before pour vérifs invariants (admin_editable, deprecated_at).
    let before: Option<(Uuid, bool, Option<chrono::DateTime<chrono::Utc>>, Value)> =
        sqlx::query_as(
            "SELECT id, admin_editable, deprecated_at,
                jsonb_build_object(
                    'id', id, 'slug', slug, 'output_type', output_type,
                    'output_variant', output_variant, 'display_name', display_name,
                    'description', description, 'icon_key', icon_key,
                    'conditions', conditions, 'rarity', rarity,
                    'admin_editable', admin_editable, 'ui_metadata', ui_metadata,
                    'deprecated_at', deprecated_at
                ) AS row_json
         FROM badge_rules WHERE slug = $1",
        )
        .bind(&slug)
        .fetch_optional(&state.db)
        .await?;
    let (id, admin_editable, deprecated_at, before_json) =
        before.ok_or_else(|| AppError::NotFound(format!("badge_rule '{slug}' not found")))?;

    if !admin_editable {
        return Err(AppError::Validation(
            "badge_rule is admin_editable=false — core rule protected".into(),
        ));
    }
    if deprecated_at.is_some() {
        return Err(AppError::Validation(
            "badge_rule is deprecated — create a new slug instead".into(),
        ));
    }
    if let Some(ot) = body.output_type.as_ref() {
        if !ALLOWED_OUTPUT_TYPES.contains(&ot.as_str()) {
            return Err(AppError::Validation(format!(
                "output_type invalid; allowed: {ALLOWED_OUTPUT_TYPES:?}"
            )));
        }
    }
    if let Some(r) = body.rarity.as_ref() {
        if !ALLOWED_RARITIES.contains(&r.as_str()) {
            return Err(AppError::Validation(format!(
                "rarity invalid; allowed: {ALLOWED_RARITIES:?}"
            )));
        }
    }
    if let Some(c) = body.conditions.as_ref() {
        validate_conditions(c)?;
    }

    // Dry-run : preview + users_impacted_count.
    let users_impacted: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_badges WHERE rule_id = $1 AND revoked_at IS NULL",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    let dry = q.dry_run || crate::middleware::admin_destructive::is_admin_dry_run();
    if dry {
        return Ok(Json(build_response_with(
            json!({ "rule": before_json.clone() }),
            json!({
                "dry_run_preview": {
                    "before": before_json,
                    "patch": {
                        "output_type": body.output_type, "output_variant": body.output_variant,
                        "display_name": body.display_name, "description": body.description,
                        "icon_key": body.icon_key, "conditions": body.conditions,
                        "rarity": body.rarity, "admin_editable": body.admin_editable,
                        "ui_metadata": body.ui_metadata,
                    },
                    "users_impacted_count": users_impacted,
                }
            }),
        )));
    }

    // COALESCE via SQL pour patch partiel.
    sqlx::query(
        r#"
        UPDATE badge_rules SET
            output_type    = COALESCE($2, output_type),
            output_variant = COALESCE($3, output_variant),
            display_name   = COALESCE($4, display_name),
            description    = COALESCE($5, description),
            icon_key       = COALESCE($6, icon_key),
            conditions     = COALESCE($7, conditions),
            rarity         = COALESCE($8, rarity),
            admin_editable = COALESCE($9, admin_editable),
            ui_metadata    = COALESCE($10, ui_metadata),
            updated_at     = NOW()
        WHERE id = $1
        "#,
    )
    .bind(id)
    .bind(&body.output_type)
    .bind(&body.output_variant)
    .bind(&body.display_name)
    .bind(&body.description)
    .bind(&body.icon_key)
    .bind(&body.conditions)
    .bind(&body.rarity)
    .bind(body.admin_editable)
    .bind(&body.ui_metadata)
    .execute(&state.db)
    .await?;

    let after: Value = sqlx::query_scalar(
        "SELECT jsonb_build_object(
            'id', id, 'slug', slug, 'output_type', output_type,
            'output_variant', output_variant, 'display_name', display_name,
            'description', description, 'icon_key', icon_key,
            'conditions', conditions, 'rarity', rarity,
            'admin_editable', admin_editable, 'ui_metadata', ui_metadata
         ) FROM badge_rules WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await?;

    crate::services::audit::record(
        &state.db,
        crate::services::audit::AuditEntry {
            actor_type: crate::services::audit::ActorType::Admin,
            actor_id: Some(auth.user_id),
            action: "badge_rule.update",
            target_type: Some("badge_rule"),
            target_id: Some(id),
            metadata: Some(json!({"before": before_json, "after": after})),
            headers: None,
        },
    )
    .await;

    Ok(Json(build_response(json!({ "rule": after }))))
}

// ═══════════════════════════════════════════════════════════════════
// POST /admin/badge-rules/{slug}/deprecate
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct DeprecateBody {
    reason: String,
}

async fn deprecate_rule(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Query(q): Query<DryRunQuery>,
    Json(body): Json<DeprecateBody>,
) -> Result<Json<Value>, AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await?;
    crate::middleware::admin_destructive::enforce_admin_destructive(&state, auth.user_id).await?;

    if body.reason.trim().len() < 8 {
        return Err(AppError::Validation(
            "reason must be at least 8 chars".into(),
        ));
    }

    let row: Option<(Uuid, Option<chrono::DateTime<chrono::Utc>>)> =
        sqlx::query_as("SELECT id, deprecated_at FROM badge_rules WHERE slug = $1")
            .bind(&slug)
            .fetch_optional(&state.db)
            .await?;
    let (id, existing) =
        row.ok_or_else(|| AppError::NotFound(format!("badge_rule '{slug}' not found")))?;

    let users_with_badge: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_badges WHERE rule_id = $1 AND revoked_at IS NULL",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    let dry = q.dry_run || crate::middleware::admin_destructive::is_admin_dry_run();
    if dry {
        return Ok(Json(build_response_with(
            json!({ "deprecated": existing.is_some(), "slug": slug.clone() }),
            json!({ "dry_run_preview": { "users_with_badge_count": users_with_badge } }),
        )));
    }

    let deprecated_at: chrono::DateTime<chrono::Utc> = if let Some(ts) = existing {
        // Idempotent : ne réécrit pas le timestamp historique.
        ts
    } else {
        let (ts,): (chrono::DateTime<chrono::Utc>,) = sqlx::query_as(
            "UPDATE badge_rules SET deprecated_at = NOW(), updated_at = NOW()
             WHERE id = $1 RETURNING deprecated_at",
        )
        .bind(id)
        .fetch_one(&state.db)
        .await?;
        ts
    };

    crate::services::audit::record(
        &state.db,
        crate::services::audit::AuditEntry {
            actor_type: crate::services::audit::ActorType::Admin,
            actor_id: Some(auth.user_id),
            action: "badge_rule.deprecate",
            target_type: Some("badge_rule"),
            target_id: Some(id),
            metadata: Some(json!({
                "reason": body.reason,
                "users_with_badge_count": users_with_badge,
            })),
            headers: None,
        },
    )
    .await;

    Ok(Json(build_response(json!({
        "deprecated": true,
        "slug": slug,
        "deprecated_at": deprecated_at.to_rfc3339(),
    }))))
}
