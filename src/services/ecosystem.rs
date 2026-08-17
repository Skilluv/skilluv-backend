//! The ecosystem line: labels, a creators marketplace, sponsored cohorts.
//!
//! ## Paying does not certify
//!
//! Four of these tickets sell a name: certified reviewer, certified partner,
//! certified studio, certified team. What is actually being sold is Skilluv's
//! word, spent on somebody else — and the person it misleads if the word is
//! wrong is not the buyer. It is the contributor who took the job because the
//! badge said the company pays fairly.
//!
//! So an audit is required before anything is issued, the score has to clear
//! the programme's pass mark, and the fee is booked at issue rather than at
//! order. Failing costs the fee and gets no badge, which is the only version
//! of this product that is worth anything.
//!
//! ## The marketplace commission
//!
//! Higher on small items, because the cost of handling a sale barely moves
//! with its size and a flat rate would make a two-euro item cost more to
//! process than it earns. Stated as a schedule so a creator can work out
//! their own take before listing.

use bigdecimal::BigDecimal;
use bigdecimal::num_traits::Signed;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::ledger;

pub const LICENSE_TYPES: &[&str] = &["personal_use", "commercial", "extended_commercial"];

/// What Skilluv keeps on a marketplace sale, by price.
///
/// Twenty per cent below twenty euros, fifteen above. The cost of taking a
/// payment, hosting the files and handling a dispute barely moves with the
/// price, so a flat rate would make small items cost more to process than
/// they earn — and small items are the ones that make a marketplace worth
/// browsing.
pub fn commission_percent(price: &BigDecimal) -> BigDecimal {
    if price < &BigDecimal::from(20) {
        BigDecimal::from(20)
    } else {
        BigDecimal::from(15)
    }
}

/// How a sale divides.
///
/// The creator absorbs no rounding: the commission is rounded down and the
/// creator takes the rest, so the two always add back to what was paid.
pub fn split_sale(price: &BigDecimal) -> (BigDecimal, BigDecimal, BigDecimal) {
    let percent = commission_percent(price);
    let commission = (price * &percent / BigDecimal::from(100))
        .with_scale_round(2, bigdecimal::RoundingMode::Down);
    let payout = price - &commission;
    (percent, commission, payout)
}

/// How long a download link lives, in hours.
///
/// Long enough to survive a bad connection and a night's sleep, short enough
/// that a link posted in a group chat has stopped working before it spreads.
pub const DOWNLOAD_WINDOW_HOURS: i64 = 48;

/// How many times one purchase may be downloaded.
///
/// A ceiling rather than one: files get lost, laptops die, and a buyer who
/// paid should not have to ask. High enough to be invisible to a buyer and
/// low enough to be visible on a share.
pub const DOWNLOAD_LIMIT: i16 = 10;

// ═══════════════════════════════════════════════════════════════════
// Certifications
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Program {
    pub slug: String,
    pub label: String,
    pub description: String,
    pub subject_kind: String,
    pub annual_fee: BigDecimal,
    pub currency: String,
    pub valid_months: i16,
    pub pass_mark: BigDecimal,
}

pub async fn programs(db: &PgPool) -> Result<Vec<Program>, AppError> {
    let rows = sqlx::query_as::<_, Program>(
        "SELECT slug, label, description, subject_kind, annual_fee, currency,
                valid_months, pass_mark
           FROM certification_programs WHERE is_active ORDER BY subject_kind, annual_fee",
    )
    .fetch_all(db)
    .await?;
    Ok(rows)
}

async fn program(db: &PgPool, slug: &str) -> Result<Program, AppError> {
    sqlx::query_as::<_, Program>(
        "SELECT slug, label, description, subject_kind, annual_fee, currency,
                valid_months, pass_mark
           FROM certification_programs WHERE slug = $1 AND is_active",
    )
    .bind(slug)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("no certification programme '{slug}'")))
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Certification {
    pub id: Uuid,
    pub program: String,
    pub subject_user_id: Option<Uuid>,
    pub subject_enterprise_id: Option<Uuid>,
    pub subject_org_name: Option<String>,
    pub fee: BigDecimal,
    pub currency: String,
    pub scope: Vec<String>,
    pub audit_score: Option<BigDecimal>,
    pub issued_at: Option<chrono::DateTime<chrono::Utc>>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub renewals: i16,
    pub status: String,
}

