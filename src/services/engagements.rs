//! Teams Skilluv sells, and the money that moves through them.
//!
//! ## The cascade, in one place
//!
//! The client pays the whole engagement up front. Nothing reaches anybody
//! until a milestone is accepted, and when one is:
//!
//!   1. the milestone's share of the contract is worked out;
//!   2. the platform's margin comes off it;
//!   3. what is left is split between the people on the engagement, by their
//!      agreed shares;
//!   4. each share lands in that person's *pending* balance, and the ordinary
//!      release window applies.
//!
//! Written once because six near-identical tables would have been six chances
//! to get step 3 wrong, and the person who notices is the one underpaid.
//!
//! ## Why the shares must total a hundred
//!
//! A set of shares summing to ninety does not leave a tenth unallocated — it
//! quietly pays everybody ninety per cent of what they agreed. The check is
//! here and at the moment work starts, because the number is only wrong once
//! somebody adds a member and forgets to rebalance.

use bigdecimal::BigDecimal;
use bigdecimal::num_traits::{Signed, ToPrimitive};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

pub const KINDS: &[&str] = &["outsourcing", "discovery", "sprint", "fractional"];

pub const PRICING_MODELS: &[&str] = &["fixed_price", "retainer_monthly", "day_rate"];

/// What Skilluv keeps when it assembles people for one piece of work.
pub const AD_HOC_MARGIN: f64 = 15.0;

/// What a standing team costs. Higher because the client is buying an
/// assembled team with a track record and management included, rather than a
/// list of people who happened to be free.
pub const STUDIO_MARGIN: f64 = 25.0;

/// The band both figures live in, from `docs/business/PRICING.md`.
///
/// Below the floor the coordination is unpaid and done badly; above the
/// ceiling Skilluv is a consultancy with a database, and there are already
/// plenty. The constants are asserted against this band in the tests, so a
/// rate changed here without changing the published grid fails the build
/// rather than surprising a client.
pub const MARGIN_FLOOR: f64 = 15.0;
pub const MARGIN_CEILING: f64 = 30.0;

/// The margin for an engagement, from who is doing it.
pub fn margin_for(has_studio: bool) -> f64 {
    if has_studio {
        STUDIO_MARGIN
    } else {
        AD_HOC_MARGIN
    }
}

/// Whether a set of shares accounts for the whole pot.
///
/// Tolerant to a hundredth, because three equal shares are 33.33 each and
/// demanding an exact hundred would forbid the commonest split there is.
pub fn shares_are_whole(shares: &[f64]) -> bool {
    if shares.is_empty() {
        return false;
    }
    let total: f64 = shares.iter().sum();
    (total - 100.0).abs() <= 0.05
}

