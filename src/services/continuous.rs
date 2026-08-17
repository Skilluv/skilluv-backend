//! Three products that keep a relationship going rather than closing a
//! transaction: onboarding a new hire, a living lab, a team's own proposal.
//!
//! ## Bought by one person, done with another
//!
//! Onboarding is paid for by an employer and delivered to their new hire. The
//! person is not the customer, and the whole product is three months of
//! somebody's attention on them — so it does not start until they agree, and
//! nothing is recorded about how long they stayed if they never did.
//!
//! ## The last one runs the other way
//!
//! Everywhere else on the platform a company states a need. A team proposal
//! is a team saying "here is a problem we think you have". It is the only
//! place the offer originates with the people who would do the work, and the
//! proposal has to open with the problem: one that opens with the solution is
//! a team describing what it wants to build.

use bigdecimal::BigDecimal;
use bigdecimal::num_traits::Signed;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::ledger;

/// What a mentor takes of an onboarding fee.
///
/// The majority, because they do the three months. The rest is Skilluv
/// designing the run, holding the monthly check-ins and chasing the ones that
/// go quiet — which is real, and is not most of it.
pub const MENTOR_SHARE: f64 = 60.0;

/// What Skilluv takes when a team's own proposal turns into a contract.
///
/// Low, deliberately: the team found the problem, wrote the approach and
/// convinced the client. Skilluv held the meeting.
pub const FACILITATION_PERCENT: f64 = 10.0;

/// How a fee divides between a mentor and the platform.
pub fn split_fee(fee: &BigDecimal, mentor_percent: &BigDecimal) -> (BigDecimal, BigDecimal) {
    let mentor = (fee * mentor_percent / BigDecimal::from(100))
        .with_scale_round(2, bigdecimal::RoundingMode::Down);
    let platform = fee - &mentor;
    (mentor, platform)
}

/// What each accepted contribution is worth this month.
///
/// The pool divided by the accepted contributions, rounded down. Recomputed
/// each month rather than fixed per contribution, because the point of a pool
/// is that a quiet month pays the few people who showed up more, not that it
/// leaves money unspent.
pub fn contribution_reward(pool: &BigDecimal, accepted: i64) -> BigDecimal {
    if accepted <= 0 {
        return BigDecimal::from(0);
    }
    (pool / BigDecimal::from(accepted)).with_scale_round(2, bigdecimal::RoundingMode::Down)
}