const CERT_SELECT: &str = r#"
    SELECT id, program, subject_user_id, subject_enterprise_id, subject_org_name,
           fee, currency, scope, audit_score, issued_at, expires_at, renewals, status
      FROM certifications
"#;

#[derive(Debug, Clone, Deserialize)]
pub struct CertificationInput {
    pub program: String,
    #[serde(default)]
    pub subject_user_id: Option<Uuid>,
    #[serde(default)]
    pub subject_enterprise_id: Option<Uuid>,
    #[serde(default)]
    pub subject_org_name: Option<String>,
    #[serde(default)]
    pub subject_org_url: Option<String>,
    #[serde(default)]
    pub scope: Vec<String>,
    /// A negotiated figure. The published price is used when absent.
    #[serde(default)]
    pub fee: Option<BigDecimal>,
}

pub async fn request_certification(
    db: &PgPool,
    input: CertificationInput,
) -> Result<Certification, AppError> {
    let program = program(db, &input.program).await?;

    // The subject has to match what the programme certifies. A studio
    // certification pointed at a person certifies nothing anybody asked for.
    let named = [
        input.subject_user_id.is_some(),
        input.subject_enterprise_id.is_some(),
        input.subject_org_name.is_some(),
    ];
    let expected = match program.subject_kind.as_str() {
        "person" => 0,
        "enterprise" => 1,
        _ => 2,
    };
    if named.iter().filter(|n| **n).count() != 1 || !named[expected] {
        return Err(AppError::Validation(format!(
            "the {} programme certifies a {} — name exactly that",
            program.label, program.subject_kind
        )));
    }

    let fee = input.fee.unwrap_or_else(|| program.annual_fee.clone());

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO certifications
            (program, subject_user_id, subject_enterprise_id, subject_org_name,
             subject_org_url, scope, fee, currency)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
         RETURNING id",
    )
    .bind(&input.program)
    .bind(input.subject_user_id)
    .bind(input.subject_enterprise_id)
    .bind(input.subject_org_name.as_deref())
    .bind(input.subject_org_url.as_deref())
    .bind(&input.scope)
    .bind(&fee)
    .bind(&program.currency)
    .fetch_one(db)
    .await
    .map_err(|e| {
        if e.to_string().contains("idx_one_live_certification") {
            AppError::Validation(
                "there is already a live certification of that kind for this subject. \
                 Two would let them show whichever suits."
                    .into(),
            )
        } else {
            AppError::from(e)
        }
    })?;

    certification(db, id).await
}

pub async fn certification(db: &PgPool, id: Uuid) -> Result<Certification, AppError> {
    let sql = format!("{CERT_SELECT} WHERE id = $1");
    sqlx::query_as::<_, Certification>(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NotFound("certification not found".into()))
}

/// What is live right now, for a public badge page.
pub async fn live_certifications(db: &PgPool) -> Result<Vec<Certification>, AppError> {
    let sql = format!(
        "{CERT_SELECT} WHERE status = 'issued' AND expires_at > NOW()
          ORDER BY issued_at DESC"
    );
    let rows = sqlx::query_as::<_, Certification>(sqlx::AssertSqlSafe(sql))
        .fetch_all(db)
        .await?;
    Ok(rows)
}

#[derive(Debug, Clone, Deserialize)]
pub struct Finding {
    pub criterion: String,
    pub score: BigDecimal,
    #[serde(default)]
    pub weight: Option<BigDecimal>,
    pub evidence: String,
}

/// The weighted mean of an audit's findings.
pub fn audit_score(findings: &[(BigDecimal, BigDecimal)]) -> Option<BigDecimal> {
    if findings.is_empty() {
        return None;
    }
    let mut weighted = BigDecimal::from(0);
    let mut total_weight = BigDecimal::from(0);
    for (score, weight) in findings {
        weighted += score * weight;
        total_weight += weight;
    }
    if !total_weight.is_positive() {
        return None;
    }
    Some((weighted / total_weight).with_scale_round(2, bigdecimal::RoundingMode::HalfUp))
}

