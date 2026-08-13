//! Double-entry ledger — the source of truth for real money.
//!
//! Every movement is a [`Posting`]: a reason, an optional idempotency key,
//! and legs that sum to zero per currency. The database enforces the balance
//! rule (migration 0153), so money cannot be posted into existence even by
//! mistake — the transaction fails at commit.
//!
//! ## Why not just update a balance
//!
//! A balance column records what someone has and nothing about how they came
//! to have it. It cannot express "earned but not yet released", cannot be
//! reconciled against a provider statement, and cannot survive a partial
//! failure: this codebase has already shipped three payout paths that marked
//! work as paid when no money moved. Here a balance is the sum of a history —
//! derived, replayable, and impossible to set directly.
//!
//! ## Signs
//!
//! Positive is a debit, negative a credit, as in migration 0153. Assets
//! (`psp:*`) read naturally: positive means we hold that much there. Claims
//! (`user:*`, `platform:*`) are stored negative, because they are what we
//! owe; [`user_balance`] flips them so no caller has to know.
//!
//! Prefer the business movements at the bottom of this file over assembling
//! legs by hand. A reversed sign is the easiest mistake to make in a ledger
//! and the hardest to notice later, so the places that write signs are few
//! and each is covered by a test.
//!
//! ## States
//!
//! Money owed to a person sits in one of three accounts:
//!
//! * `pending` — earned, not withdrawable. Where funds wait out the window
//!   in which the payer can still complain.
//! * `available` — released, withdrawable.
//! * `disputed` — frozen while a human decides.
//!
//! Nothing skips straight to `available`: [`release`] is the only way in, so
//! every flow has to state when it considers work settled instead of paying
//! out on completion and hoping.

use bigdecimal::BigDecimal;
use serde::Serialize;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::errors::AppError;

/// Currency of an amount. Mirrors the CHECK constraint in migration 0153.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Currency {
    Eur,
    Xof,
}

impl Currency {
    pub fn as_str(&self) -> &'static str {
        match self {
            Currency::Eur => "EUR",
            Currency::Xof => "XOF",
        }
    }
}

impl std::str::FromStr for Currency {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self, AppError> {
        match s.to_uppercase().as_str() {
            "EUR" => Ok(Currency::Eur),
            "XOF" => Ok(Currency::Xof),
            other => Err(AppError::Validation(format!(
                "unsupported currency '{other}' (EUR or XOF)"
            ))),
        }
    }
}

/// Which of a person's three accounts an amount sits in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum State {
    /// Earned, not withdrawable — the payer can still contest.
    Pending,
    /// Released and withdrawable.
    Available,
    /// Frozen pending a human decision.
    Disputed,
}

impl State {
    pub fn as_str(&self) -> &'static str {
        match self {
            State::Pending => "pending",
            State::Available => "available",
            State::Disputed => "disputed",
        }
    }
}

/// An account, identified by what it is rather than by a row id. Resolved to
/// a row on first use, so callers never create one explicitly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Account {
    /// A claim: what we owe one person, in a given state.
    User {
        user_id: Uuid,
        state: State,
        currency: Currency,
    },
    /// A claim in our favour: `revenue`, `fees`, …
    Platform {
        bucket: &'static str,
        currency: Currency,
    },
    /// An asset: money held at a provider. Its balance is what should match
    /// their statement.
    Psp {
        provider: &'static str,
        currency: Currency,
    },
    /// The counterparty outside the system, for movements crossing the
    /// boundary with no claim behind them.
    World { currency: Currency },
}

impl Account {
    /// Stable, readable identity, and the unique key in the database. The
    /// format is part of the schema: changing it orphans every account
    /// already written.
    pub fn code(&self) -> String {
        match self {
            Account::User {
                user_id,
                state,
                currency,
            } => format!("user:{user_id}:{}:{}", state.as_str(), currency.as_str()),
            Account::Platform { bucket, currency } => {
                format!("platform:{bucket}:{}", currency.as_str())
            }
            Account::Psp { provider, currency } => {
                format!("psp:{provider}:settlement:{}", currency.as_str())
            }
            Account::World { currency } => format!("external:world:{}", currency.as_str()),
        }
    }

