//! Where a talent's money goes — not how much of it there is.
//!
//! This used to be a ledger: two balance columns and an append-only
//! `talent_transactions` table with a hash chain. Migration 0153 replaced
//! that with double-entry bookkeeping, and 0158 removed the old one. Keeping
//! both would have meant two answers to "how much do we owe this person",
//! with nothing forcing them to agree.
//!
//! What remains is the destination: residency, Mobile Money number and
//! operator, Stripe Connect account and its verification state. None of that
//! is a balance, and none of it belongs in the ledger.
//!
//! Balances come from [`crate::services::ledger`].

use bigdecimal::BigDecimal;
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::ledger::{self, Currency, State};

/// Payout destinations for one talent.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct TalentWallet {
    pub user_id: Uuid,
    /// ISO 3166-1 alpha-2. Decides which rails can reach this person, so it
    /// is the first thing a payout looks at.
    pub residency_country: Option<String>,
    pub stripe_account_id: Option<String>,
    pub stripe_kyc_status: String,
    pub momo_phone: Option<String>,
    pub momo_phone_verified: bool,
    /// `orange` | `mtn` | `wave`. Belongs to the number, not to a
    /// transaction (migration 0151).
    pub momo_provider: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// What a talent holds, in every state and currency.
///
/// Derived from the ledger on every read. There is deliberately no cached
/// copy: a cache that can drift from the books is the problem this module
/// was rewritten to remove.
#[derive(Debug, Clone, Serialize)]
pub struct WalletBalances {
    /// Earned, still inside its release window.
    pub pending_eur: BigDecimal,
    pub pending_xof: BigDecimal,
    /// Withdrawable now.
    pub available_eur: BigDecimal,
    pub available_xof: BigDecimal,
    /// Frozen while a complaint is examined.
    pub disputed_eur: BigDecimal,
    pub disputed_xof: BigDecimal,
}

/// Fetch or create a talent's wallet row. Idempotent.
pub async fn get_or_init_wallet(db: &PgPool, user_id: Uuid) -> Result<TalentWallet, AppError> {
    let wallet = sqlx::query_as::<_, TalentWallet>(
        r#"
        INSERT INTO talent_wallets (user_id)
        VALUES ($1)
        ON CONFLICT (user_id) DO UPDATE SET user_id = talent_wallets.user_id
        RETURNING user_id, residency_country, stripe_account_id, stripe_kyc_status,
                  momo_phone, momo_phone_verified, momo_provider, created_at, updated_at
        "#,
    )
    .bind(user_id)
    .fetch_one(db)
    .await?;
    Ok(wallet)
}

/// Read every balance for a talent.
pub async fn balances(db: &PgPool, user_id: Uuid) -> Result<WalletBalances, AppError> {
    Ok(WalletBalances {
        pending_eur: ledger::user_balance(db, user_id, State::Pending, Currency::Eur).await?,
        pending_xof: ledger::user_balance(db, user_id, State::Pending, Currency::Xof).await?,
        available_eur: ledger::user_balance(db, user_id, State::Available, Currency::Eur).await?,
        available_xof: ledger::user_balance(db, user_id, State::Available, Currency::Xof).await?,
        disputed_eur: ledger::user_balance(db, user_id, State::Disputed, Currency::Eur).await?,
        disputed_xof: ledger::user_balance(db, user_id, State::Disputed, Currency::Xof).await?,
    })
}

/// Declare where a talent lives, which decides the default payout rail.
pub async fn set_residency_country(
    db: &PgPool,
    user_id: Uuid,
    country: &str,
) -> Result<TalentWallet, AppError> {
    let country = country.trim().to_uppercase();
    if country.len() != 2 || !country.chars().all(|c| c.is_ascii_alphabetic()) {
        return Err(AppError::Validation(
            "country must be an ISO 3166-1 alpha-2 code, e.g. 'CI'".into(),
        ));
    }

    let _ = get_or_init_wallet(db, user_id).await?;
    let wallet = sqlx::query_as::<_, TalentWallet>(
        r#"
        UPDATE talent_wallets
        SET residency_country = $1, updated_at = NOW()
        WHERE user_id = $2
        RETURNING user_id, residency_country, stripe_account_id, stripe_kyc_status,
                  momo_phone, momo_phone_verified, momo_provider, created_at, updated_at
        "#,
    )
    .bind(&country)
    .bind(user_id)
    .fetch_one(db)
    .await?;
    Ok(wallet)
}

/// One movement, as shown to the talent.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct WalletMovement {
    pub id: Uuid,
    /// `capture`, `release`, `withdrawal`, `dispute_hold`, …
    pub reason: String,
    /// Positive when it increases what we owe them.
    pub amount: BigDecimal,
    pub currency: String,
    /// `pending`, `available` or `disputed`.
    pub state: String,
    pub provider: Option<String>,
    pub provider_reference: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// A talent's own movements, newest first.
///
/// Only their claim accounts: the provider-side legs of the same
/// transactions are ours, not theirs, and showing them would expose the
/// platform's float.
pub async fn list_movements(
    db: &PgPool,
    user_id: Uuid,
    limit: i64,
) -> Result<Vec<WalletMovement>, AppError> {
    let rows = sqlx::query_as(
        r#"
        SELECT e.id,
               t.reason,
               -- Claims are stored negative; a talent reads a credit as a
               -- positive number.
               -e.amount AS amount,
               e.currency,
               a.state,
               t.provider,
               t.provider_reference,
               e.created_at
          FROM ledger_entries e
          JOIN ledger_accounts a ON a.id = e.account_id
          JOIN ledger_transactions t ON t.id = e.transaction_id
         WHERE a.owner_user_id = $1
         ORDER BY e.created_at DESC
         LIMIT $2
        "#,
    )
    .bind(user_id)
    .bind(limit.clamp(1, 200))
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// Net amount withdrawn in a rolling window, for the daily and monthly caps.
///
/// A refused payout nets out against its reversal, so it does not consume
/// someone's limit — being told "no" by a provider should not cost you your
/// daily allowance.
pub async fn withdrawn_within(
    db: &PgPool,
    user_id: Uuid,
    currency: Currency,
    hours: i32,
) -> Result<BigDecimal, AppError> {
    let total: BigDecimal = sqlx::query_scalar("SELECT ledger_withdrawn_within($1, $2, $3)")
        .bind(user_id)
        .bind(currency.as_str())
        .bind(hours)
        .fetch_one(db)
        .await?;
    Ok(total)
}

/// Compliance export: every movement affecting this talent, as CSV.
///
/// Rebuilt from the ledger, so it reports what the books say rather than a
/// separate record that could disagree with them.
pub async fn statement_csv(db: &PgPool, user_id: Uuid) -> Result<String, AppError> {
    let movements = list_movements(db, user_id, 200).await?;

    let mut csv = String::from("date,reason,state,amount,currency,provider,reference\n");
    for m in movements {
        // Quote every field and double any embedded quote: a provider
        // reference is data we did not choose the shape of.
        let escape = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
        csv.push_str(&format!(
            "{},{},{},{},{},{},{}\n",
            m.created_at.to_rfc3339(),
            escape(&m.reason),
            escape(&m.state),
            m.amount,
            escape(&m.currency),
            escape(m.provider.as_deref().unwrap_or("")),
            escape(m.provider_reference.as_deref().unwrap_or("")),
        ));
    }
    Ok(csv)
}
