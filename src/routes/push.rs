//! Web Push subscription endpoints — Phase 4.12.
//!
//! Backend part only. Actual VAPID delivery uses the `web-push` protocol ; a full
//! push-sending helper is registered but sending itself is deferred to when the
//! `web-push` crate is added (or a homemade JWT/ECDH sender is implemented).

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;

pub fn push_routes() -> Router<AppState> {
    Router::new()
        .route("/notifications/push/vapid-public-key", get(vapid_public_key))
        .route("/notifications/push/subscribe", post(subscribe))
        .route("/notifications/push/{id}", delete(unsubscribe))
        .route("/manifest.webmanifest", get(pwa_manifest))
        // P15.1 — mobile push tokens (FCM + APNS)
        .route("/users/me/push-tokens/register", post(register_mobile_token))
        .route(
            "/users/me/push-tokens/{device_id}",
            delete(revoke_mobile_token),
        )
        .route("/users/me/push-tokens", get(list_mobile_tokens))
}

fn build_response(data: Value) -> Value {
    json!({
        "data": data,
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })
}

async fn vapid_public_key() -> Result<Json<Value>, AppError> {
    let key = std::env::var("VAPID_PUBLIC_KEY")
        .ok()
        .filter(|s| !s.is_empty())
        .ok_or(AppError::Internal("VAPID_PUBLIC_KEY not set".into()))?;
    Ok(Json(build_response(json!({ "public_key": key }))))
}

#[derive(Deserialize)]
struct SubscribeBody {
    endpoint: String,
    p256dh: String,
    auth: String,
}

async fn subscribe(
    State(state): State<AppState>,
    auth_user: AuthUser,
    headers: HeaderMap,
    Json(body): Json<SubscribeBody>,
) -> Result<Json<Value>, AppError> {
    if body.endpoint.is_empty() || body.p256dh.is_empty() || body.auth.is_empty() {
        return Err(AppError::Validation("missing endpoint / p256dh / auth".into()));
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
    Ok(Json(build_response(json!({ "subscription_id": row.0 }))))
}

async fn unsubscribe(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    sqlx::query("DELETE FROM push_subscriptions WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(auth_user.user_id)
        .execute(&state.db)
        .await?;
    Ok(Json(build_response(json!({ "removed": true }))))
}

// ═══════════════════════════════════════════════════════════════════
// P15.1 — Mobile push tokens (FCM + APNS)
// ═══════════════════════════════════════════════════════════════════

#[derive(Deserialize)]
struct RegisterMobileTokenBody {
    /// "fcm" | "apns"
    platform: String,
    token: String,
    device_id: String,
}

async fn register_mobile_token(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<RegisterMobileTokenBody>,
) -> Result<Json<Value>, AppError> {
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
    Ok(Json(build_response(json!({
        "id": row.id,
        "platform": row.platform,
        "device_id": row.device_id,
    }))))
}

async fn revoke_mobile_token(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(device_id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let n = crate::services::mobile_push::revoke_token(&state.db, auth.user_id, &device_id)
        .await?;
    Ok(Json(build_response(json!({ "removed": n > 0 }))))
}

async fn list_mobile_tokens(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let tokens = crate::services::mobile_push::list_tokens_for_user(&state.db, auth.user_id)
        .await?;
    // Ne pas exposer les tokens en clair — juste les metadata.
    let items: Vec<Value> = tokens
        .iter()
        .map(|t| {
            json!({
                "id": t.id,
                "platform": t.platform,
                "device_id": t.device_id,
                "last_seen_at": t.last_seen_at,
                "created_at": t.created_at,
            })
        })
        .collect();
    Ok(Json(build_response(json!({ "tokens": items }))))
}

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
        [(axum::http::header::CONTENT_TYPE, "application/manifest+json")],
        body.to_string(),
    )
}
