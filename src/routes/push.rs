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
