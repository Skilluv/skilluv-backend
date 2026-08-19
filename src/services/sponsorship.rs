//! Events and the companies that pay for them.
//!
//! Four things live here because they are four sides of one transaction: the
//! grid we publish, the deal actually signed, what the sponsor gets out of it
//! (a stand, leads, a named challenge, a slot in the stream), and the line it
//! books in the revenue ledger.
//!
//! Two rules run through all of it.
//!
//! **A negotiated price is a fact about that deal.** The grid says what
//! Bronze costs; the sponsorship says what this sponsor paid. Discounting by
//! editing the grid would rewrite history for every other sponsor at the same
//! tier.
//!
//! **A lead is an act by the person named.** Nothing about a participant
//! reaches a sponsor because they attended — only because they walked up and
//! said they were interested, and said the sponsor could have their details.

use bigdecimal::BigDecimal;
use bigdecimal::num_traits::Signed;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

pub const TIERS: &[&str] = &["bronze", "silver", "gold", "platinum", "custom"];

/// What one event costs under an annual contract.
///
/// The discount is what the company is paid for committing to the number, so
/// the per-event price falls as the commitment rises. Returns the list price
/// untouched when there is no contract.
pub fn discounted_fee(list_fee: &BigDecimal, discount_percent: &BigDecimal) -> BigDecimal {
    let kept = BigDecimal::from(100) - discount_percent;
    (list_fee * kept / BigDecimal::from(100)).with_scale_round(2, bigdecimal::RoundingMode::HalfUp)
}

/// Whether a discount is one Skilluv actually offers.
///
/// Above thirty per cent an annual contract costs more to service than the
/// events it covers bring in, and the company that negotiated it hardest is
/// the one we lose money on.
pub fn discount_is_offerable(percent: &BigDecimal) -> bool {
    !percent.is_negative() && percent <= &BigDecimal::from(30)
}

// ═══════════════════════════════════════════════════════════════════
// The grid
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Package {
    pub tier: String,
    pub label: String,
    pub list_fee: BigDecimal,
    pub currency: String,
    pub benefits: serde_json::Value,
    pub talent_access_credits: i32,
    pub keynote_slot: bool,
    pub physical_stand: bool,
    pub named_challenge: bool,
    pub branded_content_included: bool,
}

pub async fn packages(db: &PgPool) -> Result<Vec<Package>, AppError> {
    let rows = sqlx::query_as::<_, Package>(
        "SELECT tier, label, list_fee, currency, benefits, talent_access_credits,
                keynote_slot, physical_stand, named_challenge, branded_content_included
           FROM event_sponsorship_packages
          WHERE is_active ORDER BY sort_order",
    )
    .fetch_all(db)
    .await?;
    Ok(rows)
}

async fn package(db: &PgPool, tier: &str) -> Result<Package, AppError> {
    let row = sqlx::query_as::<_, Package>(
        "SELECT tier, label, list_fee, currency, benefits, talent_access_credits,
                keynote_slot, physical_stand, named_challenge, branded_content_included
           FROM event_sponsorship_packages WHERE tier = $1 AND is_active",
    )
    .bind(tier)
    .fetch_optional(db)
    .await?;

    row.ok_or_else(|| {
        AppError::Validation(format!(
            "'{tier}' is not a package we sell — one of: {}",
            TIERS.join(", ")
        ))
    })
}

