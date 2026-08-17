//! What a subscription includes, and what is left of it.
//!
//! ## The distinction that runs through this file
//!
//! A **quota** is spent and does not come back — credits, campaigns, a bounty
//! pool. A **ceiling** is a limit measured against reality — ten open
//! positions at a time, not ten ever. A **discount** is a percentage applied
//! elsewhere. A **flag** is on or off.
//!
//! Conflating the first two is the mistake worth avoiding: a quota at zero
//! means "spent" and a ceiling at zero means "none allowed", and a dashboard
//! that shows them the same way lies about one of them.
//!
//! Ceilings are therefore never counted here. They are checked against the
//! thing they limit, at the moment somebody tries to exceed it — a counter
//! would drift the first time a row was deleted by anything but the endpoint
//! that decrements it.

use bigdecimal::BigDecimal;
use bigdecimal::num_traits::{Signed, ToPrimitive};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

/// Whether a nature has a balance that can run out.
///
/// Only a quota does. An unknown nature answers no — somebody added one to
/// the table and not here, and refusing is better than inventing a balance.
pub fn has_remainder(nature: &str) -> bool {
    nature == "quota"
}

/// Whether a nature must carry a figure.
///
/// Everything except a flag, which is on by existing. A quota with no figure
/// is an unlimited quota by accident, which is the expensive way round.
pub fn carries_an_amount(nature: &str) -> bool {
    matches!(nature, "quota" | "ceiling" | "discount")
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Entitlement {
    pub kind: String,
    pub label: String,
    pub description: String,
    pub nature: String,
    pub unit: Option<String>,
    pub granted: Option<BigDecimal>,
    pub consumed: BigDecimal,
    /// Which engagement granted it, so somebody asking why they have it has
    /// something to point at.
    pub product_id: Uuid,
    pub product_type: String,
}

const ENTITLEMENT_SELECT: &str = r#"
    SELECT k.slug AS kind, k.label, k.description, k.nature, k.unit,
           e.granted, e.consumed, e.product_id, p.product_type
      FROM enterprise_entitlements e
      JOIN entitlement_kinds k ON k.slug = e.kind
      JOIN enterprise_products p ON p.id = e.product_id
"#;

/// Everything an enterprise is entitled to under its live engagements.
///
/// Only `active` products count. An entitlement from a lapsed subscription is
/// history, and showing it as available would let somebody spend what they no
/// longer have.
pub async fn for_enterprise(
    db: &PgPool,
    enterprise_id: Uuid,
) -> Result<Vec<Entitlement>, AppError> {
    let sql = format!(
        "{ENTITLEMENT_SELECT}
         WHERE p.enterprise_id = $1 AND p.status = 'active'
         ORDER BY k.nature, k.slug"
    );
    let rows = sqlx::query_as::<_, Entitlement>(sqlx::AssertSqlSafe(sql))
        .bind(enterprise_id)
        .fetch_all(db)
        .await?;
    Ok(rows)
}

/// How much of a quota is left, across every live engagement.
///
/// Summed rather than taken from one row: a company on an annual programme
/// that also bought a credit pack has both, and asking "how many credits do
/// they have" must answer with the total.
pub async fn remaining(
    db: &PgPool,
    enterprise_id: Uuid,
    kind: &str,
) -> Result<Option<BigDecimal>, AppError> {
    let nature: Option<String> =
        sqlx::query_scalar("SELECT nature FROM entitlement_kinds WHERE slug = $1")
            .bind(kind)
            .fetch_optional(db)
            .await?;
    let Some(nature) = nature else {
        return Err(AppError::Validation(format!(
            "'{kind}' is not something a subscription grants"
        )));
    };
    if !has_remainder(&nature) {
        // A ceiling is a limit, not a balance. Asking for its remainder is a
        // bug in the caller, and answering zero would look like an exhausted
        // quota.
        return Err(AppError::Internal(format!(
            "'{kind}' is a {nature}, which has no remaining amount"
        )));
    }

    let total: Option<BigDecimal> = sqlx::query_scalar(
        "SELECT COALESCE(sum(e.granted - e.consumed), 0)
           FROM enterprise_entitlements e
           JOIN enterprise_products p ON p.id = e.product_id
          WHERE p.enterprise_id = $1 AND p.status = 'active' AND e.kind = $2",
    )
    .bind(enterprise_id)
    .bind(kind)
    .fetch_one(db)
    .await?;

    Ok(total)
}

/// Spend from a quota, oldest engagement first.
///
/// Oldest first so an entitlement that expires with its subscription is used
/// before one that does not — the opposite order silently wastes what was
/// about to lapse.
///
/// Returns what could not be covered. Zero means fully covered; anything else
/// is what the caller has to charge for separately, which is a decision for
/// them rather than an error here.
pub async fn consume(
    db: &PgPool,
    enterprise_id: Uuid,
    kind: &str,
    amount: BigDecimal,
) -> Result<BigDecimal, AppError> {
    if !amount.is_positive() {
        return Err(AppError::Validation(
            "spending nothing is not spending".into(),
        ));
    }

    let mut tx = db.begin().await?;

    // Locked, because two concurrent spends against the last of a quota would
    // both read the same remainder and both succeed.
    let rows: Vec<(Uuid, BigDecimal, BigDecimal)> = sqlx::query_as(
        "SELECT e.id, e.granted, e.consumed
           FROM enterprise_entitlements e
           JOIN enterprise_products p ON p.id = e.product_id
          WHERE p.enterprise_id = $1 AND p.status = 'active' AND e.kind = $2
            AND e.granted > e.consumed
          ORDER BY p.started_at ASC
          FOR UPDATE OF e",
    )
    .bind(enterprise_id)
    .bind(kind)
    .fetch_all(&mut *tx)
    .await?;

    let mut left = amount;
    for (id, granted, consumed) in rows {
        if !left.is_positive() {
            break;
        }
        let available = granted - consumed;
        let take = if available < left {
            available
        } else {
            left.clone()
        };

        sqlx::query("UPDATE enterprise_entitlements SET consumed = consumed + $2 WHERE id = $1")
            .bind(id)
            .bind(&take)
            .execute(&mut *tx)
            .await?;

        left -= take;
    }

    tx.commit().await?;
    Ok(left)
}

/// Whether a flag is set on any live engagement.
pub async fn has_flag(db: &PgPool, enterprise_id: Uuid, kind: &str) -> Result<bool, AppError> {
    let set: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM enterprise_entitlements e
               JOIN enterprise_products p ON p.id = e.product_id
              WHERE p.enterprise_id = $1 AND p.status = 'active' AND e.kind = $2)",
    )
    .bind(enterprise_id)
    .bind(kind)
    .fetch_one(db)
    .await?;
    Ok(set)
}

