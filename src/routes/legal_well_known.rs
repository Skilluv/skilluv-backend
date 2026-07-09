//! Legal & responsible disclosure endpoints (Phase 3.17 + 3.13 + 3.18).

use axum::extract::State;
use axum::http::header;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use chrono::Datelike;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;

pub fn well_known_routes() -> Router<AppState> {
    Router::new()
        .route("/.well-known/security.txt", get(security_txt))
        .route("/security.txt", get(security_txt))
        .route("/api/admin/accounting/export", get(admin_accounting_export))
        .route("/api/enterprise/dashboard/overview", get(dashboard_overview))
        .route("/api/enterprise/dashboard/funnel", get(dashboard_funnel))
}

// ─── security.txt (RFC 9116) ─────────────────────────────────────

async fn security_txt() -> impl IntoResponse {
    let one_year_ahead = chrono::Utc::now() + chrono::Duration::days(365);
    let body = format!(
        "Contact: mailto:security@skilluv.com\n\
         Expires: {}\n\
         Preferred-Languages: fr, en\n\
         Policy: https://skilluv.com/legal/security\n\
         Acknowledgments: https://skilluv.com/legal/security#hall-of-fame\n\
         Canonical: https://skilluv.com/.well-known/security.txt\n",
        one_year_ahead.to_rfc3339()
    );
    (
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        body,
    )
}

// ─── Accounting export (3.18) ────────────────────────────────────

async fn admin_accounting_export(
    State(state): State<AppState>,
    auth: AuthUser,
    axum::extract::Query(q): axum::extract::Query<AccountingQuery>,
) -> Result<impl IntoResponse, AppError> {
    if auth.role != "admin" {
        return Err(AppError::Forbidden);
    }
    let year = q.year.unwrap_or_else(|| chrono::Utc::now().year());
    let month = q.month.unwrap_or_else(|| chrono::Utc::now().month() as i32);
    let rows: Vec<sqlx::postgres::PgRow> = sqlx::query(
        r#"
        SELECT i.invoice_number, i.issued_at, e.company_name, e.country,
               i.amount_ht_cents, i.amount_tva_cents, i.amount_ttc_cents,
               i.tva_rate, i.currency, i.stripe_payment_intent_id
        FROM invoices i
        JOIN enterprises e ON e.id = i.enterprise_id
        WHERE EXTRACT(YEAR FROM i.issued_at) = $1
          AND EXTRACT(MONTH FROM i.issued_at) = $2
        ORDER BY i.issued_at
        "#,
    )
    .bind(year)
    .bind(month)
    .fetch_all(&state.db)
    .await?;
    use sqlx::Row;
    let mut csv = String::from(
        "invoice_number;issued_at;company;country;amount_ht;amount_tva;amount_ttc;tva_rate;currency;payment_intent\n",
    );
    for r in &rows {
        let line = format!(
            "{};{};{};{};{};{};{};{};{};{}\n",
            r.get::<String, _>("invoice_number"),
            r.get::<chrono::DateTime<chrono::Utc>, _>("issued_at").format("%Y-%m-%d"),
            r.get::<String, _>("company_name").replace(';', ","),
            r.get::<Option<String>, _>("country").unwrap_or_default(),
            r.get::<i64, _>("amount_ht_cents") as f64 / 100.0,
            r.get::<i64, _>("amount_tva_cents") as f64 / 100.0,
            r.get::<i64, _>("amount_ttc_cents") as f64 / 100.0,
            r.get::<bigdecimal::BigDecimal, _>("tva_rate"),
            r.get::<String, _>("currency"),
            r.get::<Option<String>, _>("stripe_payment_intent_id").unwrap_or_default(),
        );
        csv.push_str(&line);
    }
    let filename = format!("skilluv-accounting-{year:04}-{month:02}.csv");
    Ok((
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{filename}\""),
            ),
        ],
        csv,
    ))
}

