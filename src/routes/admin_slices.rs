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

use axum::extract::{Path, State};
use axum::routing::patch;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::models::ProjectSlice;

pub fn admin_slice_routes() -> Router<AppState> {
    Router::new().route("/admin/slices/{id}/config", patch(patch_slice_config))
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

/// SKI-106 admin — override sensibilité/rank sur une slice individuelle.
#[utoipa::path(
    patch, path = "/api/admin/slices/{id}/config", tag = "admin",
    params(("id" = Uuid, Path)),
    request_body(content = serde_json::Value),
    responses(
        (status = 200, description = "slice config updated", body = serde_json::Value),
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