// ═══════════════════════════════════════════════════════════════════
// Onboarding
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Onboarding {
    pub id: Uuid,
    pub enterprise_id: Uuid,
    pub junior_user_id: Uuid,
    pub mentor_user_id: Uuid,
    pub junior_accepted_at: Option<chrono::DateTime<chrono::Utc>>,
    pub duration_months: i16,
    pub fee: BigDecimal,
    pub currency: String,
    pub mentor_share_percent: BigDecimal,
    pub started_on: Option<chrono::NaiveDate>,
    pub retention_3m: Option<bool>,
    pub retention_6m: Option<bool>,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

const ONBOARDING_SELECT: &str = r#"
    SELECT id, enterprise_id, junior_user_id, mentor_user_id, junior_accepted_at,
           duration_months, fee, currency, mentor_share_percent, started_on,
           retention_3m, retention_6m, status, created_at
      FROM hire_onboarding_engagements
"#;

#[derive(Debug, Clone, Deserialize)]
pub struct OnboardingInput {
    pub junior_user_id: Uuid,
    pub mentor_user_id: Uuid,
    #[serde(default)]
    pub duration_months: Option<i16>,
    #[serde(default)]
    pub curriculum: Option<serde_json::Value>,
    pub fee: BigDecimal,
    #[serde(default = "eur")]
    pub currency: String,
}

fn eur() -> String {
    "EUR".into()
}

pub async fn propose_onboarding(
    db: &PgPool,
    enterprise_id: Uuid,
    author: Uuid,
    input: OnboardingInput,
) -> Result<Onboarding, AppError> {
    if input.junior_user_id == input.mentor_user_id {
        return Err(AppError::Validation(
            "the mentor and the person being onboarded are the same account".into(),
        ));
    }
    if !input.fee.is_positive() {
        return Err(AppError::Validation("set a fee".into()));
    }

    // The mentor has to be somebody we would stand behind. A first onboarding
    // is somebody's first three months in a job, and it is not the place to
    // find out whether the mentor is up to it.
    let rank: Option<String> = sqlx::query_scalar("SELECT rank FROM user_ranks WHERE user_id = $1")
        .bind(input.mentor_user_id)
        .fetch_optional(db)
        .await?;
    let rank = rank.unwrap_or_else(|| "apprenti".into());
    if !crate::services::ambassadors::rank_clears(&rank, "artisan") {
        return Err(AppError::Validation(format!(
            "an onboarding mentor opens at artisan, and this person is {rank}. This is \
             somebody's first three months in a job."
        )));
    }

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO hire_onboarding_engagements
            (enterprise_id, junior_user_id, mentor_user_id, duration_months,
             curriculum, fee, currency, mentor_share_percent, created_by)
         VALUES ($1,$2,$3,COALESCE($4,3),COALESCE($5,'{}'::jsonb),$6,$7,$8,$9)
         RETURNING id",
    )
    .bind(enterprise_id)
    .bind(input.junior_user_id)
    .bind(input.mentor_user_id)
    .bind(input.duration_months)
    .bind(input.curriculum.as_ref())
    .bind(&input.fee)
    .bind(&input.currency)
    .bind(BigDecimal::try_from(MENTOR_SHARE).unwrap_or_default())
    .bind(author)
    .fetch_one(db)
    .await?;

    onboarding(db, id).await
}

pub async fn onboarding(db: &PgPool, id: Uuid) -> Result<Onboarding, AppError> {
    let sql = format!("{ONBOARDING_SELECT} WHERE id = $1");
    sqlx::query_as::<_, Onboarding>(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NotFound("onboarding not found".into()))
}

pub async fn onboardings_for(db: &PgPool, user_id: Uuid) -> Result<Vec<Onboarding>, AppError> {
    let sql = format!(
        "{ONBOARDING_SELECT} WHERE junior_user_id = $1 OR mentor_user_id = $1
          ORDER BY created_at DESC"
    );
    let rows = sqlx::query_as::<_, Onboarding>(sqlx::AssertSqlSafe(sql))
        .bind(user_id)
        .fetch_all(db)
        .await?;
    Ok(rows)
}

/// The person being onboarded answers.
///
/// Their employer bought it; that does not make it consented to. Accepting
/// starts it and pays the mentor their share.
pub async fn respond_to_onboarding(
    db: &PgPool,
    id: Uuid,
    junior_user_id: Uuid,
    accept: bool,
) -> Result<Onboarding, AppError> {
    let done = sqlx::query(
        "UPDATE hire_onboarding_engagements
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
            "no onboarding is waiting on your answer here".into(),
        ));
    }

    let engagement = onboarding(db, id).await?;
    if !accept {
        return Ok(engagement);
    }

    let (mentor_share, platform) = split_fee(&engagement.fee, &engagement.mentor_share_percent);

    let currency: ledger::Currency = engagement.currency.parse()?;
    ledger::capture_for_recipient(
        db,
        "stripe",
        format!("onboarding:{id}"),
        engagement.mentor_user_id,
        mentor_share,
        BigDecimal::from(0),
        currency,
        "hire_onboarding",
        id,
    )
    .await?;

    sqlx::query(
        "INSERT INTO platform_revenues
            (source, related_talent_id, related_enterprise_id, amount_credits,
             fee_rate_bps, notes)
         VALUES ('onboarding_service', $1, $2, $3, $4, 'accompagnement prise de poste')",
    )
    .bind(engagement.mentor_user_id)
    .bind(engagement.enterprise_id)
    .bind(&platform)
    .bind(ledger::percent_to_bps(
        &(BigDecimal::from(100) - &engagement.mentor_share_percent),
    ))
    .execute(db)
    .await?;

    Ok(engagement)
}

