//! Paid beta testing — a hundred people paid a small fixed reward for an
//! opinion.
//!
//! Not a team, and deliberately not modelled as one. A studio is a small
//! group whose members share a pot; a beta cohort is many people each owed
//! the same fixed amount for their own piece of work. Sharing the engagement
//! machinery would have meant a share-percent column that is always
//! `100 / n` and a milestone table nobody uses.
//!
//! Two amounts, kept apart on purpose: what goes to the testers, and what
//! Skilluv charges for running it — recruiting the right people, structuring
//! the feedback, and writing the report that makes a hundred opinions
//! usable. A client should be able to see which is which.

use bigdecimal::BigDecimal;
use bigdecimal::num_traits::Signed;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::ledger;

pub const TEST_TYPES: &[&str] = &[
    "usability",
    "game_playtest",
    "security",
    "performance",
    "accessibility",
];

/// What the whole programme costs the client: the rewards it will pay out if
/// every tester is accepted, plus the fee for running it.
///
/// Quoted at the maximum rather than the expected, because a client who
/// budgets for the average and is billed for the maximum has been misled by
/// arithmetic.
pub fn quote(
    testers_wanted: i32,
    tester_reward: &BigDecimal,
    program_fee: &BigDecimal,
) -> BigDecimal {
    tester_reward * BigDecimal::from(testers_wanted.max(0)) + program_fee
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct BetaProgram {
    pub id: Uuid,
    pub enterprise_id: Uuid,
    pub product_name: String,
    pub brief_md: String,
    pub test_type: String,
    pub target_domains: Vec<String>,
    pub target_orientations: Vec<String>,
    pub testers_wanted: i16,
    pub duration_weeks: i16,
    pub tester_reward: BigDecimal,
    pub program_fee: BigDecimal,
    pub currency: String,
    pub status: String,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub ends_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

const PROGRAM_SELECT: &str = r#"
    SELECT id, enterprise_id, product_name, brief_md, test_type, target_domains,
           target_orientations, testers_wanted, duration_weeks, tester_reward,
           program_fee, currency, status, started_at, ends_at, created_at
      FROM beta_programs
"#;

#[derive(Debug, Clone, Deserialize)]
pub struct ProgramInput {
    pub product_name: String,
    pub brief_md: String,
    pub test_type: String,
    #[serde(default)]
    pub target_domains: Vec<String>,
    #[serde(default)]
    pub target_orientations: Vec<String>,
    pub testers_wanted: i16,
    pub duration_weeks: i16,
    pub tester_reward: BigDecimal,
    pub program_fee: BigDecimal,
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
    input: ProgramInput,
) -> Result<BetaProgram, AppError> {
    if !TEST_TYPES.contains(&input.test_type.as_str()) {
        return Err(AppError::Validation(format!(
            "test_type must be one of: {}",
            TEST_TYPES.join(", ")
        )));
    }
    if input.brief_md.trim().is_empty() {
        return Err(AppError::Validation(
            "say what the testers are meant to do. A brief that says 'try it and tell us' \
             produces a hundred opinions and no report."
                .into(),
        ));
    }
    crate::validators::check_max_len(&input.product_name, "product_name", 200)?;
    crate::validators::check_max_len(&input.brief_md, "brief_md", 20_000)?;

    if !input.tester_reward.is_positive() {
        return Err(AppError::Validation(
            "testers are paid. An unpaid beta is a favour, and Skilluv does not broker \
             favours as if they were work."
                .into(),
        ));
    }

    let id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO beta_programs
            (enterprise_id, product_name, brief_md, test_type, target_domains,
             target_orientations, testers_wanted, duration_weeks, tester_reward,
             program_fee, currency, created_by)
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
        RETURNING id
        "#,
    )
    .bind(enterprise_id)
    .bind(input.product_name.trim())
    .bind(input.brief_md.trim())
    .bind(&input.test_type)
    .bind(&input.target_domains)
    .bind(&input.target_orientations)
    .bind(input.testers_wanted)
    .bind(input.duration_weeks)
    .bind(&input.tester_reward)
    .bind(&input.program_fee)
    .bind(&input.currency)
    .bind(author)
    .fetch_one(db)
    .await
    .map_err(|e| {
        let m = e.to_string();
        if m.contains("testers_wanted") {
            AppError::Validation(
                "a programme runs with between 5 and 500 testers: fewer is an opinion, \
                 more is a launch"
                    .into(),
            )
        } else if m.contains("duration_weeks") {
            AppError::Validation("a test runs between one and twelve weeks".into())
        } else {
            AppError::from(e)
        }
    })?;

    by_id(db, id).await
}

pub async fn by_id(db: &PgPool, id: Uuid) -> Result<BetaProgram, AppError> {
    let sql = format!("{PROGRAM_SELECT} WHERE id = $1");
    sqlx::query_as::<_, BetaProgram>(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NotFound("programme not found".into()))
}

/// What a tester can join, newest first.
pub async fn recruiting(
    db: &PgPool,
    test_type: Option<&str>,
) -> Result<Vec<BetaProgram>, AppError> {
    let sql = format!(
        "{PROGRAM_SELECT} WHERE status = 'recruiting'
            AND ($1::TEXT IS NULL OR test_type = $1)
          ORDER BY created_at DESC LIMIT 100"
    );
    let rows = sqlx::query_as::<_, BetaProgram>(sqlx::AssertSqlSafe(sql))
        .bind(test_type)
        .fetch_all(db)
        .await?;
    Ok(rows)
}

pub async fn for_enterprise(
    db: &PgPool,
    enterprise_id: Uuid,
) -> Result<Vec<BetaProgram>, AppError> {
    let sql = format!("{PROGRAM_SELECT} WHERE enterprise_id = $1 ORDER BY created_at DESC");
    let rows = sqlx::query_as::<_, BetaProgram>(sqlx::AssertSqlSafe(sql))
        .bind(enterprise_id)
        .fetch_all(db)
        .await?;
    Ok(rows)
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Tester {
    pub user_id: Uuid,
    pub username: String,
    pub status: String,
    pub feedback_md: Option<String>,
    pub submitted_at: Option<chrono::DateTime<chrono::Utc>>,
    pub rejection_reason: Option<String>,
    pub reward_paid_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub async fn testers(db: &PgPool, program_id: Uuid) -> Result<Vec<Tester>, AppError> {
    let rows = sqlx::query_as::<_, Tester>(
        "SELECT t.user_id, u.username, t.status, t.feedback_md, t.submitted_at,
                t.rejection_reason, t.reward_paid_at
           FROM beta_testers t
           JOIN users u ON u.id = t.user_id
          WHERE t.program_id = $1
          ORDER BY t.joined_at",
    )
    .bind(program_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// Somebody signs up.
///
/// The full check is a trigger, not this function: two people taking the last
/// place at the same moment would both pass a check written here.
pub async fn join(db: &PgPool, program_id: Uuid, user_id: Uuid) -> Result<(), AppError> {
    sqlx::query("INSERT INTO beta_testers (program_id, user_id) VALUES ($1, $2)")
        .bind(program_id)
        .bind(user_id)
        .execute(db)
        .await
        .map_err(|e| {
            let m = e.to_string();
            if m.contains("beta_testers_pkey") {
                AppError::Validation("you are already on this programme".into())
            } else if m.contains("already has its") || m.contains("not recruiting") {
                // The trigger's own words: it knows the count and the status,
                // and repeating them here would let the two drift apart.
                AppError::Validation(
                    m.rsplit("ERROR:")
                        .next()
                        .unwrap_or("this programme is full")
                        .trim()
                        .to_string(),
                )
            } else {
                AppError::from(e)
            }
        })?;
    Ok(())
}

pub async fn submit_feedback(
    db: &PgPool,
    program_id: Uuid,
    user_id: Uuid,
    feedback_md: &str,
) -> Result<(), AppError> {
    if feedback_md.trim().len() < 50 {
        return Err(AppError::Validation(
            "fifty characters is the floor. Below it there is nothing for the report to \
             be built from, and the reward would be paid for nothing."
                .into(),
        ));
    }
    crate::validators::check_max_len(feedback_md, "feedback_md", 50_000)?;

    let done = sqlx::query(
        "UPDATE beta_testers
            SET status = 'submitted', feedback_md = $3, submitted_at = NOW()
          WHERE program_id = $1 AND user_id = $2 AND status IN ('joined', 'submitted')",
    )
    .bind(program_id)
    .bind(user_id)
    .bind(feedback_md.trim())
    .execute(db)
    .await?;

    if done.rows_affected() == 0 {
        return Err(AppError::NotFound(
            "you are not on this programme, or your feedback has already been reviewed".into(),
        ));
    }
    Ok(())
}

/// The client judges one tester's feedback, and the reward follows.
///
/// Accepting pays; rejecting carries a reason, because somebody spent hours
/// on what is being refused and "no" without a reason is how a platform
/// loses the testers it took weeks to recruit.
pub async fn review_feedback(
    db: &PgPool,
    program_id: Uuid,
    user_id: Uuid,
    accept: bool,
    reason: Option<&str>,
) -> Result<Option<BigDecimal>, AppError> {
    let program = by_id(db, program_id).await?;

    if !accept {
        let reason = reason
            .map(str::trim)
            .filter(|r| !r.is_empty())
            .ok_or_else(|| {
                AppError::Validation(
                    "say why it was refused. Somebody spent hours on this, and a refusal \
                 without a reason is how a programme loses the testers it took weeks to \
                 recruit."
                        .into(),
                )
            })?;

        let done = sqlx::query(
            "UPDATE beta_testers
                SET status = 'rejected', rejection_reason = $3, reviewed_at = NOW()
              WHERE program_id = $1 AND user_id = $2 AND status = 'submitted'",
        )
        .bind(program_id)
        .bind(user_id)
        .bind(reason)
        .execute(db)
        .await?;

        if done.rows_affected() == 0 {
            return Err(AppError::NotFound(
                "no feedback from that tester is waiting for review".into(),
            ));
        }
        return Ok(None);
    }

    let done = sqlx::query(
        "UPDATE beta_testers
            SET status = 'accepted', reviewed_at = NOW(), reward_paid_at = NOW()
          WHERE program_id = $1 AND user_id = $2 AND status = 'submitted'",
    )
    .bind(program_id)
    .bind(user_id)
    .execute(db)
    .await?;

    if done.rows_affected() == 0 {
        return Err(AppError::NotFound(
            "no feedback from that tester is waiting for review".into(),
        ));
    }

    let currency: ledger::Currency = program.currency.parse()?;
    ledger::capture_for_recipient(
        db,
        "stripe",
        format!("beta:{program_id}:{user_id}"),
        user_id,
        program.tester_reward.clone(),
        // The programme fee is charged once, on the programme; taking a cut of
        // each reward on top would charge the client twice for the same work.
        BigDecimal::from(0),
        currency,
        "beta_program",
        program_id,
    )
    .await?;

    Ok(Some(program.tester_reward))
}

/// The programme closes and Skilluv books what it charged for running it.
///
/// Booked at closing rather than at opening: the fee is earned by delivering
/// the report, and a programme cancelled in its first week has earned none
/// of it.
pub async fn close(db: &PgPool, program_id: Uuid) -> Result<BigDecimal, AppError> {
    let program = by_id(db, program_id).await?;
    if program.status == "closed" {
        return Err(AppError::Validation("already closed".into()));
    }

    let unreviewed: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM beta_testers
          WHERE program_id = $1 AND status = 'submitted'",
    )
    .bind(program_id)
    .fetch_one(db)
    .await?;
    if unreviewed > 0 {
        return Err(AppError::Validation(format!(
            "{unreviewed} tester(s) are still waiting on a verdict. Closing now would \
             leave them unpaid with no way to ask why."
        )));
    }

    let mut tx = db.begin().await?;
    sqlx::query("UPDATE beta_programs SET status = 'closed', closed_at = NOW() WHERE id = $1")
        .bind(program_id)
        .execute(&mut *tx)
        .await?;

    if program.program_fee.is_positive() {
        sqlx::query(
            "INSERT INTO platform_revenues
                (source, related_enterprise_id, amount_credits, fee_rate_bps, notes)
             VALUES ('beta_program_fee', $1, $2, 0, $3)",
        )
        .bind(program.enterprise_id)
        .bind(&program.program_fee)
        .bind(format!(
            "programme de test {} ({} testeurs)",
            program.product_name, program.testers_wanted
        ))
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(program.program_fee)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn dec(s: &str) -> BigDecimal {
        BigDecimal::from_str(s).unwrap()
    }

    #[test]
    fn the_quote_is_the_maximum_not_the_average() {
        // A client who budgets for the average and is billed for the maximum
        // has been misled by arithmetic.
        assert_eq!(quote(100, &dec("25.00"), &dec("2000.00")), dec("4500.00"));
        assert_eq!(quote(5, &dec("10.00"), &dec("0.00")), dec("50.00"));
    }

    #[test]
    fn a_programme_with_no_testers_still_costs_the_fee() {
        assert_eq!(quote(0, &dec("25.00"), &dec("2000.00")), dec("2000.00"));
    }

    #[test]
    fn every_test_type_is_a_known_one() {
        assert_eq!(TEST_TYPES.len(), 5);
        assert!(TEST_TYPES.contains(&"game_playtest"));
        assert!(TEST_TYPES.contains(&"accessibility"));
    }
}
