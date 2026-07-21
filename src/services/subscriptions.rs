//! Enterprise subscriptions — Phase 4.6.
//!
//! Managed via Stripe Subscriptions today ; other PSPs (Paystack, Flutterwave) have
//! their own subscription APIs but wiring them is deferred to Phase 5.

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct EnterpriseSubscription {
    pub id: Uuid,
    pub enterprise_id: Uuid,
    pub plan_slug: String,
    pub stripe_customer_id: Option<String>,
    pub stripe_subscription_id: Option<String>,
    pub status: String,
    pub current_period_start: Option<DateTime<Utc>>,
    pub current_period_end: Option<DateTime<Utc>>,
    pub cancel_at_period_end: bool,
    pub monthly_credit_grant: i32,
    pub last_grant_period_start: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub async fn active_for(
    db: &PgPool,
    enterprise_id: Uuid,
) -> Result<Option<EnterpriseSubscription>, AppError> {
    let row: Option<EnterpriseSubscription> = sqlx::query_as(
        r#"
        SELECT * FROM enterprise_subscriptions
        WHERE enterprise_id = $1 AND status IN ('trialing', 'active', 'past_due')
        ORDER BY created_at DESC LIMIT 1
        "#,
    )
    .bind(enterprise_id)
    .fetch_optional(db)
    .await?;
    Ok(row)
}

/// Paramètres pour [`upsert_from_stripe`].
#[derive(Debug, Clone)]
pub struct StripeSubscriptionUpsert<'a> {
    pub enterprise_id: Uuid,
    pub plan_slug: &'a str,
    pub stripe_customer_id: Option<&'a str>,
    pub stripe_subscription_id: &'a str,
    pub status: &'a str,
    pub current_period_start: Option<DateTime<Utc>>,
    pub current_period_end: Option<DateTime<Utc>>,
    pub cancel_at_period_end: bool,
    pub monthly_credit_grant: i32,
}

pub async fn upsert_from_stripe(
    db: &PgPool,
    params: StripeSubscriptionUpsert<'_>,
) -> Result<EnterpriseSubscription, AppError> {
    let StripeSubscriptionUpsert {
        enterprise_id,
        plan_slug,
        stripe_customer_id,
        stripe_subscription_id,
        status,
        current_period_start,
        current_period_end,
        cancel_at_period_end,
        monthly_credit_grant,
    } = params;
    let sub: EnterpriseSubscription = sqlx::query_as(
        r#"
        INSERT INTO enterprise_subscriptions
            (enterprise_id, plan_slug, stripe_customer_id, stripe_subscription_id,
             status, current_period_start, current_period_end, cancel_at_period_end,
             monthly_credit_grant)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        ON CONFLICT (stripe_subscription_id) DO UPDATE SET
            status = EXCLUDED.status,
            current_period_start = EXCLUDED.current_period_start,
            current_period_end = EXCLUDED.current_period_end,
            cancel_at_period_end = EXCLUDED.cancel_at_period_end,
            monthly_credit_grant = EXCLUDED.monthly_credit_grant,
            updated_at = NOW()
        RETURNING *
        "#,
    )
    .bind(enterprise_id)
    .bind(plan_slug)
    .bind(stripe_customer_id)
    .bind(stripe_subscription_id)
    .bind(status)
    .bind(current_period_start)
    .bind(current_period_end)
    .bind(cancel_at_period_end)
    .bind(monthly_credit_grant)
    .fetch_one(db)
    .await?;
    Ok(sub)
}

/// Grant the monthly included credits once per period. Idempotent via `last_grant_period_start`.
pub async fn grant_monthly_credits_if_due(
    db: &PgPool,
    sub: &EnterpriseSubscription,
) -> Result<i32, AppError> {
    if sub.monthly_credit_grant <= 0 {
        return Ok(0);
    }
    let Some(period_start) = sub.current_period_start else {
        return Ok(0);
    };
    if sub
        .last_grant_period_start
        .map(|d| d == period_start)
        .unwrap_or(false)
    {
        return Ok(0);
    }
    let amount = crate::services::credits::dec(&sub.monthly_credit_grant.to_string());
    crate::services::credits::grant(
        db,
        crate::services::credits::GrantInput {
            enterprise_id: sub.enterprise_id,
            amount: &amount,
            reason: "subscription_grant",
            related_payment_id: None,
            related_promo_code_id: None,
            notes: Some(&format!("Pipeline {} monthly credits", sub.plan_slug)),
            actor_user_id: None,
            expires_at: None,
        },
    )
    .await?;
    sqlx::query(
        "UPDATE enterprise_subscriptions SET last_grant_period_start = $1, updated_at = NOW() WHERE id = $2",
    )
    .bind(period_start)
    .bind(sub.id)
    .execute(db)
    .await?;
    Ok(sub.monthly_credit_grant)
}
