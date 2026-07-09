//! Enterprise credits service — Phase 3 monétisation.
//!
//! Atomic balance operations. Every change is journaled in `credit_transactions`.
//! Designed to be safe under concurrent spend (the spend path uses an atomic
//! `UPDATE ... WHERE balance >= amount` + check rows affected).

use bigdecimal::BigDecimal;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use num_traits::ToPrimitive;
use serde::Serialize;
use sqlx::PgPool;
use std::str::FromStr;
use uuid::Uuid;

use crate::errors::AppError;

pub const SIGNUP_BONUS_CREDITS: &str = "1";
pub const SIGNUP_BONUS_TTL_DAYS: i64 = 30;
pub const SPEND_INTEREST_REQUEST_AMOUNT: &str = "1";
pub const REFUND_RATIO_PARTIAL: &str = "0.5"; // 50% refund on refused/timeout

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct CreditsRow {
    pub enterprise_id: Uuid,
    pub balance: BigDecimal,
    pub total_purchased: i32,
    pub total_used: BigDecimal,
    pub total_refunded: BigDecimal,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct CreditTransaction {
    pub id: Uuid,
    pub enterprise_id: Uuid,
    pub delta: BigDecimal,
    pub balance_after: BigDecimal,
    pub reason: String,
    pub related_interest_request_id: Option<Uuid>,
    pub related_payment_id: Option<Uuid>,
    pub related_promo_code_id: Option<Uuid>,
    pub notes: Option<String>,
    pub actor_user_id: Option<Uuid>,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

pub fn dec(s: &str) -> BigDecimal {
    BigDecimal::from_str(s).expect("compile-time constant")
}

pub fn dec_from_f64(v: f64) -> BigDecimal {
    BigDecimal::from_str(&format!("{:.2}", v)).expect("formatted f64")
}

// ─── Read ─────────────────────────────────────────────────────────

pub async fn get_or_init_credits(
    db: &PgPool,
    enterprise_id: Uuid,
) -> Result<CreditsRow, AppError> {
    let row: CreditsRow = sqlx::query_as(
        r#"
        INSERT INTO enterprise_credits (enterprise_id) VALUES ($1)
        ON CONFLICT (enterprise_id) DO UPDATE SET enterprise_id = enterprise_credits.enterprise_id
        RETURNING *
        "#,
    )
    .bind(enterprise_id)
    .fetch_one(db)
    .await?;
    Ok(row)
}

pub async fn list_transactions(
    db: &PgPool,
    enterprise_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<Vec<CreditTransaction>, AppError> {
    let rows = sqlx::query_as(
        r#"
        SELECT * FROM credit_transactions
        WHERE enterprise_id = $1
        ORDER BY created_at DESC
        LIMIT $2 OFFSET $3
        "#,
    )
    .bind(enterprise_id)
    .bind(limit.clamp(1, 200))
    .bind(offset.max(0))
    .fetch_all(db)
    .await?;
    Ok(rows)
}

// ─── Grant credits (purchase, bonus, admin, promo, subscription) ──

pub struct GrantInput<'a> {
    pub enterprise_id: Uuid,
    pub amount: &'a BigDecimal,
    pub reason: &'static str,
    pub related_payment_id: Option<Uuid>,
    pub related_promo_code_id: Option<Uuid>,
    pub notes: Option<&'a str>,
    pub actor_user_id: Option<Uuid>,
    pub expires_at: Option<DateTime<Utc>>,
}

pub async fn grant(db: &PgPool, input: GrantInput<'_>) -> Result<CreditTransaction, AppError> {
    if input.amount <= &BigDecimal::from(0) {
        return Err(AppError::Validation("amount must be > 0".into()));
    }
    let mut tx = db.begin().await?;

    // Ensure row exists, update atomically, also update aggregate counters when relevant.
    let updated_balance: BigDecimal = sqlx::query_scalar(
        r#"
        INSERT INTO enterprise_credits (enterprise_id, balance, total_purchased)
        VALUES ($1, $2, CASE WHEN $3 = 'purchase' THEN 1 ELSE 0 END)
        ON CONFLICT (enterprise_id) DO UPDATE SET
            balance = enterprise_credits.balance + EXCLUDED.balance,
            total_purchased = enterprise_credits.total_purchased + (CASE WHEN $3 = 'purchase' THEN 1 ELSE 0 END),
            updated_at = NOW()
        RETURNING balance
        "#,
    )
    .bind(input.enterprise_id)
    .bind(input.amount)
    .bind(input.reason)
    .fetch_one(&mut *tx)
    .await?;

    let txn: CreditTransaction = sqlx::query_as(
        r#"
        INSERT INTO credit_transactions
            (enterprise_id, delta, balance_after, reason, related_payment_id, related_promo_code_id, notes, actor_user_id, expires_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        RETURNING *
        "#,
    )
    .bind(input.enterprise_id)
    .bind(input.amount)
    .bind(&updated_balance)
    .bind(input.reason)
    .bind(input.related_payment_id)
    .bind(input.related_promo_code_id)
    .bind(input.notes)
    .bind(input.actor_user_id)
    .bind(input.expires_at)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(txn)
}

pub async fn grant_signup_bonus(
    db: &PgPool,
    enterprise_id: Uuid,
) -> Result<Option<CreditTransaction>, AppError> {
    // Refuse to re-grant the bonus if it was already given.
    let already: Option<(i32,)> = sqlx::query_as(
        "SELECT 1 FROM credit_transactions WHERE enterprise_id = $1 AND reason = 'signup_bonus' LIMIT 1",
    )
    .bind(enterprise_id)
    .fetch_optional(db)
    .await?;
    if already.is_some() {
        return Ok(None);
    }
    let amount = dec(SIGNUP_BONUS_CREDITS);
    let expires_at = Utc::now() + ChronoDuration::days(SIGNUP_BONUS_TTL_DAYS);
    let txn = grant(
        db,
        GrantInput {
            enterprise_id,
            amount: &amount,
            reason: "signup_bonus",
            related_payment_id: None,
            related_promo_code_id: None,
            notes: Some("Welcome bonus — 1 free credit valid 30 days"),
            actor_user_id: None,
            expires_at: Some(expires_at),
        },
    )
    .await?;
    Ok(Some(txn))
}

// ─── Spend ────────────────────────────────────────────────────────

pub struct SpendInput<'a> {
    pub enterprise_id: Uuid,
    pub amount: &'a BigDecimal,
    pub reason: &'static str,
    pub related_interest_request_id: Option<Uuid>,
    pub actor_user_id: Option<Uuid>,
    pub notes: Option<&'a str>,
}

/// Atomic spend with balance guard. Returns the transaction row.
/// On insufficient balance, returns AppError::InsufficientCredits-style validation error.
pub async fn spend(db: &PgPool, input: SpendInput<'_>) -> Result<CreditTransaction, AppError> {
    if input.amount <= &BigDecimal::from(0) {
        return Err(AppError::Validation("amount must be > 0".into()));
    }
    let mut tx = db.begin().await?;

    // Atomic guard: only decrement if balance >= amount.
    let row: Option<(BigDecimal,)> = sqlx::query_as(
        r#"
        UPDATE enterprise_credits
        SET balance = balance - $1,
            total_used = total_used + $1,
            updated_at = NOW()
        WHERE enterprise_id = $2 AND balance >= $1
        RETURNING balance
        "#,
    )
    .bind(input.amount)
    .bind(input.enterprise_id)
    .fetch_optional(&mut *tx)
    .await?;

    let balance_after = match row {
        Some((b,)) => b,
        None => {
            return Err(AppError::Validation(
                "Insufficient credits — recharge the enterprise account before contacting talents.".into(),
            ));
        }
    };

    let neg = -input.amount.clone();
    let txn: CreditTransaction = sqlx::query_as(
        r#"
        INSERT INTO credit_transactions
            (enterprise_id, delta, balance_after, reason, related_interest_request_id, notes, actor_user_id)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING *
        "#,
    )
    .bind(input.enterprise_id)
    .bind(&neg)
    .bind(&balance_after)
    .bind(input.reason)
    .bind(input.related_interest_request_id)
    .bind(input.notes)
    .bind(input.actor_user_id)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(txn)
}

// ─── Refund ───────────────────────────────────────────────────────

/// Refund a fraction of a spend transaction. Used on interest_request refusal / timeout.
/// `ratio_str` like "0.5" for 50%. Returns the refund transaction (None if original spend
/// not found or already fully refunded).
pub async fn refund_spend(
    db: &PgPool,
    spend_transaction_id: Uuid,
    ratio_str: &str,
    reason: &'static str,
) -> Result<Option<CreditTransaction>, AppError> {
    let original: Option<CreditTransaction> =
        sqlx::query_as("SELECT * FROM credit_transactions WHERE id = $1")
            .bind(spend_transaction_id)
            .fetch_optional(db)
            .await?;
    let Some(original) = original else {
        return Ok(None);
    };
    if !matches!(original.reason.as_str(), "spend_interest_request") {
        return Err(AppError::Validation("Can only refund spend transactions".into()));
    }
    // Did we already refund this transaction? (1 refund max per spend)
    let already_refunded: Option<(i32,)> = sqlx::query_as(
        r#"
        SELECT 1 FROM credit_transactions
        WHERE reason IN ('refund_refused', 'refund_timeout', 'refund_admin')
          AND related_interest_request_id IS NOT DISTINCT FROM $1
        LIMIT 1
        "#,
    )
    .bind(original.related_interest_request_id)
    .fetch_optional(db)
    .await?;
    if already_refunded.is_some() {
        return Ok(None);
    }

    let ratio = BigDecimal::from_str(ratio_str)
        .map_err(|_| AppError::Internal("invalid ratio".into()))?;
    let original_amount = original.delta.abs();
    let refund_amount = (&original_amount * &ratio).round(2);
    if refund_amount <= BigDecimal::from(0) {
        return Ok(None);
    }
    let txn = grant(
        db,
        GrantInput {
            enterprise_id: original.enterprise_id,
            amount: &refund_amount,
            reason,
            related_payment_id: None,
            related_promo_code_id: None,
            notes: Some("Refund (interest request refused or timed out)"),
            actor_user_id: None,
            expires_at: None,
        },
    )
    .await?;
    // Bump total_refunded
    let _ = sqlx::query(
        "UPDATE enterprise_credits SET total_refunded = total_refunded + $1 WHERE enterprise_id = $2",
    )
    .bind(&refund_amount)
    .bind(original.enterprise_id)
    .execute(db)
    .await;
    Ok(Some(txn))
}

// ─── Stripe webhook idempotency ───────────────────────────────────

/// Returns true if this is the first time we see this event_id.
pub async fn mark_webhook_event(
    db: &PgPool,
    event_id: &str,
    event_type: &str,
) -> Result<bool, AppError> {
    let inserted = sqlx::query(
        "INSERT INTO stripe_webhook_events (event_id, event_type) VALUES ($1, $2) ON CONFLICT (event_id) DO NOTHING",
    )
    .bind(event_id)
    .bind(event_type)
    .execute(db)
    .await?
    .rows_affected();
    Ok(inserted == 1)
}

pub async fn mark_webhook_processed(db: &PgPool, event_id: &str) -> Result<(), AppError> {
    sqlx::query("UPDATE stripe_webhook_events SET processed_at = NOW() WHERE event_id = $1")
        .bind(event_id)
        .execute(db)
        .await?;
    Ok(())
}

/// Convenience: balance as f64 for response payloads.
pub fn balance_as_f64(b: &BigDecimal) -> f64 {
    b.to_f64().unwrap_or(0.0)
}

/// Background task — refund 50% of credits for interest_requests that stayed
/// `pending` for more than 30 days.
pub fn start_interest_timeout_refunder(db: PgPool) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(6 * 3600));
        loop {
            ticker.tick().await;
            if let Err(err) = sweep_expired_interest_requests(&db).await {
                tracing::warn!(error = %err, "interest request timeout sweep failed");
            }
        }
    });
}

