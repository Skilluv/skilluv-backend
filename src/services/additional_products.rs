//! Long placements, corporate learning seats, and open calls for proposals.
//!
//! Four of section 12's seven products needed no table at all — the
//! newsletter is an audience plan, rank-as-a-service is a scope on the
//! existing metered API, consulting is a third kind of consultation, and
//! media sponsorship is sponsored content with a null event. What is here is
//! the rest.
//!
//! ## The guarantee replaces the service, not the person
//!
//! A long placement promises a company a replacement if the person leaves
//! inside the guarantee. That promise is about Skilluv's obligation, not
//! about the person's: nothing here obliges anybody to stay, and a placement
//! that ends because somebody found something better is a normal ending with
//! a reason of its own.
//!
//! ## An open call is answered
//!
//! Every proposal gets a decision, and a refusal carries a reason. People
//! wrote them for nothing; silence is the one thing not owed to them, and the
//! award is blocked until everybody has heard back.

use bigdecimal::BigDecimal;
use bigdecimal::num_traits::Signed;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::ledger;

/// Why a long placement ended.
pub const END_REASONS: &[&str] = &[
    "completed",
    "person_left",
    "company_ended",
    "mutual",
    "dismissed",
];

/// Whether a departure triggers the replacement guarantee.
///
/// Only when the person left or was dismissed, and only inside the window. A
/// company that restructured has not been let down, and a completed contract
/// has not either — charging Skilluv for those would make the guarantee a
/// refund clause for anything at all.
pub fn guarantee_applies(reason: &str, months_elapsed: i64, guarantee_months: i64) -> bool {
    if months_elapsed >= guarantee_months {
        return false;
    }
    matches!(reason, "person_left" | "dismissed")
}

/// What a corporate learning subscription costs a month.
pub fn monthly_cost(fee_per_seat: &BigDecimal, seats: i64) -> BigDecimal {
    fee_per_seat * BigDecimal::from(seats.max(0))
}

// ═══════════════════════════════════════════════════════════════════
// Long placements
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Placement {
    pub id: Uuid,
    pub enterprise_id: Uuid,
    pub junior_user_id: Uuid,
    pub mentor_user_id: Option<Uuid>,
    pub duration_months: i16,
    pub annual_salary_declared: BigDecimal,
    pub currency: String,
    pub upfront_fee: BigDecimal,
    pub monthly_monitoring_fee: BigDecimal,
    pub guarantee_months: i16,
    pub started_on: Option<chrono::NaiveDate>,
    pub status: String,
    pub junior_accepted_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

const PLACEMENT_SELECT: &str = r#"
    SELECT id, enterprise_id, junior_user_id, mentor_user_id, duration_months,
           annual_salary_declared, currency, upfront_fee, monthly_monitoring_fee,
           guarantee_months, started_on, status, junior_accepted_at, created_at
      FROM long_term_placements
"#;

#[derive(Debug, Clone, Deserialize)]
pub struct PlacementInput {
    pub junior_user_id: Uuid,
    #[serde(default)]
    pub mentor_user_id: Option<Uuid>,
    #[serde(default)]
    pub duration_months: Option<i16>,
    pub annual_salary_declared: BigDecimal,
    pub upfront_fee: BigDecimal,
    #[serde(default)]
    pub monthly_monitoring_fee: Option<BigDecimal>,
    #[serde(default)]
    pub guarantee_months: Option<i16>,
    #[serde(default = "eur")]
    pub currency: String,
}

fn eur() -> String {
    "EUR".into()
}

pub async fn propose_placement(
    db: &PgPool,
    enterprise_id: Uuid,
    author: Uuid,
    input: PlacementInput,
) -> Result<Placement, AppError> {
    if !input.annual_salary_declared.is_positive() {
        return Err(AppError::Validation(
            "the declared salary has to be a figure — the fees rest on it".into(),
        ));
    }

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO long_term_placements
            (enterprise_id, junior_user_id, mentor_user_id, duration_months,
             annual_salary_declared, currency, upfront_fee, monthly_monitoring_fee,
             guarantee_months, created_by)
         VALUES ($1,$2,$3,COALESCE($4,24),$5,$6,$7,COALESCE($8,0),
                 COALESCE($9,12),$10)
         RETURNING id",
    )
    .bind(enterprise_id)
    .bind(input.junior_user_id)
    .bind(input.mentor_user_id)
    .bind(input.duration_months)
    .bind(&input.annual_salary_declared)
    .bind(&input.currency)
    .bind(&input.upfront_fee)
    .bind(input.monthly_monitoring_fee.as_ref())
    .bind(input.guarantee_months)
    .bind(author)
    .fetch_one(db)
    .await
    .map_err(|e| {
        if e.to_string().contains("monitoring_needs_somebody_doing_it") {
            AppError::Validation(
                "a monthly monitoring fee needs somebody assigned to do the monitoring. \
                 Otherwise it is a charge for nothing."
                    .into(),
            )
        } else {
            AppError::from(e)
        }
    })?;

    placement(db, id).await
}

