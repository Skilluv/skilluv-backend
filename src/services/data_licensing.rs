//! What Skilluv licenses: the data, and the platform.
//!
//! Two products that look different and share one discipline. A licence sells
//! something about people who are not the customer, so it runs on their
//! consent and pays them a share; a white-label deployment sells the platform
//! itself, and when the partner is a state it also lends Skilluv's
//! attestations an official weight that has to rest on a signed contract.
//!
//! ## Royalties are why people said yes
//!
//! A commercial licence pays a share back to everybody whose consent it runs
//! on, divided between them. It is not a gesture: it is the term of the
//! agreement, and a licence booked without it would be revenue taken from
//! people who were told they would be paid.
//!
//! The cohort size is recorded on each royalty row because the share was
//! worked out from it. Somebody consenting next month must not change what
//! was already paid this one.

use bigdecimal::BigDecimal;
use bigdecimal::num_traits::Signed;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::{data_consent, ledger};

pub const LICENSEE_TYPES: &[&str] = &[
    "research_lab",
    "university",
    "government",
    "development_bank",
    "enterprise",
    "ngo",
];

pub const PARTNER_TYPES: &[&str] = &[
    "university",
    "bootcamp",
    "coding_school",
    "corporate_academy",
    "government",
];

/// What each person in a licence is owed for a period.
///
/// The whole share divided by the cohort, then rounded down; the remainder
/// stays with the platform only because it cannot be divided further, and it
/// is at most one centime per period.
///
/// Returns nothing when the cohort is empty rather than dividing by zero: a
/// licence with nobody in it should not have been signed, and inventing a
/// payment would hide that.
pub fn royalty_each(
    total_fee: &BigDecimal,
    share_percent: &BigDecimal,
    cohort_size: i64,
) -> BigDecimal {
    if cohort_size <= 0 || !share_percent.is_positive() {
        return BigDecimal::from(0);
    }
    let pot = total_fee * share_percent / BigDecimal::from(100);
    (pot / BigDecimal::from(cohort_size)).with_scale_round(2, bigdecimal::RoundingMode::Down)
}

/// What a white-label deployment costs in its first year.
///
/// Setup plus twelve months, or the annual figure when one was negotiated.
/// Stated once so a quote and an invoice cannot disagree.
pub fn first_year_cost(
    setup_fee: &BigDecimal,
    monthly_fee: &BigDecimal,
    annual_fee: Option<&BigDecimal>,
) -> BigDecimal {
    match annual_fee {
        Some(annual) if annual.is_positive() => setup_fee + annual,
        _ => setup_fee + monthly_fee * BigDecimal::from(12),
    }
}

// ═══════════════════════════════════════════════════════════════════
// Reports
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Report {
    pub id: Uuid,
    pub client_type: String,
    pub client_org: String,
    pub title: String,
    pub scope_md: String,
    pub delivery_formats: Vec<String>,
    pub fee: BigDecimal,
    pub currency: String,
    pub minimum_cohort_size: i32,
    pub status: String,
    pub document_url: Option<String>,
    pub delivered_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

const REPORT_SELECT: &str = r#"
    SELECT id, client_type, client_org, title, scope_md, delivery_formats, fee,
           currency, minimum_cohort_size, status, document_url, delivered_at,
           created_at
      FROM talent_intelligence_reports
"#;

#[derive(Debug, Clone, Deserialize)]
pub struct ReportInput {
    pub client_type: String,
    pub client_org: String,
    pub title: String,
    pub scope_md: String,
    #[serde(default)]
    pub delivery_formats: Option<Vec<String>>,
    pub fee: BigDecimal,
    #[serde(default = "eur")]
    pub currency: String,
    #[serde(default)]
    pub enterprise_id: Option<Uuid>,
}

fn eur() -> String {
    "EUR".into()
}

pub async fn commission_report(
    db: &PgPool,
    author: Uuid,
    input: ReportInput,
) -> Result<Report, AppError> {
    if input.scope_md.trim().is_empty() {
        return Err(AppError::Validation(
            "say what the report is meant to answer. A scope written after the fact is \
             a scope argued about at delivery."
                .into(),
        ));
    }
    crate::validators::check_max_len(&input.title, "title", 200)?;
    crate::validators::check_max_len(&input.scope_md, "scope_md", 20_000)?;

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO talent_intelligence_reports
            (client_type, client_org, enterprise_id, title, scope_md,
             delivery_formats, fee, currency, author_user_id)
         VALUES ($1,$2,$3,$4,$5,COALESCE($6,'{pdf}'),$7,$8,$9)
         RETURNING id",
    )
    .bind(&input.client_type)
    .bind(input.client_org.trim())
    .bind(input.enterprise_id)
    .bind(input.title.trim())
    .bind(input.scope_md.trim())
    .bind(input.delivery_formats.as_ref())
    .bind(&input.fee)
    .bind(&input.currency)
    .bind(author)
    .fetch_one(db)
    .await?;

    report(db, id).await
}

