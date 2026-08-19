//! The applicant tracker (migration 0530).
//!
//! A company opens a position, candidates arrive — from their own inbound or
//! pushed across from a Skilluv shortlist — and move through stages until
//! somebody is hired or told no.
//!
//! ## What this module holds that nothing else here does
//!
//! Personal data about people who never signed up to Skilluv. Everything else
//! on this platform is about somebody with an account who chose to be here;
//! an external candidate did not. So three rules run through the code:
//!
//!   * **the company owns the rows, Skilluv keeps them.** Deleting a
//!     subscription deletes its openings, and deleting an opening deletes its
//!     candidates, by foreign key rather than by a job somebody has to
//!     remember to run;
//!   * **every candidate has an erasure date**, defaulted from the plan and
//!     never absent. An ATS that never forgets becomes a CV database nobody
//!     consented to;
//!   * **a refusal carries a reason.** The same rule the mission
//!     applications hold. This platform sells the tooling; it does not sell
//!     the tooling that makes silence easy.
//!
//! ## Why a Skilluv candidate is a link and not a copy
//!
//! Their proofs are read live from their profile. Copying them into the
//! pipeline would mean a revoked attestation still counting inside somebody's
//! hiring tool, which is the single failure this platform sells against.

use bigdecimal::Zero;
use chrono::{Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::errors::AppError;

pub const SOURCES: &[&str] = &["inbound", "skilluv_shortlist", "sourced", "referral"];

/// The stages an opening starts with.
///
/// A default rather than a fixture: it is what most people would have typed,
/// and every one of them can be renamed, reordered or removed afterwards. An
/// ATS that imposes its own process is one people keep a spreadsheet beside.
pub const DEFAULT_STAGES: &[(&str, bool, bool)] = &[
    ("Candidature reçue", false, false),
    ("Premier échange", false, false),
    ("Entretien technique", false, false),
    ("Entretien final", false, false),
    ("Offre envoyée", false, false),
    ("Recruté", true, false),
    ("Refusé", false, true),
];

#[derive(Debug, Clone, Serialize, sqlx::FromRow, ToSchema)]
pub struct Plan {
    pub slug: String,
    pub label: String,
    pub max_open_positions: Option<i32>,
    pub max_candidates_per_opening: Option<i32>,
    pub included_credits: i32,
    #[schema(value_type = String)]
    pub monthly_fee: bigdecimal::BigDecimal,
    pub currency: String,
    pub retention_days: i16,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Opening {
    pub id: Uuid,
    pub enterprise_id: Uuid,
    pub title: String,
    pub description_md: String,
    pub positions_count: i16,
    pub remote_ok: bool,
    pub location: Option<String>,
    pub status: String,
    pub opened_at: Option<chrono::DateTime<chrono::Utc>>,
    pub closed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct OpeningInput {
    pub title: String,
    #[serde(default)]
    pub description_md: Option<String>,
    #[serde(default)]
    pub orientation_slug: Option<String>,
    #[serde(default = "one")]
    pub positions_count: i16,
    #[serde(default = "yes")]
    pub remote_ok: bool,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    #[schema(value_type = Option<String>)]
    pub salary_min: Option<bigdecimal::BigDecimal>,
    #[serde(default)]
    #[schema(value_type = Option<String>)]
    pub salary_max: Option<bigdecimal::BigDecimal>,
    #[serde(default)]
    pub salary_currency: Option<String>,
}

fn one() -> i16 {
    1
}
fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct CandidateInput {
    /// Set when the candidate is on Skilluv.
    #[serde(default)]
    pub user_id: Option<Uuid>,
    #[serde(default)]
    pub external_name: Option<String>,
    #[serde(default)]
    pub external_email: Option<String>,
    #[serde(default)]
    pub resume_url: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
}

// ═══════════════════════════════════════════════════════════════════
// The subscription
// ═══════════════════════════════════════════════════════════════════

/// The plan a company is on, and what it allows.
///
/// Absent means no tracker: the free tier is a row like any other, claimed
/// rather than assumed, so "has an ATS" is one question with one answer
/// instead of a default hidden in a handler.
pub async fn plan_for(db: &PgPool, enterprise_id: Uuid) -> Result<Option<Plan>, AppError> {
    sqlx::query_as::<_, Plan>(
        "SELECT p.slug, p.label, p.max_open_positions, p.max_candidates_per_opening,
                p.included_credits, p.monthly_fee, p.currency, p.retention_days
           FROM ats_subscriptions s
           JOIN ats_plans p ON p.slug = s.plan
          WHERE s.enterprise_id = $1 AND s.status = 'active'",
    )
    .bind(enterprise_id)
    .fetch_optional(db)
    .await
    .map_err(AppError::from)
}

/// What choosing a plan produced.
#[derive(Debug, Clone, Serialize)]
pub struct Chosen {
    pub subscription_id: Uuid,
    pub plan: String,
    /// True when the tracker is usable now. False means a paid plan waiting
    /// on its first payment, and the caller has a checkout to start.
    pub active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub monthly_fee: Option<String>,
}

/// Choose a plan.
///
/// The free tier is active the moment it is chosen: there is nothing to pay,
/// and making a company wait for a payment of zero would be theatre. A paid
/// plan lands `pending` and is activated by `fulfilment` when the money
/// arrives — an upgrade takes effect when it is paid for and not a moment
/// earlier, which is the only version a company can dispute.
///
/// Idempotent on the enterprise: one tracker per company, because two would
/// mean two pipelines for one hiring process.
pub async fn choose_plan(
    db: &PgPool,
    enterprise_id: Uuid,
    plan: &str,
    product_id: Option<Uuid>,
) -> Result<Chosen, AppError> {
    let fee: Option<bigdecimal::BigDecimal> =
        sqlx::query_scalar("SELECT monthly_fee FROM ats_plans WHERE slug = $1 AND is_active")
            .bind(plan)
            .fetch_optional(db)
            .await?;

    let fee = fee.ok_or_else(|| AppError::Validation(format!("'{plan}' is not a plan")))?;
    // A plan at zero is free whatever the scale of the stored decimal:
    // `0.00` and `0` are the same price.
    let free = fee.is_zero();

    // A company already on a paid plan keeps it while a change is pending:
    // downgrading them at the click, before the new plan is paid for, would
    // take away what they are still paying for.
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO ats_subscriptions (enterprise_id, plan, product_id, status)
         VALUES ($1, $2, $3, CASE WHEN $4 THEN 'active' ELSE 'pending' END)
         ON CONFLICT (enterprise_id) DO UPDATE
             SET plan = CASE WHEN $4 OR ats_subscriptions.status <> 'active'
                             THEN EXCLUDED.plan ELSE ats_subscriptions.plan END,
                 product_id = COALESCE(EXCLUDED.product_id, ats_subscriptions.product_id),
                 status = CASE WHEN $4 THEN 'active' ELSE ats_subscriptions.status END,
                 cancelled_at = CASE WHEN $4 THEN NULL ELSE ats_subscriptions.cancelled_at END
         RETURNING id",
    )
    .bind(enterprise_id)
    .bind(plan)
    .bind(product_id)
    .bind(free)
    .fetch_one(db)
    .await?;

    Ok(Chosen {
        subscription_id: id,
        plan: plan.to_string(),
        active: free,
        monthly_fee: (!free).then(|| fee.to_string()),
    })
}

/// Activate a subscription that has been paid for, and push its renewal out.
///
/// Called from `fulfilment`. The period runs from the payment rather than
/// from the order: a company that pays a week late gets a full month, which
/// is the only reading that does not quietly shorten what they bought.
pub async fn activate(db: &PgPool, subscription_id: Uuid) -> Result<Uuid, AppError> {
    let enterprise: Option<Uuid> = sqlx::query_scalar(
        "UPDATE ats_subscriptions
            SET status = 'active',
                cancelled_at = NULL,
                renews_at = GREATEST(COALESCE(renews_at, NOW()), NOW()) + INTERVAL '30 days'
          WHERE id = $1
          RETURNING enterprise_id",
    )
    .bind(subscription_id)
    .fetch_optional(db)
    .await?;

    enterprise.ok_or_else(|| AppError::NotFound("no such subscription".into()))
}

// ═══════════════════════════════════════════════════════════════════
// Openings
// ═══════════════════════════════════════════════════════════════════

/// Open a position, with the stages a company would have typed anyway.
pub async fn open_position(
    db: &PgPool,
    enterprise_id: Uuid,
    author: Uuid,
    input: OpeningInput,
) -> Result<Opening, AppError> {
    let plan = plan_for(db, enterprise_id)
        .await?
        .ok_or_else(|| AppError::Validation("this company has no applicant tracker".into()))?;

    if input.title.trim().is_empty() {
        return Err(AppError::Validation("an opening needs a title".into()));
    }
    crate::validators::check_max_len(&input.title, "title", 200)?;

    // The ceiling is checked against what is actually open, not against what
    // was ever created: a company that closed five roles has five back.
    if let Some(max) = plan.max_open_positions {
        let open: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM ats_openings
              WHERE enterprise_id = $1 AND status = 'open'",
        )
        .bind(enterprise_id)
        .fetch_one(db)
        .await?;

        if open >= max as i64 {
            return Err(AppError::Validation(format!(
                "the {} plan allows {max} open positions at once. Close one, or \
                 move up a plan.",
                plan.label
            )));
        }
    }

    let orientation_id: Option<Uuid> = match input.orientation_slug.as_deref() {
        Some(slug) => {
            let found: Option<Uuid> =
                sqlx::query_scalar("SELECT id FROM orientations WHERE slug = $1")
                    .bind(slug)
                    .fetch_optional(db)
                    .await?;
            Some(found.ok_or_else(|| {
                AppError::Validation(format!("'{slug}' is not a trade in the catalogue"))
            })?)
        }
        None => None,
    };

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO ats_openings
            (enterprise_id, title, description_md, orientation_id, positions_count,
             remote_ok, location, salary_min, salary_max, salary_currency,
             status, opened_at, created_by)
         VALUES ($1,$2,COALESCE($3,''),$4,$5,$6,$7,$8,$9,$10,'open',NOW(),$11)
         RETURNING id",
    )
    .bind(enterprise_id)
    .bind(input.title.trim())
    .bind(input.description_md.as_deref())
    .bind(orientation_id)
    .bind(input.positions_count)
    .bind(input.remote_ok)
    .bind(input.location.as_deref())
    .bind(input.salary_min.as_ref())
    .bind(input.salary_max.as_ref())
    .bind(input.salary_currency.as_deref())
    .bind(author)
    .fetch_one(db)
    .await?;

    for (position, (name, hired, rejected)) in DEFAULT_STAGES.iter().enumerate() {
        sqlx::query(
            "INSERT INTO ats_stages
                (opening_id, name, position, is_terminal_hired, is_terminal_rejected)
             VALUES ($1,$2,$3,$4,$5)",
        )
        .bind(id)
        .bind(*name)
        .bind(position as i16)
        .bind(*hired)
        .bind(*rejected)
        .execute(db)
        .await?;
    }

    opening(db, id).await
}