/// The best discount available, as a percentage.
///
/// The best rather than the sum: two subscriptions each granting ten per cent
/// do not make twenty, and adding them would eventually make a discount
/// exceed the price.
pub async fn best_discount(db: &PgPool, enterprise_id: Uuid, kind: &str) -> Result<f64, AppError> {
    let best: Option<BigDecimal> = sqlx::query_scalar(
        "SELECT max(e.granted)
           FROM enterprise_entitlements e
           JOIN enterprise_products p ON p.id = e.product_id
          WHERE p.enterprise_id = $1 AND p.status = 'active' AND e.kind = $2",
    )
    .bind(enterprise_id)
    .bind(kind)
    .fetch_one(db)
    .await?;

    Ok(best.and_then(|d| d.to_f64()).unwrap_or(0.0))
}

/// Whether a ceiling would be exceeded.
///
/// `current` is counted by the caller against the thing being limited, not
/// read from a column here. A ceiling with no live engagement granting it is
/// unlimited: nothing has said otherwise, and inventing a default would cap
/// somebody who never agreed to a cap.
pub async fn within_ceiling(
    db: &PgPool,
    enterprise_id: Uuid,
    kind: &str,
    current: i64,
) -> Result<bool, AppError> {
    let ceiling: Option<BigDecimal> = sqlx::query_scalar(
        "SELECT max(e.granted)
           FROM enterprise_entitlements e
           JOIN enterprise_products p ON p.id = e.product_id
          WHERE p.enterprise_id = $1 AND p.status = 'active' AND e.kind = $2",
    )
    .bind(enterprise_id)
    .bind(kind)
    .fetch_one(db)
    .await?;

    match ceiling.and_then(|c| c.to_i64()) {
        Some(limit) => Ok(current < limit),
        None => Ok(true),
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct GrantInput {
    pub kind: String,
    /// Absent for a flag, required for everything else.
    #[serde(default)]
    pub granted: Option<BigDecimal>,
}

/// Attach an entitlement to an engagement.
pub async fn grant(db: &PgPool, product_id: Uuid, input: &GrantInput) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO enterprise_entitlements (product_id, kind, granted)
         VALUES ($1, $2, $3)
         ON CONFLICT (product_id, kind) DO UPDATE
             SET granted = EXCLUDED.granted",
    )
    .bind(product_id)
    .bind(&input.kind)
    .bind(input.granted.as_ref())
    .execute(db)
    .await
    .map_err(nature_error)?;
    Ok(())
}

/// The trigger speaks in natures; this says the same in words the person
/// filling in the form can act on.
fn nature_error(e: sqlx::Error) -> AppError {
    let message = e.to_string();
    if message.contains("carries no amount") {
        return AppError::Validation("that entitlement is on or off — it takes no amount".into());
    }
    if message.contains("must say how much") {
        return AppError::Validation(
            "say how much: an entitlement with no figure is an unlimited one by accident".into(),
        );
    }
    if message.contains("nothing_is_overspent") {
        return AppError::Validation("that would grant less than has already been spent".into());
    }
    AppError::from(e)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_quota_has_a_remainder() {
        // The distinction the whole module turns on. A quota at zero means
        // "spent"; a ceiling at zero means "none allowed"; a dashboard that
        // shows them the same way lies about one of them.
        assert!(has_remainder("quota"));
        assert!(!has_remainder("ceiling"));
        assert!(!has_remainder("discount"));
        assert!(!has_remainder("flag"));
    }

    #[test]
    fn only_a_flag_carries_no_amount() {
        assert!(!carries_an_amount("flag"));
        for nature in ["quota", "ceiling", "discount"] {
            assert!(
                carries_an_amount(nature),
                "a {nature} with no figure is an unlimited one by accident"
            );
        }
    }

    #[test]
    fn an_unknown_nature_is_treated_as_carrying_nothing() {
        // Somebody added a nature to the table and not here. Answering
        // "no remainder, no amount" is the reading that refuses rather than
        // the one that invents a balance.
        assert!(!has_remainder("something_new"));
        assert!(!carries_an_amount("something_new"));
    }
}