pub async fn report(db: &PgPool, id: Uuid) -> Result<Report, AppError> {
    let sql = format!("{REPORT_SELECT} WHERE id = $1");
    sqlx::query_as::<_, Report>(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NotFound("report not found".into()))
}

pub async fn reports(db: &PgPool) -> Result<Vec<Report>, AppError> {
    let sql = format!("{REPORT_SELECT} ORDER BY created_at DESC LIMIT 200");
    let rows = sqlx::query_as::<_, Report>(sqlx::AssertSqlSafe(sql))
        .fetch_all(db)
        .await?;
    Ok(rows)
}

/// Deliver a report, and book what it was sold for.
///
/// Refused when the population it rests on is too small to publish. The
/// commercial pressure runs the other way — a client asking for "Cotonou,
/// backend, three years' experience" wants exactly the slice that names four
/// people — so the check is here and not in a style guide.
pub async fn deliver_report(
    db: &PgPool,
    id: Uuid,
    document_url: &str,
    purpose: &str,
) -> Result<BigDecimal, AppError> {
    let report = report(db, id).await?;
    if report.status == "delivered" {
        return Err(AppError::Validation("already delivered".into()));
    }
    if !document_url.starts_with("https://") {
        return Err(AppError::Validation(
            "the document URL must be https".into(),
        ));
    }

    let cohort = data_consent::cohort_size(db, purpose).await?;
    if cohort < report.minimum_cohort_size as i64 {
        return Err(AppError::Validation(format!(
            "this report would rest on {cohort} people and its floor is {}. A figure \
             drawn from fewer names them, whatever the header says.",
            report.minimum_cohort_size
        )));
    }

    let mut tx = db.begin().await?;
    sqlx::query(
        "UPDATE talent_intelligence_reports
            SET status = 'delivered', document_url = $2, delivered_at = NOW()
          WHERE id = $1",
    )
    .bind(id)
    .bind(document_url.trim())
    .execute(&mut *tx)
    .await?;

    if report.fee.is_positive() {
        sqlx::query(
            "INSERT INTO platform_revenues
                (source, related_enterprise_id, amount_credits, fee_rate_bps, notes)
             VALUES ('intelligence_report', NULL, $1, 10000, $2)",
        )
        .bind(&report.fee)
        .bind(format!("{} — {}", report.client_org, report.title))
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(report.fee)
}

// ═══════════════════════════════════════════════════════════════════
// Licensing contracts
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct LicenceContract {
    pub id: Uuid,
    pub licensee_org: String,
    pub licensee_type: String,
    pub purpose: String,
    pub contract_purpose_md: String,
    pub data_scope: serde_json::Value,
    pub starts_on: chrono::NaiveDate,
    pub ends_on: Option<chrono::NaiveDate>,
    pub total_fee: BigDecimal,
    pub currency: String,
    pub talents_share_percent: BigDecimal,
    pub status: String,
    pub signed_at: Option<chrono::DateTime<chrono::Utc>>,
}

const LICENCE_SELECT: &str = r#"
    SELECT id, licensee_org, licensee_type, purpose, contract_purpose_md,
           data_scope, starts_on, ends_on, total_fee, currency,
           talents_share_percent, status, signed_at
      FROM data_licensing_contracts
"#;

#[derive(Debug, Clone, Deserialize)]
pub struct LicenceInput {
    pub licensee_org: String,
    pub licensee_type: String,
    pub purpose: String,
    pub contract_purpose_md: String,
    #[serde(default)]
    pub data_scope: Option<serde_json::Value>,
    pub starts_on: chrono::NaiveDate,
    #[serde(default)]
    pub ends_on: Option<chrono::NaiveDate>,
    pub total_fee: BigDecimal,
    #[serde(default = "eur")]
    pub currency: String,
    #[serde(default)]
    pub talents_share_percent: Option<BigDecimal>,
    #[serde(default)]
    pub contract_url: Option<String>,
}

pub async fn open_licence(db: &PgPool, input: LicenceInput) -> Result<LicenceContract, AppError> {
    if !LICENSEE_TYPES.contains(&input.licensee_type.as_str()) {
        return Err(AppError::Validation(format!(
            "licensee_type must be one of: {}",
            LICENSEE_TYPES.join(", ")
        )));
    }
    if !data_consent::PURPOSES.contains(&input.purpose.as_str()) {
        return Err(AppError::Validation(format!(
            "purpose must be one of: {}",
            data_consent::PURPOSES.join(", ")
        )));
    }
    if input.contract_purpose_md.trim().is_empty() {
        return Err(AppError::Validation(
            "say what the licensee will do with it. That sentence is what the people in \
             the dataset agreed to, and a blank cannot be agreed to."
                .into(),
        ));
    }

    // A licence with nobody in it should not be signed. Selling access to an
    // empty set is selling nothing, and the buyer finds out after paying.
    let cohort = data_consent::cohort_size(db, &input.purpose).await?;
    if !data_consent::cohort_is_publishable(cohort) {
        return Err(AppError::Validation(format!(
            "only {cohort} people have agreed to '{}'. Below {} the dataset names the \
             people in it, and there is nothing here that can honestly be licensed.",
            input.purpose,
            data_consent::COHORT_FLOOR
        )));
    }

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO data_licensing_contracts
            (licensee_org, licensee_type, purpose, contract_purpose_md, data_scope,
             starts_on, ends_on, total_fee, currency, talents_share_percent,
             contract_url, signed_at)
         VALUES ($1,$2,$3,$4,COALESCE($5,'{}'::jsonb),$6,$7,$8,$9,
                 COALESCE($10, 1.00), $11,
                 CASE WHEN $11::TEXT IS NULL THEN NULL ELSE NOW() END)
         RETURNING id",
    )
    .bind(input.licensee_org.trim())
    .bind(&input.licensee_type)
    .bind(&input.purpose)
    .bind(input.contract_purpose_md.trim())
    .bind(input.data_scope.as_ref())
    .bind(input.starts_on)
    .bind(input.ends_on)
    .bind(&input.total_fee)
    .bind(&input.currency)
    .bind(input.talents_share_percent.as_ref())
    .bind(input.contract_url.as_deref())
    .fetch_one(db)
    .await
    .map_err(|e| {
        if e.to_string()
            .contains("a_commercial_licence_pays_the_people_in_it")
        {
            AppError::Validation(
                "a commercial licence pays a share back to the people in it. Zero is \
                 defensible for a public research dataset and is not for a sale."
                    .into(),
            )
        } else {
            AppError::from(e)
        }
    })?;

    licence(db, id).await
}