/// What a milestone releases, and how it divides.
///
/// Returns the platform's margin and each person's share, in the order the
/// shares were given. The two always add back to the milestone's value: the
/// last person absorbs the rounding rather than the platform, because a
/// centime kept by the platform is a centime taken from wages.
pub fn split_milestone(
    contract_value: &BigDecimal,
    value_percent: &BigDecimal,
    margin_percent: &BigDecimal,
    shares: &[BigDecimal],
) -> (BigDecimal, Vec<BigDecimal>) {
    let milestone = (contract_value * value_percent / BigDecimal::from(100))
        .with_scale_round(2, bigdecimal::RoundingMode::Down);

    let margin = (&milestone * margin_percent / BigDecimal::from(100))
        .with_scale_round(2, bigdecimal::RoundingMode::Down);
    let pot = &milestone - &margin;

    if shares.is_empty() {
        return (margin, vec![]);
    }

    let mut paid = Vec::with_capacity(shares.len());
    let mut running = BigDecimal::from(0);
    for share in shares.iter().take(shares.len() - 1) {
        let amount = (&pot * share / BigDecimal::from(100))
            .with_scale_round(2, bigdecimal::RoundingMode::Down);
        running += &amount;
        paid.push(amount);
    }
    // The remainder, so nothing is lost to rounding and nobody is short by a
    // centime nobody can account for.
    paid.push(&pot - &running);

    (margin, paid)
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Engagement {
    pub id: Uuid,
    pub enterprise_id: Uuid,
    pub studio_id: Option<Uuid>,
    pub kind: String,
    pub title: String,
    pub brief_md: String,
    pub domains_required: Vec<String>,
    pub orientations_required: Vec<String>,
    pub team_size_min: i16,
    pub team_size_max: i16,
    pub duration_weeks: Option<i16>,
    pub days_per_week: Option<BigDecimal>,
    pub pricing_model: String,
    pub budget: Option<BigDecimal>,
    pub monthly_retainer: Option<BigDecimal>,
    pub day_rate: Option<BigDecimal>,
    pub currency: String,
    pub margin_percent: BigDecimal,
    pub nda_required: bool,
    pub ip_terms: String,
    pub status: String,
    pub project_lead_user_id: Option<Uuid>,
    pub starts_at: Option<chrono::DateTime<chrono::Utc>>,
    pub ends_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

const ENGAGEMENT_SELECT: &str = r#"
    SELECT id, enterprise_id, studio_id, kind, title, brief_md,
           domains_required, orientations_required, team_size_min, team_size_max,
           duration_weeks, days_per_week, pricing_model, budget,
           monthly_retainer, day_rate, currency, margin_percent, nda_required,
           ip_terms, status, project_lead_user_id, starts_at, ends_at, created_at
      FROM team_engagements
"#;

#[derive(Debug, Clone, Deserialize)]
pub struct BriefInput {
    pub kind: String,
    #[serde(default)]
    pub studio_id: Option<Uuid>,
    pub title: String,
    pub brief_md: String,
    #[serde(default)]
    pub domains_required: Vec<String>,
    #[serde(default)]
    pub orientations_required: Vec<String>,
    #[serde(default = "one")]
    pub team_size_min: i16,
    pub team_size_max: i16,
    #[serde(default)]
    pub duration_weeks: Option<i16>,
    #[serde(default)]
    pub days_per_week: Option<BigDecimal>,
    pub pricing_model: String,
    #[serde(default)]
    pub budget: Option<BigDecimal>,
    #[serde(default)]
    pub monthly_retainer: Option<BigDecimal>,
    #[serde(default)]
    pub day_rate: Option<BigDecimal>,
    #[serde(default = "eur")]
    pub currency: String,
    #[serde(default)]
    pub nda_required: Option<bool>,
    #[serde(default)]
    pub ip_terms: Option<String>,
    #[serde(default)]
    pub upstream_license_spdx: Option<String>,
}

fn one() -> i16 {
    1
}
fn eur() -> String {
    "EUR".into()
}

/// Take a brief.
pub async fn open(
    db: &PgPool,
    enterprise_id: Uuid,
    author: Uuid,
    input: BriefInput,
) -> Result<Engagement, AppError> {
    if !KINDS.contains(&input.kind.as_str()) {
        return Err(AppError::Validation(format!(
            "kind must be one of: {}",
            KINDS.join(", ")
        )));
    }
    if !PRICING_MODELS.contains(&input.pricing_model.as_str()) {
        return Err(AppError::Validation(format!(
            "pricing_model must be one of: {}",
            PRICING_MODELS.join(", ")
        )));
    }
    if input.title.trim().is_empty() || input.brief_md.trim().is_empty() {
        return Err(AppError::Validation(
            "a title and a brief are required — an engagement with neither cannot be \
             staffed or priced"
                .into(),
        ));
    }
    crate::validators::check_max_len(&input.title, "title", 200)?;
    crate::validators::check_max_len(&input.brief_md, "brief_md", 20_000)?;

    // A studio must exist and be sellable. Booking a forming team is booking
    // people who have not been recruited.
    if let Some(studio_id) = input.studio_id {
        let status: Option<String> = sqlx::query_scalar("SELECT status FROM studios WHERE id = $1")
            .bind(studio_id)
            .fetch_optional(db)
            .await?;
        match status.as_deref() {
            Some("active") => {}
            Some(other) => {
                return Err(AppError::Validation(format!(
                    "that studio is {other} — only an active one can take work"
                )));
            }
            None => return Err(AppError::NotFound("studio not found".into())),
        }
    }

    for slug in &input.orientations_required {
        let resolved: Option<Uuid> = sqlx::query_scalar("SELECT resolve_orientation($1)")
            .bind(slug)
            .fetch_one(db)
            .await?;
        if resolved.is_none() {
            return Err(AppError::Validation(format!(
                "'{slug}' is not a trade Skilluv knows"
            )));
        }
    }

    let margin = margin_for(input.studio_id.is_some());

    let id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO team_engagements
            (enterprise_id, studio_id, kind, title, brief_md, domains_required,
             orientations_required, team_size_min, team_size_max, duration_weeks,
             days_per_week, pricing_model, budget, monthly_retainer, day_rate,
             currency, margin_percent, nda_required, ip_terms,
             upstream_license_spdx, created_by)
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21)
        RETURNING id
        "#,
    )
    .bind(enterprise_id)
    .bind(input.studio_id)
    .bind(&input.kind)
    .bind(input.title.trim())
    .bind(input.brief_md.trim())
    .bind(&input.domains_required)
    .bind(&input.orientations_required)
    .bind(input.team_size_min)
    .bind(input.team_size_max)
    .bind(input.duration_weeks)
    .bind(input.days_per_week.as_ref())
    .bind(&input.pricing_model)
    .bind(input.budget.as_ref())
    .bind(input.monthly_retainer.as_ref())
    .bind(input.day_rate.as_ref())
    .bind(&input.currency)
    .bind(BigDecimal::try_from(margin).unwrap_or_default())
    .bind(input.nda_required.unwrap_or(true))
    .bind(input.ip_terms.as_deref().unwrap_or("full_ownership_client"))
    .bind(input.upstream_license_spdx.as_deref())
    .bind(author)
    .fetch_one(db)
    .await
    .map_err(shape_error)?;

    by_id(db, id).await
}