    pub fn currency(&self) -> Currency {
        match self {
            Account::User { currency, .. }
            | Account::Platform { currency, .. }
            | Account::Psp { currency, .. }
            | Account::World { currency } => *currency,
        }
    }

    fn kind(&self) -> &'static str {
        match self {
            Account::User { .. } => "user",
            Account::Platform { .. } => "platform",
            Account::Psp { .. } => "psp",
            Account::World { .. } => "external",
        }
    }

    fn owner(&self) -> Option<Uuid> {
        match self {
            Account::User { user_id, .. } => Some(*user_id),
            _ => None,
        }
    }

    fn state(&self) -> Option<&'static str> {
        match self {
            Account::User { state, .. } => Some(state.as_str()),
            _ => None,
        }
    }
}

/// Shorthand for a person's claim account.
pub fn owed(user_id: Uuid, state: State, currency: Currency) -> Account {
    Account::User {
        user_id,
        state,
        currency,
    }
}

/// One side of a movement. Prefer the named constructors over building this
/// directly, and the business movements below over either.
#[derive(Debug, Clone)]
pub struct Leg {
    pub account: Account,
    pub amount: BigDecimal,
}

impl Leg {
    /// Debit: increases an asset, or decreases what we owe.
    pub fn debit(account: Account, amount: BigDecimal) -> Self {
        Self { account, amount }
    }

    /// Credit: decreases an asset, or increases what we owe.
    pub fn credit(account: Account, amount: BigDecimal) -> Self {
        Self {
            account,
            amount: -amount,
        }
    }
}

/// A complete movement. Legs must sum to zero per currency; the database
/// rejects the transaction otherwise.
#[derive(Debug, Clone)]
pub struct Posting<'a> {
    /// Business meaning, e.g. `capture`, `withdrawal`.
    pub reason: &'a str,
    /// Replay guard. A provider webhook is delivered more than once by
    /// design, so posting twice under the same key is a no-op rather than a
    /// doubled amount.
    pub idempotency_key: Option<String>,
    pub subject_type: Option<&'a str>,
    pub subject_id: Option<Uuid>,
    pub provider: Option<&'a str>,
    pub provider_reference: Option<String>,
    pub notes: Option<&'a str>,
    pub legs: Vec<Leg>,
}

impl<'a> Posting<'a> {
    pub fn new(reason: &'a str, legs: Vec<Leg>) -> Self {
        Self {
            reason,
            idempotency_key: None,
            subject_type: None,
            subject_id: None,
            provider: None,
            provider_reference: None,
            notes: None,
            legs,
        }
    }

    pub fn idempotent(mut self, key: impl Into<String>) -> Self {
        self.idempotency_key = Some(key.into());
        self
    }

    pub fn about(mut self, subject_type: &'a str, subject_id: Uuid) -> Self {
        self.subject_type = Some(subject_type);
        self.subject_id = Some(subject_id);
        self
    }

    pub fn via(mut self, provider: &'a str, reference: impl Into<String>) -> Self {
        self.provider = Some(provider);
        self.provider_reference = Some(reference.into());
        self
    }

    pub fn note(mut self, notes: &'a str) -> Self {
        self.notes = Some(notes);
        self
    }
}

/// Outcome of a post.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Posted {
    /// Written now.
    Recorded(Uuid),
    /// The idempotency key had already been used. Nothing was written; the
    /// original transaction is returned.
    AlreadyRecorded(Uuid),
}

impl Posted {
    pub fn transaction_id(&self) -> Uuid {
        match self {
            Posted::Recorded(id) | Posted::AlreadyRecorded(id) => *id,
        }
    }

