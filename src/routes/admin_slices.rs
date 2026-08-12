//! P26 v2 SKI-106 — admin override of per-slice gates.
//!
//! Unblocks the "admin can force-open or force-restrict a specific
//! challenge without touching the code" workflow. Corresponds to
//! Part 2 of the front ticket SKI-98.
//!
//! Endpoint : `PATCH /api/admin/slices/{id}/config`
//!
//! Body (all fields optional; explicit null clears the override) :
//! ```json
//! {
//!   "required_orientation_slugs": ["frontend-svelte"] | null,
//!   "min_rank": "artisan" | null,
//!   "note": "raison écrite dans l'audit log"
//! }
//! ```
//!
//! Rationale for having this as an admin-only surface : the SKI-78 and
//! SKI-79 gates are the primary access control; letting non-admins
//! bypass would defeat them. But an admin sometimes needs to open a
//! `min_rank='doyen'` slice to a specific ranger for pedagogical
//! reasons — this endpoint is the escape hatch, with audit trail.

use axum::extract::{Path, Query, State};
use axum::routing::{get, patch};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::models::ProjectSlice;

pub fn admin_slice_routes() -> Router<AppState> {
    Router::new()
        .route("/admin/slices/{id}/config", patch(patch_slice_config))
        // SKI-112 (M-06) — admin can list slices in any status.
        .route("/admin/slices", get(list_slices))
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

/// Distinguishes "field absent from JSON" (no change) from "field is
/// null" (clear the override). `Option<Option<T>>` with
/// `#[serde(default, deserialize_with = "double_option")]` is the
/// canonical way — but we settle for a simpler pattern: an explicit
/// wrapper enum. Rust's serde is not as ergonomic here as it looks.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SliceConfigBody {
    #[serde(default, deserialize_with = "deserialize_double_option")]
    required_orientation_slugs: Option<Option<Vec<String>>>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    min_rank: Option<Option<String>>,
    /// Free-form note for the audit trail. Not stored on the slice.
    #[serde(default)]
    note: Option<String>,
}

/// Serde helper : maps missing field → `None`, JSON `null` → `Some(None)`,
/// value → `Some(Some(v))`. Standard trick for PATCH semantics.
fn deserialize_double_option<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: serde::Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    Ok(Some(Option::<T>::deserialize(deserializer)?))
}

const VALID_RANKS: &[&str] = &["apprenti", "ranger", "artisan", "maitre", "doyen"];

fn validate_min_rank(rank: &Option<String>) -> Result<(), AppError> {
    if let Some(r) = rank
        && !VALID_RANKS.contains(&r.as_str())
    {
        return Err(AppError::Validation(format!(
            "min_rank must be one of {VALID_RANKS:?} or null"
        )));
    }
    Ok(())
}

