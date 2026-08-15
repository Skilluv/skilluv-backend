//! Email preferences + unsubscribe + Brevo webhook (Phase 1.7).

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Html;
use axum::routing::{get, post};
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
        // SKI-287 — the contract the front end consumes. Flat `data`, and a
        // PUT that replaces all three flags at once.
        .route(
            "/users/me/email-preferences",
            get(get_prefs_v2).put(replace_prefs),
        )
        // One-click unsubscribe with the token in the path (RFC 8058 style).
        .route("/email/unsubscribe/{token}", get(unsubscribe_by_path))
        // SKI-293 — `/auth/me/email-preferences` removed. It answered the
        // same question as the `/users/me` pair above with a different shape
        // (`data.preferences` versus flat `data`) and partial-update
        // semantics, and it was the one the OpenAPI document advertised. No
        // caller was left: checked across the front and admin repos and the
        // test suite.
        //
        // `/email/unsubscribe` stays: its link is printed in emails already
        // delivered, and those cannot be revised.
        .route("/email/unsubscribe", get(unsubscribe))
        .route("/webhooks/brevo", post(brevo_webhook))
        .route("/admin/digest/run-weekly", post(admin_run_weekly_digest))
}

/// The three opt-out categories. Transactional mail (email verification,
/// password reset, security alerts, payment receipts) is never listed here
/// and cannot be disabled.
pub const EMAIL_CATEGORIES: &[&str] = &["digest_weekly", "streak_reminder", "marketing"];

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
pub struct AdminDigestResponse {
    pub digest: DigestRunReport,
}

// ═══════════════════════════════════════════════════════════════════
// SKI-287 — /users/me/email-preferences
// ═══════════════════════════════════════════════════════════════════

