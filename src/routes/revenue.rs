//! What the platform earns, by stream and by pillar.
//!
//! Admin only. The catalogue itself is not a secret — the business model is
//! public in `docs/` — but the figures are, and splitting the two into
//! separate endpoints would mean two places to get the authorisation wrong.

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use bigdecimal::BigDecimal;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;

pub fn revenue_routes() -> Router<AppState> {
    Router::new()
        .route("/admin/revenue/streams", get(list_streams))
        .route("/admin/revenue/by-pillar", get(by_pillar))
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

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct RevenueStream {
    pub slug: String,
    pub pillar: String,
    pub label: String,
    pub description: String,
    pub recurring: bool,
    /// False until something has actually booked revenue under it.
    pub is_live: bool,
    /// How much, over the window asked for. Serialised as a decimal string
    /// rather than a float: money that round-trips through an IEEE double is
    /// money that stops adding up.
    #[schema(value_type = String)]
    pub amount: BigDecimal,
    pub entries: i64,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct WindowQuery {
    /// How many days back to total. Defaults to 365 — a business with
    /// seasonal revenue reads nothing useful from thirty.
    #[serde(default = "default_days")]
    #[param(minimum = 1, maximum = 3650)]
    pub days: i32,
}

fn default_days() -> i32 {
    365
}

/// Every stream, with what it has actually earned.
///
/// Streams with nothing booked are included and marked `is_live: false`. A
/// catalogue that hid them would read as a business with twenty-seven live
/// revenue lines, which is the number of ideas, not the number of streams.
#[utoipa::path(
    get, path = "/api/admin/revenue/streams", tag = "admin",
    params(WindowQuery),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 403, description = "Not an administrator", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn list_streams(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<WindowQuery>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    if !(1..=3650).contains(&q.days) {
        return Err(AppError::Validation(
            "days must be between 1 and 3650".into(),
        ));
    }

    let streams = sqlx::query_as::<_, RevenueStream>(
        r#"
        SELECT s.slug, s.pillar, s.label, s.description, s.recurring, s.is_live,
               COALESCE(sum(r.amount_credits), 0) AS amount,
               count(r.id) AS entries
          FROM revenue_streams s
          LEFT JOIN platform_revenues r
                 ON r.source = s.slug
                AND r.created_at > NOW() - ($1 || ' days')::INTERVAL
         GROUP BY s.slug, s.pillar, s.label, s.description, s.recurring, s.is_live
         ORDER BY sum(r.amount_credits) DESC NULLS LAST, s.pillar, s.slug
        "#,
    )
    .bind(q.days.to_string())
    .fetch_all(&state.db)
    .await?;

    let live = streams.iter().filter(|s| s.is_live).count();
    Ok(Json(build_response(json!({
        "streams": streams,
        "window_days": q.days,
        // Said plainly. The gap between these two is the honest measure of
        // how much of the business model is a business and how much is a plan.
        "live_streams": live,
        "planned_streams": streams.len() - live,
    }))))
}

/// Totals by pillar, which is how the business model is actually argued
/// about.
#[utoipa::path(
    get, path = "/api/admin/revenue/by-pillar", tag = "admin",
    params(WindowQuery),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 403, description = "Not an administrator", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn by_pillar(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<WindowQuery>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;

    let rows: Vec<(String, BigDecimal, BigDecimal, i64)> = sqlx::query_as(
        r#"
        SELECT s.pillar,
               COALESCE(sum(r.amount_credits), 0) AS total,
               -- Split out, because a business that reads its one-off
               -- revenue as run-rate is a business that overstates itself.
               COALESCE(sum(r.amount_credits) FILTER (WHERE s.recurring), 0) AS recurring,
               count(r.id) AS entries
          FROM revenue_streams s
          LEFT JOIN platform_revenues r
                 ON r.source = s.slug
                AND r.created_at > NOW() - ($1 || ' days')::INTERVAL
         GROUP BY s.pillar
         ORDER BY sum(r.amount_credits) DESC NULLS LAST, s.pillar
        "#,
    )
    .bind(q.days.max(1).to_string())
    .fetch_all(&state.db)
    .await?;

    let pillars: Vec<Value> = rows
        .into_iter()
        .map(|(pillar, total, recurring, entries)| {
            json!({
                "pillar": pillar,
                "total": total,
                "recurring": recurring,
                "entries": entries,
            })
        })
        .collect();

    Ok(Json(build_response(json!({
        "pillars": pillars,
        "window_days": q.days,
    }))))
}