fn validate_orientation_slugs(slugs: &Option<Vec<String>>) -> Result<(), AppError> {
    let Some(list) = slugs else {
        return Ok(());
    };
    for s in list {
        let len = s.chars().count();
        if !(3..=60).contains(&len)
            || !s
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return Err(AppError::Validation(format!(
                "orientation slug '{s}' invalid: 3-60 chars, lowercase alnum + dashes"
            )));
        }
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════
// SKI-111 — response schemas
// ═══════════════════════════════════════════════════════════════════

/// Payload of `PATCH /admin/slices/{id}/config`.
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct SliceConfigData {
    pub slice: crate::models::ProjectSlice,
}

/// Response of `GET /admin/slices`.
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct AdminSliceListResponse {
    pub data: Vec<crate::models::ProjectSlice>,
    pub pagination: crate::api_response::Pagination,
    pub meta: crate::api_response::MetaInfo,
}

/// SKI-106 admin — override sensibilité/rank sur une slice individuelle.
#[utoipa::path(
    patch, path = "/api/admin/slices/{id}/config", tag = "admin",
    params(("id" = Uuid, Path)),
    request_body(content = serde_json::Value),
    responses(
        (status = 200, description = "slice config updated", body = crate::api_response::ApiResponse<SliceConfigData>),
        (status = 400, body = crate::api_response::ErrorResponse),
        (status = 404, body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn patch_slice_config(
    _gate: crate::middleware::admin_gate::AdminGate,
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<SliceConfigBody>,
) -> Result<Json<Value>, AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await?;

    // Validate what's present. `Some(None)` (explicit clear) is fine —
    // we only validate the actual values that would land in the DB.
    if let Some(Some(rank_val)) = &body.min_rank {
        validate_min_rank(&Some(rank_val.clone()))?;
    }
    if let Some(Some(slugs_val)) = &body.required_orientation_slugs {
        validate_orientation_slugs(&Some(slugs_val.clone()))?;
    }

    // Build the UPDATE dynamically-ish: bind the new value or the
    // existing column value (COALESCE-like) depending on whether the
    // caller intended a change vs a no-op vs a clear.
    // We use two params per column: (set_it_bool, new_value) so the
    // SQL stays a single UPDATE without branching.
    let (set_slugs, slugs_val): (bool, Option<Vec<String>>) = match body.required_orientation_slugs
    {
        None => (false, None),                  // field absent → no change
        Some(None) => (true, Some(Vec::new())), // explicit null → clear (empty array = "no restriction")
        Some(Some(v)) => (true, Some(v)),
    };
    let (set_rank, rank_val): (bool, Option<String>) = match body.min_rank {
        None => (false, None),
        Some(None) => (true, None), // explicit null → clear (NULL = no floor)
        Some(Some(v)) => (true, Some(v)),
    };

    let slice = sqlx::query_as::<_, ProjectSlice>(
        r#"
        UPDATE project_slices SET
            required_orientation_slugs = CASE WHEN $2 THEN COALESCE($3, '{}'::text[]) ELSE required_orientation_slugs END,
            min_rank = CASE WHEN $4 THEN $5 ELSE min_rank END,
            updated_at = NOW()
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(id)
    .bind(set_slugs)
    .bind(&slugs_val)
    .bind(set_rank)
    .bind(&rank_val)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("slice {id} not found")))?;

    crate::services::audit::record(
        &state.db,
        crate::services::audit::AuditEntry {
            actor_type: crate::services::audit::ActorType::Admin,
            actor_id: Some(auth.user_id),
            action: "slice.config.override",
            target_type: Some("project_slice"),
            target_id: Some(id),
            metadata: Some(json!({
                "note": body.note,
                "set_orientation": set_slugs,
                "new_orientation": slugs_val,
                "set_rank": set_rank,
                "new_rank": rank_val,
            })),
            headers: None,
        },
    )
    .await;

    Ok(Json(wrap(json!({ "slice": slice }))))
}

// ═══════════════════════════════════════════════════════════════════
// GET /admin/slices — SKI-112 (M-06)
// ═══════════════════════════════════════════════════════════════════
//
// The public GET /api/slices only returns status='open'. Everything
// else (claimed, in_progress, submitted, pending_validation, validated,
// merged...) is unreachable from the admin panel without knowing the
// UUID — which is exactly the state where overrides are most needed.
// This endpoint lifts the implicit filter and adds admin-oriented
// filters (multi-status CSV, claimant, free-text search on title /
// external_ref).

#[derive(Debug, Deserialize)]
pub struct AdminSlicesQuery {
    #[serde(default)]
    project_id: Option<Uuid>,
    /// CSV of statuses, e.g. `?status=pending_validation,submitted`.
    /// Empty / absent = all statuses.
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    domain: Option<String>,
    #[serde(default)]
    claimed_by_user_id: Option<Uuid>,
    /// Free-text ILIKE match on `title` OR `external_ref`.
    #[serde(default)]
    q: Option<String>,
    #[serde(default)]
    page: Option<i64>,
    #[serde(default)]
    per_page: Option<i64>,
}

const VALID_STATUSES: &[&str] = &[
    "draft",
    "open",
    "claimed",
    "in_review",
    "in_progress",
    "submitted",
    "ci_green",
    "pending_validation",
    "validated",
    "merged",
    "closed",
    "expired",
];

fn parse_status_csv(raw: Option<&str>) -> Result<Option<Vec<String>>, AppError> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let parts: Vec<String> = raw
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    if parts.is_empty() {
        return Ok(None);
    }
    for p in &parts {
        if !VALID_STATUSES.contains(&p.as_str()) {
            return Err(AppError::Validation(format!(
                "status '{p}' invalid: must be one of {VALID_STATUSES:?}"
            )));
        }
    }
    Ok(Some(parts))
}

fn validate_q(q: &Option<String>) -> Result<Option<String>, AppError> {
    let Some(q) = q else { return Ok(None) };
    let trimmed = q.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.len() > 200 {
        return Err(AppError::Validation("q must be <= 200 chars".into()));
    }
    Ok(Some(trimmed.to_string()))
}

