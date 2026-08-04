//! Email preferences + unsubscribe + Brevo webhook (Phase 1.7).

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Html;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::digest::{self, DigestRunReport};

pub fn email_prefs_routes() -> Router<AppState> {
    Router::new()
        .route("/auth/me/email-preferences", get(get_prefs))
        .route("/auth/me/email-preferences", put(update_prefs))
        .route("/email/unsubscribe", get(unsubscribe))
        .route("/webhooks/brevo", post(brevo_webhook))
        .route("/admin/digest/run-weekly", post(admin_run_weekly_digest))
}

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct EmailPrefs {
    /// Opt-in to the weekly activity digest.
    pub digest_weekly: bool,
    /// Opt-in to the daily streak-at-risk reminder.
    pub streak_reminder: bool,
    /// Opt-in to marketing / product-news emails.
    pub marketing: bool,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EmailPrefsResponse {
    pub preferences: EmailPrefs,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminDigestResponse {
    pub digest: DigestRunReport,
}

/// Read the caller's email preferences. Rows are lazily upserted with
/// the marketing-opt-out defaults on first read — the endpoint always
/// returns a full record.
#[utoipa::path(
    get,
    path = "/api/auth/me/email-preferences",
    tag = "auth",
    responses(
        (status = 200, description = "Current preferences", body = ApiResponse<EmailPrefsResponse>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn get_prefs(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<EmailPrefsResponse>>, AppError> {
    // Upsert defaults on first read.
    let prefs: EmailPrefs = sqlx::query_as(
        r#"
        INSERT INTO user_email_preferences (user_id)
        VALUES ($1)
        ON CONFLICT (user_id) DO UPDATE SET user_id = user_email_preferences.user_id
        RETURNING digest_weekly, streak_reminder, marketing, updated_at
        "#,
    )
    .bind(auth.user_id)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(EmailPrefsResponse {
        preferences: prefs,
    })))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdatePrefsRequest {
    pub digest_weekly: Option<bool>,
    pub streak_reminder: Option<bool>,
    pub marketing: Option<bool>,
}

/// Partial update of the caller's email preferences. Any missing
/// field keeps its current value (COALESCE-based upsert).
#[utoipa::path(
    put,
    path = "/api/auth/me/email-preferences",
    tag = "auth",
    request_body = UpdatePrefsRequest,
    responses(
        (status = 200, description = "Updated preferences", body = ApiResponse<EmailPrefsResponse>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn update_prefs(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<UpdatePrefsRequest>,
) -> Result<Json<ApiResponse<EmailPrefsResponse>>, AppError> {
    let prefs: EmailPrefs = sqlx::query_as(
        r#"
        INSERT INTO user_email_preferences (user_id, digest_weekly, streak_reminder, marketing)
        VALUES ($1, COALESCE($2, TRUE), COALESCE($3, TRUE), COALESCE($4, FALSE))
        ON CONFLICT (user_id) DO UPDATE SET
            digest_weekly = COALESCE($2, user_email_preferences.digest_weekly),
            streak_reminder = COALESCE($3, user_email_preferences.streak_reminder),
            marketing = COALESCE($4, user_email_preferences.marketing),
            updated_at = NOW()
        RETURNING digest_weekly, streak_reminder, marketing, updated_at
        "#,
    )
    .bind(auth.user_id)
    .bind(body.digest_weekly)
    .bind(body.streak_reminder)
    .bind(body.marketing)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(EmailPrefsResponse {
        preferences: prefs,
    })))
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct UnsubscribeQuery {
    /// HMAC-signed unsubscribe token embedded in the email footer.
    pub token: String,
    /// One of `digest_weekly`, `streak_reminder`, `marketing`.
    pub kind: String,
}

/// One-click unsubscribe. No login required. Token is HMAC-signed;
/// only the targeted user can land here (or admin with full secret
/// access). Returns a plain HTML confirmation suitable for showing in
/// a browser — **not** JSON, so this endpoint is intentionally left
/// out of the ApiResponse envelope.
#[utoipa::path(
    get,
    path = "/api/email/unsubscribe",
    tag = "auth",
    params(UnsubscribeQuery),
    responses(
        (status = 200, description = "HTML confirmation page", content_type = "text/html"),
        (status = 400, description = "Unsupported unsubscribe kind or kind/token mismatch", body = crate::api_response::ErrorResponse),
        (status = 401, description = "Invalid or forged token", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn unsubscribe(
    State(state): State<AppState>,
    Query(query): Query<UnsubscribeQuery>,
) -> Result<Html<String>, AppError> {
    let secret = unsub_secret(&state.config.jwt_secret);
    let (user_id, token_kind) =
        digest::verify_unsubscribe_token(&query.token, &secret).ok_or(AppError::Unauthorized)?;
    if token_kind != query.kind {
        return Err(AppError::Validation("Token kind mismatch".into()));
    }

    let column = match query.kind.as_str() {
        "digest_weekly" => "digest_weekly",
        "streak_reminder" => "streak_reminder",
        "marketing" => "marketing",
        _ => {
            return Err(AppError::Validation(format!(
                "Unsupported unsubscribe kind: {}",
                query.kind
            )));
        }
    };
    let sql = format!(
        r#"
        INSERT INTO user_email_preferences (user_id, {col})
        VALUES ($1, FALSE)
        ON CONFLICT (user_id) DO UPDATE SET {col} = FALSE, updated_at = NOW()
        "#,
        col = column
    );
    sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(user_id)
        .execute(&state.db)
        .await?;

    tracing::info!(user_id = %user_id, kind = %query.kind, "user unsubscribed");

    Ok(Html(format!(
        r#"<!doctype html>
<html lang="fr"><head><meta charset="utf-8"><title>Désinscrit·e — Skilluv</title>
<style>body{{font-family:system-ui;max-width:540px;margin:80px auto;padding:0 24px;color:#1a1a2e}}h1{{color:#6c5ce7}}</style>
</head><body>
<h1>C'est fait ✓</h1>
<p>Tu ne recevras plus d'emails de type <strong>{kind}</strong> de Skilluv.</p>
<p>Si tu changes d'avis, tu peux réactiver depuis <a href="https://skilluv.com/settings/notifications">tes paramètres</a>.</p>
</body></html>"#,
        kind = query.kind
    )))
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct BrevoWebhookQuery {
    /// Shared secret matching `BREVO_WEBHOOK_TOKEN` env var.
    pub token: String,
}

/// Brevo webhook for delivery / bounce / complaint events.
/// Authenticated via `?token=...` matching `BREVO_WEBHOOK_TOKEN`.
/// Body shape is defined by Brevo (event, email, message-id, ts) and
/// documented as a free-form JSON blob since Brevo evolves it.
#[utoipa::path(
    post,
    path = "/api/webhooks/brevo",
    tag = "webhooks",
    params(BrevoWebhookQuery),
    request_body(content = serde_json::Value, description = "Raw Brevo event payload"),
    responses(
        (status = 200, description = "Event processed (or intentionally ignored)"),
        (status = 401, description = "Token mismatch", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn brevo_webhook(
    State(state): State<AppState>,
    Query(q): Query<BrevoWebhookQuery>,
    Json(body): Json<Value>,
) -> Result<StatusCode, AppError> {
    // BREVO_WEBHOOK_TOKEN absent (dev/CI/deployments sans Brevo) : ack
    // silencieusement le webhook avec 200. Best-practice pour webhooks
    // externes — un non-200 declenche des retries indefinis. On log
    // pour observabilite.
    let Ok(expected) = std::env::var("BREVO_WEBHOOK_TOKEN") else {
        tracing::warn!("Brevo webhook received but BREVO_WEBHOOK_TOKEN not set — acking silently");
        return Ok(StatusCode::OK);
    };
    if q.token != expected {
        return Err(AppError::Unauthorized);
    }

    // Brevo sends: {"event": "hard_bounce" | "soft_bounce" | "delivered" | "opened" | "spam" | ..., "email": "...", "message-id": "...", "ts": ...}
    let event = body.get("event").and_then(|v| v.as_str()).unwrap_or("");
    let email = body.get("email").and_then(|v| v.as_str()).unwrap_or("");
    let provider_msg_id = body
        .get("message-id")
        .and_then(|v| v.as_str())
        .map(String::from);

    if email.is_empty() {
        return Ok(StatusCode::OK);
    }

    // Find the user by email
    let user_id: Option<(uuid::Uuid,)> = sqlx::query_as("SELECT id FROM users WHERE email = $1")
        .bind(email)
        .fetch_optional(&state.db)
        .await?;
    let Some((user_id,)) = user_id else {
        // Not our user — Brevo can send events for other senders; ignore.
        return Ok(StatusCode::OK);
    };

    match event {
        "hard_bounce" | "blocked" | "unsubscribed" | "spam" => {
            sqlx::query(
                r#"
                UPDATE users SET
                    email_disabled = TRUE,
                    email_bounce_count = email_bounce_count + 1,
                    email_last_bounce_at = NOW()
                WHERE id = $1
                "#,
            )
            .bind(user_id)
            .execute(&state.db)
            .await?;
            tracing::warn!(user_id = %user_id, event, "email disabled (hard event)");
        }
        "soft_bounce" => {
            // Disable after 3 soft bounces
            sqlx::query(
                r#"
                UPDATE users SET
                    email_bounce_count = email_bounce_count + 1,
                    email_last_bounce_at = NOW(),
                    email_disabled = CASE WHEN email_bounce_count + 1 >= 3 THEN TRUE ELSE email_disabled END
                WHERE id = $1
                "#,
            )
            .bind(user_id)
            .execute(&state.db)
            .await?;
        }
        "delivered" => {
            if let Some(ref msg) = provider_msg_id {
                sqlx::query(
                    "UPDATE email_log SET delivered_at = NOW() WHERE provider_message_id = $1",
                )
                .bind(msg)
                .execute(&state.db)
                .await?;
            }
        }
        "opened" => {
            if let Some(ref msg) = provider_msg_id {
                sqlx::query(
                    "UPDATE email_log SET opened_at = NOW() WHERE provider_message_id = $1",
                )
                .bind(msg)
                .execute(&state.db)
                .await?;
            }
        }
        _ => {
            tracing::debug!(event, %email, "brevo webhook event ignored");
        }
    }

    Ok(StatusCode::OK)
}

/// Admin-only: manually kick off a weekly digest run. Same job the
/// cron worker triggers automatically. Returns per-bucket counters
/// for verification.
#[utoipa::path(
    post,
    path = "/api/admin/digest/run-weekly",
    tag = "admin",
    responses(
        (status = 200, description = "Digest run complete", body = ApiResponse<AdminDigestResponse>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not an admin", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn admin_run_weekly_digest(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<AdminDigestResponse>>, AppError> {
    if auth.role != "admin" {
        return Err(AppError::Forbidden);
    }
    let secret = unsub_secret(&state.config.jwt_secret);
    let svc = digest::DigestService {
        db: &state.db,
        email: &state.email,
        base_url: &state.config.base_url,
        unsubscribe_secret: &secret,
    };
    let report = svc.run_weekly().await?;
    Ok(Json(ApiResponse::new(AdminDigestResponse {
        digest: report,
    })))
}

/// Derive the unsubscribe-token HMAC key from JWT_SECRET. Avoids a separate secret in env.
fn unsub_secret(jwt_secret: &str) -> Vec<u8> {
    use hmac::{Hmac, KeyInit, Mac};
    type HmacSha256 = Hmac<sha2::Sha256>;
    let mut mac =
        HmacSha256::new_from_slice(jwt_secret.as_bytes()).expect("HMAC accepts any key size");
    mac.update(b"skilluv-unsubscribe-v1");
    mac.finalize().into_bytes().to_vec()
}
