//! Web Push subscription endpoints — Phase 4.12.
//!
//! Backend part only. Actual VAPID delivery uses the `web-push` protocol ; a full
//! push-sending helper is registered but sending itself is deferred to when the
//! `web-push` crate is added (or a homemade JWT/ECDH sender is implemented).

use std::str::FromStr;

use axum::extract::{Path, State};
use axum::http::HeaderMap;
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

pub fn push_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/notifications/push/vapid-public-key",
            get(vapid_public_key),
        )
        .route("/notifications/push/subscribe", post(subscribe))
        .route("/notifications/push/{id}", delete(unsubscribe))
        .route("/manifest.webmanifest", get(pwa_manifest))
        // P15.1 — mobile push tokens (FCM + APNS)
        .route(
            "/users/me/push-tokens/register",
            post(register_mobile_token),
        )
        .route(
            "/users/me/push-tokens/{device_id}",
            delete(revoke_mobile_token),
        )
        .route("/users/me/push-tokens", get(list_mobile_tokens))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct VapidPublicKeyResponse {
    /// Base64URL-encoded VAPID public key. Front hands it to
    /// `PushManager.subscribe`.
    pub public_key: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SubscribeBody {
    /// Endpoint URL returned by `PushManager.subscribe`.
    #[schema(max_length = 10000)]
    pub endpoint: String,
    /// P-256 ECDH public key (base64url).
    #[schema(max_length = 10000)]
    pub p256dh: String,
    /// Auth secret (base64url) for AES-GCM encryption.
    #[schema(max_length = 10000)]
    pub auth: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SubscribeResponse {
    pub subscription_id: Uuid,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UnsubscribeResponse {
    pub removed: bool,
}

/// Return the VAPID public key that the browser needs to subscribe to
/// push. 500 when the deployer forgot to set `VAPID_PUBLIC_KEY`.
#[utoipa::path(
    get,
    path = "/api/notifications/push/vapid-public-key",
    tag = "profile",
    responses(
        (status = 200, description = "VAPID public key", body = ApiResponse<VapidPublicKeyResponse>),
        (status = 500, description = "VAPID_PUBLIC_KEY env var missing", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn vapid_public_key() -> Result<Json<ApiResponse<VapidPublicKeyResponse>>, AppError> {
    let key = std::env::var("VAPID_PUBLIC_KEY")
        .ok()
        .filter(|s| !s.is_empty())
        .ok_or(AppError::Internal("VAPID_PUBLIC_KEY not set".into()))?;
    Ok(Json(ApiResponse::new(VapidPublicKeyResponse {
        public_key: key,
    })))
}

/// Subscribe the current session to Web Push. Idempotent via
/// `(user_id, endpoint)` — re-subscribing the same endpoint updates
/// the keys and resets the failure counter.
#[utoipa::path(
    post,
    path = "/api/notifications/push/subscribe",
    tag = "profile",
    request_body = SubscribeBody,
    responses(
        (status = 200, description = "Subscribed", body = ApiResponse<SubscribeResponse>),
        (status = 400, description = "Missing endpoint / p256dh / auth", body = crate::api_response::ErrorResponse),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn subscribe(
    State(state): State<AppState>,
    auth_user: AuthUser,
    headers: HeaderMap,
    Json(body): Json<SubscribeBody>,
) -> Result<Json<ApiResponse<SubscribeResponse>>, AppError> {
    if body.endpoint.is_empty() || body.p256dh.is_empty() || body.auth.is_empty() {
        return Err(AppError::Validation(
            "missing endpoint / p256dh / auth".into(),
        ));
    }
    let ua = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .map(String::from);
    let row: (Uuid,) = sqlx::query_as(
        r#"
        INSERT INTO push_subscriptions (user_id, endpoint, p256dh_key, auth_secret, user_agent)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (user_id, endpoint) DO UPDATE SET
            p256dh_key = EXCLUDED.p256dh_key,
            auth_secret = EXCLUDED.auth_secret,
            user_agent = EXCLUDED.user_agent,
            failure_count = 0,
            last_failure_at = NULL
        RETURNING id
        "#,
    )
    .bind(auth_user.user_id)
    .bind(&body.endpoint)
    .bind(&body.p256dh)
    .bind(&body.auth)
    .bind(&ua)
    .fetch_one(&state.db)
    .await?;
    metrics::counter!("skilluv_push_subscriptions_total").increment(1);
    Ok(Json(ApiResponse::new(SubscribeResponse {
        subscription_id: row.0,
    })))
}

/// Delete one of the caller's push subscriptions.
#[utoipa::path(
    delete,
    path = "/api/notifications/push/{id}",
    tag = "profile",
    params(("id" = Uuid, Path, description = "Subscription UUID")),
    responses(
        (status = 200, description = "Removed", body = ApiResponse<UnsubscribeResponse>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn unsubscribe(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<UnsubscribeResponse>>, AppError> {
    sqlx::query("DELETE FROM push_subscriptions WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(auth_user.user_id)
        .execute(&state.db)
        .await?;
    Ok(Json(ApiResponse::new(UnsubscribeResponse {
        removed: true,
    })))
}

// ═══════════════════════════════════════════════════════════════════
// P15.1 — Mobile push tokens (FCM + APNS)
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, ToSchema)]
pub struct RegisterMobileTokenBody {
    /// `"fcm"` (Android) or `"apns"` (iOS).
    #[schema(max_length = 10000)]
    pub platform: String,
    /// Opaque provider token from FCM or APNS.
    #[schema(max_length = 10000)]
    pub token: String,
    /// Stable device id (survives token rotation).
    #[schema(max_length = 10000)]
    pub device_id: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MobileTokenRegisteredResponse {
    pub id: Uuid,
    pub platform: String,
    pub device_id: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MobileTokenRevokedResponse {
    pub removed: bool,
}

/// Sanitised mobile token metadata — the raw token is never exposed
/// back to the client, only the identifying info.
#[derive(Debug, Serialize, ToSchema)]
pub struct MobileTokenSummary {
    pub id: Uuid,
    pub platform: String,
    pub device_id: String,
    pub last_seen_at: chrono::DateTime<chrono::Utc>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MobileTokensListResponse {
    pub tokens: Vec<MobileTokenSummary>,
}

/// Register or refresh a mobile push token for the caller's device.
#[utoipa::path(
    post,
    path = "/api/users/me/push-tokens/register",
    tag = "profile",
    request_body = RegisterMobileTokenBody,
    responses(
        (status = 200, description = "Token registered", body = ApiResponse<MobileTokenRegisteredResponse>),
        (status = 400, description = "Unsupported platform", body = crate::api_response::ErrorResponse),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn register_mobile_token(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<RegisterMobileTokenBody>,
) -> Result<Json<ApiResponse<MobileTokenRegisteredResponse>>, AppError> {
    let platform = crate::services::mobile_push::Platform::from_str(&body.platform)?;
    let row = crate::services::mobile_push::register_token(
        &state.db,
        auth.user_id,
        platform,
        &body.token,
        &body.device_id,
    )
    .await?;
    metrics::counter!(
        "skilluv_mobile_push_tokens_registered_total",
        "platform" => platform.as_str().to_string()
    )
    .increment(1);
    Ok(Json(ApiResponse::new(MobileTokenRegisteredResponse {
        id: row.id,
        platform: row.platform,
        device_id: row.device_id,
    })))
}

/// Revoke a mobile token by its device id.
#[utoipa::path(
    delete,
    path = "/api/users/me/push-tokens/{device_id}",
    tag = "profile",
    params(("device_id" = String, Path, description = "Stable device id")),
    responses(
        (status = 200, description = "Token revoked (may have already been absent)", body = ApiResponse<MobileTokenRevokedResponse>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn revoke_mobile_token(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(device_id): Path<String>,
) -> Result<Json<ApiResponse<MobileTokenRevokedResponse>>, AppError> {
    let n = crate::services::mobile_push::revoke_token(&state.db, auth.user_id, &device_id).await?;
    Ok(Json(ApiResponse::new(MobileTokenRevokedResponse {
        removed: n > 0,
    })))
}

/// List the caller's registered mobile push tokens (metadata only —
/// the raw provider tokens are never echoed back).
#[utoipa::path(
    get,
    path = "/api/users/me/push-tokens",
    tag = "profile",
    responses(
        (status = 200, description = "Registered tokens", body = ApiResponse<MobileTokensListResponse>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn list_mobile_tokens(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<MobileTokensListResponse>>, AppError> {
    let tokens =
        crate::services::mobile_push::list_tokens_for_user(&state.db, auth.user_id).await?;
    // Ne pas exposer les tokens en clair — juste les metadata.
    let items: Vec<MobileTokenSummary> = tokens
        .iter()
        .map(|t| MobileTokenSummary {
            id: t.id,
            platform: t.platform.clone(),
            device_id: t.device_id.clone(),
            last_seen_at: t.last_seen_at,
            created_at: t.created_at,
        })
        .collect();
    Ok(Json(ApiResponse::new(MobileTokensListResponse {
        tokens: items,
    })))
}

/// PWA web app manifest. Not JSON — served as
/// `application/manifest+json`. Intentionally omitted from the OpenAPI
/// schema (`/manifest.webmanifest` is served at root, not under /api).
async fn pwa_manifest() -> impl IntoResponse {
    let body = serde_json::json!({
        "name": "Skilluv",
        "short_name": "Skilluv",
        "start_url": "/",
        "display": "standalone",
        "background_color": "#1a1a2e",
        "theme_color": "#6c5ce7",
        "orientation": "portrait",
        "icons": [
            { "src": "/icons/icon-192.png", "sizes": "192x192", "type": "image/png" },
            { "src": "/icons/icon-256.png", "sizes": "256x256", "type": "image/png" },
            { "src": "/icons/icon-512.png", "sizes": "512x512", "type": "image/png" }
        ]
    });
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "application/manifest+json",
        )],
        body.to_string(),
    )
}