/// Record a monthly check-in.
///
/// Both sides write, because an onboarding assessed only by the person paid
/// to deliver it assesses itself.
pub async fn record_check_in(
    db: &PgPool,
    engagement_id: Uuid,
    author: Uuid,
    month_number: i16,
    notes_md: &str,
    going_well: Option<bool>,
) -> Result<(), AppError> {
    let engagement = onboarding(db, engagement_id).await?;
    let is_mentor = author == engagement.mentor_user_id;
    let is_junior = author == engagement.junior_user_id;

    if !is_mentor && !is_junior {
        return Err(AppError::NotFound("not your onboarding".into()));
    }
    if notes_md.trim().is_empty() {
        return Err(AppError::Validation(
            "say how it is going. A check-in with no note is a tick in a box.".into(),
        ));
    }

    let column = if is_mentor {
        "mentor_notes_md"
    } else {
        "junior_notes_md"
    };

    let sql = format!(
        "INSERT INTO onboarding_check_ins
            (engagement_id, month_number, {column}, going_well)
         VALUES ($1,$2,$3,$4)
         ON CONFLICT (engagement_id, month_number) DO UPDATE
             SET {column} = EXCLUDED.{column},
                 going_well = COALESCE(EXCLUDED.going_well, onboarding_check_ins.going_well)"
    );

    sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(engagement_id)
        .bind(month_number)
        .bind(notes_md.trim())
        .bind(going_well)
        .execute(db)
        .await?;

    Ok(())
}

