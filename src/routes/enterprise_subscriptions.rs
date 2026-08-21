//! Enterprise subscriptions endpoints — Phase 4.6 (finalisation).
//!
//! POST /api/enterprise/subscriptions/subscribe   {plan_slug}   → checkout Stripe
//! GET  /api/enterprise/subscriptions/current                   statut + prochaine facture
//! POST /api/enterprise/subscriptions/cancel                     cancel at period end

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::middleware::AuthUser;

pub fn enterprise_subscription_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/enterprise/subscriptions/subscribe",
            post(subscribe_to_pipeline),
        )
        .route(
            "/enterprise/subscriptions/current",
            get(current_subscription),
        )
        .route(
            "/enterprise/subscriptions/cancel",
            post(cancel_subscription),
        )
}

#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = EnterpriseSubscriptionsSubscribeBody)]
pub struct SubscribeBody {
    /// Slug of the subscription pack (see `packs` table where
    /// `kind = 'subscription'`).
    #[schema(max_length = 10000)]
    pub plan_slug: String,
}

/// Response of the subscribe endpoint. When the enterprise already has
/// an active subscription, `message` + `current_plan` + `status` are
/// populated; otherwise `checkout_url` + `session_id` for the Stripe
/// flow.
#[derive(Debug, Serialize, ToSchema)]
#[schema(as = EnterpriseSubscriptionsSubscribeResponse)]
pub struct SubscribeResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_plan: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkout_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SubscriptionDetail {
    pub id: Uuid,
    pub plan_slug: String,
    /// `active`, `trialing`, `past_due`, `cancelled`, `unpaid`.
    pub status: String,
    pub current_period_start: Option<chrono::DateTime<chrono::Utc>>,
    pub current_period_end: Option<chrono::DateTime<chrono::Utc>>,
    pub cancel_at_period_end: bool,
    pub monthly_credit_grant: i32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CurrentSubscriptionResponse {
    /// `None` when the enterprise has no active subscription.
    pub subscription: Option<SubscriptionDetail>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CancelSubscriptionResponse {
    pub cancel_at_period_end: bool,
    pub current_period_end: Option<chrono::DateTime<chrono::Utc>>,
}

/// Kick off a Stripe checkout for a subscription pack. Refuses if the
/// enterprise already has an active subscription (returns the current
/// plan info instead — front routes to the manage-subscription screen).
#[utoipa::path(
    post,
    path = "/api/enterprise/subscriptions/subscribe",
    tag = "wallet",
    request_body = SubscribeBody,
    responses(
        (status = 200, description = "Checkout URL OR already-subscribed marker", body = ApiResponse<SubscribeResponse>),
        (status = 400, description = "plan_slug is not a subscription pack", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Caller has no enterprise", body = crate::api_response::ErrorResponse),
        (status = 500, description = "Stripe not configured", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn subscribe_to_pipeline(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<SubscribeBody>,
) -> Result<Json<ApiResponse<SubscribeResponse>>, AppError> {
    // SKI-66 — owner-only gate. Was previously `current_enterprise_for`
    // which admitted any enterprise MEMBER; that caused non-owner
    // enterprise members to hit these endpoints, get a 401 downstream,
    // and see the front redirect them to /auth/login instead of a clean
    // 403. The gate below enforces owner status + returns Forbidden with
    // a stable shape so the front can distinguish "not signed in" from
    // "signed in but not authorised".
    let enterprise = crate::routes::enterprise::require_enterprise_owner_pub(&state, &auth).await?;
    let enterprise_id = enterprise.id;
    // Vérifier qu'il n'y a pas déjà un abo actif.
    let existing = crate::services::subscriptions::active_for(&state.db, enterprise_id).await?;
    if let Some(sub) = existing {
        return Ok(Json(ApiResponse::new(SubscribeResponse {
            message: Some("already subscribed".to_string()),
            current_plan: Some(sub.plan_slug),
            status: Some(sub.status),
            checkout_url: None,
            session_id: None,
        })));
    }
    // Vérifier que le pack existe en kind='subscription'
    let pack = crate::services::fx::pack_by_slug(&state.db, &body.plan_slug).await?;
    if pack.kind != "subscription" {
        return Err(AppError::Validation(
            "plan_slug does not reference a subscription pack".into(),
        ));
    }
    let cfg = crate::services::stripe::StripeConfig::from_env().ok_or(
        AppError::ServiceUnavailable("payments are not configured on this deployment".into()),
    )?;
    let email = sqlx::query_scalar::<_, String>("SELECT email FROM users WHERE id = $1")
        .bind(auth.user_id)
        .fetch_one(&state.db)
        .await?;
    let session = crate::services::stripe::create_subscription_checkout(
        &cfg,
        &format!("skilluv_sub_{}", body.plan_slug),
        &email,
        &enterprise_id.to_string(),
        &[
            ("purpose", "subscription".to_string()),
            ("enterprise_id", enterprise_id.to_string()),
            ("plan_slug", body.plan_slug.clone()),
            ("monthly_credit_grant", pack.credit_count.to_string()),
        ],
    )
    .await?;
    metrics::counter!(
        "skilluv_subscriptions_checkout_created",
        "plan" => body.plan_slug.clone()
    )
    .increment(1);
    Ok(Json(ApiResponse::new(SubscribeResponse {
        message: None,
        current_plan: None,
        status: None,
        checkout_url: Some(session.checkout_url),
        session_id: Some(session.session_id),
    })))
}

/// Read the caller enterprise's current subscription (or `null`).
#[utoipa::path(
    get,
    path = "/api/enterprise/subscriptions/current",
    tag = "wallet",
    responses(
        (status = 200, description = "Current subscription detail", body = ApiResponse<CurrentSubscriptionResponse>),
        (status = 403, description = "Caller has no enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn current_subscription(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<CurrentSubscriptionResponse>>, AppError> {
    // SKI-66 — owner-only gate. Was previously `current_enterprise_for`
    // which admitted any enterprise MEMBER; that caused non-owner
    // enterprise members to hit these endpoints, get a 401 downstream,
    // and see the front redirect them to /auth/login instead of a clean
    // 403. The gate below enforces owner status + returns Forbidden with
    // a stable shape so the front can distinguish "not signed in" from
    // "signed in but not authorised".
    let enterprise = crate::routes::enterprise::require_enterprise_owner_pub(&state, &auth).await?;
    let enterprise_id = enterprise.id;
    let sub = crate::services::subscriptions::active_for(&state.db, enterprise_id).await?;
    let Some(sub) = sub else {
        return Ok(Json(ApiResponse::new(CurrentSubscriptionResponse {
            subscription: None,
        })));
    };
    Ok(Json(ApiResponse::new(CurrentSubscriptionResponse {
        subscription: Some(SubscriptionDetail {
            id: sub.id,
            plan_slug: sub.plan_slug,
            status: sub.status,
            current_period_start: sub.current_period_start,
            current_period_end: sub.current_period_end,
            cancel_at_period_end: sub.cancel_at_period_end,
            monthly_credit_grant: sub.monthly_credit_grant,
        }),
    })))
}

/// Mark the subscription for cancellation at the end of the current
/// period. The Stripe webhook `customer.subscription.updated` will
/// reconcile the final state.
#[utoipa::path(
    post,
    path = "/api/enterprise/subscriptions/cancel",
    tag = "wallet",
    responses(
        (status = 200, description = "Cancellation scheduled", body = ApiResponse<CancelSubscriptionResponse>),
        (status = 403, description = "Caller has no enterprise", body = crate::api_response::ErrorResponse),
        (status = 404, description = "No active subscription", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
    operation_id = "enterpriseSubscriptionsCancelSubscription",
)]
pub async fn cancel_subscription(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<CancelSubscriptionResponse>>, AppError> {
    // SKI-66 — owner-only gate. Was previously `current_enterprise_for`
    // which admitted any enterprise MEMBER; that caused non-owner
    // enterprise members to hit these endpoints, get a 401 downstream,
    // and see the front redirect them to /auth/login instead of a clean
    // 403. The gate below enforces owner status + returns Forbidden with
    // a stable shape so the front can distinguish "not signed in" from
    // "signed in but not authorised".
    let enterprise = crate::routes::enterprise::require_enterprise_owner_pub(&state, &auth).await?;
    let enterprise_id = enterprise.id;
    let sub = crate::services::subscriptions::active_for(&state.db, enterprise_id).await?;
    let Some(sub) = sub else {
        return Err(AppError::NotFound("no active subscription".into()));
    };
    // Marquer localement, l'appel Stripe subscription cancel étant deferred.
    // Le webhook customer.subscription.updated reflétera l'état final.
    sqlx::query(
        "UPDATE enterprise_subscriptions SET cancel_at_period_end = TRUE, updated_at = NOW() WHERE id = $1",
    )
    .bind(sub.id)
    .execute(&state.db)
    .await?;
    metrics::counter!("skilluv_subscriptions_cancel_requested_total").increment(1);
    Ok(Json(ApiResponse::new(CancelSubscriptionResponse {
        cancel_at_period_end: true,
        current_period_end: sub.current_period_end,
    })))
}
