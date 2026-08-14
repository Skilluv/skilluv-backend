//! Notification settings — what a person is told, and how.
//!
//! The one place a preference is stored. `/users/me/email-preferences`
//! still answers in three words — digest, streak, marketing — because
//! unsubscribe links already delivered speak them, but it is a view over
//! these rows rather than a second table.
//!
//! Rows exist only where someone changed something, so the response merges
//! the catalogue's defaults with the stored overrides. A caller therefore
//! always sees the full picture without the database holding one row per
//! user per kind per channel — tens of millions of rows saying "yes, the
//! default".

use axum::extract::State;
use axum::routing::{get, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::middleware::AuthUser;

pub fn notification_preferences_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/users/me/notification-preferences",
            get(list_preferences).put(update_preferences),
        )
        .route(
            "/users/me/notification-preferences/reset",
            put(reset_preferences),
        )
        // Quiet hours apply across every kind at once, so they are a
        // setting of their own rather than a column on each row.
        .route("/users/me/quiet-hours", put(set_quiet_hours))
}

/// When not to buzz someone's phone.
///
/// Both bounds or neither, and a zone with them. Half a window is a window
/// nobody can interpret, and a window with no zone cannot be placed in
/// time — assuming UTC would silence a talent in Cotonou at the wrong
/// hours for half the year.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct QuietHoursRequest {
    /// Local hour pushes stop, 0-23. `null` with the others clears the
    /// window entirely.
    pub start: Option<i16>,
    /// Local hour they resume, 0-23. A window may wrap midnight.
    pub end: Option<i16>,
    /// IANA name, e.g. `Africa/Porto-Novo`.
    pub timezone: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct QuietHours {
    pub start: Option<i16>,
    pub end: Option<i16>,
    pub timezone: Option<String>,
}

#[utoipa::path(
    put,
    path = "/api/users/me/quiet-hours",
    tag = "profile",
    request_body = QuietHoursRequest,
    responses(
        (status = 200, description = "Quiet hours saved", body = ApiResponse<QuietHours>),
        (status = 400, description = "Incomplete window or unknown timezone", body = crate::api_response::ErrorResponse),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn set_quiet_hours(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<QuietHoursRequest>,
) -> Result<Json<ApiResponse<QuietHours>>, AppError> {
    let clearing = body.start.is_none() && body.end.is_none();

    if !clearing {
        let (Some(start), Some(end)) = (body.start, body.end) else {
            return Err(AppError::Validation(
                "start and end must both be given, or both omitted to clear the window".into(),
            ));
        };
        if !(0..=23).contains(&start) || !(0..=23).contains(&end) {
            return Err(AppError::Validation(
                "hours must be between 0 and 23".into(),
            ));
        }
        if start == end {
            return Err(AppError::Validation(
                "a window that starts and ends at the same hour would silence the whole day".into(),
            ));
        }

        let Some(tz) = body.timezone.as_deref() else {
            return Err(AppError::Validation(
                "a timezone is required — an hour with no zone cannot be placed in time".into(),
            ));
        };
        // Validated here rather than at delivery: a name we cannot parse
        // would make the window silently not apply, which looks exactly
        // like the feature not working.
        if tz.parse::<chrono_tz::Tz>().is_err() {
            return Err(AppError::Validation(format!(
                "unknown timezone '{tz}' — use an IANA name such as Africa/Porto-Novo"
            )));
        }
    }

    let saved: QuietHoursRow = sqlx::query_as(
        "UPDATE users
            SET quiet_hours_start = $2,
                quiet_hours_end = $3,
                timezone = COALESCE($4, timezone)
          WHERE id = $1
      RETURNING quiet_hours_start, quiet_hours_end, timezone",
    )
    .bind(auth.user_id)
    .bind(if clearing { None } else { body.start })
    .bind(if clearing { None } else { body.end })
    .bind(body.timezone.as_deref())
    .fetch_one(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(QuietHours {
        start: saved.quiet_hours_start,
        end: saved.quiet_hours_end,
        timezone: saved.timezone,
    })))
}

