//! P18.4 — API capabilities.
//!
//! Endpoints :
//!   - `GET /api/users/{id}/capabilities`         (public : capabilities actives)
//!   - `GET /api/users/me/capabilities`            (auth : profil user courant)
//!   - `POST /api/admin/users/{id}/capabilities`  (require admin capability)
//!   - `DELETE /api/admin/users/{id}/capabilities/{cap}` (revoke)

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::middleware::capabilities::require_capability;

pub fn capability_routes() -> Router<AppState> {
    Router::new()
        .route("/users/{id}/capabilities", get(user_capabilities_public))
        .route("/users/me/capabilities", get(my_capabilities))
        .route(
            "/admin/users/{id}/capabilities",
            post(admin_grant_capability),
        )
        .route(
            "/admin/users/{id}/capabilities/{cap}",
            delete(admin_revoke_capability),
        )
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

#[derive(Debug, Serialize, sqlx::FromRow)]
struct CapabilityRow {
    capability: String,
    granted_at: chrono::DateTime<chrono::Utc>,
    granted_reason: String,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

async fn fetch_active(db: &sqlx::PgPool, user_id: Uuid) -> Result<Vec<CapabilityRow>, AppError> {
    Ok(sqlx::query_as::<_, CapabilityRow>(
        r#"
        SELECT capability, granted_at, granted_reason, expires_at
        FROM user_capabilities
        WHERE user_id = $1
          AND revoked_at IS NULL
          AND (expires_at IS NULL OR expires_at > NOW())
        ORDER BY capability
        "#,
    )
    .bind(user_id)
    .fetch_all(db)
    .await?)
}

async fn user_capabilities_public(
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let rows = fetch_active(&state.db, user_id).await?;
    Ok(Json(wrap(
        json!({ "user_id": user_id, "capabilities": rows }),
    )))
}

async fn my_capabilities(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let rows = fetch_active(&state.db, auth.user_id).await?;
    Ok(Json(wrap(
        json!({ "user_id": auth.user_id, "capabilities": rows }),
    )))
}

#[derive(Debug, Deserialize)]
struct GrantBody {
    capability: String,
    #[serde(default)]
    granted_reason: Option<String>,
    #[serde(default)]
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

async fn admin_grant_capability(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(target_id): Path<Uuid>,
    Json(body): Json<GrantBody>,
) -> Result<impl IntoResponse, AppError> {
    require_capability(&state.db, auth.user_id, "admin").await?;

    let reason = body
        .granted_reason
        .unwrap_or_else(|| format!("admin_grant:by_{}", auth.user_id));

    sqlx::query(
        r#"
        INSERT INTO user_capabilities
            (user_id, capability, granted_reason, granted_by, expires_at)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(target_id)
    .bind(&body.capability)
    .bind(&reason)
    .bind(auth.user_id)
    .bind(body.expires_at)
    .execute(&state.db)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(wrap(json!({
            "granted": true,
            "user_id": target_id,
            "capability": body.capability,
        }))),
    ))
}

async fn admin_revoke_capability(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((target_id, cap)): Path<(Uuid, String)>,
) -> Result<Json<Value>, AppError> {
    require_capability(&state.db, auth.user_id, "admin").await?;
    let res = sqlx::query(
        r#"
        UPDATE user_capabilities
        SET revoked_at = NOW(),
            revoked_reason = COALESCE(revoked_reason, 'admin_revoke:by_' || $3::TEXT)
        WHERE user_id = $1 AND capability = $2 AND revoked_at IS NULL
        "#,
    )
    .bind(target_id)
    .bind(&cap)
    .bind(auth.user_id.to_string())
    .execute(&state.db)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound(format!(
            "active capability '{cap}' not found on user {target_id}"
        )));
    }
    Ok(Json(wrap(json!({
        "revoked": true,
        "user_id": target_id,
        "capability": cap,
    }))))
}
