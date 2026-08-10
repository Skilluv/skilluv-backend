//! Hygiène pré-prod SKI-33 — admin CRUD for DB-backed feature flags.
//!
//! Routes:
//!   GET    /api/admin/feature-flags       list all
//!   POST   /api/admin/feature-flags       upsert one (create or update)
//!   DELETE /api/admin/feature-flags/{key} delete one

use axum::extract::{Path, State};
use axum::routing::{delete, get};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::feature_flags;

pub fn admin_feature_flag_routes() -> Router<AppState> {
    Router::new()
        .route("/admin/feature-flags", get(list).post(upsert))
        .route("/admin/feature-flags/{key}", delete(remove))
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

/// SKI-33 admin — list all feature flags.
#[utoipa::path(
    get, path = "/api/admin/feature-flags", tag = "admin",
    responses(
        (status = 200, description = "list feature flags", body = serde_json::Value),
        (status = 403, body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn list(
    _gate: crate::middleware::admin_gate::AdminGate,
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await?;
    let flags = feature_flags::list_flags(&state.db).await?;
    Ok(Json(wrap(json!({ "flags": flags }))))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpsertBody {
    pub key: String,
    pub enabled: bool,
    #[serde(default = "default_rollout")]
    pub rollout_percent: i16,
    #[serde(default)]
    pub description: Option<String>,
}

fn default_rollout() -> i16 {
    100
}

/// SKI-33 admin — create or update a flag (idempotent upsert).
#[utoipa::path(
    post, path = "/api/admin/feature-flags", tag = "admin",
    request_body(content = serde_json::Value),
    responses(
        (status = 200, description = "flag upserted", body = serde_json::Value),
        (status = 400, body = crate::api_response::ErrorResponse),
        (status = 403, body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn upsert(
    _gate: crate::middleware::admin_gate::AdminGate,
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<UpsertBody>,
) -> Result<Json<Value>, AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await?;
    let flag = feature_flags::upsert_flag(
        &state.db,
        &body.key,
        body.enabled,
        body.rollout_percent,
        body.description.as_deref(),
        auth.user_id,
    )
    .await?;
    Ok(Json(wrap(json!({ "flag": flag }))))
}

/// SKI-33 admin — delete a flag.
#[utoipa::path(
    delete, path = "/api/admin/feature-flags/{key}", tag = "admin",
    params(("key" = String, Path)),
    responses(
        (status = 200, description = "flag removed", body = serde_json::Value),
        (status = 403, body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn remove(
    _gate: crate::middleware::admin_gate::AdminGate,
    State(state): State<AppState>,
    auth: AuthUser,
    Path(key): Path<String>,
) -> Result<Json<Value>, AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await?;
    let removed = feature_flags::delete_flag(&state.db, &key).await?;
    Ok(Json(wrap(json!({ "removed": removed, "key": key }))))
}