/// Record an audit and decide.
///
/// One call: the findings, the score derived from them, and the verdict. A
/// score entered separately from its findings is a score somebody typed.
pub async fn audit(
    db: &PgPool,
    certification_id: Uuid,
    auditor: Uuid,
    findings: Vec<Finding>,
    notes: Option<&str>,
) -> Result<Certification, AppError> {
    if findings.is_empty() {
        return Err(AppError::Validation(
            "an audit with no findings is a signature. Say what was looked at.".into(),
        ));
    }
    for finding in &findings {
        if finding.evidence.trim().is_empty() {
            return Err(AppError::Validation(format!(
                "'{}' was scored without evidence, which makes it an opinion with a \
                 number on it",
                finding.criterion
            )));
        }
    }

    let cert = certification(db, certification_id).await?;
    if matches!(cert.status.as_str(), "issued" | "revoked") {
        return Err(AppError::Validation(format!(
            "this certification is {} and is not waiting for an audit",
            cert.status
        )));
    }
    let program = program(db, &cert.program).await?;

    let pairs: Vec<(BigDecimal, BigDecimal)> = findings
        .iter()
        .map(|f| {
            (
                f.score.clone(),
                f.weight.clone().unwrap_or_else(|| BigDecimal::from(1)),
            )
        })
        .collect();
    let score = audit_score(&pairs)
        .ok_or_else(|| AppError::Validation("the weights add up to nothing".into()))?;

    let passed = score >= program.pass_mark;

    let mut tx = db.begin().await?;

    sqlx::query("DELETE FROM certification_audit_findings WHERE certification_id = $1")
        .bind(certification_id)
        .execute(&mut *tx)
        .await?;

    for finding in &findings {
        sqlx::query(
            "INSERT INTO certification_audit_findings
                (certification_id, criterion, score, weight, evidence)
             VALUES ($1,$2,$3,COALESCE($4,1.00),$5)",
        )
        .bind(certification_id)
        .bind(finding.criterion.trim())
        .bind(&finding.score)
        .bind(finding.weight.as_ref())
        .bind(finding.evidence.trim())
        .execute(&mut *tx)
        .await?;
    }

    if passed {
        sqlx::query(
            "UPDATE certifications
                SET audit_score = $2, audit_notes = $3, audit_by = $4, audited_at = NOW(),
                    status = 'issued', issued_at = NOW(),
                    expires_at = NOW() + ($5 || ' months')::INTERVAL,
                    renewals = CASE WHEN issued_at IS NULL THEN renewals ELSE renewals + 1 END
              WHERE id = $1",
        )
        .bind(certification_id)
        .bind(&score)
        .bind(notes)
        .bind(auditor)
        .bind(program.valid_months.to_string())
        .execute(&mut *tx)
        .await?;

        // Booked at issue, not at order. Paying does not certify.
        if cert.fee.is_positive() {
            sqlx::query(
                "INSERT INTO platform_revenues
                    (source, related_talent_id, related_enterprise_id, amount_credits,
                     fee_rate_bps, notes)
                 VALUES ('certification_program', $1, $2, $3, 10000, $4)",
            )
            .bind(cert.subject_user_id)
            .bind(cert.subject_enterprise_id)
            .bind(&cert.fee)
            .bind(format!("{} — score {score}", program.label))
            .execute(&mut *tx)
            .await?;
        }
    } else {
        sqlx::query(
            "UPDATE certifications
                SET audit_score = $2, audit_notes = $3, audit_by = $4, audited_at = NOW(),
                    status = 'failed',
                    failure_reason = $5
              WHERE id = $1",
        )
        .bind(certification_id)
        .bind(&score)
        .bind(notes)
        .bind(auditor)
        .bind(format!(
            "Score {score} sur un seuil de {}. Les constats sont joints.",
            program.pass_mark
        ))
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    certification(db, certification_id).await
}

/// Withdraw a certification.
pub async fn revoke(db: &PgPool, id: Uuid, reason: &str) -> Result<(), AppError> {
    if reason.trim().is_empty() {
        return Err(AppError::Validation(
            "say why. Somebody has been showing this badge, and they are owed the \
             reason it stopped being true."
                .into(),
        ));
    }
    sqlx::query(
        "UPDATE certifications
            SET status = 'revoked', revoked_at = NOW(), revoked_reason = $2
          WHERE id = $1 AND status = 'issued'",
    )
    .bind(id)
    .bind(reason.trim())
    .execute(db)
    .await?;
    Ok(())
}

/// Mark everything past its date as expired.
///
/// Run on a schedule. Reading the expiry at query time would be enough for
/// the API, but a status that lags reality shows up in every export and every
/// admin list, and somebody eventually trusts one of those.
pub async fn expire_lapsed(db: &PgPool) -> Result<u64, AppError> {
    let done = sqlx::query(
        "UPDATE certifications SET status = 'expired'
          WHERE status = 'issued' AND expires_at <= NOW()",
    )
    .execute(db)
    .await?;
    Ok(done.rows_affected())
}

// ═══════════════════════════════════════════════════════════════════
// The marketplace
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Item {
    pub id: Uuid,
    pub slug: String,
    pub creator_user_id: Uuid,
    pub item_type: String,
    pub skill_domain: String,
    pub title: String,
    pub description_md: String,
    pub thumbnail_url: String,
    pub preview_urls: Vec<String>,
    pub license_type: String,
    pub license_summary: String,
    pub price: BigDecimal,
    pub currency: String,
    pub downloads_count: i32,
    pub rating_avg: Option<BigDecimal>,
    pub rating_count: i32,
    pub status: String,
}

const ITEM_SELECT: &str = r#"
    SELECT id, slug, creator_user_id, item_type, skill_domain, title, description_md,
           thumbnail_url, preview_urls, license_type, license_summary, price, currency,
           downloads_count, rating_avg, rating_count, status
      FROM marketplace_items
"#;

#[derive(Debug, Clone, Deserialize)]
pub struct ItemInput {
    pub slug: String,
    pub item_type: String,
    pub skill_domain: String,
    pub title: String,
    pub description_md: String,
    pub thumbnail_url: String,
    #[serde(default)]
    pub preview_urls: Vec<String>,
    pub file_keys: Vec<String>,
    pub license_type: String,
    pub license_summary: String,
    pub price: BigDecimal,
    #[serde(default = "eur")]
    pub currency: String,
}

fn eur() -> String {
    "EUR".into()
}

pub async fn list_item(db: &PgPool, creator: Uuid, input: ItemInput) -> Result<Item, AppError> {
    if !LICENSE_TYPES.contains(&input.license_type.as_str()) {
        return Err(AppError::Validation(format!(
            "license_type must be one of: {}",
            LICENSE_TYPES.join(", ")
        )));
    }
    if input.license_summary.trim().len() < 20 {
        return Err(AppError::Validation(
            "say what a buyer may do with it, in a sentence they can read. A licence \
             nobody can read is a licence nobody follows."
                .into(),
        ));
    }
    if input.file_keys.is_empty() {
        return Err(AppError::Validation(
            "an item with no files is a listing for nothing".into(),
        ));
    }
    if !input.price.is_positive() {
        return Err(AppError::Validation(
            "set a price. Free work belongs in the portfolio, where it is not confused \
             with something somebody paid for."
                .into(),
        ));
    }

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO marketplace_items
            (slug, creator_user_id, item_type, skill_domain, title, description_md,
             thumbnail_url, preview_urls, file_keys, license_type, license_summary,
             price, currency)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
         RETURNING id",
    )
    .bind(input.slug.trim())
    .bind(creator)
    .bind(&input.item_type)
    .bind(&input.skill_domain)
    .bind(input.title.trim())
    .bind(input.description_md.trim())
    .bind(input.thumbnail_url.trim())
    .bind(&input.preview_urls)
    .bind(&input.file_keys)
    .bind(&input.license_type)
    .bind(input.license_summary.trim())
    .bind(&input.price)
    .bind(&input.currency)
    .fetch_one(db)
    .await
    .map_err(|e| {
        let m = e.to_string();
        if m.contains("marketplace_items_slug_key") {
            AppError::Validation("that slug is taken".into())
        } else if m.contains("thumbnail_url") {
            AppError::Validation("the thumbnail has to be an https URL".into())
        } else {
            AppError::from(e)
        }
    })?;

    item(db, id).await
}