pub async fn licence(db: &PgPool, id: Uuid) -> Result<LicenceContract, AppError> {
    let sql = format!("{LICENCE_SELECT} WHERE id = $1");
    sqlx::query_as::<_, LicenceContract>(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NotFound("contract not found".into()))
}

pub async fn licences(db: &PgPool) -> Result<Vec<LicenceContract>, AppError> {
    let sql = format!("{LICENCE_SELECT} ORDER BY starts_on DESC LIMIT 200");
    let rows = sqlx::query_as::<_, LicenceContract>(sqlx::AssertSqlSafe(sql))
        .fetch_all(db)
        .await?;
    Ok(rows)
}

/// Book the fee, and pay everybody in the dataset their share.
///
/// One transaction for the platform side; the ledger movements follow, so a
/// failure leaves an invoiced contract with visible unpaid royalties rather
/// than payments with nothing behind them.
pub async fn settle_period(
    db: &PgPool,
    contract_id: Uuid,
    period_start: chrono::NaiveDate,
    period_end: chrono::NaiveDate,
) -> Result<(i64, BigDecimal), AppError> {
    let contract = licence(db, contract_id).await?;
    if contract.status == "negotiating" {
        return Err(AppError::Validation(
            "this contract is not signed yet".into(),
        ));
    }
    if period_end <= period_start {
        return Err(AppError::Validation(
            "the period has to end after it starts".into(),
        ));
    }

    // The people covered are read from the consent rows at settlement, never
    // from a stored list. A stored list would be a copy of a decision people
    // can change, and it would pay somebody who withdrew last week.
    let cohort: Vec<Uuid> = sqlx::query_scalar(
        "SELECT user_id FROM talent_data_consent
          WHERE purpose = $1 AND revoked_at IS NULL",
    )
    .bind(&contract.purpose)
    .fetch_all(db)
    .await?;

    if cohort.is_empty() {
        return Err(AppError::Validation(
            "nobody is covered by this contract's purpose any more. Settling would pay \
             a share to nobody and book the fee anyway."
                .into(),
        ));
    }

    let each = royalty_each(
        &contract.total_fee,
        &contract.talents_share_percent,
        cohort.len() as i64,
    );

    let mut paid = 0i64;
    let currency: ledger::Currency = contract.currency.parse()?;

    for user_id in &cohort {
        let inserted = sqlx::query(
            "INSERT INTO talent_data_royalties
                (contract_id, user_id, period_start, period_end, amount, currency,
                 cohort_size)
             VALUES ($1,$2,$3,$4,$5,$6,$7)
             ON CONFLICT (contract_id, user_id, period_start) DO NOTHING",
        )
        .bind(contract_id)
        .bind(user_id)
        .bind(period_start)
        .bind(period_end)
        .bind(&each)
        .bind(&contract.currency)
        .bind(cohort.len() as i32)
        .execute(db)
        .await?;

        if inserted.rows_affected() == 0 || !each.is_positive() {
            continue;
        }

        ledger::capture_for_recipient(
            db,
            "stripe",
            format!("royalty:{contract_id}:{user_id}:{period_start}"),
            *user_id,
            each.clone(),
            BigDecimal::from(0),
            currency,
            "data_royalty",
            contract_id,
        )
        .await?;

        sqlx::query(
            "UPDATE talent_data_royalties SET paid_at = NOW()
              WHERE contract_id = $1 AND user_id = $2 AND period_start = $3",
        )
        .bind(contract_id)
        .bind(user_id)
        .bind(period_start)
        .execute(db)
        .await?;

        paid += 1;
    }

    // What Skilluv keeps: the fee less what went back to the people in it.
    let to_talents = &each * BigDecimal::from(cohort.len() as i64);
    let kept = &contract.total_fee - &to_talents;

    if kept.is_positive() {
        sqlx::query(
            "INSERT INTO platform_revenues
                (source, amount_credits, fee_rate_bps, notes)
             VALUES ('data_licensing', $1, $2, $3)",
        )
        .bind(&kept)
        .bind(ledger::percent_to_bps(
            &(BigDecimal::from(100) - &contract.talents_share_percent),
        ))
        .bind(format!(
            "licence {} — {} à {}",
            contract.licensee_org, period_start, period_end
        ))
        .execute(db)
        .await?;
    }

    Ok((paid, each))
}