pub async fn opening(db: &PgPool, id: Uuid) -> Result<Opening, AppError> {
    sqlx::query_as::<_, Opening>(
        "SELECT id, enterprise_id, title, description_md, positions_count, remote_ok,
                location, status, opened_at, closed_at, created_at
           FROM ats_openings WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| AppError::NotFound("no such opening".into()))
}

pub async fn openings_for(db: &PgPool, enterprise_id: Uuid) -> Result<Vec<Opening>, AppError> {
    sqlx::query_as::<_, Opening>(
        "SELECT id, enterprise_id, title, description_md, positions_count, remote_ok,
                location, status, opened_at, closed_at, created_at
           FROM ats_openings
          WHERE enterprise_id = $1
          ORDER BY created_at DESC",
    )
    .bind(enterprise_id)
    .fetch_all(db)
    .await
    .map_err(AppError::from)
}

/// Close a position.
///
/// The candidates stay, and their erasure dates stay with them. Closing is
/// not a delete: somebody in the pipeline may be told no next week, and
/// deleting the record now would lose the reason they were owed.
pub async fn close_position(
    db: &PgPool,
    id: Uuid,
    enterprise_id: Uuid,
) -> Result<Opening, AppError> {
    let done = sqlx::query(
        "UPDATE ats_openings
            SET status = 'closed', closed_at = NOW()
          WHERE id = $1 AND enterprise_id = $2 AND status = 'open'",
    )
    .bind(id)
    .bind(enterprise_id)
    .execute(db)
    .await?;

    if done.rows_affected() == 0 {
        return Err(AppError::NotFound("no open position of yours there".into()));
    }
    opening(db, id).await
}