/// Record whether somebody is still there.
pub async fn record_retention(
    db: &PgPool,
    engagement_id: Uuid,
    months: i16,
    still_there: bool,
) -> Result<(), AppError> {
    let column = match months {
        3 => "retention_3m",
        6 => "retention_6m",
        _ => {
            return Err(AppError::Validation(
                "retention is checked at three and six months".into(),
            ));
        }
    };

    let sql = format!(
        "UPDATE hire_onboarding_engagements
            SET {column} = $2, retention_checked_at = NOW()
          WHERE id = $1"
    );

    sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(engagement_id)
        .bind(still_there)
        .execute(db)
        .await
        .map_err(|e| {
            if e.to_string().contains("retention_follows_an_agreement") {
                AppError::Validation(
                    "nothing is recorded about how long somebody stayed if they never \
                     agreed to be accompanied in the first place"
                        .into(),
                )
            } else {
                AppError::from(e)
            }
        })?;

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════
// Living labs
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Lab {
    pub id: Uuid,
    pub enterprise_id: Uuid,
    pub product_name: String,
    pub scope_md: String,
    pub community_target: i16,
    pub activity_types: Vec<String>,
    pub monthly_fee: BigDecimal,
    pub monthly_reward_pool: BigDecimal,
    pub currency: String,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

const LAB_SELECT: &str = r#"
    SELECT id, enterprise_id, product_name, scope_md, community_target,
           activity_types, monthly_fee, monthly_reward_pool, currency, status,
           created_at
      FROM living_lab_engagements
"#;

#[derive(Debug, Clone, Deserialize)]
pub struct LabInput {
    pub product_name: String,
    pub scope_md: String,
    pub community_target: i16,
    pub activity_types: Vec<String>,
    pub monthly_fee: BigDecimal,
    pub monthly_reward_pool: BigDecimal,
    #[serde(default = "eur")]
    pub currency: String,
}

pub async fn open_lab(
    db: &PgPool,
    enterprise_id: Uuid,
    author: Uuid,
    input: LabInput,
) -> Result<Lab, AppError> {
    if input.activity_types.is_empty() {
        return Err(AppError::Validation(
            "say what the community will be asked to do".into(),
        ));
    }
    if !input.monthly_reward_pool.is_positive() {
        return Err(AppError::Validation(
            "set a reward pool. A lab with none is a company asking a hundred people to \
             work on its product for the pleasure of it, with Skilluv charging a monthly \
             fee for arranging that."
                .into(),
        ));
    }

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO living_lab_engagements
            (enterprise_id, product_name, scope_md, community_target, activity_types,
             monthly_fee, monthly_reward_pool, currency, created_by)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
         RETURNING id",
    )
    .bind(enterprise_id)
    .bind(input.product_name.trim())
    .bind(input.scope_md.trim())
    .bind(input.community_target)
    .bind(&input.activity_types)
    .bind(&input.monthly_fee)
    .bind(&input.monthly_reward_pool)
    .bind(&input.currency)
    .bind(author)
    .fetch_one(db)
    .await?;

    lab(db, id).await
}

pub async fn lab(db: &PgPool, id: Uuid) -> Result<Lab, AppError> {
    let sql = format!("{LAB_SELECT} WHERE id = $1");
    sqlx::query_as::<_, Lab>(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NotFound("lab not found".into()))
}

pub async fn open_labs(db: &PgPool) -> Result<Vec<Lab>, AppError> {
    let sql = format!(
        "{LAB_SELECT} WHERE status IN ('recruiting', 'running')
          ORDER BY created_at DESC LIMIT 100"
    );
    let rows = sqlx::query_as::<_, Lab>(sqlx::AssertSqlSafe(sql))
        .fetch_all(db)
        .await?;
    Ok(rows)
}

/// Join a lab, agreeing to the terms that come with seeing an unreleased
/// product.
pub async fn join_lab(db: &PgPool, lab_id: Uuid, user_id: Uuid) -> Result<(), AppError> {
    let lab = lab(db, lab_id).await?;
    if !matches!(lab.status.as_str(), "recruiting" | "running") {
        return Err(AppError::Validation("this lab is not taking people".into()));
    }

    let members: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM living_lab_members WHERE lab_id = $1 AND left_at IS NULL",
    )
    .bind(lab_id)
    .fetch_one(db)
    .await?;
    if members >= lab.community_target as i64 {
        return Err(AppError::Validation(format!(
            "this lab asked for {} people and has them",
            lab.community_target
        )));
    }

    sqlx::query(
        "INSERT INTO living_lab_members (lab_id, user_id, nda_accepted_at)
         VALUES ($1, $2, NOW())
         ON CONFLICT (lab_id, user_id) DO UPDATE SET left_at = NULL",
    )
    .bind(lab_id)
    .bind(user_id)
    .execute(db)
    .await?;
    Ok(())
}

/// Record a contribution.
pub async fn contribute(
    db: &PgPool,
    lab_id: Uuid,
    user_id: Uuid,
    activity_type: &str,
    summary_md: &str,
) -> Result<Uuid, AppError> {
    let lab = lab(db, lab_id).await?;
    if !lab.activity_types.iter().any(|a| a == activity_type) {
        return Err(AppError::Validation(format!(
            "this lab asks for: {}",
            lab.activity_types.join(", ")
        )));
    }
    if summary_md.trim().len() < 30 {
        return Err(AppError::Validation(
            "say what you found, in enough words to be useful to somebody who was not \
             there"
                .into(),
        ));
    }

    let month = first_of_this_month();

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO living_lab_contributions
            (lab_id, user_id, activity_type, summary_md, counts_for_month)
         VALUES ($1,$2,$3,$4,$5)
         RETURNING id",
    )
    .bind(lab_id)
    .bind(user_id)
    .bind(activity_type)
    .bind(summary_md.trim())
    .bind(month)
    .fetch_one(db)
    .await
    .map_err(|e| {
        if e.to_string()
            .contains("living_lab_contributions_lab_id_user_id_fkey")
        {
            AppError::Validation("you are not on this lab".into())
        } else {
            AppError::from(e)
        }
    })?;

    Ok(id)
}