#[derive(serde::Deserialize)]
struct AccountingQuery {
    year: Option<i32>,
    month: Option<i32>,
}

// ─── Enterprise dashboard (3.13) ─────────────────────────────────

async fn current_enterprise_for(
    db: &sqlx::PgPool,
    user_id: Uuid,
) -> Result<Uuid, AppError> {
    let row: Option<(Uuid,)> = sqlx::query_as(
        "SELECT enterprise_id FROM enterprise_members WHERE user_id = $1 AND status = 'active' LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(db)
    .await?;
    row.map(|(id,)| id).ok_or(AppError::Forbidden)
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

async fn dashboard_overview(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let enterprise_id = current_enterprise_for(&state.db, auth.user_id).await?;
    let credits = crate::services::credits::get_or_init_credits(&state.db, enterprise_id).await?;

    let interest_stats: (i64, i64, i64, i64) = sqlx::query_as(
        r#"
        SELECT
            COUNT(*) FILTER (WHERE status = 'pending')::BIGINT,
            COUNT(*) FILTER (WHERE status = 'accepted')::BIGINT,
            COUNT(*) FILTER (WHERE status = 'declined')::BIGINT,
            COUNT(*)::BIGINT
        FROM interest_requests WHERE enterprise_id = $1
        "#,
    )
    .bind(enterprise_id)
    .fetch_one(&state.db)
    .await?;

    let last_30d: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM interest_requests WHERE enterprise_id = $1 AND created_at > NOW() - INTERVAL '30 days'",
    )
    .bind(enterprise_id)
    .fetch_one(&state.db)
    .await?;

    let active_conversations: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM conversations WHERE enterprise_id = $1 AND closed = FALSE",
    )
    .bind(enterprise_id)
    .fetch_one(&state.db)
    .await?;

    let sponsored_live: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM challenges
        WHERE sponsor_enterprise_id = $1 AND sponsor_visible_until > NOW()
        "#,
    )
    .bind(enterprise_id)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(build_response(json!({
        "credits": {
            "balance": crate::services::credits::balance_as_f64(&credits.balance),
            "total_purchased": credits.total_purchased,
            "total_used": crate::services::credits::balance_as_f64(&credits.total_used),
            "total_refunded": crate::services::credits::balance_as_f64(&credits.total_refunded),
        },
        "interest_requests": {
            "pending": interest_stats.0,
            "accepted": interest_stats.1,
            "declined": interest_stats.2,
            "total": interest_stats.3,
            "last_30d": last_30d,
        },
        "conversations_active": active_conversations,
        "sponsored_challenges_live": sponsored_live,
    }))))
}

async fn dashboard_funnel(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let enterprise_id = current_enterprise_for(&state.db, auth.user_id).await?;
    let row: (i64, i64, i64, i64) = sqlx::query_as(
        r#"
        SELECT
            COUNT(*)::BIGINT AS sent,
            COUNT(*) FILTER (WHERE status = 'accepted')::BIGINT AS accepted,
            (SELECT COUNT(*) FROM conversations WHERE enterprise_id = $1)::BIGINT AS conversations,
            (SELECT COUNT(*) FROM messages m
              JOIN conversations c ON c.id = m.conversation_id
              WHERE c.enterprise_id = $1 AND m.sender_id <> c.talent_id)::BIGINT AS messages_sent
        FROM interest_requests WHERE enterprise_id = $1
        "#,
    )
    .bind(enterprise_id)
    .fetch_one(&state.db)
    .await?;

    let accept_rate = if row.0 > 0 {
        (row.1 as f64) / (row.0 as f64) * 100.0
    } else {
        0.0
    };

    Ok(Json(build_response(json!({
        "funnel": {
            "interest_requests_sent": row.0,
            "interest_requests_accepted": row.1,
            "conversations_started": row.2,
            "messages_sent_by_enterprise": row.3,
            "accept_rate_pct": (accept_rate * 100.0).round() / 100.0,
        }
    }))))
}