// ═══════════════════════════════════════════════════════════════════
// Sponsorships
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Sponsorship {
    pub id: Uuid,
    pub event_id: Uuid,
    pub enterprise_id: Uuid,
    pub package_tier: String,
    pub agreed_fee: BigDecimal,
    pub currency: String,
    pub extra_benefits: serde_json::Value,
    pub logo_placement: Vec<String>,
    pub named_challenge_slug: Option<String>,
    pub physical_stand: bool,
    pub virtual_stand_url: Option<String>,
    pub annual_contract_id: Option<Uuid>,
    pub status: String,
    pub signed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

const SPONSORSHIP_SELECT: &str = r#"
    SELECT id, event_id, enterprise_id, package_tier, agreed_fee, currency,
           extra_benefits, logo_placement, named_challenge_slug, physical_stand,
           virtual_stand_url, annual_contract_id, status, signed_at, created_at
      FROM event_sponsorships
"#;

#[derive(Debug, Clone, Deserialize)]
pub struct SponsorshipInput {
    pub event_id: Uuid,
    pub package_tier: String,
    /// What was actually agreed. Falls back to the published price.
    #[serde(default)]
    pub agreed_fee: Option<BigDecimal>,
    #[serde(default)]
    pub extra_benefits: Option<serde_json::Value>,
    #[serde(default)]
    pub logo_placement: Vec<String>,
    #[serde(default)]
    pub named_challenge_slug: Option<String>,
    #[serde(default)]
    pub virtual_stand_url: Option<String>,
    #[serde(default)]
    pub annual_contract_id: Option<Uuid>,
}

/// Propose a sponsorship.
pub async fn propose(
    db: &PgPool,
    enterprise_id: Uuid,
    author: Uuid,
    input: SponsorshipInput,
) -> Result<Sponsorship, AppError> {
    let package = package(db, &input.package_tier).await?;

    let event: Option<(String, String)> =
        sqlx::query_as("SELECT slug, status FROM events WHERE id = $1")
            .bind(input.event_id)
            .fetch_optional(db)
            .await?;
    let (_slug, status) = event.ok_or_else(|| AppError::NotFound("event not found".into()))?;
    if matches!(status.as_str(), "finished" | "cancelled") {
        return Err(AppError::Validation(format!(
            "that event is {status} — a sponsorship signed now buys nothing"
        )));
    }

    // A custom deal has to say what it costs. The grid says nothing about it
    // by definition, so a missing figure is a blank cheque either way.
    let agreed_fee = match input.agreed_fee {
        Some(fee) => fee,
        None if input.package_tier == "custom" => {
            return Err(AppError::Validation(
                "a custom package has no published price — say what was agreed".into(),
            ));
        }
        None => package.list_fee.clone(),
    };
    if agreed_fee.is_negative() {
        return Err(AppError::Validation("a fee cannot be negative".into()));
    }

    if input.named_challenge_slug.is_some() && !package.named_challenge {
        return Err(AppError::Validation(format!(
            "the {} package does not include a named challenge",
            package.label
        )));
    }

    let id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO event_sponsorships
            (event_id, enterprise_id, package_tier, agreed_fee, currency,
             extra_benefits, logo_placement, named_challenge_slug,
             physical_stand, virtual_stand_url, annual_contract_id, created_by)
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
        RETURNING id
        "#,
    )
    .bind(input.event_id)
    .bind(enterprise_id)
    .bind(&input.package_tier)
    .bind(&agreed_fee)
    .bind(&package.currency)
    .bind(
        input
            .extra_benefits
            .unwrap_or_else(|| serde_json::json!({})),
    )
    .bind(&input.logo_placement)
    .bind(input.named_challenge_slug.as_deref())
    .bind(package.physical_stand)
    .bind(input.virtual_stand_url.as_deref())
    .bind(input.annual_contract_id)
    .bind(author)
    .fetch_one(db)
    .await
    .map_err(|e| {
        let m = e.to_string();
        if m.contains("event_sponsorships_event_id_enterprise_id_key") {
            AppError::Validation(
                "this company already sponsors that event — change the existing \
                 sponsorship rather than adding a second logo"
                    .into(),
            )
        } else if m.contains("covers") && m.contains("events") {
            AppError::Validation(
                m.rsplit("ERROR:")
                    .next()
                    .unwrap_or("that annual contract is fully used")
                    .trim()
                    .to_string(),
            )
        } else if m.contains("a_virtual_stand_has_a_page") {
            AppError::Validation("the stand URL must be https".into())
        } else {
            AppError::from(e)
        }
    })?;

    by_id(db, id).await
}

pub async fn by_id(db: &PgPool, id: Uuid) -> Result<Sponsorship, AppError> {
    let sql = format!("{SPONSORSHIP_SELECT} WHERE id = $1");
    sqlx::query_as::<_, Sponsorship>(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NotFound("sponsorship not found".into()))
}

