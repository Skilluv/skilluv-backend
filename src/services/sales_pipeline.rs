//! The internal sales pipeline, and what a company actually has with us.
//!
//! ## Deliberately small
//!
//! Ticket 14-08 asks for a CRM. There is no sales team — three volunteers, no
//! users, no revenue — and building a Salesforce would be building for a
//! company we are not. What is here is the part that does not depend on
//! headcount: who we are talking to, what was said, what is due to renew.
//!
//! No lead scoring, no forecast, no territories. Those can be added when
//! there is somebody whose job it is to want them, and adding them now would
//! mean maintaining a model of a sales process nobody has run yet.
//!
//! ## Renewals are read, never stored
//!
//! Every recurring product already knows when it lapses. A renewal date here
//! would be a copy of six other columns, wrong the first time one of them
//! moved — so `upcoming_renewals` is a view over the products themselves.

use bigdecimal::BigDecimal;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

pub const STAGES: &[&str] = &[
    "lead",
    "qualified",
    "proposal",
    "negotiation",
    "won",
    "lost",
];

pub const ACTIVITY_KINDS: &[&str] = &["call", "email", "meeting", "demo", "proposal_sent", "note"];

/// How far through the pipeline a stage is, as a fraction.
///
/// Used for a weighted total rather than for a forecast: with no closed deals
/// to calibrate against, these are guesses and are labelled as such wherever
/// they are shown.
pub fn stage_weight(stage: &str) -> f64 {
    match stage {
        "lead" => 0.1,
        "qualified" => 0.25,
        "proposal" => 0.5,
        "negotiation" => 0.75,
        "won" => 1.0,
        _ => 0.0,
    }
}

