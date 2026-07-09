use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::{AuthService, WebhookService};

pub fn developer_routes() -> Router<AppState> {
    Router::new()
        // API Keys
        .route("/developer/keys", post(create_key))
        .route("/developer/keys", get(list_keys))
        .route("/developer/keys/{id}", delete(revoke_key))
        .route("/developer/keys/{id}/regenerate", post(regenerate_key))
        .route("/developer/keys/{id}/usage", get(key_usage))
        // Webhooks
        .route("/developer/webhooks", post(create_webhook))
        .route("/developer/webhooks", get(list_webhooks))
        .route("/developer/webhooks/{id}", put(update_webhook))
        .route("/developer/webhooks/{id}", delete(delete_webhook))
        .route("/developer/webhooks/{id}/test", post(test_webhook))
}

fn build_response(data: serde_json::Value) -> serde_json::Value {
    json!({
        "data": data,
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })
}

fn generate_api_key() -> String {
    let random: String = (0..32).map(|_| format!("{:x}", rand_byte())).collect();
    format!("sk_live_{random}")
}

fn rand_byte() -> u8 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    ((nanos ^ (nanos >> 8) ^ (nanos >> 16)) & 0xFF) as u8
}

// ─── Request types ──────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct CreateKeyRequest {
    name: String,
    permissions: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct CreateWebhookRequest {
    url: String,
    events: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct UpdateWebhookRequest {
    url: Option<String>,
    events: Option<Vec<String>>,
    active: Option<bool>,
}

// ─── Structs ────────────────────────────────────────────────────

#[derive(Debug, serde::Serialize, sqlx::FromRow)]
struct ApiKeyInfo {
    id: Uuid,
    name: String,
    key_prefix: String,
    permissions: serde_json::Value,
    last_used_at: Option<chrono::DateTime<chrono::Utc>>,
    request_count: i64,
    active: bool,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, serde::Serialize, sqlx::FromRow)]
struct WebhookInfo {
    id: Uuid,
    url: String,
    events: Vec<String>,
    active: bool,
    last_triggered_at: Option<chrono::DateTime<chrono::Utc>>,
    failure_count: i32,
    created_at: chrono::DateTime<chrono::Utc>,
}

// ─── API Keys ───────────────────────────────────────────────────

// POST /api/developer/keys
async fn create_key(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateKeyRequest>,
) -> Result<impl IntoResponse, AppError> {
    if body.name.trim().is_empty() || body.name.len() > 100 {
        return Err(AppError::Validation(
            "Name must be between 1 and 100 characters".to_string(),
        ));
    }

    let raw_key = generate_api_key();
    let prefix = &raw_key[..12];
    let key_hash = AuthService::hash_password(&raw_key)?;

    let valid_perms = [
        "read:profile",
        "read:skills",
        "read:badges",
        "read:leaderboard",
        "*",
    ];
    let permissions = body
        .permissions
        .unwrap_or_else(|| vec!["read:profile".to_string()]);

    for perm in &permissions {
        if !valid_perms.contains(&perm.as_str()) {
            return Err(AppError::Validation(format!(
                "Invalid permission: {perm}. Valid: {}",
                valid_perms.join(", ")
            )));
        }
    }

    let perms_json = serde_json::to_value(&permissions)
        .map_err(|e| AppError::Internal(format!("JSON error: {e}")))?;

    let key: ApiKeyInfo = sqlx::query_as(
        r#"
        INSERT INTO api_keys (user_id, name, key_prefix, key_hash, permissions)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, name, key_prefix, permissions, last_used_at, request_count, active, created_at
        "#,
    )
    .bind(auth.user_id)
    .bind(body.name.trim())
    .bind(prefix)
    .bind(&key_hash)
    .bind(&perms_json)
    .fetch_one(&state.db)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(build_response(json!({
            "key": key,
            "secret": raw_key,
            "message": "Store the secret securely — it cannot be retrieved later."
        }))),
    ))
}

// GET /api/developer/keys
async fn list_keys(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, AppError> {
    let keys: Vec<ApiKeyInfo> = sqlx::query_as(
        "SELECT id, name, key_prefix, permissions, last_used_at, request_count, active, created_at FROM api_keys WHERE user_id = $1 ORDER BY created_at DESC",
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(build_response(json!({ "keys": keys }))))
}

// DELETE /api/developer/keys/:id
async fn revoke_key(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let result = sqlx::query("UPDATE api_keys SET active = FALSE WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(auth.user_id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("API key not found".to_string()));
    }

    Ok(Json(build_response(json!({
        "message": "API key revoked"
    }))))
}

// POST /api/developer/keys/:id/regenerate
async fn regenerate_key(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let raw_key = generate_api_key();
    let prefix = &raw_key[..12];
    let key_hash = AuthService::hash_password(&raw_key)?;

    let result = sqlx::query(
        "UPDATE api_keys SET key_prefix = $1, key_hash = $2, request_count = 0 WHERE id = $3 AND user_id = $4 AND active = TRUE",
    )
    .bind(prefix)
    .bind(&key_hash)
    .bind(id)
    .bind(auth.user_id)
    .execute(&state.db)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(
            "API key not found or inactive".to_string(),
        ));
    }

    Ok(Json(build_response(json!({
        "secret": raw_key,
        "message": "API key regenerated. Store the new secret securely."
    }))))
}