    pub fn was_replay(&self) -> bool {
        matches!(self, Posted::AlreadyRecorded(_))
    }
}

/// Resolve an account to its row, creating it on first use.
///
/// The insert takes the conflict rather than checking first: two requests
/// crediting the same person at once would otherwise race into a unique
/// violation.
async fn ensure_account(
    tx: &mut Transaction<'_, Postgres>,
    account: &Account,
) -> Result<Uuid, AppError> {
    let id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO ledger_accounts (code, kind, owner_user_id, state, currency)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (code) DO UPDATE SET code = ledger_accounts.code
        RETURNING id
        "#,
    )
    .bind(account.code())
    .bind(account.kind())
    .bind(account.owner())
    .bind(account.state())
    .bind(account.currency().as_str())
    .fetch_one(&mut **tx)
    .await?;
    Ok(id)
}

/// Post a movement inside an existing transaction.
///
/// Use this when money has to move atomically with something else — a
/// session marked complete, a slice marked merged. The balance rule is
/// checked at commit, so a caller that fails afterwards leaves nothing
/// behind.
pub async fn post_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    posting: Posting<'_>,
) -> Result<Posted, AppError> {
    if posting.legs.len() < 2 {
        return Err(AppError::Internal(format!(
            "posting '{}' has {} leg(s): money must move between at least two accounts",
            posting.reason,
            posting.legs.len()
        )));
    }

    // Checked here as well as in the database so the message names the
    // posting. The deferred constraint fires at commit, far from the code
    // that caused it.
    for currency in [Currency::Eur, Currency::Xof] {
        let total: BigDecimal = posting
            .legs
            .iter()
            .filter(|l| l.account.currency() == currency)
            .map(|l| l.amount.clone())
            .sum();
        use num_traits::Zero;
        if !total.is_zero() {
            return Err(AppError::Internal(format!(
                "posting '{}' does not balance in {}: off by {}",
                posting.reason,
                currency.as_str(),
                total
            )));
        }
    }

    if let Some(key) = posting.idempotency_key.as_deref() {
        let existing: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM ledger_transactions WHERE idempotency_key = $1")
                .bind(key)
                .fetch_optional(&mut **tx)
                .await?;
        if let Some(id) = existing {
            return Ok(Posted::AlreadyRecorded(id));
        }
    }

    let transaction_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO ledger_transactions
            (reason, idempotency_key, subject_type, subject_id, provider,
             provider_reference, notes)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING id
        "#,
    )
    .bind(posting.reason)
    .bind(posting.idempotency_key.as_deref())
    .bind(posting.subject_type)
    .bind(posting.subject_id)
    .bind(posting.provider)
    .bind(posting.provider_reference.as_deref())
    .bind(posting.notes)
    .fetch_one(&mut **tx)
    .await?;

    for leg in &posting.legs {
        let account_id = ensure_account(tx, &leg.account).await?;
        sqlx::query(
            "INSERT INTO ledger_entries (transaction_id, account_id, amount, currency)
             VALUES ($1, $2, $3, $4)",
        )
        .bind(transaction_id)
        .bind(account_id)
        .bind(&leg.amount)
        .bind(leg.account.currency().as_str())
        .execute(&mut **tx)
        .await?;
    }

    Ok(Posted::Recorded(transaction_id))
}

/// Post a movement in its own transaction.
pub async fn post(db: &PgPool, posting: Posting<'_>) -> Result<Posted, AppError> {
    let mut tx = db.begin().await?;
    let posted = post_in_tx(&mut tx, posting).await?;
    tx.commit().await?;
    Ok(posted)
}

/// Raw signed balance of an account. Reads as an asset; for a person's claim
/// use [`user_balance`], which presents it as a positive amount.
pub async fn balance(db: &PgPool, account: &Account) -> Result<BigDecimal, AppError> {
    let value: BigDecimal = sqlx::query_scalar("SELECT ledger_balance($1)")
        .bind(account.code())
        .fetch_one(db)
        .await?;
    Ok(value)
}

