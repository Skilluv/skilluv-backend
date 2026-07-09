//! Email preferences + unsubscribe + Brevo webhook (Phase 1.7).

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Html;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::digest;

pub fn email_prefs_routes() -> Router<AppState> {
    Router::new()
        .route("/auth/me/email-preferences", get(get_prefs))
        .route("/auth/me/email-preferences", put(update_prefs))
        .route("/email/unsubscribe", get(unsubscribe))
        .route("/webhooks/brevo", post(brevo_webhook))
        .route("/admin/digest/run-weekly", post(admin_run_weekly_digest))
}

fn build_response(data: Value) -> Value {
    json!({
        "data": data,
        "meta": {
            "request_id": uuid::Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })
}

#[derive(serde::Serialize, sqlx::FromRow)]
struct EmailPrefs {
    digest_weekly: bool,
    streak_reminder: bool,
    marketing: bool,
    updated_at: chrono::DateTime<chrono::Utc>,
}

async fn get_prefs(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
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

    Ok(Json(build_response(json!({ "preferences": prefs }))))
}

#[derive(Deserialize)]
struct UpdatePrefsRequest {
    digest_weekly: Option<bool>,
    streak_reminder: Option<bool>,
    marketing: Option<bool>,
}

async fn update_prefs(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<UpdatePrefsRequest>,
) -> Result<Json<Value>, AppError> {
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

    Ok(Json(build_response(json!({ "preferences": prefs }))))
}

#[derive(Deserialize)]
struct UnsubscribeQuery {
    token: String,
    kind: String,
}

/// One-click unsubscribe. No login required. Token is HMAC-signed; only the targeted
/// user can land here (or admin with full secret access). Returns a plain HTML
/// confirmation suitable for showing in a browser.
async fn unsubscribe(
    State(state): State<AppState>,
    Query(query): Query<UnsubscribeQuery>,
) -> Result<Html<String>, AppError> {
    let secret = unsub_secret(&state.config.jwt_secret);
    let (user_id, token_kind) = digest::verify_unsubscribe_token(&query.token, &secret)
        .ok_or(AppError::Unauthorized)?;
    if token_kind != query.kind {
        return Err(AppError::Validation(
            "Token kind mismatch".into(),
        ));
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
    sqlx::query(&sql).bind(user_id).execute(&state.db).await?;

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

#[derive(Deserialize)]
struct BrevoWebhookQuery {
    token: String,
}

/// Brevo webhook for delivery / bounce / complaint events.
/// Authenticated via `?token=...` matching `BREVO_WEBHOOK_TOKEN`.
async fn brevo_webhook(
    State(state): State<AppState>,
    Query(q): Query<BrevoWebhookQuery>,
    Json(body): Json<Value>,
) -> Result<StatusCode, AppError> {
    let expected = std::env::var("BREVO_WEBHOOK_TOKEN")
        .map_err(|_| AppError::Internal("BREVO_WEBHOOK_TOKEN not set".into()))?;
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
    let user_id: Option<(uuid::Uuid,)> =
        sqlx::query_as("SELECT id FROM users WHERE email = $1")
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
                sqlx::query("UPDATE email_log SET opened_at = NOW() WHERE provider_message_id = $1")
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

async fn admin_run_weekly_digest(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
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
    Ok(Json(build_response(json!({ "digest": report }))))
}

/// Derive the unsubscribe-token HMAC key from JWT_SECRET. Avoids a separate secret in env.
fn unsub_secret(jwt_secret: &str) -> Vec<u8> {
    use hmac::{Hmac, Mac};
    type HmacSha256 = Hmac<sha2::Sha256>;
    let mut mac = HmacSha256::new_from_slice(jwt_secret.as_bytes())
        .expect("HMAC accepts any key size");
    mac.update(b"skilluv-unsubscribe-v1");
    mac.finalize().into_bytes().to_vec()
}
