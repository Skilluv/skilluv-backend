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
use utoipa::ToSchema;
use uuid::Uuid;

use crate::AppState;
use crate::api_response::ApiResponse;
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

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct CapabilityRow {
    /// Enum value from `user_capabilities` (`admin`, `forum_mod`,
    /// `plagiarism_reviewer`, `kyc_reviewer`, `community_moderator`,
    /// `community_curator`, `mentor`, `super_admin`, `steward`).
    pub capability: String,
    pub granted_at: chrono::DateTime<chrono::Utc>,
    pub granted_reason: String,
    /// `None` for permanent grants; otherwise the auto-expiry deadline.
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserCapabilitiesResponse {
    pub user_id: Uuid,
    pub capabilities: Vec<CapabilityRow>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CapabilityGrantResponse {
    pub granted: bool,
    pub user_id: Uuid,
    pub capability: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CapabilityRevokeResponse {
    pub revoked: bool,
    pub user_id: Uuid,
    pub capability: String,
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

/// Public: list every active capability granted to a user. Used by
/// front to render moderator/mentor badges next to the display name.
#[utoipa::path(
    get,
    path = "/api/users/{id}/capabilities",
    tag = "profile",
    params(("id" = Uuid, Path, description = "User UUID")),
    responses(
        (status = 200, description = "Active capabilities", body = ApiResponse<UserCapabilitiesResponse>),
    ),
)]
pub async fn user_capabilities_public(
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
) -> Result<Json<ApiResponse<UserCapabilitiesResponse>>, AppError> {
    let rows = fetch_active(&state.db, user_id).await?;
    Ok(Json(ApiResponse::new(UserCapabilitiesResponse {
        user_id,
        capabilities: rows,
    })))
}

/// Authenticated: the caller's own capabilities. Used by admin/mod
/// panels to gate UI without hitting the public endpoint (avoids
/// leaking the current user's ID in the URL).
#[utoipa::path(
    get,
    path = "/api/users/me/capabilities",
    tag = "profile",
    responses(
        (status = 200, description = "Caller's capabilities", body = ApiResponse<UserCapabilitiesResponse>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn my_capabilities(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<UserCapabilitiesResponse>>, AppError> {
    let rows = fetch_active(&state.db, auth.user_id).await?;
    Ok(Json(ApiResponse::new(UserCapabilitiesResponse {
        user_id: auth.user_id,
        capabilities: rows,
    })))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct GrantBody {
    /// Capability to grant (see `CapabilityRow.capability` for the enum).
    pub capability: String,
    /// Free-text audit reason. Defaults to `admin_grant:by_<uuid>`.
    #[serde(default)]
    pub granted_reason: Option<String>,
    /// Auto-expiry; `None` = permanent.
    #[serde(default)]
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Admin only: grant a capability to a user. Idempotent (ON CONFLICT
/// DO NOTHING). Requires the caller to hold the `admin` capability.
#[utoipa::path(
    post,
    path = "/api/admin/users/{id}/capabilities",
    tag = "admin",
    params(("id" = Uuid, Path, description = "Target user UUID")),
    request_body = GrantBody,
    responses(
        (status = 201, description = "Capability granted (or already present)", body = ApiResponse<CapabilityGrantResponse>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Caller lacks 'admin' capability", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn admin_grant_capability(
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
        Json(ApiResponse::new(CapabilityGrantResponse {
            granted: true,
            user_id: target_id,
            capability: body.capability,
        })),
    ))
}

/// Admin only: revoke an active capability. Sets `revoked_at` and a
/// stamped `revoked_reason`. 404 if the capability isn't currently
/// active on the target.
#[utoipa::path(
    delete,
    path = "/api/admin/users/{id}/capabilities/{cap}",
    tag = "admin",
    params(
        ("id" = Uuid, Path, description = "Target user UUID"),
        ("cap" = String, Path, description = "Capability slug to revoke"),
    ),
    responses(
        (status = 200, description = "Capability revoked", body = ApiResponse<CapabilityRevokeResponse>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Caller lacks 'admin' capability", body = crate::api_response::ErrorResponse),
        (status = 404, description = "No active capability of that slug on the target", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn admin_revoke_capability(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((target_id, cap)): Path<(Uuid, String)>,
) -> Result<Json<ApiResponse<CapabilityRevokeResponse>>, AppError> {
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
    Ok(Json(ApiResponse::new(CapabilityRevokeResponse {
        revoked: true,
        user_id: target_id,
        capability: cap,
    })))
}