pub async fn item(db: &PgPool, id: Uuid) -> Result<Item, AppError> {
    let sql = format!("{ITEM_SELECT} WHERE id = $1");
    sqlx::query_as::<_, Item>(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NotFound("item not found".into()))
}

pub async fn published_items(db: &PgPool, domain: Option<&str>) -> Result<Vec<Item>, AppError> {
    let sql = format!(
        "{ITEM_SELECT} WHERE status = 'published'
            AND ($1::TEXT IS NULL OR skill_domain = $1)
          ORDER BY published_at DESC LIMIT 100"
    );
    let rows = sqlx::query_as::<_, Item>(sqlx::AssertSqlSafe(sql))
        .bind(domain)
        .fetch_all(db)
        .await?;
    Ok(rows)
}

pub async fn publish_item(db: &PgPool, id: Uuid) -> Result<Item, AppError> {
    sqlx::query(
        "UPDATE marketplace_items
            SET status = 'published', published_at = COALESCE(published_at, NOW())
          WHERE id = $1 AND status IN ('draft', 'in_review')",
    )
    .bind(id)
    .execute(db)
    .await?;
    item(db, id).await
}

/// Buy an item.
///
/// The price and the commission are frozen onto the purchase. A creator
/// raising their price must not change what somebody already paid, and a
/// report read next year has to show the price of the day.
pub async fn purchase(
    db: &PgPool,
    item_id: Uuid,
    buyer_user_id: Option<Uuid>,
    buyer_enterprise_id: Option<Uuid>,
) -> Result<(Uuid, String, BigDecimal), AppError> {
    let item = item(db, item_id).await?;
    if item.status != "published" {
        return Err(AppError::Validation("this item is not on sale".into()));
    }
    if buyer_user_id.is_some() == buyer_enterprise_id.is_some() {
        return Err(AppError::Validation(
            "a purchase has exactly one buyer".into(),
        ));
    }
    if buyer_user_id == Some(item.creator_user_id) {
        return Err(AppError::Validation(
            "you made this one. Buying your own item would inflate its sales and its \
             rating, which are the two numbers a buyer reads."
                .into(),
        ));
    }

    let (percent, commission, payout) = split_sale(&item.price);
    let token = Uuid::new_v4().simple().to_string();

    let purchase_id: Uuid = sqlx::query_scalar(
        "INSERT INTO marketplace_purchases
            (item_id, buyer_user_id, buyer_enterprise_id, amount_paid,
             commission_percent, commission_amount, creator_payout, currency,
             download_token, token_expires_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9, NOW() + ($10 || ' hours')::INTERVAL)
         RETURNING id",
    )
    .bind(item_id)
    .bind(buyer_user_id)
    .bind(buyer_enterprise_id)
    .bind(&item.price)
    .bind(&percent)
    .bind(&commission)
    .bind(&payout)
    .bind(&item.currency)
    .bind(&token)
    .bind(DOWNLOAD_WINDOW_HOURS.to_string())
    .fetch_one(db)
    .await?;

    sqlx::query(
        "INSERT INTO platform_revenues
            (source, related_talent_id, related_enterprise_id, amount_credits,
             fee_rate_bps, notes)
         VALUES ('marketplace_creators_commission', $1, $2, $3, $4, $5)",
    )
    .bind(item.creator_user_id)
    .bind(buyer_enterprise_id)
    .bind(&commission)
    .bind(ledger::percent_to_bps(&percent))
    .bind(format!("vente de « {} »", item.title))
    .execute(db)
    .await?;

    let currency: ledger::Currency = item.currency.parse()?;
    ledger::capture_for_recipient(
        db,
        "stripe",
        format!("marketplace:{purchase_id}"),
        item.creator_user_id,
        payout.clone(),
        BigDecimal::from(0),
        currency,
        "marketplace_sale",
        purchase_id,
    )
    .await?;

    Ok((purchase_id, token, payout))
}

