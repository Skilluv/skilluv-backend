//! Contesting a payment, and settling it without a human where possible.
//!
//! The release window is a promise: for seven days after a session, the
//! person who paid can say it did not happen. Everything needed to keep
//! that promise existed — the frozen state, the refund, the notification —
//! and nothing could reach it, because no endpoint called
//! `release::dispute`. The window was seven days during which nothing could
//! be done.
//!
//! ## The shape
//!
//! ```text
//!   payer raises  ──▶  open  ──▶  recipient concedes  ──▶  refunded
//!                        │
//!                        └────▶  recipient contests  ──▶  contested
//!                                                            │
//!                                          operator decides  ▼
//!                                              refunded / released
//! ```
//!
//! The recipient answers first on purpose. A dispute that goes straight to
//! an operator makes every disagreement our problem, and is how a
//! marketplace ends up staffing a call centre for arguments the two parties
//! would have settled between them. Conceding costs nobody anything and
//! resolves immediately; only a real disagreement reaches a human.
//!
//! ## Money moves once
//!
//! Raising a dispute freezes the hold — the money leaves the recipient's
//! pending balance for a `disputed` account and stays there until the
//! outcome. Nothing can release it in the meantime: the sweep skips
//! disputed holds, and `release_now` refuses one outright.

use bigdecimal::BigDecimal;
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::ledger::{self, Currency};

/// Everything a decision needs to know about the money underneath.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DisputedHold {
    pub dispute_id: Uuid,
    pub pending_release_id: Uuid,
    pub status: String,
    pub beneficiary_id: Uuid,
    pub payer_id: Option<Uuid>,
    pub subject_type: String,
    pub subject_id: Uuid,
    pub amount: BigDecimal,
    pub currency: String,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct Dispute {
    pub id: Uuid,
    pub status: String,
    pub reason: String,
    pub recipient_response: Option<String>,
    pub resolution_note: Option<String>,
    pub subject_type: String,
    pub subject_id: Uuid,
    pub amount: String,
    pub currency: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub resolved_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Who won.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The money goes back to whoever paid it.
    Payer,
    /// The money is handed to whoever earned it.
    Recipient,
}

/// Raise a dispute, freezing the money.
///
/// Only the payer, only while the hold is unreleased, and only once. A
/// released hold is not disputable: getting money back after it has been
/// handed over is a refund, which is a different operation with a different
/// risk, and pretending otherwise here would let someone claw back a payout
/// that has already left for an operator's network.
pub async fn raise(
    db: &PgPool,
    subject_type: &str,
    subject_id: Uuid,
    raised_by: Uuid,
    reason: &str,
) -> Result<Uuid, AppError> {
    if reason.trim().chars().count() < 10 {
        return Err(AppError::Validation(
            "say what went wrong — the recipient has to be able to answer it".into(),
        ));
    }

    #[derive(sqlx::FromRow)]
    struct Hold {
        id: Uuid,
        beneficiary_id: Uuid,
        payer_id: Option<Uuid>,
        released_at: Option<chrono::DateTime<chrono::Utc>>,
        disputed_at: Option<chrono::DateTime<chrono::Utc>>,
    }

    let hold: Option<Hold> = sqlx::query_as(
        "SELECT id, beneficiary_id, payer_id, released_at, disputed_at
           FROM pending_releases
          WHERE subject_type = $1 AND subject_id = $2",
    )
    .bind(subject_type)
    .bind(subject_id)
    .fetch_optional(db)
    .await?;

    let Some(hold) = hold else {
        return Err(AppError::NotFound(
            "no payment is being held for this".into(),
        ));
    };

    // Not `Forbidden` with a vague message: the payer needs to know whether
    // they are too late or simply not the right person.
    if hold.released_at.is_some() {
        return Err(AppError::Validation(
            "these funds were released before this was raised — this is now a \
             refund request, which support handles"
                .into(),
        ));
    }
    if hold.payer_id != Some(raised_by) {
        return Err(AppError::Forbidden);
    }
    if hold.disputed_at.is_some() {
        return Err(AppError::Conflict(
            "this payment is already being disputed".into(),
        ));
    }

    // Freeze first. If the row below fails, the money is frozen with no
    // dispute attached — visible and correctable. The other order would
    // leave a dispute over money that is still on its way out.
    crate::services::release::dispute(db, subject_type, subject_id).await?;

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO disputes (pending_release_id, raised_by, reason)
         VALUES ($1, $2, $3)
         RETURNING id",
    )
    .bind(hold.id)
    .bind(raised_by)
    .bind(reason.trim())
    .fetch_one(db)
    .await?;

    metrics::counter!(
        "skilluv_disputes_raised_total",
        "subject" => subject_type.to_string()
    )
    .increment(1);
    tracing::info!(
        dispute = %id,
        subject = subject_type,
        payer = %raised_by,
        recipient = %hold.beneficiary_id,
        "payment disputed — funds frozen"
    );

    Ok(id)
}