fn first_of_this_month() -> chrono::NaiveDate {
    use chrono::Datelike;
    let now = chrono::Utc::now();
    chrono::NaiveDate::from_ymd_opt(now.year(), now.month(), 1).unwrap_or_default()
}

/// Accept or refuse a contribution.
pub async fn judge_contribution(
    db: &PgPool,
    contribution_id: Uuid,
    accept: bool,
    reason: Option<&str>,
) -> Result<(), AppError> {
    if !accept {
        let reason = reason
            .map(str::trim)
            .filter(|r| !r.is_empty())
            .ok_or_else(|| {
                AppError::Validation(
                    "say why. Somebody spent an evening on this and the pool is what they \
                 were promised for it."
                        .into(),
                )
            })?;

        sqlx::query(
            "UPDATE living_lab_contributions
                SET accepted = FALSE, rejection_reason = $2 WHERE id = $1",
        )
        .bind(contribution_id)
        .bind(reason)
        .execute(db)
        .await?;
        return Ok(());
    }

    sqlx::query("UPDATE living_lab_contributions SET accepted = TRUE WHERE id = $1")
        .bind(contribution_id)
        .execute(db)
        .await?;
    Ok(())
}

/// Divide a month's pool between the accepted contributions and pay it out.
///
/// A quiet month pays the few people who showed up more, rather than leaving
/// money unspent — which is the point of a pool rather than a per-item rate.
pub async fn settle_month(
    db: &PgPool,
    lab_id: Uuid,
    month: chrono::NaiveDate,
) -> Result<(i64, BigDecimal), AppError> {
    let lab = lab(db, lab_id).await?;

    let payable: Vec<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT id, user_id FROM living_lab_contributions
          WHERE lab_id = $1 AND counts_for_month = $2
            AND accepted IS TRUE AND paid_at IS NULL",
    )
    .bind(lab_id)
    .bind(month)
    .fetch_all(db)
    .await?;

    if payable.is_empty() {
        return Err(AppError::Validation(
            "nothing accepted and unpaid that month".into(),
        ));
    }

    let each = contribution_reward(&lab.monthly_reward_pool, payable.len() as i64);
    let currency: ledger::Currency = lab.currency.parse()?;

    for (contribution_id, user_id) in &payable {
        sqlx::query(
            "UPDATE living_lab_contributions SET reward = $2, paid_at = NOW()
              WHERE id = $1",
        )
        .bind(contribution_id)
        .bind(&each)
        .execute(db)
        .await?;

        if each.is_positive() {
            ledger::capture_for_recipient(
                db,
                "stripe",
                format!("lab_contribution:{contribution_id}"),
                *user_id,
                each.clone(),
                BigDecimal::from(0),
                currency,
                "living_lab_contribution",
                lab_id,
            )
            .await?;
        }
    }

    sqlx::query(
        "INSERT INTO platform_revenues
            (source, related_enterprise_id, amount_credits, fee_rate_bps, notes)
         VALUES ('living_lab_subscription', $1, $2, 10000, $3)",
    )
    .bind(lab.enterprise_id)
    .bind(&lab.monthly_fee)
    .bind(format!("living lab {} — {month}", lab.product_name))
    .execute(db)
    .await?;

    Ok((payable.len() as i64, each))
}