/// Redeem a download token.
///
/// Bounded by both the window and the count. A permanent link posted once is
/// the whole catalogue given away.
pub async fn redeem_download(db: &PgPool, token: &str) -> Result<Vec<String>, AppError> {
    let row: Option<(Uuid, Uuid, i16)> = sqlx::query_as(
        "SELECT id, item_id, downloads_used FROM marketplace_purchases
          WHERE download_token = $1 AND token_expires_at > NOW()
            AND refunded_at IS NULL",
    )
    .bind(token)
    .fetch_optional(db)
    .await?;

    let (purchase_id, item_id, used) = row.ok_or_else(|| {
        AppError::NotFound(
            "this link has expired. Ask us and we will issue another — you paid for the \
             files, not for the link."
                .into(),
        )
    })?;

    if used >= DOWNLOAD_LIMIT {
        return Err(AppError::Validation(format!(
            "this purchase has been downloaded {DOWNLOAD_LIMIT} times. Ask us if you \
             genuinely need more."
        )));
    }

    let keys: Vec<String> =
        sqlx::query_scalar("SELECT unnest(file_keys) FROM marketplace_items WHERE id = $1")
            .bind(item_id)
            .fetch_all(db)
            .await?;

    sqlx::query(
        "UPDATE marketplace_purchases SET downloads_used = downloads_used + 1
          WHERE id = $1",
    )
    .bind(purchase_id)
    .execute(db)
    .await?;

    sqlx::query(
        "UPDATE marketplace_items SET downloads_count = downloads_count + 1
          WHERE id = $1",
    )
    .bind(item_id)
    .execute(db)
    .await?;

    Ok(keys)
}

