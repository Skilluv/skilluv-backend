//! Corporate ambassadors — people who speak for a company under their own
//! name.
//!
//! The thing being sold is somebody's credibility, which is why two rules
//! here are stricter than they are anywhere else on the platform.
//!
//! **A rank floor.** An ambassadorship is worth something to a company
//! because the person's name is worth something to the community. Below the
//! floor the name means nothing yet, and the arrangement is just paid
//! posting.
//!
//! **Their own answer, always.** Nobody is entered into an ambassadorship on
//! their behalf — not by an admin, not by the company. Lending a name is not
//! something a third party can consent to.
//!
//! ## The stipend
//!
//! Monthly, and pro-rated by what was actually delivered. Paying in full
//! regardless would make the deliverable count decorative; paying nothing for
//! a short month would make the arrangement a piece-rate contract with extra
//! steps. The arithmetic is in [`stipend_for`] and tested, because it is the
//! part somebody will want to change and the part that must not drift.

use bigdecimal::BigDecimal;
use bigdecimal::num_traits::Signed;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::ledger;

/// The ranks that can be asked, weakest first. Mirrors `user_ranks.rank`.
pub const RANKS: &[&str] = &["apprenti", "ranger", "artisan", "maitre", "doyen"];

/// Whether a rank clears a program's floor.
pub fn rank_clears(rank: &str, floor: &str) -> bool {
    match (
        RANKS.iter().position(|r| *r == rank),
        RANKS.iter().position(|r| *r == floor),
    ) {
        (Some(has), Some(needs)) => has >= needs,
        // An unknown rank on either side is not a pass. Guessing here would
        // let a typo in a program's floor admit everybody.
        _ => false,
    }
}