// ═══════════════════════════════════════════════════════════════════
// Candidates
// ═══════════════════════════════════════════════════════════════════

/// When a record entered today would be erased, on this plan.
pub fn erase_after(retention_days: i16) -> NaiveDate {
    (Utc::now() + Duration::days(retention_days as i64)).date_naive()
}

/// Add somebody to a pipeline.
pub async fn add_candidate(
    db: &PgPool,
    opening_id: Uuid,
    enterprise_id: Uuid,
    input: CandidateInput,
) -> Result<Uuid, AppError> {
    let plan = plan_for(db, enterprise_id)
        .await?
        .ok_or_else(|| AppError::Validation("this company has no applicant tracker".into()))?;

    let owns: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM ats_openings
                         WHERE id = $1 AND enterprise_id = $2 AND status = 'open')",
    )
    .bind(opening_id)
    .bind(enterprise_id)
    .fetch_one(db)
    .await?;

    if !owns {
        return Err(AppError::NotFound("no open position of yours there".into()));
    }

    if input.user_id.is_none()
        && input
            .external_name
            .as_deref()
            .is_none_or(|n| n.trim().is_empty())
    {
        return Err(AppError::Validation(
            "a candidate is a Skilluv account or a name — a row with neither is \
             somebody nobody can contact"
                .into(),
        ));
    }

    let source = input.source.unwrap_or_else(|| "inbound".into());
    if !SOURCES.contains(&source.as_str()) {
        return Err(AppError::Validation(format!(
            "'{source}' is not a source we record"
        )));
    }

    if let Some(email) = input.external_email.as_deref()
        && !email.contains('@')
    {
        return Err(AppError::Validation("that is not an email address".into()));
    }

    if let Some(max) = plan.max_candidates_per_opening {
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM ats_candidates WHERE opening_id = $1")
                .bind(opening_id)
                .fetch_one(db)
                .await?;
        if count >= max as i64 {
            return Err(AppError::Validation(format!(
                "the {} plan holds {max} candidates per opening",
                plan.label
            )));
        }
    }

    // The first stage, so nobody lands outside the pipeline.
    let first_stage: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM ats_stages WHERE opening_id = $1 ORDER BY position LIMIT 1",
    )
    .bind(opening_id)
    .fetch_optional(db)
    .await?;

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO ats_candidates
            (opening_id, user_id, external_name, external_email, resume_url,
             source, current_stage_id, erase_after)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
         RETURNING id",
    )
    .bind(opening_id)
    .bind(input.user_id)
    .bind(input.external_name.as_deref().map(str::trim))
    .bind(input.external_email.as_deref().map(str::trim))
    .bind(input.resume_url.as_deref())
    .bind(&source)
    .bind(first_stage)
    .bind(erase_after(plan.retention_days))
    .fetch_one(db)
    .await
    .map_err(|e| {
        if crate::services::missions::is_unique_violation(&e) {
            return AppError::Validation("that person is already in this pipeline".into());
        }
        AppError::from(e)
    })?;

    Ok(id)
}