/// Read the caller's email preferences.
///
/// A user who has never touched their settings gets the documented
/// defaults, not a 404: the absence of a row means "never customised",
/// which is a perfectly good answer to "what are my preferences".
///
/// The payload is the preference object itself rather than
/// `{ preferences: … }` — the shape the settings screen consumes.
#[utoipa::path(
    get,
    path = "/api/users/me/email-preferences",
    tag = "profile",
    responses(
        (status = 200, description = "Current preferences", body = ApiResponse<EmailPrefs>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn get_prefs_v2(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<EmailPrefs>>, AppError> {
    Ok(Json(ApiResponse::new(
        read_categories(&state.db, auth.user_id).await?,
    )))
}

/// The three categories, computed from the catalogue.
///
/// `user_email_preferences` used to hold these as columns, and the digest
/// and drip services read that table while `notify` read the catalogue —
/// two answers to "may we email this person", and the marketing one won by
/// accident. There is one now; this is a narrower view of it, kept because
/// the settings screen and the unsubscribe links already delivered speak in
/// these three words.
async fn read_categories(db: &sqlx::PgPool, user_id: uuid::Uuid) -> Result<EmailPrefs, AppError> {
    use crate::services::notify::{Channel, wants_kind};

    let digest_weekly = wants_kind(db, user_id, "digest.weekly", Channel::Email).await;
    // The reminder is a push by default and an email only if asked for, so
    // "do you want streak reminders" is either channel saying yes.
    let streak_reminder = wants_kind(db, user_id, "streak.reminder", Channel::Push).await
        || wants_kind(db, user_id, "streak.reminder", Channel::Email).await;

    // One consent covered six sequences, so any of them being on means it
    // was given.
    let mut marketing = false;
    for kind in lifecycle_kinds(db).await? {
        if wants_kind(db, user_id, &kind, Channel::Email).await {
            marketing = true;
            break;
        }
    }

    // The most recent decision across the rows behind these three words.
    // No row means the person never answered, which is now.
    let updated_at: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT MAX(updated_at) FROM notification_preferences
          WHERE user_id = $1
            AND kind IN (SELECT kind FROM notification_kinds
                          WHERE category IN ('digest', 'lifecycle'))",
    )
    .bind(user_id)
    .fetch_one(db)
    .await?;

    Ok(EmailPrefs {
        digest_weekly,
        streak_reminder,
        marketing,
        updated_at: updated_at.unwrap_or_else(chrono::Utc::now),
    })
}

/// Every kind the single `marketing` box stands for.
///
/// Read from the catalogue rather than listed here, so a sequence added
/// later is covered by the same consent instead of being sent to everyone.
async fn lifecycle_kinds(db: &sqlx::PgPool) -> Result<Vec<String>, AppError> {
    Ok(
        sqlx::query_scalar("SELECT kind FROM notification_kinds WHERE category = 'lifecycle'")
            .fetch_all(db)
            .await?,
    )
}

/// Turn one of the three words into rows on the kinds behind it.
///
/// Enabling writes an explicit yes rather than removing the override:
/// marketing defaults to off, and consent has to be recorded as given, not
/// inferred from the absence of a refusal.
async fn write_category(
    db: &sqlx::PgPool,
    user_id: uuid::Uuid,
    rows: &[(&str, &str, bool)],
) -> Result<(), AppError> {
    for (kind, channel, enabled) in rows {
        sqlx::query(
            "INSERT INTO notification_preferences (user_id, kind, channel, enabled)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (user_id, kind, channel)
             DO UPDATE SET enabled = EXCLUDED.enabled, updated_at = NOW()",
        )
        .bind(user_id)
        .bind(kind)
        .bind(channel)
        .bind(enabled)
        .execute(db)
        .await?;
    }
    Ok(())
}

/// Full replacement of the caller's email preferences.
///
/// All three flags are required. A partial payload is rejected rather than
/// merged: with an opt-in category like `marketing`, "field absent" is
/// ambiguous between "leave it" and "set it false", and guessing wrong on
/// a GDPR consent flag is not acceptable in either direction.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ReplacePrefsRequest {
    pub digest_weekly: bool,
    pub streak_reminder: bool,
    pub marketing: bool,
}

/// Validate a full-replacement payload, naming the offending field.
///
/// Every category must be present and boolean. Reported as
/// `AppError::Validation` (400) rather than letting serde produce a 422:
/// the screen shows this message to a human.
fn parse_replace_prefs(raw: &serde_json::Value) -> Result<ReplacePrefsRequest, AppError> {
    let obj = raw
        .as_object()
        .ok_or_else(|| AppError::Validation("body must be a JSON object".into()))?;

    let read = |key: &str| -> Result<bool, AppError> {
        match obj.get(key) {
            Some(serde_json::Value::Bool(b)) => Ok(*b),
            Some(_) => Err(AppError::Validation(format!("{key} must be a boolean"))),
            None => Err(AppError::Validation(format!(
                "{key} is required — this endpoint replaces all three \
                 categories at once"
            ))),
        }
    };

    let digest_weekly = read("digest_weekly")?;
    let streak_reminder = read("streak_reminder")?;
    let marketing = read("marketing")?;

    if let Some(unknown) = obj.keys().find(|k| !EMAIL_CATEGORIES.contains(&k.as_str())) {
        return Err(AppError::Validation(format!(
            "unknown field '{unknown}' — expected exactly: {}",
            EMAIL_CATEGORIES.join(", ")
        )));
    }

    Ok(ReplacePrefsRequest {
        digest_weekly,
        streak_reminder,
        marketing,
    })
}

#[utoipa::path(
    put,
    path = "/api/users/me/email-preferences",
    tag = "profile",
    request_body = ReplacePrefsRequest,
    responses(
        (status = 200, description = "Updated preferences", body = ApiResponse<EmailPrefs>),
        (status = 400, description = "Missing or non-boolean field", body = crate::api_response::ErrorResponse),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn replace_prefs(
    State(state): State<AppState>,
    auth: AuthUser,
    // Taken as a raw value rather than `Json<ReplacePrefsRequest>` so a
    // missing or mistyped field is a 400 VALIDATION_ERROR, as the contract
    // specifies. Axum's own `Json` rejection is a 422 with a serde message,
    // which is neither the documented status nor a useful sentence for the
    // settings screen to display.
    Json(raw): Json<serde_json::Value>,
) -> Result<Json<ApiResponse<EmailPrefs>>, AppError> {
    let body = parse_replace_prefs(&raw)?;

    write_category(
        &state.db,
        auth.user_id,
        &[("digest.weekly", "email", body.digest_weekly)],
    )
    .await?;

    // Turning the reminder on means the channel it is designed for, not
    // every channel: a streak nudge that arrives tomorrow by email is not a
    // nudge, and silently opting someone into it would be a second decision
    // they did not make.
    write_category(
        &state.db,
        auth.user_id,
        &[
            ("streak.reminder", "push", body.streak_reminder),
            ("streak.reminder", "email", false),
        ],
    )
    .await?;

    let lifecycle: Vec<String> = lifecycle_kinds(&state.db).await?;
    let marketing_rows: Vec<(&str, &str, bool)> = lifecycle
        .iter()
        .map(|kind| (kind.as_str(), "email", body.marketing))
        .collect();
    write_category(&state.db, auth.user_id, &marketing_rows).await?;

    Ok(Json(ApiResponse::new(
        read_categories(&state.db, auth.user_id).await?,
    )))
}

/// One-click unsubscribe with the token in the path.
///
/// The category is read from the signed token rather than from a separate
/// parameter: a mail client prefetching the link must not be able to
/// unsubscribe a different category by editing the URL, and RFC 8058
/// one-click flows send no parameters of their own.
///
/// Deliberately no expiry. A footer link in a two-year-old email must
/// still work — a dead unsubscribe link is what turns into a spam
/// complaint, which is the outcome this endpoint exists to prevent.
/// Revocation, if ever needed, is a secret rotation.
#[utoipa::path(
    get,
    path = "/api/email/unsubscribe/{token}",
    tag = "auth",
    params(("token" = String, Path, description = "HMAC-signed token carrying user and category")),
    responses(
        (status = 200, description = "HTML confirmation page", content_type = "text/html"),
        (status = 400, description = "Unsupported category", body = crate::api_response::ErrorResponse),
        (status = 401, description = "Invalid or forged token", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn unsubscribe_by_path(
    State(state): State<AppState>,
    axum::extract::Path(token): axum::extract::Path<String>,
) -> Result<Html<String>, AppError> {
    let secret = unsub_secret(&state.config.jwt_secret);
    let (user_id, kind) =
        digest::verify_unsubscribe_token(&token, &secret).ok_or(AppError::Unauthorized)?;
    apply_unsubscribe(&state, user_id, &kind).await
}

/// Turn one category off for one user. Idempotent — unsubscribing twice
/// is a normal consequence of a mail client prefetching the link.
async fn apply_unsubscribe(
    state: &AppState,
    user_id: uuid::Uuid,
    kind: &str,
) -> Result<Html<String>, AppError> {
    if !EMAIL_CATEGORIES.contains(&kind) {
        return Err(AppError::Validation(format!(
            "Unsupported unsubscribe kind: {kind}"
        )));
    }

    // These three words are printed in links inside emails already
    // delivered, and those cannot be revised. They resolve to the kinds
    // behind them rather than to columns of a table that no longer exists.
    let rows: Vec<(String, &str)> = match kind {
        "digest_weekly" => vec![("digest.weekly".to_string(), "email")],
        "streak_reminder" => vec![
            ("streak.reminder".to_string(), "push"),
            ("streak.reminder".to_string(), "email"),
        ],
        "marketing" => lifecycle_kinds(&state.db)
            .await?
            .into_iter()
            .map(|k| (k, "email"))
            .collect(),
        other => {
            return Err(AppError::Validation(format!(
                "Unsupported unsubscribe kind: {other}"
            )));
        }
    };

    let off: Vec<(&str, &str, bool)> = rows.iter().map(|(k, c)| (k.as_str(), *c, false)).collect();
    write_category(&state.db, user_id, &off).await?;

    Ok(Html(unsubscribe_confirmation_html(kind)))
}

/// Confirmation page shown in the browser after a one-click unsubscribe.
/// Self-contained (inline styles, no assets): it is opened by mail clients
/// and proxies with unpredictable capabilities.
fn unsubscribe_confirmation_html(kind: &str) -> String {
    let label = match kind {
        "digest_weekly" => "résumé hebdomadaire",
        "streak_reminder" => "rappel de série",
        "marketing" => "annonces produit",
        other => other,
    };
    format!(
        r#"<!doctype html>
<html lang="fr"><head><meta charset="utf-8"><title>Désinscrit·e — Skilluv</title>
<style>body{{font-family:system-ui;max-width:540px;margin:80px auto;padding:0 24px;color:#1a1a2e}}h1{{color:#6c5ce7}}</style>
</head><body>
<h1>C'est fait</h1>
<p>Tu ne recevras plus d'emails de type <strong>{label}</strong> de Skilluv.</p>
<p>Les emails liés à ton compte (vérification, sécurité, reçus) continuent d'arriver.</p>
<p>Si tu changes d'avis, tu peux réactiver depuis <a href="https://skill-uv.com/settings/notifications">tes paramètres</a>.</p>
</body></html>"#
    )
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
    // This form carries the category twice: once signed inside the token,
    // once as a query parameter. They must agree, or the link has been
    // tampered with. The path form added by SKI-287 avoids the question
    // entirely by trusting only the token.
    if token_kind != query.kind {
        return Err(AppError::Validation("Token kind mismatch".into()));
    }

    let page = apply_unsubscribe(&state, user_id, &query.kind).await?;
    tracing::info!(user_id = %user_id, kind = %query.kind, "user unsubscribed");
    Ok(page)
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
    let svc = digest::DigestService {
        db: &state.db,
        email: &state.email,
        base_url: &state.config.frontend_url,
        jwt_secret: &state.config.jwt_secret,
    };
    let report = svc.run_weekly().await?;
    Ok(Json(ApiResponse::new(AdminDigestResponse {
        digest: report,
    })))
}

/// Derive the unsubscribe-token HMAC key from JWT_SECRET. Avoids a separate secret in env.
///
/// Public so integration tests can mint the same token an email footer
/// carries, rather than reimplementing the derivation and drifting from it.
pub fn unsub_secret(jwt_secret: &str) -> Vec<u8> {
    use hmac::{Hmac, KeyInit, Mac};
    type HmacSha256 = Hmac<sha2::Sha256>;
    let mut mac =
        HmacSha256::new_from_slice(jwt_secret.as_bytes()).expect("HMAC accepts any key size");
    mac.update(b"skilluv-unsubscribe-v1");
    mac.finalize().into_bytes().to_vec()
}