// ═══════════════════════════════════════════════════════════════════
// White-label
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Deployment {
    pub id: Uuid,
    pub partner_org: String,
    pub partner_type: String,
    pub country: Option<String>,
    pub deployment_host: String,
    pub branding: serde_json::Value,
    pub features_enabled: Vec<String>,
    pub official_recognition_scope: Vec<String>,
    pub setup_fee: BigDecimal,
    pub monthly_fee: BigDecimal,
    pub annual_fee: Option<BigDecimal>,
    pub currency: String,
    pub users_limit: Option<i32>,
    pub status: String,
    pub launched_on: Option<chrono::NaiveDate>,
}

const DEPLOYMENT_SELECT: &str = r#"
    SELECT id, partner_org, partner_type, country, deployment_host, branding,
           features_enabled, official_recognition_scope, setup_fee, monthly_fee,
           annual_fee, currency, users_limit, status, launched_on
      FROM white_label_deployments
"#;

#[derive(Debug, Clone, Deserialize)]
pub struct DeploymentInput {
    pub partner_org: String,
    pub partner_type: String,
    #[serde(default)]
    pub country: Option<String>,
    pub deployment_host: String,
    #[serde(default)]
    pub branding: Option<serde_json::Value>,
    #[serde(default)]
    pub features_enabled: Option<Vec<String>>,
    #[serde(default)]
    pub official_recognition_scope: Vec<String>,
    #[serde(default)]
    pub setup_fee: Option<BigDecimal>,
    #[serde(default)]
    pub monthly_fee: Option<BigDecimal>,
    #[serde(default)]
    pub annual_fee: Option<BigDecimal>,
    #[serde(default = "eur")]
    pub currency: String,
    #[serde(default)]
    pub users_limit: Option<i32>,
    #[serde(default)]
    pub contract_url: Option<String>,
}