#[derive(sqlx::FromRow)]
struct QuietHoursRow {
    quiet_hours_start: Option<i16>,
    quiet_hours_end: Option<i16>,
    timezone: Option<String>,
}

/// One notification kind, with the caller's effective settings.
#[derive(Debug, Serialize, ToSchema)]
pub struct KindPreference {
    /// Dotted identifier, e.g. `payout.sent`.
    pub kind: String,
    /// Grouping for the settings screen: `payments`, `social`, …
    pub category: String,
    /// Human-readable title in the caller's language, so the screen does not
    /// have to carry its own copy of every label.
    pub label: String,
    /// Channels this kind can use at all. A channel absent here cannot be
    /// enabled, whatever the request says.
    pub available_channels: Vec<String>,
    /// Effective setting per channel: the stored choice, else the default.
    pub in_app: bool,
    pub push: bool,
    pub email: bool,
    /// Cannot be switched off. The screen should render these as fixed
    /// rather than as toggles that snap back.
    pub transactional: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PreferencesData {
    pub preferences: Vec<KindPreference>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PreferencesResponse {
    pub data: PreferencesData,
    pub meta: crate::api_response::MetaInfo,
}

#[derive(Debug, sqlx::FromRow)]
struct EffectiveRow {
    kind: String,
    category: String,
    allows_in_app: bool,
    allows_push: bool,
    allows_email: bool,
    in_app: bool,
    push: bool,
    email: bool,
    transactional: bool,
}

/// Everything the caller can be notified about, with their settings applied.
#[utoipa::path(
    get,
    path = "/api/users/me/notification-preferences",
    tag = "profile",
    responses(
        (status = 200, description = "Effective preferences, defaults included", body = PreferencesResponse),
        (status = 401, description = "Not authenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn list_preferences(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, AppError> {
    let locale = caller_locale(&state, auth.user_id).await;

    // The merge happens in SQL: one round trip, and the "stored else
    // default" rule is written once rather than per channel.
    let rows: Vec<EffectiveRow> = sqlx::query_as(
        r#"
        SELECT k.kind,
               k.category,
               k.allows_in_app,
               k.allows_push,
               k.allows_email,
               COALESCE(p_in.enabled, k.default_in_app) AS in_app,
               COALESCE(p_pu.enabled, k.default_push)   AS push,
               COALESCE(p_em.enabled, k.default_email)  AS email,
               k.transactional
          FROM notification_kinds k
          LEFT JOIN notification_preferences p_in
                 ON p_in.kind = k.kind AND p_in.user_id = $1 AND p_in.channel = 'in_app'
          LEFT JOIN notification_preferences p_pu
                 ON p_pu.kind = k.kind AND p_pu.user_id = $1 AND p_pu.channel = 'push'
          LEFT JOIN notification_preferences p_em
                 ON p_em.kind = k.kind AND p_em.user_id = $1 AND p_em.channel = 'email'
         ORDER BY k.category, k.kind
        "#,
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await?;

    let preferences: Vec<KindPreference> = rows
        .into_iter()
        .map(|r| {
            let mut channels = Vec::new();
            if r.allows_in_app {
                channels.push("in_app".to_string());
            }
            if r.allows_push {
                channels.push("push".to_string());
            }
            if r.allows_email {
                channels.push("email".to_string());
            }
            KindPreference {
                label: crate::services::i18n::t(&locale, &format!("notification.{}.title", r.kind)),
                kind: r.kind,
                category: r.category,
                available_channels: channels,
                // A transactional kind reads as on everywhere it is allowed:
                // showing it off would be a lie, since it is sent regardless.
                in_app: r.in_app || (r.transactional && r.allows_in_app),
                push: r.push || (r.transactional && r.allows_push),
                email: r.email || (r.transactional && r.allows_email),
                transactional: r.transactional,
            }
        })
        .collect();

    Ok(Json(serde_json::json!({
        "data": { "preferences": preferences },
        "meta": {
            "request_id": uuid::Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })))
}

/// One change. Absent channels are left as they are.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PreferenceUpdate {
    pub kind: String,
    pub in_app: Option<bool>,
    pub push: Option<bool>,
    pub email: Option<bool>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdatePreferencesRequest {
    pub preferences: Vec<PreferenceUpdate>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UpdateResult {
    pub updated: usize,
    /// Changes refused, with the reason. Reported rather than silently
    /// dropped: a screen that shows a toggle moving while the server ignored
    /// it is worse than an error.
    pub rejected: Vec<String>,
}

/// Change some settings. Partial by design — a screen sends what the person
/// touched, not the whole catalogue.
#[utoipa::path(
    put,
    path = "/api/users/me/notification-preferences",
    tag = "profile",
    request_body = UpdatePreferencesRequest,
    responses(
        (status = 200, description = "Applied, with anything refused listed", body = ApiResponse<UpdateResult>),
        (status = 400, description = "Unknown kind", body = crate::api_response::ErrorResponse),
        (status = 401, description = "Not authenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn update_preferences(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<UpdatePreferencesRequest>,
) -> Result<Json<ApiResponse<UpdateResult>>, AppError> {
    let mut updated = 0usize;
    let mut rejected = Vec::new();

    for change in &body.preferences {
        let kind: Option<(bool, bool, bool, bool)> = sqlx::query_as(
            "SELECT allows_in_app, allows_push, allows_email, transactional
               FROM notification_kinds WHERE kind = $1",
        )
        .bind(&change.kind)
        .fetch_optional(&state.db)
        .await?;

        let Some((allows_in_app, allows_push, allows_email, transactional)) = kind else {
            rejected.push(format!("{}: unknown notification kind", change.kind));
            continue;
        };

        if transactional {
            // Refusing loudly. Someone who opted out of a payout receipt and
            // then did not notice a failed payout would have a legitimate
            // grievance.
            rejected.push(format!(
                "{}: this notification cannot be turned off — it tells you \
                 about money or account access",
                change.kind
            ));
            continue;
        }

        for (channel, wanted, allowed) in [
            ("in_app", change.in_app, allows_in_app),
            ("push", change.push, allows_push),
            ("email", change.email, allows_email),
        ] {
            let Some(enabled) = wanted else { continue };
            if !allowed {
                rejected.push(format!(
                    "{}: {channel} is not available for this notification",
                    change.kind
                ));
                continue;
            }
            sqlx::query(
                r#"
                INSERT INTO notification_preferences (user_id, kind, channel, enabled)
                VALUES ($1, $2, $3, $4)
                ON CONFLICT (user_id, kind, channel)
                DO UPDATE SET enabled = EXCLUDED.enabled, updated_at = NOW()
                "#,
            )
            .bind(auth.user_id)
            .bind(&change.kind)
            .bind(channel)
            .bind(enabled)
            .execute(&state.db)
            .await?;
            updated += 1;
        }
    }

    Ok(Json(ApiResponse::new(UpdateResult { updated, rejected })))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ResetResult {
    pub cleared: u64,
}

/// Drop every override and go back to the defaults.
///
/// Deletes rather than writing the defaults into rows: absence *is* the
/// default, so a later change to a default reaches everyone who never
/// expressed an opinion.
#[utoipa::path(
    put,
    path = "/api/users/me/notification-preferences/reset",
    tag = "profile",
    responses(
        (status = 200, description = "Overrides cleared", body = ApiResponse<ResetResult>),
        (status = 401, description = "Not authenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn reset_preferences(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<ResetResult>>, AppError> {
    let result = sqlx::query("DELETE FROM notification_preferences WHERE user_id = $1")
        .bind(auth.user_id)
        .execute(&state.db)
        .await?;
    Ok(Json(ApiResponse::new(ResetResult {
        cleared: result.rows_affected(),
    })))
}

async fn caller_locale(state: &AppState, user_id: uuid::Uuid) -> String {
    let stored: Option<String> =
        sqlx::query_scalar("SELECT preferred_language FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten()
            .flatten();
    crate::services::i18n::resolve(stored.as_deref(), None)
}
