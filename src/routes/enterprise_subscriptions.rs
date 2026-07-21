//! Enterprise subscriptions endpoints — Phase 4.6 (finalisation).
//!
//! POST /api/enterprise/subscriptions/subscribe   {plan_slug}   → checkout Stripe
//! GET  /api/enterprise/subscriptions/current                   statut + prochaine facture
//! POST /api/enterprise/subscriptions/cancel                     cancel at period end

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
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

fn build_response(data: Value) -> Value {
    json!({
        "data": data,
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })
}

async fn current_enterprise_for(db: &sqlx::PgPool, user_id: Uuid) -> Result<Uuid, AppError> {
    let row: Option<(Uuid,)> = sqlx::query_as(
        "SELECT enterprise_id FROM enterprise_members WHERE user_id = $1 AND status = 'active' LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(db)
    .await?;
    row.map(|(id,)| id).ok_or(AppError::Forbidden)
}

#[derive(Deserialize)]
struct SubscribeBody {
    plan_slug: String,
}

async fn subscribe_to_pipeline(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<SubscribeBody>,
) -> Result<Json<Value>, AppError> {
    let enterprise_id = current_enterprise_for(&state.db, auth.user_id).await?;
    // Vérifier qu'il n'y a pas déjà un abo actif.
    let existing = crate::services::subscriptions::active_for(&state.db, enterprise_id).await?;
    if let Some(sub) = existing {
        return Ok(Json(build_response(json!({
            "message": "already subscribed",
            "current_plan": sub.plan_slug,
            "status": sub.status,
        }))));
    }
    // Vérifier que le pack existe en kind='subscription'
    let pack = crate::services::fx::pack_by_slug(&state.db, &body.plan_slug).await?;
    if pack.kind != "subscription" {
        return Err(AppError::Validation(
            "plan_slug does not reference a subscription pack".into(),
        ));
    }
    let cfg = crate::services::stripe::StripeConfig::from_env()
        .ok_or(AppError::Internal("Stripe not configured".into()))?;
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
    Ok(Json(build_response(json!({
        "checkout_url": session.checkout_url,
        "session_id": session.session_id,
    }))))
}

async fn current_subscription(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let enterprise_id = current_enterprise_for(&state.db, auth.user_id).await?;
    let sub = crate::services::subscriptions::active_for(&state.db, enterprise_id).await?;
    let Some(sub) = sub else {
        return Ok(Json(build_response(json!({ "subscription": null }))));
    };
    Ok(Json(build_response(json!({
        "subscription": {
            "id": sub.id,
            "plan_slug": sub.plan_slug,
            "status": sub.status,
            "current_period_start": sub.current_period_start,
            "current_period_end": sub.current_period_end,
            "cancel_at_period_end": sub.cancel_at_period_end,
            "monthly_credit_grant": sub.monthly_credit_grant,
        }
    }))))
}

async fn cancel_subscription(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let enterprise_id = current_enterprise_for(&state.db, auth.user_id).await?;
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
    Ok(Json(build_response(json!({
        "cancel_at_period_end": true,
        "current_period_end": sub.current_period_end,
    }))))
}