// GET /api/developer/keys/:id/usage
async fn key_usage(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let key: ApiKeyInfo = sqlx::query_as(
        "SELECT id, name, key_prefix, permissions, last_used_at, request_count, active, created_at FROM api_keys WHERE id = $1 AND user_id = $2",
    )
    .bind(id)
    .bind(auth.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound("API key not found".to_string()))?;

    Ok(Json(build_response(json!({
        "key_id": key.id,
        "name": key.name,
        "request_count": key.request_count,
        "last_used_at": key.last_used_at.map(|d| d.to_rfc3339()),
        "active": key.active,
    }))))
}

// ─── Webhooks ───────────────────────────────────────────────────

// POST /api/developer/webhooks
async fn create_webhook(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateWebhookRequest>,
) -> Result<impl IntoResponse, AppError> {
    if body.url.trim().is_empty() || body.url.len() > 500 {
        return Err(AppError::Validation(
            "URL must be between 1 and 500 characters".to_string(),
        ));
    }
    if !body.url.starts_with("https://") && !body.url.starts_with("http://localhost") {
        return Err(AppError::Validation(
            "Webhook URL must use HTTPS (except localhost for development)".to_string(),
        ));
    }

    let valid_events = [
        "challenge.completed",
        "badge.earned",
        "title.changed",
        "leaderboard.updated",
    ];
    for event in &body.events {
        if !valid_events.contains(&event.as_str()) {
            return Err(AppError::Validation(format!(
                "Invalid event: {event}. Valid: {}",
                valid_events.join(", ")
            )));
        }
    }

    let secret = format!("whsec_{}", Uuid::new_v4().to_string().replace('-', ""));

    let webhook: WebhookInfo = sqlx::query_as(
        r#"
        INSERT INTO webhooks (user_id, url, secret, events)
        VALUES ($1, $2, $3, $4)
        RETURNING id, url, events, active, last_triggered_at, failure_count, created_at
        "#,
    )
    .bind(auth.user_id)
    .bind(body.url.trim())
    .bind(&secret)
    .bind(&body.events)
    .fetch_one(&state.db)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(build_response(json!({
            "webhook": webhook,
            "secret": secret,
            "message": "Webhook created. Store the secret for signature verification."
        }))),
    ))
}

// GET /api/developer/webhooks
async fn list_webhooks(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, AppError> {
    let webhooks: Vec<WebhookInfo> = sqlx::query_as(
        "SELECT id, url, events, active, last_triggered_at, failure_count, created_at FROM webhooks WHERE user_id = $1 ORDER BY created_at DESC",
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(build_response(json!({ "webhooks": webhooks }))))
}

// PUT /api/developer/webhooks/:id
async fn update_webhook(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateWebhookRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let webhook: WebhookInfo = sqlx::query_as(
        r#"
        UPDATE webhooks SET
            url = COALESCE($1, url),
            events = COALESCE($2, events),
            active = COALESCE($3, active),
            updated_at = NOW()
        WHERE id = $4 AND user_id = $5
        RETURNING id, url, events, active, last_triggered_at, failure_count, created_at
        "#,
    )
    .bind(&body.url)
    .bind(&body.events)
    .bind(body.active)
    .bind(id)
    .bind(auth.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound("Webhook not found".to_string()))?;

    Ok(Json(build_response(json!({ "webhook": webhook }))))
}

// DELETE /api/developer/webhooks/:id
async fn delete_webhook(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let result = sqlx::query("DELETE FROM webhooks WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(auth.user_id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Webhook not found".to_string()));
    }

    Ok(Json(build_response(json!({
        "message": "Webhook deleted"
    }))))
}

// POST /api/developer/webhooks/:id/test
async fn test_webhook(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    // Verify ownership
    let exists: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM webhooks WHERE id = $1 AND user_id = $2")
            .bind(id)
            .bind(auth.user_id)
            .fetch_optional(&state.db)
            .await?;

    if exists.is_none() {
        return Err(AppError::NotFound("Webhook not found".to_string()));
    }

    WebhookService::send_test(&state.db, id).await?;

    Ok(Json(build_response(json!({
        "message": "Test event sent. Check your endpoint and webhook deliveries."
    }))))
}