/// Rate something you bought.
///
/// Only from a purchase, and one per purchase. A rating anybody can leave is
/// a rating a competitor can leave.
pub async fn rate(
    db: &PgPool,
    purchase_id: Uuid,
    buyer: Uuid,
    rating: i16,
    review: Option<&str>,
) -> Result<(), AppError> {
    if !(1..=5).contains(&rating) {
        return Err(AppError::Validation("a rating runs from 1 to 5".into()));
    }

    let done = sqlx::query(
        "UPDATE marketplace_purchases
            SET rating = $3, review = $4, rated_at = NOW()
          WHERE id = $1 AND buyer_user_id = $2 AND refunded_at IS NULL",
    )
    .bind(purchase_id)
    .bind(buyer)
    .bind(rating)
    .bind(review.map(str::trim).filter(|r| !r.is_empty()))
    .execute(db)
    .await?;

    if done.rows_affected() == 0 {
        return Err(AppError::NotFound(
            "you have no purchase to rate here".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn dec(s: &str) -> BigDecimal {
        BigDecimal::from_str(s).unwrap()
    }

    #[test]
    fn small_items_carry_the_higher_commission() {
        // The cost of taking a payment barely moves with the price, so a flat
        // rate would make small items cost more to process than they earn.
        assert_eq!(commission_percent(&dec("3.00")), dec("20"));
        assert_eq!(commission_percent(&dec("19.99")), dec("20"));
        assert_eq!(commission_percent(&dec("20.00")), dec("15"));
        assert_eq!(commission_percent(&dec("500.00")), dec("15"));
    }

    #[test]
    fn a_sale_always_adds_back_to_what_was_paid() {
        for price in ["1.00", "3.33", "19.99", "20.00", "99.99", "500.00"] {
            let (_, commission, payout) = split_sale(&dec(price));
            assert_eq!(
                &commission + &payout,
                dec(price),
                "{price} lost or invented a centime"
            );
            assert!(!commission.is_negative());
            assert!(payout.is_positive());
        }
    }

    #[test]
    fn the_creator_absorbs_no_rounding() {
        // The commission rounds down; the creator takes the rest.
        let (_, commission, payout) = split_sale(&dec("3.33"));
        assert_eq!(commission, dec("0.66"));
        assert_eq!(payout, dec("2.67"));
    }

    #[test]
    fn an_audit_is_the_weighted_mean_of_its_findings() {
        let findings = vec![(dec("90"), dec("2")), (dec("60"), dec("1"))];
        // (90*2 + 60) / 3 = 80
        assert_eq!(audit_score(&findings), Some(dec("80.00")));
    }

    #[test]
    fn an_audit_with_no_findings_has_no_score() {
        // A score entered separately from its findings is a score somebody
        // typed.
        assert_eq!(audit_score(&[]), None);
        assert_eq!(audit_score(&[(dec("90"), dec("0"))]), None);
    }

    #[test]
    fn a_single_finding_is_its_own_score() {
        assert_eq!(audit_score(&[(dec("73.50"), dec("1"))]), Some(dec("73.50")));
    }

    #[test]
    fn the_download_window_is_long_enough_to_be_useful_and_short_enough_to_expire() {
        let (window, limit) = (DOWNLOAD_WINDOW_HOURS, DOWNLOAD_LIMIT);
        assert!(window >= 24, "shorter than a night's sleep");
        assert!(window <= 168, "a week is not a window");
        assert!(
            limit > 1,
            "a buyer whose laptop died should not have to ask"
        );
    }

    #[test]
    fn every_licence_type_is_a_known_one() {
        assert_eq!(LICENSE_TYPES.len(), 3);
        assert!(LICENSE_TYPES.contains(&"extended_commercial"));
    }
}