// ═══════════════════════════════════════════════════════════════════
// Team proposals
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Proposal {
    pub id: Uuid,
    pub slug: String,
    pub initiator_user_id: Uuid,
    pub studio_id: Option<Uuid>,
    pub title: String,
    pub problem_md: String,
    pub approach_md: String,
    pub evidence: serde_json::Value,
    pub budget_estimate: Option<BigDecimal>,
    pub currency: String,
    pub target_industries: Vec<String>,
    pub target_enterprise_ids: Vec<Uuid>,
    pub facilitation_percent: BigDecimal,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

const PROPOSAL_SELECT: &str = r#"
    SELECT id, slug, initiator_user_id, studio_id, title, problem_md, approach_md,
           evidence, budget_estimate, currency, target_industries,
           target_enterprise_ids, facilitation_percent, status, created_at
      FROM team_proposals
"#;

#[derive(Debug, Clone, Deserialize)]
pub struct ProposalInput {
    pub slug: String,
    pub title: String,
    pub problem_md: String,
    pub approach_md: String,
    #[serde(default)]
    pub studio_id: Option<Uuid>,
    #[serde(default)]
    pub evidence: Option<serde_json::Value>,
    #[serde(default)]
    pub budget_estimate: Option<BigDecimal>,
    #[serde(default = "eur")]
    pub currency: String,
    #[serde(default)]
    pub target_industries: Vec<String>,
    #[serde(default)]
    pub target_enterprise_ids: Vec<Uuid>,
    #[serde(default)]
    pub available_from: Option<chrono::NaiveDate>,
    #[serde(default)]
    pub available_until: Option<chrono::NaiveDate>,
}

pub async fn draft_proposal(
    db: &PgPool,
    initiator: Uuid,
    input: ProposalInput,
) -> Result<Proposal, AppError> {
    if input.problem_md.trim().len() < 100 {
        return Err(AppError::Validation(
            "describe the problem first, and properly. A proposal that opens with the \
             solution is a team describing what it wants to build."
                .into(),
        ));
    }
    if input.approach_md.trim().len() < 100 {
        return Err(AppError::Validation(
            "describe what you would actually do".into(),
        ));
    }

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO team_proposals
            (slug, initiator_user_id, studio_id, title, problem_md, approach_md,
             evidence, budget_estimate, currency, target_industries,
             target_enterprise_ids, facilitation_percent, available_from,
             available_until)
         VALUES ($1,$2,$3,$4,$5,$6,COALESCE($7,'[]'::jsonb),$8,$9,$10,$11,$12,$13,$14)
         RETURNING id",
    )
    .bind(input.slug.trim())
    .bind(initiator)
    .bind(input.studio_id)
    .bind(input.title.trim())
    .bind(input.problem_md.trim())
    .bind(input.approach_md.trim())
    .bind(input.evidence.as_ref())
    .bind(input.budget_estimate.as_ref())
    .bind(&input.currency)
    .bind(&input.target_industries)
    .bind(&input.target_enterprise_ids)
    .bind(BigDecimal::try_from(FACILITATION_PERCENT).unwrap_or_default())
    .bind(input.available_from)
    .bind(input.available_until)
    .fetch_one(db)
    .await
    .map_err(|e| {
        if e.to_string().contains("team_proposals_slug_key") {
            AppError::Validation("that slug is taken".into())
        } else {
            AppError::from(e)
        }
    })?;

    // The person who wrote it is on it, and has evidently agreed.
    sqlx::query(
        "INSERT INTO team_proposal_members
            (proposal_id, user_id, role_on_proposal, accepted_at)
         VALUES ($1, $2, 'Initiateur', NOW())",
    )
    .bind(id)
    .bind(initiator)
    .execute(db)
    .await?;

    proposal(db, id).await
}

pub async fn proposal(db: &PgPool, id: Uuid) -> Result<Proposal, AppError> {
    let sql = format!("{PROPOSAL_SELECT} WHERE id = $1");
    sqlx::query_as::<_, Proposal>(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NotFound("proposal not found".into()))
}

/// What a company can see: the public ones, plus the ones aimed at them.
pub async fn visible_proposals(
    db: &PgPool,
    enterprise_id: Option<Uuid>,
) -> Result<Vec<Proposal>, AppError> {
    let sql = format!(
        "{PROPOSAL_SELECT} WHERE status IN ('published', 'in_discussion')
            AND (cardinality(target_enterprise_ids) = 0
                 OR $1::UUID = ANY(target_enterprise_ids))
          ORDER BY created_at DESC LIMIT 100"
    );
    let rows = sqlx::query_as::<_, Proposal>(sqlx::AssertSqlSafe(sql))
        .bind(enterprise_id)
        .fetch_all(db)
        .await?;
    Ok(rows)
}

