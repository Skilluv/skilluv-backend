//! Product launch campaigns — the community writes, the sponsor pays.
//!
//! A company launches something and asks the community for articles, videos,
//! tutorials, integrations. Skilluv charges a fee to run it; each accepted
//! piece pays its author out of a pot the company puts up.
//!
//! ## Two gates, in this order
//!
//! Skilluv checks the piece is real work before the sponsor decides whether
//! it serves them. Both gates are needed and the order is the point: without
//! the first, a company could reject honest criticism as "poor quality";
//! without the second, a company is billed for something it never wanted.
//!
//! ## The pot is finite and known in advance
//!
//! Acceptance stops when the pot is spent. It is checked before the sponsor
//! accepts rather than after, because a writer told "accepted, but there is
//! no money left" has done the work twice: once for the article, once for the
//! argument.

use bigdecimal::BigDecimal;
use bigdecimal::num_traits::{Signed, ToPrimitive};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::ledger;

pub const CONTENT_TYPES: &[&str] = &[
    "blog_post",
    "video",
    "tutorial",
    "integration",
    "review",
    "translation",
];

/// How many pieces a pot can still pay for.
///
/// Integer division on purpose: three quarters of a reward buys nothing, and
/// rounding up would promise a payment that cannot be made.
pub fn pieces_affordable(pool: &BigDecimal, spent: &BigDecimal, per_piece: &BigDecimal) -> i64 {
    if !per_piece.is_positive() {
        return 0;
    }
    let left = pool - spent;
    if !left.is_positive() {
        return 0;
    }
    (left / per_piece).to_i64().unwrap_or(0)
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Campaign {
    pub id: Uuid,
    pub enterprise_id: Uuid,
    pub product_name: String,
    pub brief_md: String,
    pub product_launch_date: chrono::NaiveDate,
    pub starts_at: chrono::DateTime<chrono::Utc>,
    pub ends_at: chrono::DateTime<chrono::Utc>,
    pub content_types_wanted: Vec<String>,
    pub reward_pool: BigDecimal,
    pub reward_per_piece: BigDecimal,
    pub campaign_fee: BigDecimal,
    pub currency: String,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

const CAMPAIGN_SELECT: &str = r#"
    SELECT id, enterprise_id, product_name, brief_md, product_launch_date,
           starts_at, ends_at, content_types_wanted, reward_pool,
           reward_per_piece, campaign_fee, currency, status, created_at
      FROM product_launch_campaigns
"#;

#[derive(Debug, Clone, Deserialize)]
pub struct CampaignInput {
    pub product_name: String,
    pub brief_md: String,
    pub product_launch_date: chrono::NaiveDate,
    pub starts_at: chrono::DateTime<chrono::Utc>,
    pub ends_at: chrono::DateTime<chrono::Utc>,
    pub content_types_wanted: Vec<String>,
    pub reward_pool: BigDecimal,
    pub reward_per_piece: BigDecimal,
    pub campaign_fee: BigDecimal,
    #[serde(default = "eur")]
    pub currency: String,
}

fn eur() -> String {
    "EUR".into()
}

pub async fn open(
    db: &PgPool,
    enterprise_id: Uuid,
    author: Uuid,
    input: CampaignInput,
) -> Result<Campaign, AppError> {
    if input.brief_md.trim().is_empty() {
        return Err(AppError::Validation(
            "say what the product is and what would be worth writing about it. A brief \
             that is only a press release produces pieces that are only a press release."
                .into(),
        ));
    }
    crate::validators::check_max_len(&input.product_name, "product_name", 200)?;
    crate::validators::check_max_len(&input.brief_md, "brief_md", 20_000)?;

    if input.content_types_wanted.is_empty() {
        return Err(AppError::Validation(
            "say which kinds of content are wanted".into(),
        ));
    }
    for kind in &input.content_types_wanted {
        if !CONTENT_TYPES.contains(&kind.as_str()) {
            return Err(AppError::Validation(format!(
                "'{kind}' is not a content type we run — one of: {}",
                CONTENT_TYPES.join(", ")
            )));
        }
    }

    if input.reward_pool < input.reward_per_piece {
        return Err(AppError::Validation(
            "the pot has to buy at least one piece. A pool smaller than a single reward \
             is a campaign nobody can be paid from, and the person who finds out is the \
             one who already wrote the article."
                .into(),
        ));
    }

    let id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO product_launch_campaigns
            (enterprise_id, product_name, brief_md, product_launch_date, starts_at,
             ends_at, content_types_wanted, reward_pool, reward_per_piece,
             campaign_fee, currency, created_by)
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
        RETURNING id
        "#,
    )
    .bind(enterprise_id)
    .bind(input.product_name.trim())
    .bind(input.brief_md.trim())
    .bind(input.product_launch_date)
    .bind(input.starts_at)
    .bind(input.ends_at)
    .bind(&input.content_types_wanted)
    .bind(&input.reward_pool)
    .bind(&input.reward_per_piece)
    .bind(&input.campaign_fee)
    .bind(&input.currency)
    .bind(author)
    .fetch_one(db)
    .await
    .map_err(|e| {
        if e.to_string().contains("a_campaign_window_runs_forward") {
            AppError::Validation("the campaign has to end after it starts".into())
        } else {
            AppError::from(e)
        }
    })?;

    by_id(db, id).await
}

pub async fn by_id(db: &PgPool, id: Uuid) -> Result<Campaign, AppError> {
    let sql = format!("{CAMPAIGN_SELECT} WHERE id = $1");
    sqlx::query_as::<_, Campaign>(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NotFound("campaign not found".into()))
}

/// What a contributor can write for, soonest deadline first.
pub async fn open_campaigns(db: &PgPool) -> Result<Vec<Campaign>, AppError> {
    let sql = format!(
        "{CAMPAIGN_SELECT} WHERE status = 'open' AND ends_at > NOW()
          ORDER BY ends_at LIMIT 100"
    );
    let rows = sqlx::query_as::<_, Campaign>(sqlx::AssertSqlSafe(sql))
        .fetch_all(db)
        .await?;
    Ok(rows)
}

pub async fn for_enterprise(db: &PgPool, enterprise_id: Uuid) -> Result<Vec<Campaign>, AppError> {
    let sql = format!("{CAMPAIGN_SELECT} WHERE enterprise_id = $1 ORDER BY created_at DESC");
    let rows = sqlx::query_as::<_, Campaign>(sqlx::AssertSqlSafe(sql))
        .bind(enterprise_id)
        .fetch_all(db)
        .await?;
    Ok(rows)
}

/// Open a campaign for submissions.
pub async fn open_for_submissions(db: &PgPool, id: Uuid) -> Result<Campaign, AppError> {
    sqlx::query(
        "UPDATE product_launch_campaigns SET status = 'open'
          WHERE id = $1 AND status = 'briefing'",
    )
    .bind(id)
    .execute(db)
    .await?;
    by_id(db, id).await
}

/// What the pot has left.
pub async fn budget_left(db: &PgPool, campaign_id: Uuid) -> Result<(BigDecimal, i64), AppError> {
    let campaign = by_id(db, campaign_id).await?;
    let accepted: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM launch_campaign_pieces
          WHERE campaign_id = $1 AND status = 'accepted'",
    )
    .bind(campaign_id)
    .fetch_one(db)
    .await?;

    let spent = &campaign.reward_per_piece * BigDecimal::from(accepted);
    let left = &campaign.reward_pool - &spent;
    let affordable = pieces_affordable(&campaign.reward_pool, &spent, &campaign.reward_per_piece);
    Ok((left, affordable))
}