/// The database speaks in constraint names; this says the same in words the
/// person filling in the form can act on.
fn shape_error(e: sqlx::Error) -> AppError {
    let message = e.to_string();
    for (marker, said) in [
        (
            "price_matches_the_model",
            "the pricing model and the figure disagree: a fixed price needs a budget, a \
             retainer needs a monthly amount, a day rate needs a day rate",
        ),
        (
            "fractional_is_one_person",
            "a fractional placement is one person for part of their week — say how many \
             days, and set the team size to one",
        ),
        (
            "discovery_is_timeboxed",
            "a discovery runs between two and six weeks. It exists to stop an open-ended \
             exploration becoming an open-ended bill",
        ),
        (
            "a_sprint_is_short",
            "a sprint runs between one and twelve weeks — beyond that it is an \
             outsourcing project, and should be one",
        ),
        (
            "cannot promise client ownership",
            "the licence and the IP terms contradict each other",
        ),
    ] {
        if message.contains(marker) {
            return AppError::Validation(said.into());
        }
    }
    AppError::from(e)
}

pub async fn by_id(db: &PgPool, id: Uuid) -> Result<Engagement, AppError> {
    let sql = format!("{ENGAGEMENT_SELECT} WHERE id = $1");
    sqlx::query_as::<_, Engagement>(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NotFound("engagement not found".into()))
}

pub async fn for_enterprise(db: &PgPool, enterprise_id: Uuid) -> Result<Vec<Engagement>, AppError> {
    let sql = format!("{ENGAGEMENT_SELECT} WHERE enterprise_id = $1 ORDER BY created_at DESC");
    let rows = sqlx::query_as::<_, Engagement>(sqlx::AssertSqlSafe(sql))
        .bind(enterprise_id)
        .fetch_all(db)
        .await?;
    Ok(rows)
}

/// The whole value of an engagement, whatever it is priced by.
///
/// A retainer's value is the monthly amount times the months; a day rate's is
/// the rate times the days a full team would work. Both are estimates for a
/// milestone to take a share of, and both are stated rather than left for a
/// caller to reinvent differently.
pub fn contract_value(engagement: &Engagement) -> BigDecimal {
    match engagement.pricing_model.as_str() {
        "fixed_price" => engagement.budget.clone().unwrap_or_default(),
        "retainer_monthly" => {
            let months = engagement.duration_weeks.unwrap_or(4) as i64 / 4;
            engagement.monthly_retainer.clone().unwrap_or_default()
                * BigDecimal::from(months.max(1))
        }
        "day_rate" => {
            let days = engagement.duration_weeks.unwrap_or(1) as i64 * 5;
            engagement.day_rate.clone().unwrap_or_default() * BigDecimal::from(days.max(1))
        }
        _ => BigDecimal::from(0),
    }
}