/// What a month is worth, given what was delivered.
///
/// Full when the expected number was met or exceeded — over-delivering does
/// not earn more, because the stipend is for being an ambassador and not for
/// piece work. Below that, pro-rated: two of three pieces is two thirds.
///
/// Nothing delivered pays nothing. A stipend for a month with no work is how
/// a programme quietly becomes a subscription the company cannot cancel.
pub fn stipend_for(monthly: &BigDecimal, expected: i32, delivered: i64) -> BigDecimal {
    if expected <= 0 || delivered <= 0 {
        return BigDecimal::from(0);
    }
    if delivered >= expected as i64 {
        return monthly.clone();
    }
    (monthly * BigDecimal::from(delivered) / BigDecimal::from(expected))
        .with_scale_round(2, bigdecimal::RoundingMode::HalfUp)
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Program {
    pub id: Uuid,
    pub enterprise_id: Uuid,
    pub name: String,
    pub brief_md: String,
    pub target_count: i16,
    pub monthly_stipend: BigDecimal,
    pub expected_deliverables_per_month: i16,
    pub duration_months: i16,
    pub swag_included: bool,
    pub preview_products_access: bool,
    pub activation_fee: BigDecimal,
    pub management_monthly_fee: BigDecimal,
    pub currency: String,
    pub minimum_rank: String,
    pub status: String,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

const PROGRAM_SELECT: &str = r#"
    SELECT id, enterprise_id, name, brief_md, target_count, monthly_stipend,
           expected_deliverables_per_month, duration_months, swag_included,
           preview_products_access, activation_fee, management_monthly_fee,
           currency, minimum_rank, status, started_at, created_at
      FROM ambassador_programs
"#;

#[derive(Debug, Clone, Deserialize)]
pub struct ProgramInput {
    pub name: String,
    pub brief_md: String,
    pub target_count: i16,
    pub monthly_stipend: BigDecimal,
    #[serde(default)]
    pub expected_deliverables_per_month: Option<i16>,
    pub duration_months: i16,
    pub activation_fee: BigDecimal,
    #[serde(default)]
    pub management_monthly_fee: Option<BigDecimal>,
    #[serde(default = "eur")]
    pub currency: String,
    #[serde(default)]
    pub minimum_rank: Option<String>,
    #[serde(default)]
    pub swag_included: Option<bool>,
    #[serde(default)]
    pub preview_products_access: Option<bool>,
}

fn eur() -> String {
    "EUR".into()
}

pub async fn open(
    db: &PgPool,
    enterprise_id: Uuid,
    author: Uuid,
    input: ProgramInput,
) -> Result<Program, AppError> {
    if input.brief_md.trim().is_empty() {
        return Err(AppError::Validation(
            "say what an ambassador is being asked to do. Somebody is lending their name \
             to it, and they are owed a description of what for."
                .into(),
        ));
    }
    crate::validators::check_max_len(&input.name, "name", 200)?;
    crate::validators::check_max_len(&input.brief_md, "brief_md", 20_000)?;

    let floor = input.minimum_rank.as_deref().unwrap_or("artisan");
    if !RANKS.contains(&floor) {
        return Err(AppError::Validation(format!(
            "minimum_rank must be one of: {}",
            RANKS.join(", ")
        )));
    }
    if floor == "apprenti" {
        return Err(AppError::Validation(
            "an apprentice's name does not yet carry what a company would be buying. The \
             floor for an ambassadorship is ranger."
                .into(),
        ));
    }

    if !input.monthly_stipend.is_positive() {
        return Err(AppError::Validation(
            "ambassadors are paid. An unpaid one is an enthusiast, and a company does \
             not need us to find those."
                .into(),
        ));
    }

    let id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO ambassador_programs
            (enterprise_id, name, brief_md, target_count, monthly_stipend,
             expected_deliverables_per_month, duration_months, swag_included,
             preview_products_access, activation_fee, management_monthly_fee,
             currency, minimum_rank, created_by)
        VALUES ($1,$2,$3,$4,$5,COALESCE($6,1),$7,COALESCE($8,TRUE),
                COALESCE($9,TRUE),$10,COALESCE($11,0),$12,$13,$14)
        RETURNING id
        "#,
    )
    .bind(enterprise_id)
    .bind(input.name.trim())
    .bind(input.brief_md.trim())
    .bind(input.target_count)
    .bind(&input.monthly_stipend)
    .bind(input.expected_deliverables_per_month)
    .bind(input.duration_months)
    .bind(input.swag_included)
    .bind(input.preview_products_access)
    .bind(&input.activation_fee)
    .bind(input.management_monthly_fee.as_ref())
    .bind(&input.currency)
    .bind(floor)
    .bind(author)
    .fetch_one(db)
    .await?;

    by_id(db, id).await
}

pub async fn by_id(db: &PgPool, id: Uuid) -> Result<Program, AppError> {
    let sql = format!("{PROGRAM_SELECT} WHERE id = $1");
    sqlx::query_as::<_, Program>(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NotFound("programme not found".into()))
}

pub async fn recruiting(db: &PgPool) -> Result<Vec<Program>, AppError> {
    let sql = format!("{PROGRAM_SELECT} WHERE status = 'recruiting' ORDER BY created_at DESC");
    let rows = sqlx::query_as::<_, Program>(sqlx::AssertSqlSafe(sql))
        .fetch_all(db)
        .await?;
    Ok(rows)
}

pub async fn for_enterprise(db: &PgPool, enterprise_id: Uuid) -> Result<Vec<Program>, AppError> {
    let sql = format!("{PROGRAM_SELECT} WHERE enterprise_id = $1 ORDER BY created_at DESC");
    let rows = sqlx::query_as::<_, Program>(sqlx::AssertSqlSafe(sql))
        .bind(enterprise_id)
        .fetch_all(db)
        .await?;
    Ok(rows)
}

// ═══════════════════════════════════════════════════════════════════
// The people
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Ambassador {
    pub user_id: Uuid,
    pub username: String,
    pub status: String,
    pub invited_at: chrono::DateTime<chrono::Utc>,
    pub accepted_at: Option<chrono::DateTime<chrono::Utc>>,
    pub onboarded_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub async fn ambassadors(db: &PgPool, program_id: Uuid) -> Result<Vec<Ambassador>, AppError> {
    let rows = sqlx::query_as::<_, Ambassador>(
        "SELECT a.user_id, u.username, a.status, a.invited_at, a.accepted_at,
                a.onboarded_at
           FROM program_ambassadors a
           JOIN users u ON u.id = a.user_id
          WHERE a.program_id = $1
          ORDER BY a.invited_at",
    )
    .bind(program_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// Invite somebody. Checks the rank, and stops there — the answer is theirs.
pub async fn invite(db: &PgPool, program_id: Uuid, user_id: Uuid) -> Result<(), AppError> {
    let program = by_id(db, program_id).await?;
    if program.status != "recruiting" {
        return Err(AppError::Validation(format!(
            "this programme is {} and is not recruiting",
            program.status
        )));
    }

    let taken: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM program_ambassadors
          WHERE program_id = $1 AND status IN ('invited', 'active', 'paused')",
    )
    .bind(program_id)
    .fetch_one(db)
    .await?;
    if taken >= program.target_count as i64 {
        return Err(AppError::Validation(format!(
            "this programme asked for {} ambassadors and has them",
            program.target_count
        )));
    }

    let rank: Option<String> = sqlx::query_scalar("SELECT rank FROM user_ranks WHERE user_id = $1")
        .bind(user_id)
        .fetch_optional(db)
        .await?;
    let rank = rank.unwrap_or_else(|| "apprenti".into());

    if !rank_clears(&rank, &program.minimum_rank) {
        return Err(AppError::Validation(format!(
            "this programme asks for {} and this person is {rank}. The company is buying \
             a name that means something to the community; below the floor it does not \
             yet.",
            program.minimum_rank
        )));
    }

    sqlx::query(
        "INSERT INTO program_ambassadors (program_id, user_id) VALUES ($1, $2)
         ON CONFLICT (program_id, user_id) DO NOTHING",
    )
    .bind(program_id)
    .bind(user_id)
    .execute(db)
    .await?;
    Ok(())
}

/// Their own answer.
pub async fn respond(
    db: &PgPool,
    program_id: Uuid,
    user_id: Uuid,
    accept: bool,
) -> Result<(), AppError> {
    let done = sqlx::query(
        "UPDATE program_ambassadors
            SET accepted_at = CASE WHEN $3 THEN NOW() END,
                declined_at = CASE WHEN $3 THEN NULL ELSE NOW() END,
                onboarded_at = CASE WHEN $3 THEN NOW() END,
                status = CASE WHEN $3 THEN 'active' ELSE 'left' END,
                left_reason = CASE WHEN $3 THEN NULL ELSE 'a décliné l''invitation' END,
                left_at = CASE WHEN $3 THEN NULL ELSE NOW() END
          WHERE program_id = $1 AND user_id = $2 AND status = 'invited'",
    )
    .bind(program_id)
    .bind(user_id)
    .bind(accept)
    .execute(db)
    .await?;

    if done.rows_affected() == 0 {
        return Err(AppError::NotFound(
            "you have no open invitation to this programme".into(),
        ));
    }
    Ok(())
}

/// Somebody stops.
pub async fn leave(
    db: &PgPool,
    program_id: Uuid,
    user_id: Uuid,
    reason: &str,
) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE program_ambassadors
            SET status = 'left', left_at = NOW(), left_reason = $3
          WHERE program_id = $1 AND user_id = $2 AND status <> 'left'",
    )
    .bind(program_id)
    .bind(user_id)
    .bind(reason.trim())
    .execute(db)
    .await?;
    Ok(())
}

/// Start the programme, and book the activation fee.
pub async fn activate(db: &PgPool, program_id: Uuid) -> Result<BigDecimal, AppError> {
    let program = by_id(db, program_id).await?;
    if program.status != "recruiting" {
        return Err(AppError::Validation(format!(
            "this programme is already {}",
            program.status
        )));
    }

    let active: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM program_ambassadors
          WHERE program_id = $1 AND status = 'active'",
    )
    .bind(program_id)
    .fetch_one(db)
    .await?;
    if active == 0 {
        return Err(AppError::Validation(
            "nobody has accepted yet. A programme activated with no ambassadors bills \
             the company for a cohort that does not exist."
                .into(),
        ));
    }

    let ends = chrono::Utc::now() + chrono::Duration::days(30 * program.duration_months as i64);

    let mut tx = db.begin().await?;
    sqlx::query(
        "UPDATE ambassador_programs
            SET status = 'running', started_at = NOW(), ends_at = $2 WHERE id = $1",
    )
    .bind(program_id)
    .bind(ends)
    .execute(&mut *tx)
    .await?;

    let product_id: Uuid = sqlx::query_scalar(
        "INSERT INTO enterprise_products
            (enterprise_id, product_type, source_table, source_id,
             contract_value, currency)
         VALUES ($1, 'corporate_ambassador', 'ambassador_programs', $2, $3, $4)
         RETURNING id",
    )
    .bind(program.enterprise_id)
    .bind(program_id)
    .bind(&program.activation_fee)
    .bind(&program.currency)
    .fetch_one(&mut *tx)
    .await?;
    let _ = product_id;

    if program.activation_fee.is_positive() {
        sqlx::query(
            "INSERT INTO platform_revenues
                (source, related_enterprise_id, amount_credits, fee_rate_bps, notes)
             VALUES ('ambassador_program_fee', $1, $2, 10000, $3)",
        )
        .bind(program.enterprise_id)
        .bind(&program.activation_fee)
        .bind(format!("activation du programme {}", program.name))
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(program.activation_fee)
}

// ═══════════════════════════════════════════════════════════════════
// Deliverables and the monthly stipend
// ═══════════════════════════════════════════════════════════════════

/// The first day of the month a date belongs to.
fn month_of(when: chrono::DateTime<chrono::Utc>) -> chrono::NaiveDate {
    use chrono::Datelike;
    chrono::NaiveDate::from_ymd_opt(when.year(), when.month(), 1).unwrap_or_default()
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeliverableInput {
    pub kind: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    /// Which month it counts for. Defaults to this one.
    #[serde(default)]
    pub counts_for_month: Option<chrono::NaiveDate>,
}

pub async fn record_deliverable(
    db: &PgPool,
    program_id: Uuid,
    user_id: Uuid,
    input: DeliverableInput,
) -> Result<Uuid, AppError> {
    if input.kind.trim().is_empty() {
        return Err(AppError::Validation("say what was delivered".into()));
    }
    if let Some(url) = &input.url
        && !url.starts_with("https://")
    {
        return Err(AppError::Validation("the link has to be https".into()));
    }

    let month = input
        .counts_for_month
        .map(|d| {
            chrono::NaiveDate::from_ymd_opt(
                chrono::Datelike::year(&d),
                chrono::Datelike::month(&d),
                1,
            )
            .unwrap_or(d)
        })
        .unwrap_or_else(|| month_of(chrono::Utc::now()));

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO ambassador_deliverables
            (program_id, user_id, counts_for_month, kind, url, note)
         VALUES ($1,$2,$3,$4,$5,$6)
         RETURNING id",
    )
    .bind(program_id)
    .bind(user_id)
    .bind(month)
    .bind(input.kind.trim())
    .bind(input.url.as_deref())
    .bind(input.note.as_deref())
    .fetch_one(db)
    .await
    .map_err(|e| {
        if e.to_string()
            .contains("ambassador_deliverables_program_id_user_id_fkey")
        {
            AppError::Validation("you are not an ambassador on this programme".into())
        } else {
            AppError::from(e)
        }
    })?;

    Ok(id)
}

/// Pay one month, once.
///
/// The uniqueness is in the database rather than in a check here: a retry
/// that paid twice would be found by an accountant months later, if at all.
pub async fn pay_month(
    db: &PgPool,
    program_id: Uuid,
    user_id: Uuid,
    month: chrono::NaiveDate,
) -> Result<BigDecimal, AppError> {
    let program = by_id(db, program_id).await?;

    let delivered: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM ambassador_deliverables
          WHERE program_id = $1 AND user_id = $2 AND counts_for_month = $3
            AND accepted",
    )
    .bind(program_id)
    .bind(user_id)
    .bind(month)
    .fetch_one(db)
    .await?;

    let amount = stipend_for(
        &program.monthly_stipend,
        program.expected_deliverables_per_month as i32,
        delivered,
    );

    if !amount.is_positive() {
        return Err(AppError::Validation(
            "nothing was delivered that month. A stipend paid regardless is how a \
             programme quietly becomes a subscription the company cannot cancel."
                .into(),
        ));
    }

    sqlx::query(
        "INSERT INTO ambassador_stipends
            (program_id, user_id, counts_for_month, amount, currency,
             deliverables_counted)
         VALUES ($1,$2,$3,$4,$5,$6)",
    )
    .bind(program_id)
    .bind(user_id)
    .bind(month)
    .bind(&amount)
    .bind(&program.currency)
    .bind(delivered as i16)
    .execute(db)
    .await
    .map_err(|e| {
        if e.to_string()
            .contains("ambassador_stipends_program_id_user_id_counts_for_month_key")
        {
            AppError::Validation("that month has already been paid".into())
        } else {
            AppError::from(e)
        }
    })?;

    let currency: ledger::Currency = program.currency.parse()?;
    ledger::capture_for_recipient(
        db,
        "stripe",
        format!("stipend:{program_id}:{user_id}:{month}"),
        user_id,
        amount.clone(),
        BigDecimal::from(0),
        currency,
        "ambassador_stipend",
        program_id,
    )
    .await?;

    // Skilluv's monthly management fee, booked in the same month it managed.
    if program.management_monthly_fee.is_positive() {
        let already: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM ambassador_stipends
              WHERE program_id = $1 AND counts_for_month = $2",
        )
        .bind(program_id)
        .bind(month)
        .fetch_one(db)
        .await?;

        // Charged once per month, on the first stipend paid for it — not once
        // per ambassador, which would multiply the fee by the cohort size.
        if already <= 1 {
            sqlx::query(
                "INSERT INTO platform_revenues
                    (source, related_enterprise_id, amount_credits, fee_rate_bps, notes)
                 VALUES ('ambassador_program_fee', $1, $2, 10000, $3)",
            )
            .bind(program.enterprise_id)
            .bind(&program.management_monthly_fee)
            .bind(format!("gestion {} — {month}", program.name))
            .execute(db)
            .await?;
        }
    }

    Ok(amount)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn dec(s: &str) -> BigDecimal {
        BigDecimal::from_str(s).unwrap()
    }

    #[test]
    fn a_rank_at_the_floor_clears_it() {
        assert!(rank_clears("artisan", "artisan"));
        assert!(rank_clears("doyen", "artisan"));
        assert!(!rank_clears("ranger", "artisan"));
        assert!(!rank_clears("apprenti", "ranger"));
    }

    #[test]
    fn a_typo_in_a_rank_admits_nobody() {
        // Guessing here would let a mistyped floor admit everybody, which is
        // the opposite of what a floor is for.
        assert!(!rank_clears("compagnon", "artisan"));
        assert!(!rank_clears("artisan", "compagnon"));
    }

    #[test]
    fn meeting_the_target_pays_the_whole_stipend() {
        assert_eq!(stipend_for(&dec("300.00"), 3, 3), dec("300.00"));
    }

    #[test]
    fn over_delivering_does_not_pay_more() {
        // The stipend is for being an ambassador, not for piece work.
        assert_eq!(stipend_for(&dec("300.00"), 3, 30), dec("300.00"));
    }

    #[test]
    fn a_short_month_is_pro_rated() {
        assert_eq!(stipend_for(&dec("300.00"), 3, 2), dec("200.00"));
        assert_eq!(stipend_for(&dec("300.00"), 3, 1), dec("100.00"));
        assert_eq!(stipend_for(&dec("100.00"), 3, 1), dec("33.33"));
    }

    #[test]
    fn nothing_delivered_pays_nothing() {
        // A stipend for a month with no work is how a programme quietly
        // becomes a subscription the company cannot cancel.
        assert_eq!(stipend_for(&dec("300.00"), 3, 0), dec("0"));
        assert_eq!(stipend_for(&dec("300.00"), 0, 5), dec("0"));
    }

    #[test]
    fn a_stipend_never_exceeds_the_month() {
        for delivered in 0..10 {
            let paid = stipend_for(&dec("250.00"), 4, delivered);
            assert!(paid <= dec("250.00"), "{delivered} pieces overpaid");
            assert!(!paid.is_negative());
        }
    }
}