/// Move somebody to another stage.
///
/// A rejecting stage requires a reason, and the check is here rather than in
/// a CHECK constraint because only the service knows what the destination
/// stage means. Somebody who spent an evening on an interview is owed a
/// sentence, and a tracker that lets a company skip it is a tracker that
/// teaches them to.
pub async fn move_candidate(
    db: &PgPool,
    candidate_id: Uuid,
    enterprise_id: Uuid,
    to_stage_id: Uuid,
    reason: Option<&str>,
    actor: Uuid,
) -> Result<(), AppError> {
    let row: Option<(Uuid, Option<Uuid>, bool, bool)> = sqlx::query_as(
        "SELECT c.id, c.current_stage_id, s.is_terminal_hired, s.is_terminal_rejected
           FROM ats_candidates c
           JOIN ats_openings o ON o.id = c.opening_id
           JOIN ats_stages s ON s.id = $3 AND s.opening_id = c.opening_id
          WHERE c.id = $1 AND o.enterprise_id = $2",
    )
    .bind(candidate_id)
    .bind(enterprise_id)
    .bind(to_stage_id)
    .fetch_optional(db)
    .await?;

    let (_, from_stage, hired, rejected) = row.ok_or_else(|| {
        AppError::NotFound("no candidate of yours there, or that stage is not theirs".into())
    })?;

    if rejected {
        let reason = reason.map(str::trim).unwrap_or("");
        if reason.len() < 15 {
            return Err(AppError::Validation(
                "say why, in a sentence they could be told. Somebody who spent an \
                 evening on an interview is owed one, and a tracker that lets you \
                 skip it is a tracker that teaches you to."
                    .into(),
            ));
        }
    }

    let mut tx = db.begin().await?;

    sqlx::query(
        "UPDATE ats_candidates
            SET current_stage_id = $2,
                hired_at = CASE WHEN $3 THEN NOW() ELSE hired_at END,
                rejected_at = CASE WHEN $4 THEN NOW() ELSE rejected_at END
          WHERE id = $1",
    )
    .bind(candidate_id)
    .bind(to_stage_id)
    .bind(hired)
    .bind(rejected)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO ats_candidate_moves
            (candidate_id, from_stage_id, to_stage_id, reason, moved_by)
         VALUES ($1,$2,$3,$4,$5)",
    )
    .bind(candidate_id)
    .bind(from_stage)
    .bind(to_stage_id)
    .bind(reason.map(str::trim))
    .bind(actor)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    metrics::counter!(
        "skilluv_ats_moves_total",
        "outcome" => if hired { "hired" } else if rejected { "rejected" } else { "advanced" }
    )
    .increment(1);

    Ok(())
}