/// SKI-112 admin — list slices across every status, with filters.
#[utoipa::path(
    get, path = "/api/admin/slices", tag = "admin",
    params(
        ("project_id" = Option<Uuid>, Query),
        ("status" = Option<String>, Query, description = "CSV of statuses"),
        ("domain" = Option<String>, Query),
        ("claimed_by_user_id" = Option<Uuid>, Query),
        ("q" = Option<String>, Query, description = "ILIKE match on title or external_ref"),
        ("page" = Option<i64>, Query),
        ("per_page" = Option<i64>, Query),
    ),
    responses(
        (status = 200, description = "paginated slice list", body = AdminSliceListResponse),
        (status = 400, body = crate::api_response::ErrorResponse),
        (status = 403, body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn list_slices(
    _gate: crate::middleware::admin_gate::AdminGate,
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<AdminSlicesQuery>,
) -> Result<Json<Value>, AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await?;

    let statuses = parse_status_csv(q.status.as_deref())?;
    let search = validate_q(&q.q)?;
    let per_page = q.per_page.unwrap_or(25).clamp(1, 100);
    let page = q.page.unwrap_or(1).max(1);
    let offset = (page - 1) * per_page;

    // Trigrams / prefix ILIKE — pattern wrapped in the caller so we
    // bind a single string rather than concat in SQL.
    let search_pat = search.as_ref().map(|s| format!("%{s}%"));

    let slices = sqlx::query_as::<_, ProjectSlice>(
        r#"
        SELECT * FROM project_slices
        WHERE ($1::uuid       IS NULL OR project_id = $1)
          AND ($2::text[]     IS NULL OR status = ANY($2))
          AND ($3::text       IS NULL OR primary_domain = $3)
          AND ($4::uuid       IS NULL OR claimed_by_user_id = $4)
          AND ($5::text       IS NULL OR title ILIKE $5 OR external_ref ILIKE $5)
        ORDER BY updated_at DESC
        LIMIT $6 OFFSET $7
        "#,
    )
    .bind(q.project_id)
    .bind(statuses.as_deref())
    .bind(q.domain.as_deref())
    .bind(q.claimed_by_user_id)
    .bind(search_pat.as_deref())
    .bind(per_page)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let total: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM project_slices
        WHERE ($1::uuid       IS NULL OR project_id = $1)
          AND ($2::text[]     IS NULL OR status = ANY($2))
          AND ($3::text       IS NULL OR primary_domain = $3)
          AND ($4::uuid       IS NULL OR claimed_by_user_id = $4)
          AND ($5::text       IS NULL OR title ILIKE $5 OR external_ref ILIKE $5)
        "#,
    )
    .bind(q.project_id)
    .bind(statuses.as_deref())
    .bind(q.domain.as_deref())
    .bind(q.claimed_by_user_id)
    .bind(search_pat.as_deref())
    .fetch_one(&state.db)
    .await?;

    let total_pages = ((total as f64) / (per_page as f64)).ceil() as i64;

    Ok(Json(json!({
        "data": slices,
        "pagination": {
            "page": page,
            "per_page": per_page,
            "total": total,
            "total_pages": total_pages,
        },
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        },
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rank_accepts_valid_or_null() {
        assert!(validate_min_rank(&None).is_ok());
        for r in ["apprenti", "ranger", "artisan", "maitre", "doyen"] {
            assert!(validate_min_rank(&Some(r.into())).is_ok());
        }
        assert!(validate_min_rank(&Some("god-mode".into())).is_err());
    }

    #[test]
    fn slug_shape_validation() {
        assert!(validate_orientation_slugs(&None).is_ok());
        assert!(
            validate_orientation_slugs(&Some(vec!["front-svelte".into(), "ai-ml".into()])).is_ok()
        );
        assert!(validate_orientation_slugs(&Some(vec!["ab".into()])).is_err()); // too short
        assert!(validate_orientation_slugs(&Some(vec!["Front-Svelte".into()])).is_err()); // uppercase
        assert!(validate_orientation_slugs(&Some(vec!["front_svelte".into()])).is_err()); // underscore
    }

    #[test]
    fn parse_status_csv_ok() {
        assert!(matches!(parse_status_csv(None), Ok(None)));
        assert!(matches!(parse_status_csv(Some("")), Ok(None)));
        let v = parse_status_csv(Some("open,pending_validation, validated")).unwrap();
        assert_eq!(
            v.unwrap(),
            vec![
                "open".to_string(),
                "pending_validation".to_string(),
                "validated".to_string()
            ]
        );
    }

    #[test]
    fn parse_status_csv_rejects_unknown() {
        assert!(parse_status_csv(Some("open,definitely-not-a-status")).is_err());
    }

    #[test]
    fn validate_q_normalizes() {
        assert!(validate_q(&None).unwrap().is_none());
        assert!(validate_q(&Some("   ".into())).unwrap().is_none());
        assert_eq!(validate_q(&Some(" hi ".into())).unwrap().unwrap(), "hi");
        assert!(validate_q(&Some("x".repeat(201))).is_err());
    }

    #[test]
    fn double_option_serde_roundtrip() {
        // Sanity: field absent, field null, field with value — three distinct outcomes.
        let absent: SliceConfigBody = serde_json::from_str(r#"{}"#).unwrap();
        assert!(absent.min_rank.is_none()); // Option::None (no change)
        assert!(absent.required_orientation_slugs.is_none());

        let null: SliceConfigBody = serde_json::from_str(r#"{"min_rank": null}"#).unwrap();
        assert!(matches!(null.min_rank, Some(None))); // explicit null (clear)

        let value: SliceConfigBody = serde_json::from_str(r#"{"min_rank": "artisan"}"#).unwrap();
        assert!(matches!(value.min_rank, Some(Some(_))));
    }
}
