//! Collecting money — the mirror of [`crate::services::payout`].
//!
//! Payouts got a trait, a routing table as data, and one adapter per
//! provider; adding FedaPay cost one file. Collection got none of it.
//! `psp.rs` declared a `PaymentProvider` trait and a registry nobody
//! constructed, `psp_africa.rs` implemented three providers nothing
//! imported, and three route modules called Stripe directly.
//!
//! That is not a matter of taste. Stripe cannot collect Mobile Money in
//! Benin, and Mobile Money is how roughly seventy percent of adults in the
//! franc zone hold money — against about a quarter with a bank account. A
//! Beninese enterprise could not pay for credits at all, and fixing it
//! meant editing three route files.
//!
//! ## Same shape, deliberately
//!
//! * [`CollectionProvider`] — what a way of taking money must do.
//! * [`routes`] — which provider serves which country, currency and method,
//!   as rows. Opening a corridor is an INSERT.
//! * [`start`] — the one entry point. Records the attempt, asks the
//!   provider, hands back somewhere to send the payer.
//! * [`refund`] — gives it back, at the provider and in the books.
//!
//! ## Why `refund` is here and not in the ledger
//!
//! `ledger::refund_from_dispute` wrote entries saying the money had left
//! the provider. Nothing told the provider. The books said refunded and the
//! card was never credited, which is the one accounting error a customer
//! notices before you do.

use async_trait::async_trait;
use bigdecimal::BigDecimal;
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::ledger::Currency;

/// How the payer is paying.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    Card,
    MobileMoney,
    BankTransfer,
}

impl Method {
    pub fn as_str(&self) -> &'static str {
        match self {
            Method::Card => "card",
            Method::MobileMoney => "mobile_money",
            Method::BankTransfer => "bank_transfer",
        }
    }
}

impl std::str::FromStr for Method {
    type Err = AppError;
    fn from_str(s: &str) -> Result<Self, AppError> {
        match s {
            "card" => Ok(Method::Card),
            "mobile_money" => Ok(Method::MobileMoney),
            "bank_transfer" => Ok(Method::BankTransfer),
            other => Err(AppError::Validation(format!(
                "unknown payment method '{other}'"
            ))),
        }
    }
}

/// Everything a provider needs to take one payment.
#[derive(Debug, Clone)]
pub struct CollectionRequest<'a> {
    /// Who is paying, for the record and for the receipt.
    pub payer_id: Option<Uuid>,
    pub payer_enterprise_id: Option<Uuid>,
    pub payer_email: &'a str,
    pub payer_name: &'a str,
    /// ISO 3166-1 alpha-2. Decides the route, and Mobile Money needs it.
    pub payer_country: Option<&'a str>,
    /// Phone in E.164, for Mobile Money. Absent for a card.
    pub payer_phone: Option<&'a str>,

    /// What the money is for, so a dispute finds the charge.
    pub subject_type: &'a str,
    pub subject_id: Uuid,

    pub amount: &'a BigDecimal,
    pub currency: Currency,
    /// Shown to the payer at checkout.
    pub description: &'a str,
    /// Where to send them afterwards.
    pub success_url: &'a str,
    pub cancel_url: &'a str,
    /// Stable key so a double-submitted form cannot charge twice.
    pub idempotency_key: &'a str,
    /// Which operator, when the payer chose one. `None` lets the provider
    /// decide from the number, which is what a redirect flow does anyway.
    pub operator: Option<&'a str>,
    /// Credits bought, for a credit pack. Frozen here so a price change
    /// between paying and delivering cannot alter what someone receives.
    pub credits: Option<i32>,
    /// Filled in by [`start`], never by the caller: the identifier the
    /// provider echoes back and that a poller looks the payment up by.
    pub merchant_reference: Option<&'a str>,
}

/// Where to send the payer, and what to call this later.
#[derive(Debug, Clone, Serialize)]
pub struct Checkout {
    pub provider: String,
    /// The provider's identifier for this checkout.
    pub session_id: String,
    /// Where the payer completes the payment. Every method has one, card or
    /// Mobile Money — the operator's confirmation page counts.
    pub redirect_url: String,
}