/// The weighted value of a pipeline.
///
/// Honest about what it is: a sum of guesses. It exists so a small team can
/// see whether it is talking to two companies or twenty, not so anybody can
/// put a number in a board deck.
pub fn weighted_value(open: &[(String, BigDecimal)]) -> BigDecimal {
    let mut total = BigDecimal::from(0);
    for (stage, value) in open {
        let weight = BigDecimal::try_from(stage_weight(stage)).unwrap_or_default();
        total += value * weight;
    }
    total.with_scale_round(2, bigdecimal::RoundingMode::HalfUp)
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Opportunity {
    pub id: Uuid,
    pub enterprise_id: Option<Uuid>,
    pub org_name: String,
    pub contact_name: Option<String>,
    pub contact_email: Option<String>,
    pub product_type: Option<String>,
    pub estimated_value: Option<BigDecimal>,
    pub currency: String,
    pub stage: String,
    pub lost_reason: Option<String>,
    pub owner_user_id: Option<Uuid>,
    pub expected_close_on: Option<chrono::NaiveDate>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

const OPPORTUNITY_SELECT: &str = r#"
    SELECT id, enterprise_id, org_name, contact_name, contact_email, product_type,
           estimated_value, currency, stage, lost_reason, owner_user_id,
           expected_close_on, created_at
      FROM sales_opportunities
"#;

#[derive(Debug, Clone, Deserialize)]
pub struct OpportunityInput {
    pub org_name: String,
    #[serde(default)]
    pub enterprise_id: Option<Uuid>,
    #[serde(default)]
    pub contact_name: Option<String>,
    #[serde(default)]
    pub contact_email: Option<String>,
    #[serde(default)]
    pub product_type: Option<String>,
    #[serde(default)]
    pub estimated_value: Option<BigDecimal>,
    #[serde(default = "eur")]
    pub currency: String,
    #[serde(default)]
    pub expected_close_on: Option<chrono::NaiveDate>,
}

fn eur() -> String {
    "EUR".into()
}

pub async fn open_opportunity(
    db: &PgPool,
    owner: Uuid,
    input: OpportunityInput,
) -> Result<Opportunity, AppError> {
    if input.org_name.trim().is_empty() {
        return Err(AppError::Validation("name the organisation".into()));
    }

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO sales_opportunities
            (enterprise_id, org_name, contact_name, contact_email, product_type,
             estimated_value, currency, expected_close_on, owner_user_id)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
         RETURNING id",
    )
    .bind(input.enterprise_id)
    .bind(input.org_name.trim())
    .bind(input.contact_name.as_deref())
    .bind(input.contact_email.as_deref())
    .bind(input.product_type.as_deref())
    .bind(input.estimated_value.as_ref())
    .bind(&input.currency)
    .bind(input.expected_close_on)
    .bind(owner)
    .fetch_one(db)
    .await?;

    opportunity(db, id).await
}

pub async fn opportunity(db: &PgPool, id: Uuid) -> Result<Opportunity, AppError> {
    let sql = format!("{OPPORTUNITY_SELECT} WHERE id = $1");
    sqlx::query_as::<_, Opportunity>(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NotFound("opportunity not found".into()))
}

pub async fn pipeline(db: &PgPool) -> Result<Vec<Opportunity>, AppError> {
    let sql = format!(
        "{OPPORTUNITY_SELECT} WHERE stage NOT IN ('won', 'lost')
          ORDER BY expected_close_on NULLS LAST, created_at"
    );
    let rows = sqlx::query_as::<_, Opportunity>(sqlx::AssertSqlSafe(sql))
        .fetch_all(db)
        .await?;
    Ok(rows)
}

/// Move an opportunity along, or close it.
pub async fn set_stage(
    db: &PgPool,
    id: Uuid,
    stage: &str,
    lost_reason: Option<&str>,
) -> Result<Opportunity, AppError> {
    if !STAGES.contains(&stage) {
        return Err(AppError::Validation(format!(
            "stage must be one of: {}",
            STAGES.join(", ")
        )));
    }
    if stage == "lost" && lost_reason.map(str::trim).unwrap_or("").is_empty() {
        return Err(AppError::Validation(
            "say why it was lost. A pipeline that records wins and shrugs at losses \
             teaches nothing."
                .into(),
        ));
    }

    sqlx::query(
        "UPDATE sales_opportunities
            SET stage = $2, lost_reason = $3,
                closed_at = CASE WHEN $2 IN ('won', 'lost') THEN NOW() ELSE NULL END
          WHERE id = $1",
    )
    .bind(id)
    .bind(stage)
    .bind(lost_reason.map(str::trim).filter(|r| !r.is_empty()))
    .execute(db)
    .await?;

    opportunity(db, id).await
}

#[derive(Debug, Clone, Deserialize)]
pub struct ActivityInput {
    pub kind: String,
    pub summary_md: String,
    #[serde(default)]
    pub next_step: Option<String>,
    #[serde(default)]
    pub next_step_due_on: Option<chrono::NaiveDate>,
}

pub async fn record_activity(
    db: &PgPool,
    opportunity_id: Uuid,
    author: Uuid,
    input: ActivityInput,
) -> Result<Uuid, AppError> {
    if !ACTIVITY_KINDS.contains(&input.kind.as_str()) {
        return Err(AppError::Validation(format!(
            "kind must be one of: {}",
            ACTIVITY_KINDS.join(", ")
        )));
    }
    if input.summary_md.trim().is_empty() {
        return Err(AppError::Validation("say what happened".into()));
    }

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO sales_activities
            (opportunity_id, kind, summary_md, next_step, next_step_due_on,
             author_user_id)
         VALUES ($1,$2,$3,$4,$5,$6)
         RETURNING id",
    )
    .bind(opportunity_id)
    .bind(&input.kind)
    .bind(input.summary_md.trim())
    .bind(input.next_step.as_deref())
    .bind(input.next_step_due_on)
    .bind(author)
    .fetch_one(db)
    .await?;

    Ok(id)
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Activity {
    pub id: Uuid,
    pub kind: String,
    pub summary_md: String,
    pub next_step: Option<String>,
    pub next_step_due_on: Option<chrono::NaiveDate>,
    pub happened_at: chrono::DateTime<chrono::Utc>,
}

pub async fn activities(db: &PgPool, opportunity_id: Uuid) -> Result<Vec<Activity>, AppError> {
    let rows = sqlx::query_as::<_, Activity>(
        "SELECT id, kind, summary_md, next_step, next_step_due_on, happened_at
           FROM sales_activities WHERE opportunity_id = $1
          ORDER BY happened_at DESC",
    )
    .bind(opportunity_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// Everything somebody said they would do and has not.
pub async fn overdue_next_steps(db: &PgPool) -> Result<Vec<serde_json::Value>, AppError> {
    let rows = sqlx::query_scalar(
        "SELECT jsonb_build_object(
                    'opportunity_id', a.opportunity_id,
                    'org_name', o.org_name,
                    'next_step', a.next_step,
                    'due_on', a.next_step_due_on
                )
           FROM sales_activities a
           JOIN sales_opportunities o ON o.id = a.opportunity_id
          WHERE a.next_step_due_on IS NOT NULL
            AND a.next_step_due_on <= CURRENT_DATE
            AND o.stage NOT IN ('won', 'lost')
          ORDER BY a.next_step_due_on",
    )
    .fetch_all(db)
    .await?;
    Ok(rows)
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Renewal {
    pub product: String,
    pub enterprise_id: Option<Uuid>,
    pub source_id: Uuid,
    pub renews_at: Option<chrono::DateTime<chrono::Utc>>,
    pub value: Option<BigDecimal>,
    pub currency: String,
}

/// What lapses in the next N days, read from the products themselves.
pub async fn renewals_within(db: &PgPool, days: i64) -> Result<Vec<Renewal>, AppError> {
    let rows = sqlx::query_as::<_, Renewal>(
        "SELECT product, enterprise_id, source_id, renews_at, value, currency
           FROM upcoming_renewals
          WHERE renews_at IS NOT NULL
            AND renews_at BETWEEN NOW() AND NOW() + ($1 || ' days')::INTERVAL
          ORDER BY renews_at",
    )
    .bind(days.to_string())
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// Everything one company currently has with Skilluv.
///
/// The question a dashboard and a renewal conversation both start with, and
/// the reason `enterprise_products` exists: every product registers there, so
/// this is one query rather than eighteen.
pub async fn products_of(
    db: &PgPool,
    enterprise_id: Uuid,
) -> Result<Vec<serde_json::Value>, AppError> {
    let rows = sqlx::query_scalar(
        "SELECT jsonb_build_object(
                    'product_type', p.product_type,
                    'label', t.label,
                    'pillar', r.pillar,
                    'status', p.status,
                    'contract_value', p.contract_value,
                    'currency', p.currency,
                    'recurring', t.recurring,
                    'source_table', p.source_table,
                    'source_id', p.source_id,
                    'since', p.created_at
                )
           FROM enterprise_products p
           JOIN enterprise_product_types t ON t.slug = p.product_type
           LEFT JOIN revenue_streams r ON r.slug = t.revenue_stream
          WHERE p.enterprise_id = $1
          ORDER BY p.created_at DESC",
    )
    .bind(enterprise_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// What a company has spent with us, by stream.
pub async fn spend_by_stream(
    db: &PgPool,
    enterprise_id: Uuid,
) -> Result<Vec<serde_json::Value>, AppError> {
    let rows = sqlx::query_scalar(
        "SELECT jsonb_build_object(
                    'stream', pr.source,
                    'label', r.label,
                    'pillar', r.pillar,
                    'total', sum(pr.amount_credits),
                    'entries', count(*)
                )
           FROM platform_revenues pr
           LEFT JOIN revenue_streams r ON r.slug = pr.source
          WHERE pr.related_enterprise_id = $1
          GROUP BY pr.source, r.label, r.pillar
          ORDER BY sum(pr.amount_credits) DESC NULLS LAST",
    )
    .bind(enterprise_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// Products a company does not have, in pillars they already buy from.
///
/// A suggestion, not a recommendation engine. It exists so a renewal
/// conversation has somewhere obvious to go, and it deliberately does not
/// rank: with no sales history there is nothing to rank against, and a
/// confident ordering built on nothing is worse than a list.
pub async fn unused_products_in_familiar_pillars(
    db: &PgPool,
    enterprise_id: Uuid,
) -> Result<Vec<serde_json::Value>, AppError> {
    let rows = sqlx::query_scalar(
        "SELECT jsonb_build_object('product_type', t.slug, 'label', t.label,
                                   'pillar', r.pillar)
           FROM enterprise_product_types t
           JOIN revenue_streams r ON r.slug = t.revenue_stream
          WHERE r.pillar IN (
                    SELECT DISTINCT r2.pillar
                      FROM enterprise_products p2
                      JOIN enterprise_product_types t2 ON t2.slug = p2.product_type
                      JOIN revenue_streams r2 ON r2.slug = t2.revenue_stream
                     WHERE p2.enterprise_id = $1
                )
            AND t.slug NOT IN (
                    SELECT product_type FROM enterprise_products WHERE enterprise_id = $1
                )
          ORDER BY r.pillar, t.label",
    )
    .bind(enterprise_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn dec(s: &str) -> BigDecimal {
        BigDecimal::from_str(s).unwrap()
    }

    #[test]
    fn a_stage_further_along_weighs_more() {
        assert!(stage_weight("negotiation") > stage_weight("proposal"));
        assert!(stage_weight("proposal") > stage_weight("qualified"));
        assert!(stage_weight("qualified") > stage_weight("lead"));
        assert_eq!(stage_weight("won"), 1.0);
    }

    #[test]
    fn a_lost_deal_weighs_nothing() {
        assert_eq!(stage_weight("lost"), 0.0);
        assert_eq!(stage_weight("something_else"), 0.0);
    }

    #[test]
    fn a_weighted_pipeline_is_smaller_than_its_face_value() {
        let open = vec![
            ("lead".to_string(), dec("10000")),
            ("negotiation".to_string(), dec("10000")),
        ];
        let weighted = weighted_value(&open);
        assert_eq!(weighted, dec("8500.00"));
        assert!(weighted < dec("20000"));
    }

    #[test]
    fn an_empty_pipeline_is_worth_nothing() {
        assert_eq!(weighted_value(&[]), dec("0.00"));
    }

    #[test]
    fn every_stage_and_activity_kind_is_a_known_one() {
        assert_eq!(STAGES.len(), 6);
        assert_eq!(ACTIVITY_KINDS.len(), 6);
        assert!(STAGES.contains(&"negotiation"));
    }
}