pub async fn provision(db: &PgPool, input: DeploymentInput) -> Result<Deployment, AppError> {
    if !PARTNER_TYPES.contains(&input.partner_type.as_str()) {
        return Err(AppError::Validation(format!(
            "partner_type must be one of: {}",
            PARTNER_TYPES.join(", ")
        )));
    }

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO white_label_deployments
            (partner_org, partner_type, country, deployment_host, branding,
             features_enabled, official_recognition_scope, setup_fee, monthly_fee,
             annual_fee, currency, users_limit, contract_url, signed_at)
         VALUES ($1,$2,$3,$4,COALESCE($5,'{}'::jsonb),
                 COALESCE($6,'{attestations,portfolio}'),$7,
                 COALESCE($8,0),COALESCE($9,0),$10,$11,$12,$13,
                 CASE WHEN $13::TEXT IS NULL THEN NULL ELSE NOW() END)
         RETURNING id",
    )
    .bind(input.partner_org.trim())
    .bind(&input.partner_type)
    .bind(input.country.as_deref())
    .bind(input.deployment_host.trim().to_lowercase())
    .bind(input.branding.as_ref())
    .bind(input.features_enabled.as_ref())
    .bind(&input.official_recognition_scope)
    .bind(input.setup_fee.as_ref())
    .bind(input.monthly_fee.as_ref())
    .bind(input.annual_fee.as_ref())
    .bind(&input.currency)
    .bind(input.users_limit)
    .bind(input.contract_url.as_deref())
    .fetch_one(db)
    .await
    .map_err(|e| {
        let m = e.to_string();
        if m.contains("only_a_government_recognises_officially") {
            AppError::Validation(
                "only a government partner can recognise anything officially — a \
                 bootcamp saying so is a claim, not a recognition"
                    .into(),
            )
        } else if m.contains("recognition_rests_on_a_signed_contract") {
            AppError::Validation(
                "official recognition rests on a signed contract with the state. \
                 Without one it is a claim, and the people carrying the attestation \
                 are the ones who find out it was worthless."
                    .into(),
            )
        } else if m.contains("white_label_deployments_deployment_host_key") {
            AppError::Validation("that host is already deployed".into())
        } else if m.contains("deployment_host") {
            AppError::Validation("the host has to be a domain name".into())
        } else {
            AppError::from(e)
        }
    })?;

    deployment(db, id).await
}