/// Load a dispute with the money underneath it.
pub async fn load(db: &PgPool, dispute_id: Uuid) -> Result<DisputedHold, AppError> {
    sqlx::query_as(
        "SELECT d.id AS dispute_id,
                d.pending_release_id,
                d.status,
                p.beneficiary_id,
                p.payer_id,
                p.subject_type,
                p.subject_id,
                p.amount,
                p.currency
           FROM disputes d
           JOIN pending_releases p ON p.id = d.pending_release_id
          WHERE d.id = $1",
    )
    .bind(dispute_id)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| AppError::NotFound("dispute".into()))
}

/// The recipient agrees the payer is right. Refunds immediately.
///
/// No operator involved, which is the outcome worth designing for: it is
/// faster for both parties and costs the platform nothing.
pub async fn concede(db: &PgPool, dispute_id: Uuid, by: Uuid) -> Result<(), AppError> {
    let hold = load(db, dispute_id).await?;
    if hold.beneficiary_id != by {
        return Err(AppError::Forbidden);
    }
    if hold.status != "open" {
        return Err(AppError::Conflict(format!(
            "this dispute is {}, not open",
            hold.status
        )));
    }
    settle(db, &hold, Outcome::Payer, None, None).await
}

/// The recipient disagrees. A human decides.
pub async fn contest(
    db: &PgPool,
    dispute_id: Uuid,
    by: Uuid,
    response: &str,
) -> Result<(), AppError> {
    if response.trim().chars().count() < 10 {
        return Err(AppError::Validation(
            "say why you disagree — an operator has to decide between two accounts".into(),
        ));
    }

    let hold = load(db, dispute_id).await?;
    if hold.beneficiary_id != by {
        return Err(AppError::Forbidden);
    }
    if hold.status != "open" {
        return Err(AppError::Conflict(format!(
            "this dispute is {}, not open",
            hold.status
        )));
    }

    sqlx::query(
        "UPDATE disputes
            SET status = 'contested', recipient_response = $2, responded_at = NOW()
          WHERE id = $1",
    )
    .bind(dispute_id)
    .bind(response.trim())
    .execute(db)
    .await?;

    metrics::counter!("skilluv_disputes_contested_total").increment(1);
    Ok(())
}

/// The payer changes their mind. The money resumes its normal course.
pub async fn withdraw(db: &PgPool, dispute_id: Uuid, by: Uuid) -> Result<(), AppError> {
    let hold = load(db, dispute_id).await?;
    if hold.payer_id != Some(by) {
        return Err(AppError::Forbidden);
    }
    if !matches!(hold.status.as_str(), "open" | "contested") {
        return Err(AppError::Conflict(format!(
            "this dispute is {}, and cannot be withdrawn",
            hold.status
        )));
    }
    settle(db, &hold, Outcome::Recipient, None, Some("withdrawn")).await
}

/// An operator decides a contested dispute.
///
/// The note is not optional: both parties read it, and "resolved" with no
/// reason is how a marketplace loses the trust of the side that lost.
pub async fn decide(
    db: &PgPool,
    dispute_id: Uuid,
    operator: Uuid,
    outcome: Outcome,
    note: &str,
) -> Result<(), AppError> {
    if note.trim().chars().count() < 10 {
        return Err(AppError::Validation(
            "both parties read this decision — say what it is based on".into(),
        ));
    }

    let hold = load(db, dispute_id).await?;
    if hold.status != "contested" {
        return Err(AppError::Conflict(format!(
            "this dispute is {}, and needs no decision",
            hold.status
        )));
    }
    settle(db, &hold, outcome, Some((operator, note.trim())), None).await
}

