//! What each enterprise has with us.
//!
//! Two audiences. An administrator records and reads engagements across every
//! company; an enterprise reads its own. The same table answers both, and the
//! difference is one predicate — which is the point of having the table at
//! all.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use bigdecimal::BigDecimal;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;

pub fn enterprise_product_routes() -> Router<AppState> {
    Router::new()
        .route("/enterprise/products", get(my_products))
        .route("/enterprise/product-types", get(list_types))
}

/// Admin surface, mounted behind the admin gate.
pub fn admin_enterprise_product_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/admin/enterprises/{id}/products",
            get(products_of).post(record_product),
        )
        .route("/admin/enterprise-products/{id}/status", post(set_status))
        .route("/admin/enterprise-products/renewals", get(renewals))
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

/// One line of the renewal list, as the query returns it.
#[derive(sqlx::FromRow)]
struct RenewalRow {
    id: Uuid,
    company_name: String,
    product_type: String,
    product_label: String,
    renews_at: chrono::DateTime<chrono::Utc>,
    contract_value: Option<BigDecimal>,
    currency: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct EnterpriseProduct {
    pub id: Uuid,
    pub enterprise_id: Uuid,
    pub product_type: String,
    pub product_label: String,
    pub status: String,
    #[schema(value_type = Option<String>)]
    pub contract_value: Option<BigDecimal>,
    pub currency: Option<String>,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub renews_at: Option<chrono::DateTime<chrono::Utc>>,
    pub ended_at: Option<chrono::DateTime<chrono::Utc>>,
    pub ended_reason: Option<String>,
    pub notes: Option<String>,
}

const PRODUCT_SELECT: &str = r#"
    SELECT p.id, p.enterprise_id, p.product_type, t.label AS product_label,
           p.status, p.contract_value, p.currency, p.started_at,
           p.renews_at, p.ended_at, p.ended_reason, p.notes
      FROM enterprise_products p
      JOIN enterprise_product_types t ON t.slug = p.product_type
"#;

/// The catalogue: what an enterprise can have, and how each one is billed.
#[utoipa::path(
    get, path = "/api/enterprise/product-types", tag = "enterprise",
    responses((status = 200, body = serde_json::Value)),
    operation_id = "enterpriseProductsListTypes",
)]
pub async fn list_types(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let types: Vec<(String, String, String, Option<String>, bool)> = sqlx::query_as(
        "SELECT slug, label, description, revenue_stream, recurring
           FROM enterprise_product_types ORDER BY slug",
    )
    .fetch_all(&state.db)
    .await?;

    let types: Vec<Value> = types
        .into_iter()
        .map(|(slug, label, description, stream, recurring)| {
            json!({
                "slug": slug,
                "label": label,
                "description": description,
                "revenue_stream": stream,
                "recurring": recurring,
            })
        })
        .collect();
    Ok(Json(build_response(json!({ "product_types": types }))))
}

