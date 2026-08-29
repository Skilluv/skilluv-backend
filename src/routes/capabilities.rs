//! P18.4 — API capabilities.
//!
//! Endpoints :
//!   - `GET /api/users/{id}/capabilities`         (public : capabilities actives)
//!   - `GET /api/users/me/capabilities`            (auth : profil user courant)
//!   - `POST /api/admin/users/{id}/capabilities`  (require admin capability)
//!   - `DELETE /api/admin/users/{id}/capabilities/{cap}` (revoke)
//!   - `GET /api/admin/capabilities`               (the catalogue both validate against)

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
        // The catalogue the two routes above validate against. Beside them
        // deliberately: a grant endpoint whose vocabulary nothing serves is
        // how the admin panel ended up holding a stale copy.
        .route("/admin/capabilities", get(admin_capability_catalogue))
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
    #[schema(max_length = 10000)]
    pub capability: String,
    /// Free-text audit reason. Defaults to `admin_grant:by_<uuid>`.
    #[serde(default)]
    #[schema(max_length = 10000)]
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
    headers: axum::http::HeaderMap,
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

    // SKI-299 — granting a capability is how someone becomes able to ban,
    // revoke or moderate. `granted_by` sits on the row, but the row is
    // mutable and the journal is not.
    crate::services::audit::record(
        &state.db,
        crate::services::audit::AuditEntry {
            actor_type: crate::services::audit::ActorType::Admin,
            actor_id: Some(auth.user_id),
            action: "capability.grant",
            target_type: Some("user"),
            target_id: Some(target_id),
            metadata: Some(serde_json::json!({
                "capability": body.capability,
                "reason": reason,
                "expires_at": body.expires_at.map(|d| d.to_rfc3339()),
            })),
            headers: Some(&headers),
        },
    )
    .await;

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
    headers: axum::http::HeaderMap,
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
    crate::services::audit::record(
        &state.db,
        crate::services::audit::AuditEntry {
            actor_type: crate::services::audit::ActorType::Admin,
            actor_id: Some(auth.user_id),
            action: "capability.revoke",
            target_type: Some("user"),
            target_id: Some(target_id),
            metadata: Some(serde_json::json!({ "capability": cap })),
            headers: Some(&headers),
        },
    )
    .await;

    Ok(Json(ApiResponse::new(CapabilityRevokeResponse {
        revoked: true,
        user_id: target_id,
        capability: cap,
    })))
}

// ═══════════════════════════════════════════════════════════════════
// The catalogue itself
// ═══════════════════════════════════════════════════════════════════

/// The capabilities the engine grants and keeps on its own.
///
/// Copied from the `grant_if_missing` calls in `services::capabilities_engine`,
/// and the only part of this response that is not read out of the database.
/// It is here rather than in a column because it is a fact about the engine's
/// code, and a column would be a second place to keep it in step.
///
/// What it buys an operator: revoking one of these does not stick. The engine
/// puts it back on the next recompute, and somebody who does not know that
/// spends an afternoon wondering why.
const ENGINE_MANAGED: &[&str] = &[
    "challenger",
    "community_curator",
    "community_moderator",
    "forum_moderator",
    "issue_proposer",
    "mentor",
    "pr_reviewer",
    "project_steward",
    "verified_apprentice",
];

#[derive(Debug, Serialize, ToSchema)]
pub struct CatalogueEntry {
    /// What goes in `POST /admin/users/{id}/capabilities`.
    pub capability: String,
    /// The part before the colon.
    pub family: String,
    /// The part after it, absent when there is none.
    pub scope: Option<String>,
    /// What holding it lets somebody do. The reason this endpoint exists: an
    /// operator choosing between `domain_curator:design` and
    /// `community_curator` with nothing but the slugs picks the wider one.
    pub description: String,
    /// True when the orientations trigger of migration 0404 maintains the row.
    /// Those appear in no migration and change when a trade is added or moves
    /// family — which is why a client cannot hold this list as a constant.
    pub is_derived: bool,
    /// True when `services::capabilities_engine` grants and re-grants it.
    /// Still grantable by hand; revoking it is what does not stick.
    pub engine_managed: bool,
    /// How many people hold it right now — not revoked, not expired. An
    /// operator about to grant `security_reviewer:red-team` wants to know
    /// whether anybody already reviews red team work.
    pub held_by: i64,
}

/// Every capability that can be granted, as the database has them.
///
/// ## Why this could not be a list in the client
///
/// Part of the catalogue is generated. Migration 0404 replaced the CHECK that
/// five migrations had restated with a table, and put a trigger on
/// `orientations` behind it: adding a trade with a review family makes
/// `{domain}_reviewer:{family}` grantable in the same statement, and no
/// migration has to remember. So the set is a function of the trade catalogue,
/// and any copy of it is correct until somebody adds an orientation — then
/// wrong, and wrong silently.
///
/// The admin panel held such a copy, anchored to a CHECK that no longer
/// exists, which is why `domain_curator:design`, `mission_arbiter` and
/// `security_triager` could not be granted at all: they gate three surfaces
/// shipped this week and nothing could hand them to anybody (SKI-351).
///
/// ## On granting something that is not in here
///
/// You cannot. `user_capabilities.capability` is a foreign key to this table
/// since 0404, so an invented string is refused by the database rather than
/// stored and silently never matched. That was worth checking rather than
/// assuming — it is the difference between a stale list and an open door.
#[utoipa::path(
    get, path = "/api/admin/capabilities",
    operation_id = "adminCapabilityCatalogue", tag = "admin",
    responses(
        (status = 200, body = ApiResponse<Vec<CatalogueEntry>>),
        (status = 403, description = "Not an admin", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn admin_capability_catalogue(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<Vec<CatalogueEntry>>>, AppError> {
    require_capability(&state.db, auth.user_id, "admin").await?;

    let rows: Vec<(String, String, Option<String>, String, bool, i64)> = sqlx::query_as(
        "SELECT c.capability, c.family, c.scope, c.description, c.is_derived,
                (SELECT count(*) FROM user_capabilities u
                  WHERE u.capability = c.capability
                    AND u.revoked_at IS NULL
                    AND (u.expires_at IS NULL OR u.expires_at > NOW())) AS held_by
           FROM capability_catalog c
          -- Family first, then scope, so the reviewer families of one domain
          -- arrive together: that is how somebody reads a list of forty.
          ORDER BY c.family, c.scope NULLS FIRST",
    )
    .fetch_all(&state.db)
    .await?;

    let catalogue = rows
        .into_iter()
        .map(
            |(capability, family, scope, description, is_derived, held_by)| CatalogueEntry {
                engine_managed: ENGINE_MANAGED.contains(&capability.as_str()),
                capability,
                family,
                scope,
                description,
                is_derived,
                held_by,
            },
        )
        .collect();

    Ok(Json(ApiResponse::new(catalogue)))
}