/// What a person holds in one state and currency, as a positive amount.
pub async fn user_balance(
    db: &PgPool,
    user_id: Uuid,
    state: State,
    currency: Currency,
) -> Result<BigDecimal, AppError> {
    let value: BigDecimal = sqlx::query_scalar("SELECT ledger_user_balance($1, $2, $3)")
        .bind(user_id)
        .bind(state.as_str())
        .bind(currency.as_str())
        .fetch_one(db)
        .await?;
    Ok(value)
}

// ─── Business movements ───────────────────────────────────────────

/// Someone paid, and part of it is owed to a recipient.
///
/// The money lands at the provider (an asset) and is split in the same
/// breath between what we owe the recipient — held `pending`, not
/// withdrawable — and what we earned.
#[allow(clippy::too_many_arguments)]
pub async fn capture_for_recipient(
    db: &PgPool,
    provider: &'static str,
    provider_reference: impl Into<String>,
    recipient: Uuid,
    gross: BigDecimal,
    platform_share: BigDecimal,
    currency: Currency,
    subject_type: &str,
    subject_id: Uuid,
) -> Result<Posted, AppError> {
    if platform_share > gross {
        return Err(AppError::Internal(
            "platform share exceeds the amount captured".into(),
        ));
    }
    let recipient_share = gross.clone() - platform_share.clone();

    post(
        db,
        Posting::new(
            "capture",
            vec![
                Leg::debit(Account::Psp { provider, currency }, gross),
                Leg::credit(owed(recipient, State::Pending, currency), recipient_share),
                Leg::credit(
                    Account::Platform {
                        bucket: "revenue",
                        currency,
                    },
                    platform_share,
                ),
            ],
        )
        .about(subject_type, subject_id)
        .via(provider, provider_reference)
        .idempotent(format!("capture:{subject_type}:{subject_id}")),
    )
    .await
}

/// The payer's window closed, or they validated early: `pending` becomes
/// `available`.
///
/// No asset moves — the money is still at the provider. Only our idea of
/// whose it is has changed.
pub async fn release(
    db: &PgPool,
    user_id: Uuid,
    amount: BigDecimal,
    currency: Currency,
    subject_type: &str,
    subject_id: Uuid,
) -> Result<Posted, AppError> {
    post(
        db,
        Posting::new(
            "release",
            vec![
                Leg::debit(owed(user_id, State::Pending, currency), amount.clone()),
                Leg::credit(owed(user_id, State::Available, currency), amount),
            ],
        )
        .about(subject_type, subject_id)
        .idempotent(format!("release:{subject_type}:{subject_id}")),
    )
    .await
}

/// Freeze an amount while a complaint is examined.
///
/// Taken from `pending`: once released, the money is the recipient's to
/// withdraw, and clawing it back is a different and much harder problem —
/// which is the whole reason the release window exists.
pub async fn hold_dispute(
    db: &PgPool,
    user_id: Uuid,
    amount: BigDecimal,
    currency: Currency,
    subject_type: &str,
    subject_id: Uuid,
) -> Result<Posted, AppError> {
    post(
        db,
        Posting::new(
            "dispute_hold",
            vec![
                Leg::debit(owed(user_id, State::Pending, currency), amount.clone()),
                Leg::credit(owed(user_id, State::Disputed, currency), amount),
            ],
        )
        .about(subject_type, subject_id)
        .idempotent(format!("dispute:{subject_type}:{subject_id}")),
    )
    .await
}