/// One way of taking money.
///
/// Narrower than the trait it replaces, which also carried webhook
/// verification and a billing portal. Verification belongs to
/// [`crate::services::payment_webhooks`], which already does it for both
/// directions; a portal is a Stripe feature and putting it in the trait
/// forced every other provider to declare that it has none.
#[async_trait]
pub trait CollectionProvider: Send + Sync {
    /// Stable identifier. Also the ledger account segment.
    fn name(&self) -> &'static str;

    fn supports(&self, currency: Currency, method: Method) -> bool;

    async fn start(&self, request: &CollectionRequest<'_>) -> Result<Checkout, AppError>;

    /// Give money back at the provider.
    ///
    /// `amount` is `None` for the whole charge. A provider that cannot
    /// refund returns an error rather than pretending: a silent success
    /// here means the books say refunded and the payer was not.
    async fn refund(
        &self,
        provider_reference: &str,
        amount: Option<&BigDecimal>,
        currency: Currency,
        reason: &str,
    ) -> Result<String, AppError>;
}

/// A rule saying which provider takes money from where.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Route {
    pub country: Option<String>,
    pub currency: String,
    pub method: String,
    pub provider: String,
    pub priority: i16,
    pub enabled: bool,
}

/// Routes for a payer, best first.
pub async fn routes(
    db: &PgPool,
    country: Option<&str>,
    currency: Currency,
    method: Method,
) -> Result<Vec<Route>, AppError> {
    let rows = sqlx::query_as(
        r#"
        SELECT country, currency, method, provider, priority, enabled
          FROM collection_routes
         WHERE enabled = TRUE
           AND currency = $2
           AND method = $3
           AND (country = $1 OR country IS NULL)
         ORDER BY (country IS NULL), priority
        "#,
    )
    .bind(country.map(str::to_uppercase))
    .bind(currency.as_str())
    .bind(method.as_str())
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// The providers this deployment can actually use.
#[derive(Default)]
pub struct CollectionRegistry {
    providers: Vec<std::sync::Arc<dyn CollectionProvider>>,
}

impl CollectionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, provider: std::sync::Arc<dyn CollectionProvider>) {
        self.providers.push(provider);
    }

    pub fn get(&self, name: &str) -> Option<std::sync::Arc<dyn CollectionProvider>> {
        self.providers.iter().find(|p| p.name() == name).cloned()
    }

    pub fn names(&self) -> Vec<&'static str> {
        self.providers.iter().map(|p| p.name()).collect()
    }

    /// First configured provider able to take this payment.
    pub async fn resolve(
        &self,
        db: &PgPool,
        country: Option<&str>,
        currency: Currency,
        method: Method,
    ) -> Result<std::sync::Arc<dyn CollectionProvider>, AppError> {
        let candidates = routes(db, country, currency, method).await?;
        for route in &candidates {
            if let Some(provider) = self.get(&route.provider)
                && provider.supports(currency, method)
            {
                return Ok(provider);
            }
        }
        Err(AppError::Validation(format!(
            "no way to take {} by {} from {}. Routes matched: {}",
            currency.as_str(),
            method.as_str(),
            country.unwrap_or("an unspecified country"),
            if candidates.is_empty() {
                "none".to_string()
            } else {
                candidates
                    .iter()
                    .map(|r| r.provider.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        )))
    }
}

/// Start collecting, recording the attempt first.
///
/// The row goes in before the provider is asked, for the same reason the
/// ledger movement does on the way out: a process that dies mid-call must
/// leave something to reconcile. A charge nobody recorded is one nobody
/// will ever refund.
pub async fn start(
    db: &PgPool,
    provider: &dyn CollectionProvider,
    method: Method,
    request: CollectionRequest<'_>,
) -> Result<Checkout, AppError> {
    // The merchant reference is ours and travels to the provider, so a
    // payment can be asked about even when the response carrying their
    // identifier is the thing that got lost — which is exactly what a
    // closed browser tab produces.
    let payment_id: Uuid = sqlx::query_scalar(
        "INSERT INTO payments
            (payer_id, payer_enterprise_id, subject_type, subject_id, provider,
             method, amount, currency, idempotency_key, operator, credits_purchased)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
         ON CONFLICT (idempotency_key) DO UPDATE SET updated_at = NOW()
         RETURNING id",
    )
    .bind(request.payer_id)
    .bind(request.payer_enterprise_id)
    .bind(request.subject_type)
    .bind(request.subject_id)
    .bind(provider.name())
    .bind(method.as_str())
    .bind(request.amount)
    .bind(request.currency.as_str())
    .bind(request.idempotency_key)
    .bind(request.operator)
    .bind(request.credits)
    .fetch_one(db)
    .await?;

    let merchant_reference = format!("SKU-{}", payment_id.simple());
    sqlx::query(
        "UPDATE payments SET merchant_reference = $2 WHERE id = $1 AND merchant_reference IS NULL",
    )
    .bind(payment_id)
    .bind(&merchant_reference)
    .execute(db)
    .await?;

    let mut request = request;
    request.merchant_reference = Some(&merchant_reference);

    match provider.start(&request).await {
        Ok(checkout) => {
            sqlx::query("UPDATE payments SET provider_session_id = $2 WHERE id = $1")
                .bind(payment_id)
                .bind(&checkout.session_id)
                .execute(db)
                .await?;
            Ok(checkout)
        }
        Err(e) => {
            // Marked failed rather than left pending: a checkout that never
            // opened is not one the payer abandoned, and a reconciliation
            // that cannot tell them apart is one nobody trusts.
            sqlx::query("UPDATE payments SET status = 'failed', failure_reason = $2 WHERE id = $1")
                .bind(payment_id)
                .bind(e.to_string())
                .execute(db)
                .await?;
            Err(e)
        }
    }
}

/// The charge that paid for something, if there is one.
pub async fn payment_for(
    db: &PgPool,
    subject_type: &str,
    subject_id: Uuid,
) -> Result<Option<Payment>, AppError> {
    Ok(sqlx::query_as(
        "SELECT id, provider, provider_reference, amount, currency,
                refunded_amount, status
           FROM payments
          WHERE subject_type = $1 AND subject_id = $2 AND status = 'succeeded'
          ORDER BY created_at DESC
          LIMIT 1",
    )
    .bind(subject_type)
    .bind(subject_id)
    .fetch_optional(db)
    .await?)
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Payment {
    pub id: Uuid,
    pub provider: String,
    pub provider_reference: Option<String>,
    pub amount: BigDecimal,
    pub currency: String,
    pub refunded_amount: BigDecimal,
    pub status: String,
}

/// Give money back, at the provider and in our records.
///
/// Returns `Ok(None)` when there is nothing to refund at a provider — a
/// charge we never recorded, from before this table existed. The caller
/// still moves its own books; what it must not do is report success as
/// though a card had been credited.
pub async fn refund(
    db: &PgPool,
    registry: &CollectionRegistry,
    subject_type: &str,
    subject_id: Uuid,
    reason: &str,
) -> Result<Option<String>, AppError> {
    let Some(payment) = payment_for(db, subject_type, subject_id).await? else {
        tracing::warn!(
            subject = subject_type,
            id = %subject_id,
            "refunding something with no recorded charge — the books move, the payer does not get a card refund"
        );
        return Ok(None);
    };

    let Some(reference) = payment.provider_reference.as_deref() else {
        tracing::error!(
            payment = %payment.id,
            provider = %payment.provider,
            "a succeeded payment with no provider reference cannot be refunded — reconcile it by hand"
        );
        return Ok(None);
    };

    let Some(provider) = registry.get(&payment.provider) else {
        return Err(AppError::Internal(format!(
            "this deployment has no credentials for {}, so it cannot refund {reference}",
            payment.provider
        )));
    };

    let currency: Currency = payment.currency.parse()?;
    let refund_id = provider
        .refund(reference, Some(&payment.amount), currency, reason)
        .await?;

    sqlx::query(
        "UPDATE payments
            SET status = 'refunded', refunded_amount = amount
          WHERE id = $1",
    )
    .bind(payment.id)
    .execute(db)
    .await?;

    metrics::counter!(
        "skilluv_payments_refunded_total",
        "provider" => payment.provider.clone()
    )
    .increment(1);
    tracing::info!(
        payment = %payment.id,
        provider = %payment.provider,
        refund = %refund_id,
        "refunded at the provider"
    );

    Ok(Some(refund_id))
}