async fn sweep_expired_interest_requests(db: &PgPool) -> Result<(), AppError> {
    let timed_out: Vec<(Uuid,)> = sqlx::query_as(
        r#"
        SELECT id FROM interest_requests
        WHERE status = 'pending' AND created_at < NOW() - INTERVAL '30 days'
        LIMIT 200
        "#,
    )
    .fetch_all(db)
    .await?;
    for (request_id,) in timed_out {
        let _ = sqlx::query(
            "UPDATE interest_requests SET status = 'declined', declined_at = NOW(), cooldown_until = NOW() + INTERVAL '30 days' WHERE id = $1 AND status = 'pending'",
        )
        .bind(request_id)
        .execute(db)
        .await;
        if let Some(spend_txn) = sqlx::query_as::<_, (Uuid,)>(
            "SELECT id FROM credit_transactions WHERE related_interest_request_id = $1 AND reason = 'spend_interest_request' LIMIT 1",
        )
        .bind(request_id)
        .fetch_optional(db)
        .await?
        {
            let _ = refund_spend(db, spend_txn.0, REFUND_RATIO_PARTIAL, "refund_timeout").await;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dec_parses() {
        let v = dec("0.5");
        assert_eq!(v.to_string(), "0.5");
    }

    #[test]
    fn dec_from_f64_two_decimals() {
        let v = dec_from_f64(1.234);
        assert_eq!(v.to_string(), "1.23");
    }

    #[test]
    fn balance_f64_roundtrip() {
        let b = dec("12.50");
        assert_eq!(balance_as_f64(&b), 12.5);
    }
}