pub async fn add_proposal_member(
    db: &PgPool,
    proposal_id: Uuid,
    user_id: Uuid,
    role: &str,
) -> Result<(), AppError> {
    if role.trim().is_empty() {
        return Err(AppError::Validation(
            "say what this person would do on it".into(),
        ));
    }
    sqlx::query(
        "INSERT INTO team_proposal_members (proposal_id, user_id, role_on_proposal)
         VALUES ($1,$2,$3)
         ON CONFLICT (proposal_id, user_id) DO UPDATE
             SET role_on_proposal = EXCLUDED.role_on_proposal,
                 accepted_at = NULL, declined_at = NULL",
    )
    .bind(proposal_id)
    .bind(user_id)
    .bind(role.trim())
    .execute(db)
    .await?;
    Ok(())
}

pub async fn respond_to_proposal(
    db: &PgPool,
    proposal_id: Uuid,
    user_id: Uuid,
    accept: bool,
) -> Result<(), AppError> {
    let done = sqlx::query(
        "UPDATE team_proposal_members
            SET accepted_at = CASE WHEN $3 THEN NOW() END,
                declined_at = CASE WHEN $3 THEN NULL ELSE NOW() END
          WHERE proposal_id = $1 AND user_id = $2",
    )
    .bind(proposal_id)
    .bind(user_id)
    .bind(accept)
    .execute(db)
    .await?;

    if done.rows_affected() == 0 {
        return Err(AppError::NotFound("you are not on this proposal".into()));
    }
    Ok(())
}

/// Publish it, once everybody named has agreed.
pub async fn publish_proposal(db: &PgPool, id: Uuid) -> Result<Proposal, AppError> {
    let unanswered: Vec<String> = sqlx::query_scalar(
        "SELECT u.username FROM team_proposal_members m
           JOIN users u ON u.id = m.user_id
          WHERE m.proposal_id = $1 AND m.accepted_at IS NULL",
    )
    .bind(id)
    .fetch_all(db)
    .await?;

    if !unanswered.is_empty() {
        return Err(AppError::Validation(format!(
            "not everybody named has agreed: {}. A proposal listing people who did not \
             is a team assembled on paper, and the client finds out at the kickoff.",
            unanswered.join(", ")
        )));
    }

    sqlx::query(
        "UPDATE team_proposals SET status = 'published' WHERE id = $1 AND status = 'draft'",
    )
    .bind(id)
    .execute(db)
    .await?;
    proposal(db, id).await
}

/// A company says it has the problem.
pub async fn express_interest(
    db: &PgPool,
    proposal_id: Uuid,
    enterprise_id: Uuid,
    note_md: Option<&str>,
) -> Result<(), AppError> {
    let proposal = proposal(db, proposal_id).await?;
    if !matches!(proposal.status.as_str(), "published" | "in_discussion") {
        return Err(AppError::Validation("this proposal is not open".into()));
    }
    if !proposal.target_enterprise_ids.is_empty()
        && !proposal.target_enterprise_ids.contains(&enterprise_id)
    {
        return Err(AppError::NotFound("proposal not found".into()));
    }

    sqlx::query(
        "INSERT INTO proposal_enterprise_interests (proposal_id, enterprise_id, note_md)
         VALUES ($1,$2,$3)
         ON CONFLICT (proposal_id, enterprise_id) DO UPDATE SET note_md = EXCLUDED.note_md",
    )
    .bind(proposal_id)
    .bind(enterprise_id)
    .bind(note_md.map(str::trim).filter(|n| !n.is_empty()))
    .execute(db)
    .await?;

    sqlx::query(
        "UPDATE team_proposals SET status = 'in_discussion'
          WHERE id = $1 AND status = 'published'",
    )
    .bind(proposal_id)
    .execute(db)
    .await?;

    Ok(())
}