/// Move the money and close the dispute.
///
/// One function for every ending, so the ledger movement and the row can
/// never disagree about who won.
async fn settle(
    db: &PgPool,
    hold: &DisputedHold,
    outcome: Outcome,
    operator: Option<(Uuid, &str)>,
    override_status: Option<&str>,
) -> Result<(), AppError> {
    let currency: Currency = hold.currency.parse()?;

    match outcome {
        Outcome::Recipient => {
            ledger::resolve_dispute_for_recipient(
                db,
                hold.beneficiary_id,
                hold.amount.clone(),
                currency,
                &hold.subject_type,
                hold.subject_id,
            )
            .await?;
        }
        Outcome::Payer => {
            // At the provider first. `refund_from_dispute` writes entries
            // saying the money has left the provider's float; if nothing
            // tells the provider, the books say refunded and the card was
            // never credited — the one accounting error a customer notices
            // before we do.
            //
            // A provider that cannot refund returns an error, and the
            // failure stops here rather than moving our books into a state
            // the money is not in.
            let registry = crate::services::collect_adapters::registry_from_env();
            let refunded = crate::services::collect::refund(
                db,
                &registry,
                &hold.subject_type,
                hold.subject_id,
                "dispute settled for the payer",
            )
            .await?;

            if refunded.is_none() {
                // No recorded charge — a payment from before `payments`
                // existed. The books still have to move, because the money
                // is certainly not the recipient's, but somebody has to
                // give it back by hand.
                tracing::error!(
                    dispute = %hold.dispute_id,
                    subject = %hold.subject_type,
                    "refunded in the books with no provider charge to reverse — refund this by hand"
                );
                metrics::counter!("skilluv_dispute_manual_refund_needed_total").increment(1);
            }

            // The platform's cut goes back with the rest: taking a
            // commission on a service that was not delivered is not
            // defensible, and reconciling a partial refund later is worse
            // than not taking it now.
            ledger::refund_from_dispute(
                db,
                "stripe",
                hold.beneficiary_id,
                hold.amount.clone(),
                BigDecimal::from(0),
                currency,
                &hold.subject_type,
                hold.subject_id,
            )
            .await?;
        }
    }

    let status = override_status.unwrap_or(match outcome {
        Outcome::Payer => "refunded",
        Outcome::Recipient => "released",
    });

    sqlx::query(
        "UPDATE disputes
            SET status = $2, resolved_at = NOW(), resolved_by = $3, resolution_note = $4
          WHERE id = $1",
    )
    .bind(hold.dispute_id)
    .bind(status)
    .bind(operator.map(|(id, _)| id))
    .bind(operator.map(|(_, note)| note))
    .execute(db)
    .await?;

    // The hold is finished either way. Stamping `released_at` keeps the
    // sweep from ever looking at it again, whichever side won — the money
    // has already moved here, and a second release would move it twice.
    sqlx::query(
        "UPDATE pending_releases SET released_at = NOW(), disputed_at = NULL WHERE id = $1",
    )
    .bind(hold.pending_release_id)
    .execute(db)
    .await?;

    metrics::counter!(
        "skilluv_disputes_settled_total",
        "outcome" => match outcome { Outcome::Payer => "payer", Outcome::Recipient => "recipient" },
        "by_operator" => operator.is_some().to_string()
    )
    .increment(1);

    Ok(())
}

/// Every dispute one person is party to, from either side.
pub async fn for_user(db: &PgPool, user_id: Uuid) -> Result<Vec<Dispute>, AppError> {
    #[derive(sqlx::FromRow)]
    struct Row {
        id: Uuid,
        status: String,
        reason: String,
        recipient_response: Option<String>,
        resolution_note: Option<String>,
        subject_type: String,
        subject_id: Uuid,
        amount: BigDecimal,
        currency: String,
        created_at: chrono::DateTime<chrono::Utc>,
        resolved_at: Option<chrono::DateTime<chrono::Utc>>,
    }

    let rows: Vec<Row> = sqlx::query_as(
        "SELECT d.id, d.status, d.reason, d.recipient_response, d.resolution_note,
                p.subject_type, p.subject_id, p.amount, p.currency,
                d.created_at, d.resolved_at
           FROM disputes d
           JOIN pending_releases p ON p.id = d.pending_release_id
          WHERE d.raised_by = $1 OR p.beneficiary_id = $1
          ORDER BY d.created_at DESC
          LIMIT 200",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| Dispute {
            id: r.id,
            status: r.status,
            reason: r.reason,
            recipient_response: r.recipient_response,
            resolution_note: r.resolution_note,
            subject_type: r.subject_type,
            subject_id: r.subject_id,
            amount: r.amount.to_string(),
            currency: r.currency,
            created_at: r.created_at,
            resolved_at: r.resolved_at,
        })
        .collect())
}