pub async fn placement(db: &PgPool, id: Uuid) -> Result<Placement, AppError> {
    let sql = format!("{PLACEMENT_SELECT} WHERE id = $1");
    sqlx::query_as::<_, Placement>(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NotFound("placement not found".into()))
}

pub async fn placements_for_enterprise(
    db: &PgPool,
    enterprise_id: Uuid,
) -> Result<Vec<Placement>, AppError> {
    let sql = format!("{PLACEMENT_SELECT} WHERE enterprise_id = $1 ORDER BY created_at DESC");
    let rows = sqlx::query_as::<_, Placement>(sqlx::AssertSqlSafe(sql))
        .bind(enterprise_id)
        .fetch_all(db)
        .await?;
    Ok(rows)
}

/// A placement as the junior meets it: the same figures, plus who is offering
/// it and who would mentor. The enterprise id and mentor id alone do not let a
/// person decide — an anonymous job offer is not one (SKI-331, the shape
/// SKI-301 gave the cautions).
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct JuniorPlacement {
    pub id: Uuid,
    pub enterprise_id: Uuid,
    pub enterprise_name: Option<String>,
    pub junior_user_id: Uuid,
    pub mentor_user_id: Option<Uuid>,
    pub mentor_username: Option<String>,
    pub duration_months: i16,
    pub annual_salary_declared: BigDecimal,
    pub currency: String,
    pub upfront_fee: BigDecimal,
    pub monthly_monitoring_fee: BigDecimal,
    pub guarantee_months: i16,
    pub started_on: Option<chrono::NaiveDate>,
    pub status: String,
    pub junior_accepted_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// The placements offered to one person, the ones still waiting on their answer
/// first. The read that was missing entirely — a junior had the `respond`
/// endpoint but no way to find the id to give it (SKI-331).
pub async fn placements_for_junior(
    db: &PgPool,
    junior_user_id: Uuid,
) -> Result<Vec<JuniorPlacement>, AppError> {
    let rows = sqlx::query_as::<_, JuniorPlacement>(
        r#"
        SELECT p.id, p.enterprise_id, e.company_name AS enterprise_name,
               p.junior_user_id, p.mentor_user_id, m.username AS mentor_username,
               p.duration_months, p.annual_salary_declared, p.currency,
               p.upfront_fee, p.monthly_monitoring_fee, p.guarantee_months,
               p.started_on, p.status, p.junior_accepted_at, p.created_at
          FROM long_term_placements p
          LEFT JOIN enterprises e ON e.id = p.enterprise_id
          LEFT JOIN users m ON m.id = p.mentor_user_id
         WHERE p.junior_user_id = $1
         ORDER BY (p.status = 'proposed') DESC, p.created_at DESC
        "#,
    )
    .bind(junior_user_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// The person answers, and accepting books the fee up front.
pub async fn respond_to_placement(
    db: &PgPool,
    id: Uuid,
    junior_user_id: Uuid,
    accept: bool,
) -> Result<Placement, AppError> {
    let done = sqlx::query(
        "UPDATE long_term_placements
            SET junior_accepted_at = CASE WHEN $3 THEN NOW() END,
                junior_declined_at = CASE WHEN $3 THEN NULL ELSE NOW() END,
                status = CASE WHEN $3 THEN 'active' ELSE 'declined' END,
                started_on = CASE WHEN $3 THEN CURRENT_DATE END
          WHERE id = $1 AND junior_user_id = $2 AND status = 'proposed'",
    )
    .bind(id)
    .bind(junior_user_id)
    .bind(accept)
    .execute(db)
    .await?;

    if done.rows_affected() == 0 {
        return Err(AppError::NotFound(
            "no placement is waiting on your answer here".into(),
        ));
    }

    let placement = placement(db, id).await?;
    if accept && placement.upfront_fee.is_positive() {
        sqlx::query(
            "INSERT INTO platform_revenues
                (source, related_talent_id, related_enterprise_id, amount_credits,
                 fee_rate_bps, notes)
             VALUES ('long_term_placement', $1, $2, $3, 10000,
                     'frais initiaux de placement longue durée')",
        )
        .bind(placement.junior_user_id)
        .bind(placement.enterprise_id)
        .bind(&placement.upfront_fee)
        .execute(db)
        .await?;
    }

    Ok(placement)
}

/// Bill a month of monitoring, and pay the mentor doing it.
pub async fn bill_monitoring_month(
    db: &PgPool,
    placement_id: Uuid,
    month: chrono::NaiveDate,
) -> Result<BigDecimal, AppError> {
    let placement = placement(db, placement_id).await?;
    if placement.status != "active" {
        return Err(AppError::Validation(format!(
            "this placement is {} — a month that was not monitored is not billed",
            placement.status
        )));
    }
    if !placement.monthly_monitoring_fee.is_positive() {
        return Err(AppError::Validation(
            "this placement has no monitoring fee".into(),
        ));
    }

    let inserted = sqlx::query(
        "INSERT INTO placement_monitoring_months (placement_id, counts_for_month, amount)
         VALUES ($1,$2,$3)
         ON CONFLICT (placement_id, counts_for_month) DO NOTHING",
    )
    .bind(placement_id)
    .bind(month)
    .bind(&placement.monthly_monitoring_fee)
    .execute(db)
    .await?;

    if inserted.rows_affected() == 0 {
        return Err(AppError::Validation(
            "that month has already been billed".into(),
        ));
    }

    // Split with the mentor on the same terms as an onboarding: they do the
    // work, we hold the arrangement.
    if let Some(mentor) = placement.mentor_user_id {
        let (mentor_share, platform) = crate::services::continuous::split_fee(
            &placement.monthly_monitoring_fee,
            &BigDecimal::try_from(crate::services::continuous::MENTOR_SHARE).unwrap_or_default(),
        );

        let currency: ledger::Currency = placement.currency.parse()?;
        ledger::capture_for_recipient(
            db,
            "stripe",
            format!("placement_month:{placement_id}:{month}"),
            mentor,
            mentor_share,
            BigDecimal::from(0),
            currency,
            "placement_monitoring",
            placement_id,
        )
        .await?;

        sqlx::query(
            "INSERT INTO platform_revenues
                (source, related_talent_id, related_enterprise_id, amount_credits,
                 fee_rate_bps, notes)
             VALUES ('long_term_placement', $1, $2, $3, 4000, $4)",
        )
        .bind(mentor)
        .bind(placement.enterprise_id)
        .bind(&platform)
        .bind(format!("suivi mensuel — {month}"))
        .execute(db)
        .await?;
    }

    Ok(placement.monthly_monitoring_fee)
}

/// End a placement, and say whether the guarantee applies.
pub async fn end_placement(
    db: &PgPool,
    placement_id: Uuid,
    reason: &str,
) -> Result<bool, AppError> {
    if !END_REASONS.contains(&reason) {
        return Err(AppError::Validation(format!(
            "the reason must be one of: {}",
            END_REASONS.join(", ")
        )));
    }

    let placement = placement(db, placement_id).await?;
    let elapsed = placement
        .started_on
        .map(|start| {
            let days = (chrono::Utc::now().date_naive() - start).num_days();
            days / 30
        })
        .unwrap_or(0);

    let owed = guarantee_applies(reason, elapsed, placement.guarantee_months as i64);

    sqlx::query(
        "UPDATE long_term_placements
            SET status = CASE WHEN $2 = 'completed' THEN 'completed' ELSE 'ended_early' END,
                ended_reason = $2, ended_on = CURRENT_DATE
          WHERE id = $1 AND status = 'active'",
    )
    .bind(placement_id)
    .bind(reason)
    .execute(db)
    .await?;

    Ok(owed)
}

// ═══════════════════════════════════════════════════════════════════
// Corporate learning
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct LearningPlan {
    pub slug: String,
    pub label: String,
    pub monthly_fee_per_seat: BigDecimal,
    pub currency: String,
    pub features: Vec<String>,
}

pub async fn learning_plans(db: &PgPool) -> Result<Vec<LearningPlan>, AppError> {
    let rows = sqlx::query_as::<_, LearningPlan>(
        "SELECT slug, label, monthly_fee_per_seat, currency, features
           FROM corporate_learning_plans WHERE is_active ORDER BY sort_order",
    )
    .fetch_all(db)
    .await?;
    Ok(rows)
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct LearningSubscription {
    pub id: Uuid,
    pub enterprise_id: Uuid,
    pub plan: String,
    pub seats: i16,
    pub monthly_fee_per_seat: BigDecimal,
    pub currency: String,
    pub current_period_end: chrono::DateTime<chrono::Utc>,
    pub auto_renew: bool,
}

pub async fn subscribe_learning(
    db: &PgPool,
    enterprise_id: Uuid,
    plan: &str,
    seats: i16,
) -> Result<LearningSubscription, AppError> {
    if seats < 1 {
        return Err(AppError::Validation("buy at least one seat".into()));
    }

    let fee: Option<BigDecimal> = sqlx::query_scalar(
        "SELECT monthly_fee_per_seat FROM corporate_learning_plans
          WHERE slug = $1 AND is_active",
    )
    .bind(plan)
    .fetch_optional(db)
    .await?;
    let fee = fee.ok_or_else(|| AppError::NotFound("no such plan".into()))?;

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO corporate_learning_subscriptions
            (enterprise_id, plan, seats, monthly_fee_per_seat, current_period_end)
         VALUES ($1,$2,$3,$4, NOW() + INTERVAL '30 days')
         ON CONFLICT (enterprise_id) WHERE cancelled_at IS NULL
         DO UPDATE SET
             plan = EXCLUDED.plan,
             seats = EXCLUDED.seats,
             monthly_fee_per_seat = EXCLUDED.monthly_fee_per_seat,
             current_period_end = GREATEST(
                 corporate_learning_subscriptions.current_period_end, NOW()
             ) + INTERVAL '30 days',
             auto_renew = TRUE
         RETURNING id",
    )
    .bind(enterprise_id)
    .bind(plan)
    .bind(seats)
    .bind(&fee)
    .fetch_one(db)
    .await?;

    let total = monthly_cost(&fee, seats as i64);
    sqlx::query(
        "INSERT INTO platform_revenues
            (source, related_enterprise_id, amount_credits, fee_rate_bps, notes)
         VALUES ('corporate_learning', $1, $2, 10000, $3)",
    )
    .bind(enterprise_id)
    .bind(&total)
    .bind(format!("{seats} sièges — {plan}"))
    .execute(db)
    .await?;

    learning_subscription(db, id).await
}

pub async fn learning_subscription(
    db: &PgPool,
    id: Uuid,
) -> Result<LearningSubscription, AppError> {
    sqlx::query_as::<_, LearningSubscription>(
        "SELECT id, enterprise_id, plan, seats, monthly_fee_per_seat, currency,
                current_period_end, auto_renew
           FROM corporate_learning_subscriptions WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| AppError::NotFound("subscription not found".into()))
}

/// Invite an employee onto a seat.
pub async fn invite_to_seat(
    db: &PgPool,
    subscription_id: Uuid,
    employee_user_id: Uuid,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO corporate_learning_seats (subscription_id, employee_user_id)
         VALUES ($1, $2)
         ON CONFLICT (subscription_id, employee_user_id) DO UPDATE
             SET released_at = NULL",
    )
    .bind(subscription_id)
    .bind(employee_user_id)
    .execute(db)
    .await
    .map_err(|e| {
        let m = e.to_string();
        if m.contains("seats and they are all taken") {
            AppError::Validation(
                m.rsplit("ERROR:")
                    .next()
                    .unwrap_or("no seats left")
                    .trim()
                    .to_string(),
            )
        } else {
            AppError::from(e)
        }
    })?;
    Ok(())
}

/// Take a seat. Their own act, not their employer's.
pub async fn activate_seat(
    db: &PgPool,
    subscription_id: Uuid,
    employee_user_id: Uuid,
) -> Result<(), AppError> {
    let done = sqlx::query(
        "UPDATE corporate_learning_seats SET activated_at = NOW()
          WHERE subscription_id = $1 AND employee_user_id = $2
            AND released_at IS NULL AND activated_at IS NULL",
    )
    .bind(subscription_id)
    .bind(employee_user_id)
    .execute(db)
    .await?;

    if done.rows_affected() == 0 {
        return Err(AppError::NotFound("no seat is waiting for you here".into()));
    }
    Ok(())
}

/// Seats bought, handed out, and actually taken.
///
/// The third number is the honest one. A seat assigned and never used is not
/// a user, and reporting it as one lets a company believe it bought
/// engagement.
pub async fn seat_usage(db: &PgPool, subscription_id: Uuid) -> Result<(i16, i64, i64), AppError> {
    let subscription = learning_subscription(db, subscription_id).await?;
    let counts: (i64, i64) = sqlx::query_as(
        "SELECT count(*) FILTER (WHERE released_at IS NULL),
                count(*) FILTER (WHERE released_at IS NULL AND activated_at IS NOT NULL)
           FROM corporate_learning_seats WHERE subscription_id = $1",
    )
    .bind(subscription_id)
    .fetch_one(db)
    .await?;
    Ok((subscription.seats, counts.0, counts.1))
}

// ═══════════════════════════════════════════════════════════════════
// Open calls for proposals
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Rfp {
    pub id: Uuid,
    pub slug: String,
    pub enterprise_id: Uuid,
    pub title: String,
    pub context_md: String,
    pub desired_outcome_md: String,
    pub budget_min: BigDecimal,
    pub budget_max: BigDecimal,
    pub currency: String,
    pub proposal_deadline: chrono::DateTime<chrono::Utc>,
    pub selection_deadline: chrono::DateTime<chrono::Utc>,
    pub visibility: String,
    pub facilitation_fee: BigDecimal,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

const RFP_SELECT: &str = r#"
    SELECT id, slug, enterprise_id, title, context_md, desired_outcome_md,
           budget_min, budget_max, currency, proposal_deadline, selection_deadline,
           visibility, facilitation_fee, status, created_at
      FROM open_rfps
"#;

#[derive(Debug, Clone, Deserialize)]
pub struct RfpInput {
    pub slug: String,
    pub title: String,
    pub context_md: String,
    pub desired_outcome_md: String,
    pub budget_min: BigDecimal,
    pub budget_max: BigDecimal,
    #[serde(default = "eur")]
    pub currency: String,
    pub proposal_deadline: chrono::DateTime<chrono::Utc>,
    pub selection_deadline: chrono::DateTime<chrono::Utc>,
    #[serde(default)]
    pub visibility: Option<String>,
    #[serde(default)]
    pub facilitation_fee: Option<BigDecimal>,
}

pub async fn open_rfp(
    db: &PgPool,
    enterprise_id: Uuid,
    author: Uuid,
    input: RfpInput,
) -> Result<Rfp, AppError> {
    if input.context_md.trim().len() < 100 {
        return Err(AppError::Validation(
            "describe the situation properly. People are about to spend an evening \
             writing an answer to it."
                .into(),
        ));
    }
    if !input.budget_min.is_positive() || input.budget_max < input.budget_min {
        return Err(AppError::Validation(
            "publish a budget range. A call with none wastes the time of everybody \
             whose answer would have been 'not for that'."
                .into(),
        ));
    }

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO open_rfps
            (slug, enterprise_id, title, context_md, desired_outcome_md, budget_min,
             budget_max, currency, proposal_deadline, selection_deadline, visibility,
             facilitation_fee, created_by)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,COALESCE($11,'public'),
                 COALESCE($12,0),$13)
         RETURNING id",
    )
    .bind(input.slug.trim())
    .bind(enterprise_id)
    .bind(input.title.trim())
    .bind(input.context_md.trim())
    .bind(input.desired_outcome_md.trim())
    .bind(&input.budget_min)
    .bind(&input.budget_max)
    .bind(&input.currency)
    .bind(input.proposal_deadline)
    .bind(input.selection_deadline)
    .bind(input.visibility.as_deref())
    .bind(input.facilitation_fee.as_ref())
    .bind(author)
    .fetch_one(db)
    .await
    .map_err(|e| {
        let m = e.to_string();
        if m.contains("selection_follows_proposals") {
            AppError::Validation(
                "the selection deadline has to come after the proposal deadline, and \
                 both have to exist. A call with no end is a pile of unpaid proposals \
                 nobody ever answers."
                    .into(),
            )
        } else if m.contains("open_rfps_slug_key") {
            AppError::Validation("that slug is taken".into())
        } else {
            AppError::from(e)
        }
    })?;

    rfp(db, id).await
}

pub async fn rfp(db: &PgPool, id: Uuid) -> Result<Rfp, AppError> {
    let sql = format!("{RFP_SELECT} WHERE id = $1");
    sqlx::query_as::<_, Rfp>(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NotFound("call not found".into()))
}

pub async fn open_rfps(db: &PgPool) -> Result<Vec<Rfp>, AppError> {
    let sql = format!(
        "{RFP_SELECT} WHERE status = 'open' AND visibility = 'public'
            AND proposal_deadline > NOW()
          ORDER BY proposal_deadline LIMIT 100"
    );
    let rows = sqlx::query_as::<_, Rfp>(sqlx::AssertSqlSafe(sql))
        .fetch_all(db)
        .await?;
    Ok(rows)
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct RfpProposal {
    pub id: Uuid,
    pub proposer_user_id: Option<Uuid>,
    pub proposer_studio_id: Option<Uuid>,
    pub pitch_md: String,
    pub approach_md: String,
    pub estimated_price: BigDecimal,
    pub estimated_weeks: i16,
    pub credentials: Vec<String>,
    pub selected: bool,
    pub decision_note: Option<String>,
    pub submitted_at: chrono::DateTime<chrono::Utc>,
    pub decided_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub async fn rfp_proposals(db: &PgPool, rfp_id: Uuid) -> Result<Vec<RfpProposal>, AppError> {
    let rows = sqlx::query_as::<_, RfpProposal>(
        "SELECT id, proposer_user_id, proposer_studio_id, pitch_md, approach_md,
                estimated_price, estimated_weeks, credentials, selected,
                decision_note, submitted_at, decided_at
           FROM rfp_proposals WHERE rfp_id = $1 ORDER BY submitted_at",
    )
    .bind(rfp_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProposalInput {
    pub pitch_md: String,
    pub approach_md: String,
    pub estimated_price: BigDecimal,
    pub estimated_weeks: i16,
    #[serde(default)]
    pub credentials: Vec<String>,
    #[serde(default)]
    pub studio_id: Option<Uuid>,
}

pub async fn submit_proposal(
    db: &PgPool,
    rfp_id: Uuid,
    proposer: Uuid,
    input: ProposalInput,
) -> Result<Uuid, AppError> {
    let rfp = rfp(db, rfp_id).await?;
    if rfp.status != "open" {
        return Err(AppError::Validation(format!(
            "this call is {} and is not taking proposals",
            rfp.status
        )));
    }
    if rfp.proposal_deadline < chrono::Utc::now() {
        return Err(AppError::Validation("the deadline has passed".into()));
    }
    if input.pitch_md.trim().len() < 100 || input.approach_md.trim().len() < 100 {
        return Err(AppError::Validation(
            "say what you would do and how, properly. The client is choosing between \
             this and somebody else's evening."
                .into(),
        ));
    }

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO rfp_proposals
            (rfp_id, proposer_user_id, proposer_studio_id, pitch_md, approach_md,
             estimated_price, estimated_weeks, credentials)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
         RETURNING id",
    )
    .bind(rfp_id)
    .bind(if input.studio_id.is_some() {
        None
    } else {
        Some(proposer)
    })
    .bind(input.studio_id)
    .bind(input.pitch_md.trim())
    .bind(input.approach_md.trim())
    .bind(&input.estimated_price)
    .bind(input.estimated_weeks)
    .bind(&input.credentials)
    .fetch_one(db)
    .await
    .map_err(|e| {
        if e.to_string().contains("idx_one_proposal_per") {
            AppError::Validation("you have already proposed on this call".into())
        } else {
            AppError::from(e)
        }
    })?;

    Ok(id)
}

/// Answer one proposal. Refusing carries a reason.
pub async fn decide_proposal(
    db: &PgPool,
    proposal_id: Uuid,
    selected: bool,
    note: Option<&str>,
) -> Result<(), AppError> {
    if !selected {
        let note = note
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .ok_or_else(|| {
                AppError::Validation(
                    "say why. Somebody wrote this for nothing, and silence is the one thing \
                 not owed to them."
                        .into(),
                )
            })?;

        sqlx::query(
            "UPDATE rfp_proposals SET selected = FALSE, decided_at = NOW(),
                    decision_note = $2
              WHERE id = $1",
        )
        .bind(proposal_id)
        .bind(note)
        .execute(db)
        .await?;
        return Ok(());
    }

    sqlx::query(
        "UPDATE rfp_proposals SET selected = TRUE, decided_at = NOW(),
                decision_note = $2
          WHERE id = $1",
    )
    .bind(proposal_id)
    .bind(note.map(str::trim).filter(|n| !n.is_empty()))
    .execute(db)
    .await?;
    Ok(())
}

/// Award the call, once everybody has heard back.
pub async fn award_rfp(
    db: &PgPool,
    rfp_id: Uuid,
    winner_proposal_id: Uuid,
) -> Result<BigDecimal, AppError> {
    let rfp = rfp(db, rfp_id).await?;

    let mut tx = db.begin().await?;
    sqlx::query("UPDATE open_rfps SET status = 'awarded', winner_proposal_id = $2 WHERE id = $1")
        .bind(rfp_id)
        .bind(winner_proposal_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            let m = e.to_string();
            if m.contains("proposals have had no answer") {
                AppError::Validation(format!(
                    "{} The company has what it wants; the others are the ones left waiting.",
                    m.rsplit("ERROR:")
                        .next()
                        .unwrap_or("some proposals have had no answer")
                        .trim()
                ))
            } else {
                AppError::from(e)
            }
        })?;

    if rfp.facilitation_fee.is_positive() {
        sqlx::query(
            "INSERT INTO platform_revenues
                (source, related_enterprise_id, amount_credits, fee_rate_bps, notes)
             VALUES ('rfp_facilitation', $1, $2, 10000, $3)",
        )
        .bind(rfp.enterprise_id)
        .bind(&rfp.facilitation_fee)
        .bind(format!("appel à propositions « {} »", rfp.title))
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(rfp.facilitation_fee)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn dec(s: &str) -> BigDecimal {
        BigDecimal::from_str(s).unwrap()
    }

    #[test]
    fn the_guarantee_covers_a_departure_inside_the_window() {
        assert!(guarantee_applies("person_left", 4, 12));
        assert!(guarantee_applies("dismissed", 11, 12));
    }

    #[test]
    fn the_guarantee_stops_at_the_window() {
        assert!(!guarantee_applies("person_left", 12, 12));
        assert!(!guarantee_applies("person_left", 20, 12));
    }

    #[test]
    fn a_restructuring_is_not_a_failure_of_the_placement() {
        // Charging Skilluv for these would make the guarantee a refund
        // clause for anything at all.
        assert!(!guarantee_applies("company_ended", 3, 12));
        assert!(!guarantee_applies("mutual", 3, 12));
        assert!(!guarantee_applies("completed", 3, 12));
    }

    #[test]
    fn a_zero_guarantee_covers_nothing() {
        assert!(!guarantee_applies("person_left", 0, 0));
    }

    #[test]
    fn seats_multiply() {
        assert_eq!(monthly_cost(&dec("30.00"), 40), dec("1200.00"));
        assert_eq!(monthly_cost(&dec("10.00"), 1), dec("10.00"));
        assert_eq!(monthly_cost(&dec("30.00"), 0), dec("0"));
    }

    #[test]
    fn every_end_reason_is_a_known_one() {
        assert_eq!(END_REASONS.len(), 5);
        assert!(END_REASONS.contains(&"company_ended"));
    }
}