/// A contract came out of it. Book what Skilluv facilitated.
pub async fn record_signature(
    db: &PgPool,
    proposal_id: Uuid,
    enterprise_id: Uuid,
    contract_value: BigDecimal,
) -> Result<BigDecimal, AppError> {
    let proposal = proposal(db, proposal_id).await?;
    if !contract_value.is_positive() {
        return Err(AppError::Validation(
            "the contract has to have a value".into(),
        ));
    }

    let fee = (&contract_value * &proposal.facilitation_percent / BigDecimal::from(100))
        .with_scale_round(2, bigdecimal::RoundingMode::HalfUp);

    let mut tx = db.begin().await?;
    let done = sqlx::query(
        "UPDATE proposal_enterprise_interests
            SET signed_at = NOW(), contract_value = $3, facilitation_fee = $4
          WHERE proposal_id = $1 AND enterprise_id = $2",
    )
    .bind(proposal_id)
    .bind(enterprise_id)
    .bind(&contract_value)
    .bind(&fee)
    .execute(&mut *tx)
    .await?;

    if done.rows_affected() == 0 {
        return Err(AppError::NotFound(
            "that company never expressed interest in this proposal".into(),
        ));
    }

    sqlx::query("UPDATE team_proposals SET status = 'signed' WHERE id = $1")
        .bind(proposal_id)
        .execute(&mut *tx)
        .await?;

    sqlx::query(
        "INSERT INTO platform_revenues
            (source, related_talent_id, related_enterprise_id, amount_credits,
             fee_rate_bps, notes)
         VALUES ('proposal_facilitation', $1, $2, $3, $4, $5)",
    )
    .bind(proposal.initiator_user_id)
    .bind(enterprise_id)
    .bind(&fee)
    .bind(ledger::percent_to_bps(&proposal.facilitation_percent))
    .bind(format!("proposition « {} »", proposal.title))
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(fee)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn dec(s: &str) -> BigDecimal {
        BigDecimal::from_str(s).unwrap()
    }

    #[test]
    fn the_mentor_takes_the_majority_of_an_onboarding_fee() {
        // They do the three months. The rest is design and check-ins, which
        // is real and is not most of it.
        let (mentor, platform) = split_fee(&dec("6000.00"), &dec("60.00"));
        assert_eq!(mentor, dec("3600.00"));
        assert_eq!(platform, dec("2400.00"));
        assert!(mentor > platform);
    }

    #[test]
    fn a_fee_always_adds_back() {
        for fee in ["1.00", "999.99", "6000.00", "7777.77"] {
            let (mentor, platform) = split_fee(&dec(fee), &dec("60.00"));
            assert_eq!(&mentor + &platform, dec(fee));
        }
    }

    #[test]
    fn a_quiet_month_pays_the_people_who_showed_up_more() {
        // The point of a pool rather than a per-item rate.
        let busy = contribution_reward(&dec("1000.00"), 20);
        let quiet = contribution_reward(&dec("1000.00"), 4);
        assert_eq!(busy, dec("50.00"));
        assert_eq!(quiet, dec("250.00"));
        assert!(quiet > busy);
    }

    #[test]
    fn a_month_with_nothing_accepted_pays_nothing() {
        assert_eq!(contribution_reward(&dec("1000.00"), 0), dec("0"));
        assert_eq!(contribution_reward(&dec("1000.00"), -3), dec("0"));
    }

    #[test]
    fn a_reward_never_exceeds_the_pool() {
        for accepted in 1..50 {
            let each = contribution_reward(&dec("1000.00"), accepted);
            assert!(&each * BigDecimal::from(accepted) <= dec("1000.00"));
        }
    }

    #[test]
    fn facilitation_is_the_small_share() {
        // The team found the problem, wrote the approach and convinced the
        // client. Skilluv held the meeting.
        assert!(FACILITATION_PERCENT <= 15.0);
        assert!(MENTOR_SHARE >= 50.0);
    }
}