/// What the opening does not say, said where the recruiter is looking.
///
/// Only one entry today: an opening with no salary range. The range is
/// optional, and it stays optional — the opening is private, so forcing a
/// number into a box no candidate can see would be theatre rather than
/// transparency. What is not optional is noticing.
///
/// So the gap is surfaced next to the pipeline, at the moment somebody is
/// deciding who to talk to. It is a note, not a block, and the day an opening
/// can be published the range stops being optional on the published version —
/// at that point it reaches a candidate, and pay opacity is the mechanism by
/// which the people this platform exists for get underpaid.
#[derive(Debug, Clone, Serialize)]
pub struct OpeningGap {
    pub code: &'static str,
    pub note: &'static str,
}

/// The gaps worth naming on one opening.
pub async fn gaps(db: &PgPool, opening_id: Uuid) -> Result<Vec<OpeningGap>, AppError> {
    let has_range: Option<bool> = sqlx::query_scalar(
        "SELECT salary_min IS NOT NULL OR salary_max IS NOT NULL
           FROM ats_openings WHERE id = $1",
    )
    .bind(opening_id)
    .fetch_optional(db)
    .await?;

    let mut gaps = Vec::new();
    if has_range == Some(false) {
        gaps.push(OpeningGap {
            code: "no_salary_range",
            note: "Aucune fourchette de rémunération n'est renseignée. Ce poste                    n'est visible que par vous, donc rien ne l'exige — mais un                    candidat qui négocie sans fourchette négocie en aveugle, et                    c'est ce qui fait qu'on paie moins les gens qui viennent de                    loin.",
        });
    }
    Ok(gaps)
}