// ═══════════════════════════════════════════════════════════════════
// Who is on it
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Member {
    pub user_id: Uuid,
    pub username: String,
    pub role_on_engagement: String,
    pub share_percent: BigDecimal,
    pub accepted_at: Option<chrono::DateTime<chrono::Utc>>,
    pub declined_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub async fn members(db: &PgPool, engagement_id: Uuid) -> Result<Vec<Member>, AppError> {
    let rows = sqlx::query_as::<_, Member>(
        "SELECT m.user_id, u.username, m.role_on_engagement, m.share_percent,
                m.accepted_at, m.declined_at
           FROM engagement_members m
           JOIN users u ON u.id = m.user_id
          WHERE m.engagement_id = $1 AND m.left_at IS NULL
          ORDER BY m.share_percent DESC, u.username",
    )
    .bind(engagement_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// Put somebody on an engagement, and ask them.
pub async fn add_member(
    db: &PgPool,
    engagement_id: Uuid,
    user_id: Uuid,
    role: &str,
    share_percent: BigDecimal,
) -> Result<(), AppError> {
    if role.trim().is_empty() {
        return Err(AppError::Validation(
            "say what this person is doing on it".into(),
        ));
    }
    sqlx::query(
        "INSERT INTO engagement_members
            (engagement_id, user_id, role_on_engagement, share_percent)
         VALUES ($1,$2,$3,$4)
         ON CONFLICT (engagement_id, user_id) DO UPDATE
             SET role_on_engagement = EXCLUDED.role_on_engagement,
                 share_percent = EXCLUDED.share_percent,
                 -- A changed share is a changed offer, so the agreement to
                 -- the old one does not carry over.
                 accepted_at = NULL, declined_at = NULL",
    )
    .bind(engagement_id)
    .bind(user_id)
    .bind(role.trim())
    .bind(&share_percent)
    .execute(db)
    .await?;
    Ok(())
}

/// Copy a studio's members and their shares onto an engagement.
///
/// The point of a standing team: the shares were agreed once, when the studio
/// formed, rather than renegotiated on every piece of work.
pub async fn staff_from_studio(
    db: &PgPool,
    engagement_id: Uuid,
    studio_id: Uuid,
) -> Result<u64, AppError> {
    let done = sqlx::query(
        "INSERT INTO engagement_members
            (engagement_id, user_id, role_on_engagement, share_percent)
         SELECT $1, sm.user_id, sm.role_in_studio, sm.revenue_share_percent
           FROM studio_members sm
          WHERE sm.studio_id = $2 AND sm.left_at IS NULL
         ON CONFLICT (engagement_id, user_id) DO NOTHING",
    )
    .bind(engagement_id)
    .bind(studio_id)
    .execute(db)
    .await?;
    Ok(done.rows_affected())
}

/// Somebody answers.
pub async fn respond(
    db: &PgPool,
    engagement_id: Uuid,
    user_id: Uuid,
    accept: bool,
) -> Result<(), AppError> {
    let done = sqlx::query(
        "UPDATE engagement_members
            SET accepted_at = CASE WHEN $3 THEN NOW() END,
                declined_at = CASE WHEN $3 THEN NULL ELSE NOW() END
          WHERE engagement_id = $1 AND user_id = $2 AND left_at IS NULL",
    )
    .bind(engagement_id)
    .bind(user_id)
    .bind(accept)
    .execute(db)
    .await?;

    if done.rows_affected() == 0 {
        return Err(AppError::NotFound("you are not on this engagement".into()));
    }
    Ok(())
}

/// Move an engagement to `in_progress`, once it can honestly start.
///
/// Three things must hold, and all three are the kind that go wrong quietly:
/// everybody has agreed, the shares account for the whole pot, and the
/// milestones account for the whole contract.
pub async fn start(db: &PgPool, engagement_id: Uuid) -> Result<Engagement, AppError> {
    let team = members(db, engagement_id).await?;
    if team.is_empty() {
        return Err(AppError::Validation("nobody is on this engagement".into()));
    }

    let unanswered: Vec<&str> = team
        .iter()
        .filter(|m| m.accepted_at.is_none())
        .map(|m| m.username.as_str())
        .collect();
    if !unanswered.is_empty() {
        return Err(AppError::Validation(format!(
            "not everybody has agreed: {}. Nobody is put on paid work without saying yes.",
            unanswered.join(", ")
        )));
    }

    let shares: Vec<f64> = team
        .iter()
        .filter_map(|m| m.share_percent.to_f64())
        .collect();
    if !shares_are_whole(&shares) {
        let total: f64 = shares.iter().sum();
        return Err(AppError::Validation(format!(
            "the shares total {total:.2}%, not 100%. A set summing to less does not leave \
             a remainder unallocated — it quietly pays everybody less than they agreed."
        )));
    }

    let milestone_total: Option<BigDecimal> = sqlx::query_scalar(
        "SELECT COALESCE(sum(value_percent), 0) FROM engagement_milestones
          WHERE engagement_id = $1",
    )
    .bind(engagement_id)
    .fetch_one(db)
    .await?;
    let milestone_total = milestone_total.and_then(|t| t.to_f64()).unwrap_or(0.0);
    if (milestone_total - 100.0).abs() > 0.05 {
        return Err(AppError::Validation(format!(
            "the milestones account for {milestone_total:.2}% of the contract, not 100%. \
             The rest would have nowhere to be paid from."
        )));
    }

    sqlx::query(
        "UPDATE team_engagements
            SET status = 'in_progress', starts_at = COALESCE(starts_at, NOW())
          WHERE id = $1",
    )
    .bind(engagement_id)
    .execute(db)
    .await?;

    by_id(db, engagement_id).await
}

// ═══════════════════════════════════════════════════════════════════
// Milestones
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Milestone {
    pub id: Uuid,
    pub sequence: i16,
    pub title: String,
    pub acceptance_criteria: String,
    pub due_on: Option<chrono::NaiveDate>,
    pub value_percent: BigDecimal,
    pub status: String,
    pub reviewed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub review_notes: Option<String>,
    pub accepted_at: Option<chrono::DateTime<chrono::Utc>>,
    pub rejection_reason: Option<String>,
    pub released_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub async fn milestones(db: &PgPool, engagement_id: Uuid) -> Result<Vec<Milestone>, AppError> {
    let rows = sqlx::query_as::<_, Milestone>(
        "SELECT id, sequence, title, acceptance_criteria, due_on, value_percent,
                status, reviewed_at, review_notes, accepted_at, rejection_reason,
                released_at
           FROM engagement_milestones
          WHERE engagement_id = $1 ORDER BY sequence",
    )
    .bind(engagement_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

#[derive(Debug, Clone, Deserialize)]
pub struct MilestoneInput {
    pub title: String,
    pub acceptance_criteria: String,
    pub value_percent: BigDecimal,
    #[serde(default)]
    pub due_on: Option<chrono::NaiveDate>,
}

pub async fn add_milestone(
    db: &PgPool,
    engagement_id: Uuid,
    input: MilestoneInput,
) -> Result<Uuid, AppError> {
    if input.acceptance_criteria.trim().is_empty() {
        return Err(AppError::Validation(
            "say what done means for this checkpoint — a milestone defined afterwards is \
             a milestone argued about"
                .into(),
        ));
    }

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO engagement_milestones
            (engagement_id, sequence, title, acceptance_criteria, value_percent, due_on)
         VALUES ($1,
                 (SELECT COALESCE(max(sequence), 0) + 1 FROM engagement_milestones
                   WHERE engagement_id = $1),
                 $2, $3, $4, $5)
         RETURNING id",
    )
    .bind(engagement_id)
    .bind(input.title.trim())
    .bind(input.acceptance_criteria.trim())
    .bind(&input.value_percent)
    .bind(input.due_on)
    .fetch_one(db)
    .await?;

    Ok(id)
}

/// Skilluv reviews before the client sees it.
///
/// The step that distinguishes this from a freelance marketplace, and the
/// reason the margin is what it is. A milestone cannot reach the client
/// without passing here — the database refuses it.
pub async fn review(
    db: &PgPool,
    milestone_id: Uuid,
    reviewer: Uuid,
    passed: bool,
    notes: &str,
) -> Result<(), AppError> {
    if notes.trim().is_empty() {
        return Err(AppError::Validation(
            "a review with no notes is a signature, and a signature is not a review".into(),
        ));
    }

    let done = sqlx::query(
        "UPDATE engagement_milestones
            SET reviewed_by = $2, reviewed_at = NOW(), review_notes = $4,
                status = CASE WHEN $3 THEN 'submitted' ELSE 'in_progress' END
          WHERE id = $1 AND status IN ('in_review', 'in_progress')",
    )
    .bind(milestone_id)
    .bind(reviewer)
    .bind(passed)
    .bind(notes.trim())
    .execute(db)
    .await?;

    if done.rows_affected() == 0 {
        return Err(AppError::NotFound(
            "that milestone is not waiting for review".into(),
        ));
    }
    Ok(())
}

/// The client accepts, and the money moves.
///
/// The whole cascade in one transaction: margin to the platform, the rest
/// divided by the agreed shares, each part into that person's pending
/// balance. Partial success here would mean somebody paid and somebody not.
pub async fn accept_milestone(
    db: &PgPool,
    milestone_id: Uuid,
    accepted_by: Uuid,
) -> Result<Vec<(Uuid, BigDecimal)>, AppError> {
    let context: Option<(Uuid, BigDecimal, String, String)> = sqlx::query_as(
        "SELECT m.engagement_id, m.value_percent, m.status, e.currency
           FROM engagement_milestones m
           JOIN team_engagements e ON e.id = m.engagement_id
          WHERE m.id = $1",
    )
    .bind(milestone_id)
    .fetch_optional(db)
    .await?;
    let (engagement_id, value_percent, status, currency) =
        context.ok_or_else(|| AppError::NotFound("milestone not found".into()))?;

    if status != "submitted" {
        return Err(AppError::Validation(format!(
            "this milestone is {status} — only a reviewed and submitted one can be accepted"
        )));
    }

    let engagement = by_id(db, engagement_id).await?;
    let team = members(db, engagement_id).await?;
    let value = contract_value(&engagement);

    let shares: Vec<BigDecimal> = team.iter().map(|m| m.share_percent.clone()).collect();
    let (margin, paid) =
        split_milestone(&value, &value_percent, &engagement.margin_percent, &shares);

    let mut tx = db.begin().await?;

    sqlx::query(
        "UPDATE engagement_milestones
            SET status = 'accepted', accepted_at = NOW(), accepted_by = $2,
                released_at = NOW()
          WHERE id = $1",
    )
    .bind(milestone_id)
    .bind(accepted_by)
    .execute(&mut *tx)
    .await?;

    if margin.is_positive() {
        sqlx::query(
            "INSERT INTO platform_revenues
                (source, related_enterprise_id, amount_credits, fee_rate_bps, notes)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(if engagement.studio_id.is_some() {
            "studio_margin"
        } else {
            "outsourcing_margin"
        })
        .bind(engagement.enterprise_id)
        .bind(&margin)
        .bind(crate::services::ledger::percent_to_bps(
            &engagement.margin_percent,
        ))
        .bind(format!(
            "jalon {milestone_id} sur l'engagement {engagement_id}"
        ))
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    // The ledger movements happen after the milestone is marked, so a failure
    // here leaves an accepted milestone with no payment — visible and
    // repairable — rather than a payment with no milestone, which is not.
    let currency: crate::services::ledger::Currency = currency.parse()?;
    let mut settled = Vec::new();
    for (member, amount) in team.iter().zip(paid.iter()) {
        if !amount.is_positive() {
            continue;
        }
        crate::services::ledger::capture_for_recipient(
            db,
            "stripe",
            format!("milestone:{milestone_id}:{}", member.user_id),
            member.user_id,
            amount.clone(),
            BigDecimal::from(0),
            currency,
            "engagement_milestone",
            milestone_id,
        )
        .await?;
        settled.push((member.user_id, amount.clone()));
    }

    Ok(settled)
}

// ═══════════════════════════════════════════════════════════════════
// Studios
// ═══════════════════════════════════════════════════════════════════
//
// A studio is a team that outlives the work. It is the thing a client books
// by name, and the reason the margin is higher: they are buying an assembled
// team with a track record, not a list of people who were free that week.

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Studio {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub specialization: String,
    pub domains: Vec<String>,
    pub day_rate: BigDecimal,
    pub currency: String,
    pub max_members: i16,
    pub lead_user_id: Option<Uuid>,
    pub status: String,
    pub formed_at: chrono::DateTime<chrono::Utc>,
}

const STUDIO_SELECT: &str = r#"
    SELECT id, slug, name, specialization, domains, day_rate, currency,
           max_members, lead_user_id, status, formed_at
      FROM studios
"#;

/// A studio's slug is a public name — it appears in the URL a client is sent
/// and in the credit line on delivered work, so it is held to the shape a URL
/// can carry without escaping.
fn check_slug(slug: &str) -> Result<(), AppError> {
    let ok = (3..=80).contains(&slug.len())
        && slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if ok {
        Ok(())
    } else {
        Err(AppError::Validation(
            "the slug must be 3 to 80 characters of lowercase letters, digits and dashes".into(),
        ))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct StudioInput {
    pub slug: String,
    pub name: String,
    pub specialization: String,
    #[serde(default)]
    pub domains: Vec<String>,
    pub day_rate: BigDecimal,
    #[serde(default = "eur")]
    pub currency: String,
    #[serde(default)]
    pub max_members: Option<i16>,
}

pub async fn create_studio(
    db: &PgPool,
    input: StudioInput,
    lead_user_id: Option<Uuid>,
) -> Result<Studio, AppError> {
    check_slug(&input.slug)?;
    if input.specialization.trim().is_empty() {
        return Err(AppError::Validation(
            "say what this studio is for — a studio that does everything is a job board \
             with a name"
                .into(),
        ));
    }

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO studios
            (slug, name, specialization, domains, day_rate, currency, max_members,
             lead_user_id)
         VALUES ($1,$2,$3,$4,$5,$6,COALESCE($7, 15),$8)
         RETURNING id",
    )
    .bind(input.slug.trim())
    .bind(input.name.trim())
    .bind(input.specialization.trim())
    .bind(&input.domains)
    .bind(&input.day_rate)
    .bind(&input.currency)
    .bind(input.max_members)
    .bind(lead_user_id)
    .fetch_one(db)
    .await
    .map_err(|e| {
        if e.to_string().contains("studios_slug_key") {
            AppError::Validation(format!("the slug '{}' is already taken", input.slug))
        } else {
            AppError::from(e)
        }
    })?;

    studio_by_id(db, id).await
}

pub async fn studio_by_id(db: &PgPool, id: Uuid) -> Result<Studio, AppError> {
    let sql = format!("{STUDIO_SELECT} WHERE id = $1");
    sqlx::query_as::<_, Studio>(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NotFound("studio not found".into()))
}

/// The studios a client can actually book.
pub async fn bookable_studios(db: &PgPool) -> Result<Vec<Studio>, AppError> {
    let sql = format!("{STUDIO_SELECT} WHERE status = 'active' ORDER BY name");
    let rows = sqlx::query_as::<_, Studio>(sqlx::AssertSqlSafe(sql))
        .fetch_all(db)
        .await?;
    Ok(rows)
}

pub async fn studio_members(db: &PgPool, studio_id: Uuid) -> Result<Vec<Member>, AppError> {
    let rows = sqlx::query_as::<_, Member>(
        "SELECT sm.user_id, u.username, sm.role_in_studio AS role_on_engagement,
                sm.revenue_share_percent AS share_percent,
                sm.joined_at AS accepted_at, NULL::TIMESTAMPTZ AS declined_at
           FROM studio_members sm
           JOIN users u ON u.id = sm.user_id
          WHERE sm.studio_id = $1 AND sm.left_at IS NULL
          ORDER BY sm.revenue_share_percent DESC, u.username",
    )
    .bind(studio_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

pub async fn add_studio_member(
    db: &PgPool,
    studio_id: Uuid,
    user_id: Uuid,
    role: &str,
    revenue_share_percent: BigDecimal,
) -> Result<(), AppError> {
    let seats: (i64, i16) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM studio_members
                  WHERE studio_id = $1 AND left_at IS NULL),
                (SELECT max_members FROM studios WHERE id = $1)",
    )
    .bind(studio_id)
    .fetch_one(db)
    .await?;

    if seats.0 >= seats.1 as i64 {
        return Err(AppError::Validation(format!(
            "this studio holds {} people. Past that it is a department, and a \
             department needs managing rather than assembling.",
            seats.1
        )));
    }

    sqlx::query(
        "INSERT INTO studio_members
            (studio_id, user_id, role_in_studio, revenue_share_percent)
         VALUES ($1,$2,$3,$4)
         ON CONFLICT (studio_id, user_id) DO UPDATE
             SET role_in_studio = EXCLUDED.role_in_studio,
                 revenue_share_percent = EXCLUDED.revenue_share_percent,
                 left_at = NULL",
    )
    .bind(studio_id)
    .bind(user_id)
    .bind(role.trim())
    .bind(&revenue_share_percent)
    .execute(db)
    .await?;
    Ok(())
}

/// Open a studio for business.
///
/// Refused until the shares add up, because a studio's shares are what get
/// copied onto every engagement it takes: a wrong number here is a wrong
/// number on every piece of work the team ever does.
pub async fn activate_studio(
    db: &PgPool,
    studio_id: Uuid,
    lead_user_id: Uuid,
) -> Result<Studio, AppError> {
    let team = studio_members(db, studio_id).await?;
    if team.len() < 2 {
        return Err(AppError::Validation(
            "a studio is at least two people — one person is a freelancer, and Skilluv \
             has a place for that already"
                .into(),
        ));
    }

    let shares: Vec<f64> = team
        .iter()
        .filter_map(|m| m.share_percent.to_f64())
        .collect();
    if !shares_are_whole(&shares) {
        let total: f64 = shares.iter().sum();
        return Err(AppError::Validation(format!(
            "the revenue shares total {total:.2}%, not 100%. These get copied onto every \
             engagement the studio takes, so a wrong number here is wrong on every piece \
             of work the team ever does."
        )));
    }

    if !team.iter().any(|m| m.user_id == lead_user_id) {
        return Err(AppError::Validation(
            "the lead has to be on the team".into(),
        ));
    }

    sqlx::query("UPDATE studios SET status = 'active', lead_user_id = $2 WHERE id = $1")
        .bind(studio_id)
        .bind(lead_user_id)
        .execute(db)
        .await?;

    studio_by_id(db, studio_id).await
}

pub async fn disband_studio(db: &PgPool, studio_id: Uuid, reason: &str) -> Result<(), AppError> {
    if reason.trim().is_empty() {
        return Err(AppError::Validation(
            "say why. People built a reputation under this name.".into(),
        ));
    }

    let live: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM team_engagements
          WHERE studio_id = $1 AND status IN ('proposed', 'in_progress')",
    )
    .bind(studio_id)
    .fetch_one(db)
    .await?;
    if live > 0 {
        return Err(AppError::Validation(format!(
            "{live} engagement(s) are still running under this studio. Disbanding now \
             would leave a client with a team that no longer exists."
        )));
    }

    sqlx::query("UPDATE studios SET status = 'disbanded', disbanded_reason = $2 WHERE id = $1")
        .bind(studio_id)
        .bind(reason.trim())
        .execute(db)
        .await?;
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
    fn a_standing_team_costs_more_than_an_assembled_one() {
        // The client is buying a track record and management, not a list of
        // people who were free.
        assert!(margin_for(true) > margin_for(false));
        assert!(margin_for(false) >= MARGIN_FLOOR);
        assert!(margin_for(true) <= MARGIN_CEILING);
    }

    #[test]
    fn three_equal_shares_are_whole_enough() {
        // 33.33 three times is 99.99. Demanding an exact hundred would forbid
        // the commonest split there is.
        assert!(shares_are_whole(&[33.33, 33.33, 33.34]));
        assert!(shares_are_whole(&[33.33, 33.33, 33.33]));
        assert!(shares_are_whole(&[100.0]));
        assert!(shares_are_whole(&[50.0, 50.0]));
    }

    #[test]
    fn shares_that_do_not_add_up_are_refused() {
        // A set summing to ninety does not leave a tenth unallocated — it
        // quietly pays everybody ninety per cent of what they agreed.
        assert!(!shares_are_whole(&[45.0, 45.0]));
        assert!(!shares_are_whole(&[60.0, 60.0]));
        assert!(!shares_are_whole(&[]));
    }

    #[test]
    fn the_margin_and_the_shares_always_add_back_to_the_milestone() {
        for (value, percent, margin, shares) in [
            ("100000.00", "25.00", "15.00", vec!["50.00", "50.00"]),
            (
                "33333.33",
                "33.33",
                "25.00",
                vec!["33.33", "33.33", "33.34"],
            ),
            ("1.00", "100.00", "15.00", vec!["100.00"]),
            (
                "999999.99",
                "10.00",
                "30.00",
                vec!["20.00", "30.00", "50.00"],
            ),
        ] {
            let shares: Vec<BigDecimal> = shares.iter().map(|s| dec(s)).collect();
            let (kept, paid) = split_milestone(&dec(value), &dec(percent), &dec(margin), &shares);

            let expected = (dec(value) * dec(percent) / BigDecimal::from(100))
                .with_scale_round(2, bigdecimal::RoundingMode::Down);
            let total: BigDecimal = paid.iter().fold(kept.clone(), |acc, part| acc + part);

            assert_eq!(
                total, expected,
                "{value} at {percent}% lost or invented a centime"
            );
        }
    }

    #[test]
    fn the_rounding_goes_to_the_last_person_not_the_platform() {
        // A centime kept by the platform is a centime taken from wages.
        let shares = vec![dec("33.33"), dec("33.33"), dec("33.34")];
        let (margin, paid) =
            split_milestone(&dec("100.00"), &dec("100.00"), &dec("15.00"), &shares);
        assert_eq!(margin, dec("15.00"));

        let pot = dec("85.00");
        let total: BigDecimal = paid.iter().sum();
        assert_eq!(total, pot);
        // The first two are rounded down; the last carries the remainder.
        assert!(paid[2] >= paid[0]);
    }

    #[test]
    fn nobody_is_paid_from_an_engagement_with_no_members() {
        let (margin, paid) = split_milestone(&dec("10000.00"), &dec("50.00"), &dec("15.00"), &[]);
        assert_eq!(margin, dec("750.00"));
        assert!(paid.is_empty());
    }

    #[test]
    fn every_kind_and_pricing_model_is_a_known_one() {
        assert_eq!(KINDS.len(), 4);
        assert_eq!(PRICING_MODELS.len(), 3);
        assert!(KINDS.contains(&"fractional"));
        assert!(PRICING_MODELS.contains(&"retainer_monthly"));
    }
}