pub async fn for_event(db: &PgPool, event_id: Uuid) -> Result<Vec<Sponsorship>, AppError> {
    let sql = format!(
        "{SPONSORSHIP_SELECT} WHERE event_id = $1 AND status IN ('signed', 'honoured')
          ORDER BY agreed_fee DESC"
    );
    let rows = sqlx::query_as::<_, Sponsorship>(sqlx::AssertSqlSafe(sql))
        .bind(event_id)
        .fetch_all(db)
        .await?;
    Ok(rows)
}

pub async fn for_enterprise(
    db: &PgPool,
    enterprise_id: Uuid,
) -> Result<Vec<Sponsorship>, AppError> {
    let sql = format!("{SPONSORSHIP_SELECT} WHERE enterprise_id = $1 ORDER BY created_at DESC");
    let rows = sqlx::query_as::<_, Sponsorship>(sqlx::AssertSqlSafe(sql))
        .bind(enterprise_id)
        .fetch_all(db)
        .await?;
    Ok(rows)
}

/// Sign it, and grant what the tier promised.
///
/// The credits go through the entitlement machinery from migration 0503
/// rather than a counter here, because there is already one place that
/// answers "what does this company have the right to do", and a second one
/// would eventually disagree with it.
pub async fn sign(db: &PgPool, sponsorship_id: Uuid) -> Result<Sponsorship, AppError> {
    let sponsorship = by_id(db, sponsorship_id).await?;
    if !matches!(sponsorship.status.as_str(), "proposed" | "negotiating") {
        return Err(AppError::Validation(format!(
            "this sponsorship is {} — only a proposed one can be signed",
            sponsorship.status
        )));
    }

    let package = package(db, &sponsorship.package_tier).await?;

    let mut tx = db.begin().await?;
    sqlx::query("UPDATE event_sponsorships SET status = 'signed', signed_at = NOW() WHERE id = $1")
        .bind(sponsorship_id)
        .execute(&mut *tx)
        .await?;

    // The sponsorship joins the company's engagement register, and the
    // credits hang off that row. Entitlements point at a product rather than
    // at a company on purpose: when somebody asks why they have credits, the
    // answer is a thing they bought.
    let product_id: Uuid = sqlx::query_scalar(
        "INSERT INTO enterprise_products
            (enterprise_id, product_type, source_table, source_id,
             contract_value, currency)
         VALUES ($1, 'sponsoring_event', 'event_sponsorships', $2, $3, $4)
         RETURNING id",
    )
    .bind(sponsorship.enterprise_id)
    .bind(sponsorship_id)
    .bind(&sponsorship.agreed_fee)
    .bind(&sponsorship.currency)
    .fetch_one(&mut *tx)
    .await?;

    if package.talent_access_credits > 0 {
        sqlx::query(
            "INSERT INTO enterprise_entitlements (product_id, kind, granted)
             VALUES ($1, 'credits', $2)",
        )
        .bind(product_id)
        .bind(BigDecimal::from(package.talent_access_credits))
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    by_id(db, sponsorship_id).await
}

/// The event happened and the sponsor got what they paid for.
///
/// Booked here rather than at signature: a sponsorship signed for an event
/// that is later cancelled has earned nothing, and revenue recognised at
/// signature would have to be taken back.
pub async fn honour(db: &PgPool, sponsorship_id: Uuid) -> Result<BigDecimal, AppError> {
    let sponsorship = by_id(db, sponsorship_id).await?;
    if sponsorship.status != "signed" {
        return Err(AppError::Validation(format!(
            "this sponsorship is {} — only a signed one can be honoured",
            sponsorship.status
        )));
    }

    let mut tx = db.begin().await?;
    sqlx::query(
        "UPDATE event_sponsorships
            SET status = 'honoured', honoured_at = NOW() WHERE id = $1",
    )
    .bind(sponsorship_id)
    .execute(&mut *tx)
    .await?;

    if sponsorship.agreed_fee.is_positive() {
        sqlx::query(
            "INSERT INTO platform_revenues
                (source, related_enterprise_id, amount_credits, fee_rate_bps, notes)
             VALUES ('event_sponsorship', $1, $2, 10000, $3)",
        )
        .bind(sponsorship.enterprise_id)
        .bind(&sponsorship.agreed_fee)
        .bind(format!(
            "sponsoring {} sur l'événement {}",
            sponsorship.package_tier, sponsorship.event_id
        ))
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(sponsorship.agreed_fee)
}

pub async fn cancel(db: &PgPool, sponsorship_id: Uuid, reason: &str) -> Result<(), AppError> {
    if reason.trim().is_empty() {
        return Err(AppError::Validation(
            "say why — a sponsor who withdrew and a sponsor we turned down are not \
             the same company next year"
                .into(),
        ));
    }
    sqlx::query(
        "UPDATE event_sponsorships
            SET status = 'cancelled', declined_reason = $2
          WHERE id = $1 AND status <> 'honoured'",
    )
    .bind(sponsorship_id)
    .bind(reason.trim())
    .execute(db)
    .await?;
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════
// Leads
// ═══════════════════════════════════════════════════════════════════

pub const INTERACTIONS: &[&str] = &["stand_visit", "demo_booked", "question_asked", "cv_shared"];

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Lead {
    pub id: Uuid,
    pub user_id: Uuid,
    pub username: String,
    pub interaction: String,
    pub note: Option<String>,
    pub contact_consent: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// A participant walks up to a stand.
///
/// Recorded with their consent or without it, and the difference decides
/// whether the sponsor ever sees their name.
pub async fn record_lead(
    db: &PgPool,
    sponsorship_id: Uuid,
    user_id: Uuid,
    interaction: &str,
    note: Option<&str>,
    contact_consent: bool,
) -> Result<(), AppError> {
    if !INTERACTIONS.contains(&interaction) {
        return Err(AppError::Validation(format!(
            "interaction must be one of: {}",
            INTERACTIONS.join(", ")
        )));
    }

    sqlx::query(
        "INSERT INTO sponsorship_leads
            (sponsorship_id, user_id, interaction, note, contact_consent)
         VALUES ($1,$2,$3,$4,$5)
         ON CONFLICT (sponsorship_id, user_id, interaction) DO UPDATE
             SET note = EXCLUDED.note,
                 -- Consent can be withdrawn as easily as it was given, and
                 -- the later answer is the one that counts.
                 contact_consent = EXCLUDED.contact_consent",
    )
    .bind(sponsorship_id)
    .bind(user_id)
    .bind(interaction)
    .bind(note.map(str::trim).filter(|n| !n.is_empty()))
    .bind(contact_consent)
    .execute(db)
    .await?;
    Ok(())
}

/// What the sponsor is allowed to see.
///
/// Consented leads only. The count of everything else is reported separately
/// so the sponsor knows the stand worked without learning who was at it.
pub async fn leads_for_sponsor(
    db: &PgPool,
    sponsorship_id: Uuid,
) -> Result<(Vec<Lead>, i64), AppError> {
    let leads = sqlx::query_as::<_, Lead>(
        "SELECT l.id, l.user_id, u.username, l.interaction, l.note,
                l.contact_consent, l.created_at
           FROM sponsorship_leads l
           JOIN users u ON u.id = l.user_id
          WHERE l.sponsorship_id = $1 AND l.contact_consent
          ORDER BY l.created_at DESC",
    )
    .bind(sponsorship_id)
    .fetch_all(db)
    .await?;

    let anonymous: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM sponsorship_leads
          WHERE sponsorship_id = $1 AND NOT contact_consent",
    )
    .bind(sponsorship_id)
    .fetch_one(db)
    .await?;

    Ok((leads, anonymous))
}

/// Mark the consented leads as handed over.
pub async fn mark_exported(db: &PgPool, sponsorship_id: Uuid) -> Result<u64, AppError> {
    let done = sqlx::query(
        "UPDATE sponsorship_leads SET exported_at = NOW()
          WHERE sponsorship_id = $1 AND contact_consent AND exported_at IS NULL",
    )
    .bind(sponsorship_id)
    .execute(db)
    .await?;
    Ok(done.rows_affected())
}

// ═══════════════════════════════════════════════════════════════════
// Annual contracts
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AnnualContract {
    pub id: Uuid,
    pub enterprise_id: Uuid,
    pub year: i16,
    pub total_fee: BigDecimal,
    pub currency: String,
    pub max_events: i16,
    pub volume_discount_percent: BigDecimal,
    pub signed_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AnnualContractInput {
    pub year: i16,
    pub total_fee: BigDecimal,
    pub max_events: i16,
    #[serde(default)]
    pub volume_discount_percent: Option<BigDecimal>,
    #[serde(default)]
    pub contract_url: Option<String>,
}

pub async fn open_annual_contract(
    db: &PgPool,
    enterprise_id: Uuid,
    input: AnnualContractInput,
) -> Result<AnnualContract, AppError> {
    let discount = input
        .volume_discount_percent
        .clone()
        .unwrap_or_else(|| BigDecimal::from(0));

    if !discount_is_offerable(&discount) {
        return Err(AppError::Validation(
            "a volume discount runs from 0 to 30%. Past that the contract costs more to \
             service than the events it covers bring in, and the company that negotiated \
             hardest is the one we lose money on."
                .into(),
        ));
    }

    // The discount is what the company is paid for committing, so it is only
    // real once the commitment is. Signing comes with the contract file.
    let signed = input.contract_url.is_some();
    if discount.is_positive() && !signed {
        return Err(AppError::Validation(
            "a discount applies to a signed contract — attach the signed document".into(),
        ));
    }

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO annual_sponsorship_contracts
            (enterprise_id, year, total_fee, max_events, volume_discount_percent,
             contract_url, signed_at)
         VALUES ($1,$2,$3,$4,$5,$6, CASE WHEN $6::TEXT IS NULL THEN NULL ELSE NOW() END)
         RETURNING id",
    )
    .bind(enterprise_id)
    .bind(input.year)
    .bind(&input.total_fee)
    .bind(input.max_events)
    .bind(&discount)
    .bind(input.contract_url.as_deref())
    .fetch_one(db)
    .await
    .map_err(|e| {
        if e.to_string()
            .contains("annual_sponsorship_contracts_enterprise_id_year_key")
        {
            AppError::Validation(format!(
                "this company already has a {} contract — raise its event count \
                 rather than opening a second",
                input.year
            ))
        } else {
            AppError::from(e)
        }
    })?;

    annual_contract(db, id).await
}

pub async fn annual_contract(db: &PgPool, id: Uuid) -> Result<AnnualContract, AppError> {
    sqlx::query_as::<_, AnnualContract>(
        "SELECT id, enterprise_id, year, total_fee, currency, max_events,
                volume_discount_percent, signed_at
           FROM annual_sponsorship_contracts WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| AppError::NotFound("contract not found".into()))
}

/// What the contract has left, and what it has used.
pub async fn contract_usage(db: &PgPool, id: Uuid) -> Result<(i64, i16), AppError> {
    let contract = annual_contract(db, id).await?;
    let used: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM event_sponsorships
          WHERE annual_contract_id = $1 AND status <> 'cancelled'",
    )
    .bind(id)
    .fetch_one(db)
    .await?;
    Ok((used, contract.max_events))
}

// ═══════════════════════════════════════════════════════════════════
// Livestreams
// ═══════════════════════════════════════════════════════════════════

pub const PLATFORMS: &[&str] = &["youtube", "twitch", "linkedin", "self_hosted"];

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Livestream {
    pub id: Uuid,
    pub event_id: Uuid,
    pub platform: String,
    pub url: String,
    pub sponsor_ids: Vec<Uuid>,
    pub premium_content_available: bool,
    pub replay_url: Option<String>,
    pub starts_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub async fn livestreams(db: &PgPool, event_id: Uuid) -> Result<Vec<Livestream>, AppError> {
    let rows = sqlx::query_as::<_, Livestream>(
        "SELECT id, event_id, platform, url, sponsor_ids, premium_content_available,
                replay_url, starts_at
           FROM event_livestreams WHERE event_id = $1 ORDER BY starts_at",
    )
    .bind(event_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

#[derive(Debug, Clone, Deserialize)]
pub struct LivestreamInput {
    pub platform: String,
    pub url: String,
    #[serde(default)]
    pub sponsor_ids: Vec<Uuid>,
    #[serde(default)]
    pub premium_content_available: Option<bool>,
    #[serde(default)]
    pub replay_url: Option<String>,
    #[serde(default)]
    pub starts_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub async fn add_livestream(
    db: &PgPool,
    event_id: Uuid,
    input: LivestreamInput,
) -> Result<Uuid, AppError> {
    if !PLATFORMS.contains(&input.platform.as_str()) {
        return Err(AppError::Validation(format!(
            "platform must be one of: {}",
            PLATFORMS.join(", ")
        )));
    }
    if !input.url.starts_with("https://") {
        return Err(AppError::Validation("the stream URL must be https".into()));
    }

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO event_livestreams
            (event_id, platform, url, sponsor_ids, premium_content_available,
             replay_url, starts_at)
         VALUES ($1,$2,$3,$4,COALESCE($5,FALSE),$6,$7)
         ON CONFLICT (event_id, platform) DO UPDATE
             SET url = EXCLUDED.url,
                 sponsor_ids = EXCLUDED.sponsor_ids,
                 premium_content_available = EXCLUDED.premium_content_available,
                 replay_url = EXCLUDED.replay_url,
                 starts_at = EXCLUDED.starts_at
         RETURNING id",
    )
    .bind(event_id)
    .bind(&input.platform)
    .bind(input.url.trim())
    .bind(&input.sponsor_ids)
    .bind(input.premium_content_available)
    .bind(input.replay_url.as_deref())
    .bind(input.starts_at)
    .fetch_one(db)
    .await?;
    Ok(id)
}

// ═══════════════════════════════════════════════════════════════════
// Sponsored content
// ═══════════════════════════════════════════════════════════════════

pub const CONTENT_TYPES: &[&str] = &["blog_post", "video", "newsletter", "podcast", "recap"];

/// The wording that goes on a piece when the commissioner does not supply one.
///
/// A default rather than an optional field: the disclosure is the part a
/// hurried editor drops, and a piece that does not say it is sponsored is the
/// fastest way to lose an audience.
pub fn default_disclosure(sponsor_name: &str) -> String {
    format!(
        "Ce contenu est financé par {sponsor_name}. Skilluv en a gardé le contrôle \
         éditorial ; le sponsor n'a pas relu avant publication."
    )
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct SponsoredContent {
    pub id: Uuid,
    pub event_id: Option<Uuid>,
    pub sponsor_enterprise_id: Uuid,
    pub content_type: String,
    pub title: String,
    pub content_url: Option<String>,
    pub fee: BigDecimal,
    pub currency: String,
    pub disclosure_text: String,
    pub status: String,
    pub published_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ContentInput {
    #[serde(default)]
    pub event_id: Option<Uuid>,
    pub sponsor_enterprise_id: Uuid,
    pub content_type: String,
    pub title: String,
    pub fee: BigDecimal,
    #[serde(default)]
    pub disclosure_text: Option<String>,
    #[serde(default)]
    pub author_user_id: Option<Uuid>,
}

pub async fn commission_content(db: &PgPool, input: ContentInput) -> Result<Uuid, AppError> {
    if !CONTENT_TYPES.contains(&input.content_type.as_str()) {
        return Err(AppError::Validation(format!(
            "content_type must be one of: {}",
            CONTENT_TYPES.join(", ")
        )));
    }

    let sponsor_name: Option<String> =
        sqlx::query_scalar("SELECT company_name FROM enterprises WHERE id = $1")
            .bind(input.sponsor_enterprise_id)
            .fetch_optional(db)
            .await?;
    let sponsor_name =
        sponsor_name.ok_or_else(|| AppError::NotFound("sponsor not found".into()))?;

    let disclosure = input
        .disclosure_text
        .map(|d| d.trim().to_string())
        .filter(|d| d.len() >= 10)
        .unwrap_or_else(|| default_disclosure(&sponsor_name));

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO event_sponsored_content
            (event_id, sponsor_enterprise_id, content_type, title, fee,
             disclosure_text, author_user_id)
         VALUES ($1,$2,$3,$4,$5,$6,$7)
         RETURNING id",
    )
    .bind(input.event_id)
    .bind(input.sponsor_enterprise_id)
    .bind(&input.content_type)
    .bind(input.title.trim())
    .bind(&input.fee)
    .bind(&disclosure)
    .bind(input.author_user_id)
    .fetch_one(db)
    .await?;

    Ok(id)
}

/// Publish it, and book the fee.
pub async fn publish_content(
    db: &PgPool,
    content_id: Uuid,
    url: &str,
) -> Result<BigDecimal, AppError> {
    if !url.starts_with("https://") {
        return Err(AppError::Validation("the URL must be https".into()));
    }

    let content: Option<SponsoredContent> = sqlx::query_as(
        "SELECT id, event_id, sponsor_enterprise_id, content_type, title, content_url,
                fee, currency, disclosure_text, status, published_at
           FROM event_sponsored_content WHERE id = $1",
    )
    .bind(content_id)
    .fetch_optional(db)
    .await?;
    let content = content.ok_or_else(|| AppError::NotFound("content not found".into()))?;

    if content.status == "published" {
        return Err(AppError::Validation("already published".into()));
    }

    let mut tx = db.begin().await?;
    sqlx::query(
        "UPDATE event_sponsored_content
            SET status = 'published', content_url = $2, published_at = NOW()
          WHERE id = $1",
    )
    .bind(content_id)
    .bind(url.trim())
    .execute(&mut *tx)
    .await?;

    if content.fee.is_positive() {
        sqlx::query(
            "INSERT INTO platform_revenues
                (source, related_enterprise_id, amount_credits, fee_rate_bps, notes)
             VALUES ('media_sponsor_content', $1, $2, 10000, $3)",
        )
        .bind(content.sponsor_enterprise_id)
        .bind(&content.fee)
        .bind(format!("{} — {}", content.content_type, content.title))
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(content.fee)
}

// ═══════════════════════════════════════════════════════════════════
// What an individual can pay for
// ═══════════════════════════════════════════════════════════════════
//
// The only thing on the platform a person pays Skilluv for, and it is worth
// being precise about what it is not: not access to challenges, not a better
// rank, not visibility to companies. Talents do not pay to be seen. Watching
// a replay in high definition is not being seen, which is why this one is
// allowed to exist.

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AudiencePlan {
    pub slug: String,
    pub label: String,
    pub description: String,
    pub price: BigDecimal,
    pub currency: String,
    pub period: String,
}

pub async fn audience_plans(db: &PgPool) -> Result<Vec<AudiencePlan>, AppError> {
    let rows = sqlx::query_as::<_, AudiencePlan>(
        "SELECT slug, label, description, price, currency, period
           FROM audience_plans WHERE is_active ORDER BY price",
    )
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// How long a plan's period runs.
pub fn period_days(period: &str) -> i64 {
    match period {
        "monthly" => 30,
        _ => 365,
    }
}

/// Subscribe, or extend an existing subscription.
///
/// Renewing extends the row that is already there rather than adding a
/// second: two live rows would mean two charges for one access, and the
/// person paying twice is the one least likely to notice.
pub async fn subscribe(
    db: &PgPool,
    user_id: Uuid,
    plan: &str,
    payment_reference: Option<&str>,
) -> Result<chrono::DateTime<chrono::Utc>, AppError> {
    let found: Option<(String, i64)> = sqlx::query_as(
        "SELECT period, 0::BIGINT FROM audience_plans WHERE slug = $1 AND is_active",
    )
    .bind(plan)
    .fetch_optional(db)
    .await?;
    let (period, _) = found.ok_or_else(|| AppError::NotFound("no such plan".into()))?;
    let days = period_days(&period);

    let expires: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
        "INSERT INTO audience_subscriptions
            (user_id, plan, expires_at, payment_provider, payment_reference)
         VALUES ($1, $2, NOW() + ($3 || ' days')::INTERVAL, 'stripe', $4)
         ON CONFLICT (user_id, plan) WHERE cancelled_at IS NULL
         DO UPDATE SET
             -- Extend from whichever is later: renewing early must not throw
             -- away the time already paid for.
             expires_at = GREATEST(audience_subscriptions.expires_at, NOW())
                          + ($3 || ' days')::INTERVAL,
             payment_reference = EXCLUDED.payment_reference,
             auto_renew = TRUE
         RETURNING expires_at",
    )
    .bind(user_id)
    .bind(plan)
    .bind(days.to_string())
    .bind(payment_reference)
    .fetch_one(db)
    .await?;

    Ok(expires)
}

pub async fn cancel_subscription(db: &PgPool, user_id: Uuid, plan: &str) -> Result<(), AppError> {
    // Cancelling stops the renewal; it does not take back what was paid for.
    // Ending access on the day somebody cancels is how a refund request
    // starts.
    sqlx::query(
        "UPDATE audience_subscriptions SET auto_renew = FALSE
          WHERE user_id = $1 AND plan = $2 AND cancelled_at IS NULL",
    )
    .bind(user_id)
    .bind(plan)
    .execute(db)
    .await?;
    Ok(())
}

/// Whether this person's premium access is live right now.
pub async fn has_premium_access(db: &PgPool, user_id: Uuid) -> Result<bool, AppError> {
    let live: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM audience_subscriptions
              WHERE user_id = $1 AND cancelled_at IS NULL AND expires_at > NOW()
         )",
    )
    .bind(user_id)
    .fetch_one(db)
    .await?;
    Ok(live)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn dec(s: &str) -> BigDecimal {
        BigDecimal::from_str(s).unwrap()
    }

    #[test]
    fn a_contract_with_no_discount_pays_the_list_price() {
        assert_eq!(discounted_fee(&dec("15000.00"), &dec("0")), dec("15000.00"));
    }

    #[test]
    fn committing_to_more_events_lowers_the_price_of_each() {
        assert_eq!(
            discounted_fee(&dec("15000.00"), &dec("20.00")),
            dec("12000.00")
        );
        assert_eq!(
            discounted_fee(&dec("1500.00"), &dec("10.00")),
            dec("1350.00")
        );
    }

    #[test]
    fn a_discount_beyond_thirty_percent_is_not_offered() {
        // Past that the contract costs more to service than the events it
        // covers bring in.
        assert!(discount_is_offerable(&dec("0")));
        assert!(discount_is_offerable(&dec("30.00")));
        assert!(!discount_is_offerable(&dec("30.01")));
        assert!(!discount_is_offerable(&dec("-5.00")));
    }

    #[test]
    fn the_discount_never_invents_a_centime() {
        for (fee, discount) in [("999.99", "33.33"), ("1.00", "1.00"), ("50000.00", "17.50")] {
            let charged = discounted_fee(&dec(fee), &dec(discount));
            assert!(charged <= dec(fee), "{fee} at {discount}% went up");
            assert!(!charged.is_negative());
        }
    }

    #[test]
    fn a_disclosure_names_the_sponsor_and_says_who_kept_control() {
        // The part a hurried editor drops, so it is generated rather than
        // asked for.
        let text = default_disclosure("Acme");
        assert!(text.contains("Acme"));
        assert!(text.contains("éditorial"));
        assert!(text.len() >= 10, "the database refuses anything shorter");
    }

    #[test]
    fn a_yearly_plan_runs_a_year_and_a_monthly_one_a_month() {
        assert_eq!(period_days("yearly"), 365);
        assert_eq!(period_days("monthly"), 30);
        // An unknown period falls to the longer one: charging for a year and
        // granting a month is the failure that costs somebody money.
        assert_eq!(period_days("quarterly"), 365);
    }

    #[test]
    fn every_tier_and_interaction_is_a_known_one() {
        assert_eq!(TIERS.len(), 5);
        assert!(TIERS.contains(&"platinum"));
        assert_eq!(INTERACTIONS.len(), 4);
        assert!(INTERACTIONS.contains(&"cv_shared"));
    }
}