/// Everything the caller's enterprise has with us.
#[utoipa::path(
    get, path = "/api/enterprise/products", tag = "enterprise",
    responses(
        (status = 200, body = serde_json::Value),
        (status = 403, description = "Not acting for an enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn my_products(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
    let products = fetch_products(&state, enterprise.id).await?;
    Ok(Json(build_response(json!({ "products": products }))))
}

async fn fetch_products(
    state: &AppState,
    enterprise_id: Uuid,
) -> Result<Vec<EnterpriseProduct>, AppError> {
    let sql = format!(
        "{PRODUCT_SELECT} WHERE p.enterprise_id = $1
         ORDER BY (p.status = 'active') DESC, p.started_at DESC"
    );
    let rows = sqlx::query_as::<_, EnterpriseProduct>(sqlx::AssertSqlSafe(sql))
        .bind(enterprise_id)
        .fetch_all(&state.db)
        .await?;
    Ok(rows)
}

/// Admin: one company's engagements.
#[utoipa::path(
    get, path = "/api/admin/enterprises/{id}/products", tag = "admin",
    params(("id" = Uuid, Path, description = "Enterprise id")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 403, description = "Not an administrator", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn products_of(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(enterprise_id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let products = fetch_products(&state, enterprise_id).await?;
    Ok(Json(build_response(json!({ "products": products }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RecordProductBody {
    #[schema(max_length = 60)]
    pub product_type: String,
    /// Required for anything that renews: without it the engagement never
    /// appears on a renewal list and lapses because nobody was told to ask.
    pub renews_at: Option<chrono::DateTime<chrono::Utc>>,
    #[schema(value_type = Option<String>)]
    pub contract_value: Option<BigDecimal>,
    #[schema(max_length = 3)]
    pub currency: Option<String>,
    #[schema(max_length = 60)]
    pub source_table: Option<String>,
    pub source_id: Option<Uuid>,
    #[schema(max_length = 4000)]
    pub notes: Option<String>,
}

/// Admin: record an engagement.
#[utoipa::path(
    post, path = "/api/admin/enterprises/{id}/products", tag = "admin",
    params(("id" = Uuid, Path, description = "Enterprise id")),
    request_body = RecordProductBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Unknown product, or a renewing one with no date", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn record_product(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(enterprise_id): Path<Uuid>,
    Json(body): Json<RecordProductBody>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;

    let product: Option<(String, bool)> =
        sqlx::query_as("SELECT label, recurring FROM enterprise_product_types WHERE slug = $1")
            .bind(&body.product_type)
            .fetch_optional(&state.db)
            .await?;
    let (label, recurring) = product
        .ok_or_else(|| AppError::NotFound(format!("no product type '{}'", body.product_type)))?;

    // The trigger enforces this too, in words a database writes. Said here
    // first, in words the person filling in the form can act on.
    if recurring && body.renews_at.is_none() {
        return Err(AppError::Validation(format!(
            "{label} renews — say when, or it lapses because nobody was told to ask"
        )));
    }
    if let Some(notes) = &body.notes {
        crate::validators::check_max_len(notes, "notes", 4000)?;
    }

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO enterprise_products
            (enterprise_id, product_type, renews_at, contract_value, currency,
             source_table, source_id, notes, created_by)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
         RETURNING id",
    )
    .bind(enterprise_id)
    .bind(&body.product_type)
    .bind(body.renews_at)
    .bind(body.contract_value.as_ref())
    .bind(body.currency.as_deref())
    .bind(body.source_table.as_deref())
    .bind(body.source_id)
    .bind(body.notes.as_deref())
    .bind(auth.user_id)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(build_response(json!({ "product_id": id }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ProductStatusBody {
    /// `active`, `completed`, `cancelled` or `lapsed`.
    #[schema(max_length = 20)]
    pub status: String,
    /// Required to cancel.
    #[schema(max_length = 2000)]
    pub reason: Option<String>,
    /// Push the next renewal out. Only meaningful while active.
    pub renews_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Admin: move an engagement along.
#[utoipa::path(
    post, path = "/api/admin/enterprise-products/{id}/status", tag = "admin",
    params(("id" = Uuid, Path, description = "Engagement id")),
    request_body = ProductStatusBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Unknown status, or cancelled with no reason", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
    operation_id = "enterpriseProductsSetStatus",
)]
pub async fn set_status(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ProductStatusBody>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;

    if !matches!(
        body.status.as_str(),
        "pending" | "active" | "completed" | "cancelled" | "lapsed"
    ) {
        return Err(AppError::Validation(
            "status must be pending, active, completed, cancelled or lapsed".into(),
        ));
    }
    let reason = body
        .reason
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if body.status == "cancelled" && reason.is_none() {
        return Err(AppError::Validation(
            "stopping early requires a reason — the next person needs it exactly then".into(),
        ));
    }

    let updated = sqlx::query(
        "UPDATE enterprise_products
            SET status = $2,
                ended_reason = COALESCE($3, ended_reason),
                renews_at = COALESCE($4, renews_at),
                ended_at = CASE WHEN $2 IN ('completed', 'cancelled', 'lapsed')
                                THEN COALESCE(ended_at, NOW()) ELSE NULL END
          WHERE id = $1",
    )
    .bind(id)
    .bind(&body.status)
    .bind(reason)
    .bind(body.renews_at)
    .execute(&state.db)
    .await?;

    if updated.rows_affected() == 0 {
        return Err(AppError::NotFound("engagement not found".into()));
    }
    Ok(Json(build_response(json!({ "status": body.status }))))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct RenewalQuery {
    /// How far ahead to look. Defaults to sixty days, which is roughly the
    /// notice most annual contracts need.
    #[serde(default = "default_horizon")]
    #[param(minimum = 1, maximum = 365)]
    pub within_days: i32,
}

fn default_horizon() -> i32 {
    60
}

/// Admin: what is coming up for renewal.
///
/// The list this table exists for. Overdue renewals are included and sort
/// first: a renewal date that has passed with the engagement still active is
/// the case somebody most needs to see.
#[utoipa::path(
    get, path = "/api/admin/enterprise-products/renewals", tag = "admin",
    params(RenewalQuery),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 403, description = "Not an administrator", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
    operation_id = "enterpriseProductsRenewals",
)]
pub async fn renewals(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<RenewalQuery>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    if !(1..=365).contains(&q.within_days) {
        return Err(AppError::Validation(
            "within_days must be between 1 and 365".into(),
        ));
    }

    let rows = sqlx::query_as::<_, RenewalRow>(
        "SELECT p.id, e.company_name, p.product_type,
                t.label AS product_label, p.renews_at,
                p.contract_value, p.currency
           FROM enterprise_products p
           JOIN enterprises e ON e.id = p.enterprise_id
           JOIN enterprise_product_types t ON t.slug = p.product_type
          WHERE p.status = 'active'
            AND p.renews_at IS NOT NULL
            AND p.renews_at < NOW() + ($1 || ' days')::INTERVAL
          ORDER BY p.renews_at ASC",
    )
    .bind(q.within_days.to_string())
    .fetch_all(&state.db)
    .await?;

    let now = chrono::Utc::now();
    let renewals: Vec<Value> = rows
        .into_iter()
        .map(|r| {
            json!({
                "id": r.id,
                "company_name": r.company_name,
                "product_type": r.product_type,
                "product_label": r.product_label,
                "renews_at": r.renews_at,
                // Said rather than left to be worked out from two dates.
                "overdue": r.renews_at < now,
                "contract_value": r.contract_value,
                "currency": r.currency,
            })
        })
        .collect();

    Ok(Json(build_response(json!({
        "renewals": renewals,
        "within_days": q.within_days,
    }))))
}