/// The pipeline of one opening, stage by stage.
///
/// A Skilluv candidate carries their craft score and their verified count,
/// read live. Nothing is copied: a revoked attestation stops counting here
/// the moment it stops counting anywhere.
pub async fn pipeline(
    db: &PgPool,
    opening_id: Uuid,
    enterprise_id: Uuid,
) -> Result<Vec<serde_json::Value>, AppError> {
    let owns: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM ats_openings WHERE id = $1 AND enterprise_id = $2)",
    )
    .bind(opening_id)
    .bind(enterprise_id)
    .fetch_one(db)
    .await?;

    if !owns {
        return Err(AppError::NotFound("no opening of yours there".into()));
    }

    sqlx::query_scalar(
        r#"
        SELECT jsonb_build_object(
                   'stage', jsonb_build_object(
                       'id', s.id, 'name', s.name, 'position', s.position,
                       'is_terminal_hired', s.is_terminal_hired,
                       'is_terminal_rejected', s.is_terminal_rejected),
                   'candidates', COALESCE((
                       SELECT jsonb_agg(jsonb_build_object(
                           'id', c.id,
                           'username', u.username,
                           'display_name', u.display_name,
                           'external_name', c.external_name,
                           'external_email', c.external_email,
                           'resume_url', c.resume_url,
                           'source', c.source,
                           'craft_score', cs.score,
                           'verified_deliverables', (
                               SELECT count(*) FROM deliverables d
                                WHERE d.user_id = c.user_id
                                  AND d.verification_status = 'verified'
                                  AND d.revoked_at IS NULL),
                           'erase_after', c.erase_after,
                           'created_at', c.created_at)
                           ORDER BY c.created_at)
                         FROM ats_candidates c
                         LEFT JOIN users u ON u.id = c.user_id
                         LEFT JOIN craft_scores cs
                                ON cs.user_id = c.user_id
                               AND cs.skill_domain = 'code'
                        WHERE c.current_stage_id = s.id
                   ), '[]'::jsonb))
          FROM ats_stages s
         WHERE s.opening_id = $1
         ORDER BY s.position
        "#,
    )
    .bind(opening_id)
    .fetch_all(db)
    .await
    .map_err(AppError::from)
}

/// Erase every candidate record past its date.
///
/// Runs daily. This is the half of the retention promise that is not a
/// column: a date nobody acts on is a comment.
pub async fn erase_expired(db: &PgPool) -> Result<u64, AppError> {
    let done = sqlx::query("DELETE FROM ats_candidates WHERE erase_after < CURRENT_DATE")
        .execute(db)
        .await?;

    let erased = done.rows_affected();
    if erased > 0 {
        metrics::counter!("skilluv_ats_candidates_erased_total").increment(erased);
    }
    Ok(erased)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_pipeline_has_exactly_one_of_each_end() {
        let hired = DEFAULT_STAGES.iter().filter(|(_, h, _)| *h).count();
        let rejected = DEFAULT_STAGES.iter().filter(|(_, _, r)| *r).count();
        assert_eq!(hired, 1, "one stage means hired");
        assert_eq!(rejected, 1, "one stage means refused");
        // And no stage is both, which a report would have to resolve by
        // guessing.
        assert!(!DEFAULT_STAGES.iter().any(|(_, h, r)| *h && *r));
    }

    #[test]
    fn the_ends_are_the_last_two_stages() {
        // Not a cosmetic point: a terminal stage in the middle would put
        // somebody past the end of the pipeline while still in it.
        let terminal_positions: Vec<usize> = DEFAULT_STAGES
            .iter()
            .enumerate()
            .filter(|(_, (_, h, r))| *h || *r)
            .map(|(i, _)| i)
            .collect();
        assert_eq!(terminal_positions, vec![5, 6]);
    }

    #[test]
    fn retention_is_a_date_in_the_future() {
        let today = Utc::now().date_naive();
        assert!(erase_after(180) > today);
        // And a shorter plan really is shorter, which is the whole reason
        // the number sits on the plan rather than in the code.
        assert!(erase_after(180) < erase_after(365));
    }
}