/// Dispute resolved for the recipient: frozen money becomes withdrawable.
pub async fn resolve_dispute_for_recipient(
    db: &PgPool,
    user_id: Uuid,
    amount: BigDecimal,
    currency: Currency,
    subject_type: &str,
    subject_id: Uuid,
) -> Result<Posted, AppError> {
    post(
        db,
        Posting::new(
            "dispute_resolved_recipient",
            vec![
                Leg::debit(owed(user_id, State::Disputed, currency), amount.clone()),
                Leg::credit(owed(user_id, State::Available, currency), amount),
            ],
        )
        .about(subject_type, subject_id)
        .idempotent(format!("dispute_resolved:{subject_type}:{subject_id}")),
    )
    .await
}

/// Dispute resolved for the payer: the money goes back out.
///
/// Our commission goes back with it. Keeping a fee on a refunded service
/// would be indefensible, and it also keeps the books honest — the asset
/// leaving has to equal the claims cancelled.
#[allow(clippy::too_many_arguments)]
pub async fn refund_from_dispute(
    db: &PgPool,
    provider: &'static str,
    user_id: Uuid,
    recipient_share: BigDecimal,
    platform_share: BigDecimal,
    currency: Currency,
    subject_type: &str,
    subject_id: Uuid,
) -> Result<Posted, AppError> {
    let total = recipient_share.clone() + platform_share.clone();
    post(
        db,
        Posting::new(
            "dispute_refunded",
            vec![
                Leg::debit(owed(user_id, State::Disputed, currency), recipient_share),
                Leg::debit(
                    Account::Platform {
                        bucket: "revenue",
                        currency,
                    },
                    platform_share,
                ),
                Leg::credit(Account::Psp { provider, currency }, total),
            ],
        )
        .about(subject_type, subject_id)
        .idempotent(format!("refund:{subject_type}:{subject_id}")),
    )
    .await
}

/// A recipient withdrew: we owe them less, and the float at the paying
/// provider drops by the same amount.
///
/// The paying provider need not be the one that collected — a card payment
/// in EUR can leave as Mobile Money in XOF. What connects the two is this
/// ledger, not the money itself.
pub async fn withdraw(
    db: &PgPool,
    user_id: Uuid,
    amount: BigDecimal,
    currency: Currency,
    provider: &'static str,
    provider_reference: impl Into<String>,
    idempotency_key: impl Into<String>,
) -> Result<Posted, AppError> {
    post(
        db,
        Posting::new(
            "withdrawal",
            vec![
                Leg::debit(owed(user_id, State::Available, currency), amount.clone()),
                Leg::credit(Account::Psp { provider, currency }, amount),
            ],
        )
        .via(provider, provider_reference)
        .idempotent(idempotency_key),
    )
    .await
}

/// A withdrawal the provider refused after it was recorded.
///
/// Puts the money back rather than editing the original entries, which are
/// immutable by design: the failed attempt stays visible, which is what lets
/// anyone ask later why a payout was tried twice.
pub async fn reverse_withdrawal(
    db: &PgPool,
    user_id: Uuid,
    amount: BigDecimal,
    currency: Currency,
    provider: &'static str,
    original_key: &str,
) -> Result<Posted, AppError> {
    post(
        db,
        Posting::new(
            "withdrawal_reversed",
            vec![
                Leg::debit(Account::Psp { provider, currency }, amount.clone()),
                Leg::credit(owed(user_id, State::Available, currency), amount),
            ],
        )
        .idempotent(format!("reverse:{original_key}")),
    )
    .await
}

/// What the books say we hold at each provider. Compared against the
/// provider's own statement, any difference is drift, and it wants a human
/// before it compounds.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ProviderPosition {
    pub account_code: String,
    pub currency: String,
    pub balance: BigDecimal,
}

pub async fn provider_positions(db: &PgPool) -> Result<Vec<ProviderPosition>, AppError> {
    let rows = sqlx::query_as(
        "SELECT account_code, currency, balance FROM ledger_provider_positions
          ORDER BY account_code",
    )
    .fetch_all(db)
    .await?;
    Ok(rows)
}