pub async fn deployment(db: &PgPool, id: Uuid) -> Result<Deployment, AppError> {
    let sql = format!("{DEPLOYMENT_SELECT} WHERE id = $1");
    sqlx::query_as::<_, Deployment>(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NotFound("deployment not found".into()))
}

pub async fn deployments(db: &PgPool) -> Result<Vec<Deployment>, AppError> {
    let sql = format!("{DEPLOYMENT_SELECT} ORDER BY created_at DESC");
    let rows = sqlx::query_as::<_, Deployment>(sqlx::AssertSqlSafe(sql))
        .fetch_all(db)
        .await?;
    Ok(rows)
}

/// Take a deployment live, and book the setup fee.
pub async fn go_live(db: &PgPool, id: Uuid) -> Result<BigDecimal, AppError> {
    let deployment = deployment(db, id).await?;
    if deployment.status == "live" {
        return Err(AppError::Validation("already live".into()));
    }

    let mut tx = db.begin().await?;
    sqlx::query(
        "UPDATE white_label_deployments
            SET status = 'live', launched_on = COALESCE(launched_on, CURRENT_DATE)
          WHERE id = $1",
    )
    .bind(id)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        if e.to_string().contains("a_live_deployment_is_signed") {
            AppError::Validation(
                "a deployment goes live on a signed contract, not on an intention".into(),
            )
        } else {
            AppError::from(e)
        }
    })?;

    if deployment.setup_fee.is_positive() {
        sqlx::query(
            "INSERT INTO platform_revenues
                (source, amount_credits, fee_rate_bps, notes)
             VALUES ('white_label_platform', $1, 10000, $2)",
        )
        .bind(&deployment.setup_fee)
        .bind(format!(
            "mise en service {} ({})",
            deployment.partner_org, deployment.deployment_host
        ))
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(deployment.setup_fee)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn dec(s: &str) -> BigDecimal {
        BigDecimal::from_str(s).unwrap()
    }

    #[test]
    fn a_royalty_divides_the_agreed_share_between_the_cohort() {
        // 1% of 100 000 is 1 000, split a hundred ways.
        assert_eq!(
            royalty_each(&dec("100000.00"), &dec("1.00"), 100),
            dec("10.00")
        );
    }

    #[test]
    fn a_royalty_rounds_down_rather_than_promising_a_centime_that_is_not_there() {
        // 1% of 1000 is 10, split three ways: 3.33 each, and the remaining
        // centime cannot be divided further.
        let each = royalty_each(&dec("1000.00"), &dec("1.00"), 3);
        assert_eq!(each, dec("3.33"));
        assert!(&each * BigDecimal::from(3) <= dec("10.00"));
    }

    #[test]
    fn an_empty_cohort_is_paid_nothing_rather_than_dividing_by_zero() {
        assert_eq!(royalty_each(&dec("100000.00"), &dec("1.00"), 0), dec("0"));
        assert_eq!(royalty_each(&dec("100000.00"), &dec("1.00"), -5), dec("0"));
    }

    #[test]
    fn a_zero_share_pays_nothing() {
        assert_eq!(royalty_each(&dec("100000.00"), &dec("0"), 100), dec("0"));
    }

    #[test]
    fn a_year_is_setup_plus_twelve_months_unless_a_year_was_negotiated() {
        assert_eq!(
            first_year_cost(&dec("20000"), &dec("2000"), None),
            dec("44000")
        );
        assert_eq!(
            first_year_cost(&dec("20000"), &dec("2000"), Some(&dec("18000"))),
            dec("38000")
        );
        // A zero annual figure is not a negotiated one.
        assert_eq!(
            first_year_cost(&dec("20000"), &dec("2000"), Some(&dec("0"))),
            dec("44000")
        );
    }

    #[test]
    fn every_type_is_a_known_one() {
        assert_eq!(LICENSEE_TYPES.len(), 6);
        assert_eq!(PARTNER_TYPES.len(), 5);
        assert!(PARTNER_TYPES.contains(&"government"));
    }
}