// ═══════════════════════════════════════════════════════════════════
// Pieces
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Piece {
    pub id: Uuid,
    pub author_user_id: Uuid,
    pub username: String,
    pub content_type: String,
    pub title: String,
    pub url: String,
    pub status: String,
    pub quality_notes: Option<String>,
    pub rejection_reason: Option<String>,
    pub submitted_at: chrono::DateTime<chrono::Utc>,
    pub reward_paid_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub async fn pieces(db: &PgPool, campaign_id: Uuid) -> Result<Vec<Piece>, AppError> {
    let rows = sqlx::query_as::<_, Piece>(
        "SELECT p.id, p.author_user_id, u.username, p.content_type, p.title, p.url,
                p.status, p.quality_notes, p.rejection_reason, p.submitted_at,
                p.reward_paid_at
           FROM launch_campaign_pieces p
           JOIN users u ON u.id = p.author_user_id
          WHERE p.campaign_id = $1
          ORDER BY p.submitted_at DESC",
    )
    .bind(campaign_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// What the sponsor is shown: only what has passed Skilluv's check.
pub async fn pieces_for_sponsor(db: &PgPool, campaign_id: Uuid) -> Result<Vec<Piece>, AppError> {
    let rows = sqlx::query_as::<_, Piece>(
        "SELECT p.id, p.author_user_id, u.username, p.content_type, p.title, p.url,
                p.status, p.quality_notes, p.rejection_reason, p.submitted_at,
                p.reward_paid_at
           FROM launch_campaign_pieces p
           JOIN users u ON u.id = p.author_user_id
          WHERE p.campaign_id = $1
            AND p.status IN ('quality_passed', 'accepted', 'rejected')
          ORDER BY p.submitted_at DESC",
    )
    .bind(campaign_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

#[derive(Debug, Clone, Deserialize)]
pub struct PieceInput {
    pub content_type: String,
    pub title: String,
    pub url: String,
}

pub async fn submit(
    db: &PgPool,
    campaign_id: Uuid,
    author: Uuid,
    input: PieceInput,
) -> Result<Uuid, AppError> {
    let campaign = by_id(db, campaign_id).await?;
    if campaign.status != "open" {
        return Err(AppError::Validation(format!(
            "this campaign is {} and is not taking submissions",
            campaign.status
        )));
    }
    if campaign.ends_at < chrono::Utc::now() {
        return Err(AppError::Validation(
            "the campaign window has closed".into(),
        ));
    }
    if !campaign
        .content_types_wanted
        .iter()
        .any(|k| k == &input.content_type)
    {
        return Err(AppError::Validation(format!(
            "this campaign wants: {}",
            campaign.content_types_wanted.join(", ")
        )));
    }
    if !input.url.starts_with("https://") {
        return Err(AppError::Validation(
            "the piece has to be published somewhere reachable, over https".into(),
        ));
    }

    // Told before the work is judged rather than after: a writer who submits
    // into an empty pot has done the work for nothing, and finding out at the
    // verdict is the worst moment to hear it.
    let (_left, affordable) = budget_left(db, campaign_id).await?;
    if affordable <= 0 {
        return Err(AppError::Validation(
            "this campaign's pot is spent — nothing submitted now could be paid".into(),
        ));
    }

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO launch_campaign_pieces
            (campaign_id, author_user_id, content_type, title, url)
         VALUES ($1,$2,$3,$4,$5)
         RETURNING id",
    )
    .bind(campaign_id)
    .bind(author)
    .bind(&input.content_type)
    .bind(input.title.trim())
    .bind(input.url.trim())
    .fetch_one(db)
    .await
    .map_err(|e| {
        if e.to_string()
            .contains("launch_campaign_pieces_campaign_id_url_key")
        {
            AppError::Validation("that piece has already been submitted".into())
        } else {
            AppError::from(e)
        }
    })?;

    Ok(id)
}

/// Skilluv's gate: is this real work.
pub async fn review_quality(
    db: &PgPool,
    piece_id: Uuid,
    reviewer: Uuid,
    passed: bool,
    notes: &str,
) -> Result<(), AppError> {
    if notes.trim().is_empty() {
        return Err(AppError::Validation(
            "a verdict with no notes tells the author nothing they can act on".into(),
        ));
    }

    let done = sqlx::query(
        "UPDATE launch_campaign_pieces
            SET quality_reviewed_by = $2, quality_reviewed_at = NOW(),
                quality_notes = $4,
                status = CASE WHEN $3 THEN 'quality_passed' ELSE 'quality_failed' END,
                rejection_reason = CASE WHEN $3 THEN NULL ELSE $4 END
          WHERE id = $1 AND status = 'submitted'",
    )
    .bind(piece_id)
    .bind(reviewer)
    .bind(passed)
    .bind(notes.trim())
    .execute(db)
    .await?;

    if done.rows_affected() == 0 {
        return Err(AppError::NotFound(
            "that piece is not waiting for a quality check".into(),
        ));
    }
    Ok(())
}

/// The sponsor's gate: does it serve them. Accepting pays the author.
pub async fn decide(
    db: &PgPool,
    piece_id: Uuid,
    accept: bool,
    reason: Option<&str>,
) -> Result<Option<BigDecimal>, AppError> {
    let context: Option<(Uuid, Uuid, String)> = sqlx::query_as(
        "SELECT campaign_id, author_user_id, status
           FROM launch_campaign_pieces WHERE id = $1",
    )
    .bind(piece_id)
    .fetch_optional(db)
    .await?;
    let (campaign_id, author, status) =
        context.ok_or_else(|| AppError::NotFound("piece not found".into()))?;

    if status != "quality_passed" {
        return Err(AppError::Validation(format!(
            "this piece is {status} — only one that has passed Skilluv's check reaches \
             the sponsor"
        )));
    }

    if !accept {
        let reason = reason
            .map(str::trim)
            .filter(|r| !r.is_empty())
            .ok_or_else(|| {
                AppError::Validation(
                    "say why. The piece already passed a quality check, so a refusal here is \
                 an editorial choice and the author is owed the reason for it."
                        .into(),
                )
            })?;

        sqlx::query(
            "UPDATE launch_campaign_pieces
                SET status = 'rejected', rejection_reason = $2, decided_at = NOW()
              WHERE id = $1",
        )
        .bind(piece_id)
        .bind(reason)
        .execute(db)
        .await?;
        return Ok(None);
    }

    let campaign = by_id(db, campaign_id).await?;
    let (_left, affordable) = budget_left(db, campaign_id).await?;
    if affordable <= 0 {
        return Err(AppError::Validation(
            "the pot is spent. Accepting now would promise a payment that cannot be \
             made — raise the pool first."
                .into(),
        ));
    }

    sqlx::query(
        "UPDATE launch_campaign_pieces
            SET status = 'accepted', decided_at = NOW(), reward_paid_at = NOW()
          WHERE id = $1",
    )
    .bind(piece_id)
    .execute(db)
    .await?;

    let currency: ledger::Currency = campaign.currency.parse()?;
    ledger::capture_for_recipient(
        db,
        "stripe",
        format!("launch_piece:{piece_id}"),
        author,
        campaign.reward_per_piece.clone(),
        // Skilluv's cut is the campaign fee, charged once to the company.
        // Taking a slice of each reward on top would charge twice for one
        // piece of work.
        BigDecimal::from(0),
        currency,
        "launch_campaign_piece",
        piece_id,
    )
    .await?;

    Ok(Some(campaign.reward_per_piece))
}

/// Close the campaign and book what Skilluv charged to run it.
pub async fn close(db: &PgPool, campaign_id: Uuid) -> Result<BigDecimal, AppError> {
    let campaign = by_id(db, campaign_id).await?;
    if campaign.status == "closed" {
        return Err(AppError::Validation("already closed".into()));
    }

    let undecided: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM launch_campaign_pieces
          WHERE campaign_id = $1 AND status IN ('submitted', 'quality_passed')",
    )
    .bind(campaign_id)
    .fetch_one(db)
    .await?;
    if undecided > 0 {
        return Err(AppError::Validation(format!(
            "{undecided} piece(s) have no verdict. Closing now would leave their authors \
             unpaid with nothing to appeal against."
        )));
    }

    let mut tx = db.begin().await?;
    sqlx::query(
        "UPDATE product_launch_campaigns
            SET status = 'closed', closed_at = NOW() WHERE id = $1",
    )
    .bind(campaign_id)
    .execute(&mut *tx)
    .await?;

    if campaign.campaign_fee.is_positive() {
        sqlx::query(
            "INSERT INTO platform_revenues
                (source, related_enterprise_id, amount_credits, fee_rate_bps, notes)
             VALUES ('product_launch_campaign', $1, $2, 10000, $3)",
        )
        .bind(campaign.enterprise_id)
        .bind(&campaign.campaign_fee)
        .bind(format!("campagne de lancement {}", campaign.product_name))
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(campaign.campaign_fee)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn dec(s: &str) -> BigDecimal {
        BigDecimal::from_str(s).unwrap()
    }

    #[test]
    fn a_pot_buys_whole_pieces_only() {
        // Three quarters of a reward buys nothing, and rounding up would
        // promise a payment that cannot be made.
        assert_eq!(pieces_affordable(&dec("1000"), &dec("0"), &dec("300")), 3);
        assert_eq!(pieces_affordable(&dec("1000"), &dec("900"), &dec("300")), 0);
        assert_eq!(pieces_affordable(&dec("900"), &dec("0"), &dec("300")), 3);
    }

    #[test]
    fn a_spent_pot_affords_nothing() {
        assert_eq!(pieces_affordable(&dec("1000"), &dec("1000"), &dec("50")), 0);
        // Overspent is not negative capacity; it is still nothing.
        assert_eq!(pieces_affordable(&dec("1000"), &dec("1200"), &dec("50")), 0);
    }

    #[test]
    fn a_reward_of_nothing_affords_nothing() {
        // Otherwise the division would be by zero and the campaign would
        // appear to afford an unlimited number of pieces.
        assert_eq!(pieces_affordable(&dec("1000"), &dec("0"), &dec("0")), 0);
    }

    #[test]
    fn every_content_type_is_a_known_one() {
        assert_eq!(CONTENT_TYPES.len(), 6);
        assert!(CONTENT_TYPES.contains(&"integration"));
        assert!(CONTENT_TYPES.contains(&"translation"));
    }
}
