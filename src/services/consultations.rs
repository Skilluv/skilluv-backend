//! One-off consultation: an hour with an expert, a panel on a document, an
//! audit of a client's team.
//!
//! ## Two products, one shape
//!
//! An advisory call and an architecture review are both a company buying
//! expert judgement on a question it has written down. The difference is how
//! many people answer, and the commission reflects what Skilluv actually did:
//! lower on advisory, which is an introduction and an hour in a calendar;
//! higher on a review, where Skilluv assembles the panel, holds the deadline
//! and writes the synthesis.
//!
//! ## The audit is the careful one
//!
//! A skill audit assesses the client's own employees. The person assessed is
//! not the customer, did not ask for it, and may be managed out of a job on
//! the strength of it. Two rules follow, and both are enforced rather than
//! promised: nothing is written about somebody who has not been told, and
//! nothing is delivered to the client until everybody assessed has seen what
//! was concluded about them.
//!
//! The commercial pressure runs the other way — the client wants the report
//! on their date — which is why the gate is in the database.

use bigdecimal::BigDecimal;
use bigdecimal::num_traits::ToPrimitive;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::ledger;

pub const KINDS: &[&str] = &["advisory", "architecture_review", "implementation"];

/// What a company can be helped to build internally.
pub const IMPLEMENTATION_TYPES: &[&str] = &[
    "compagnonnage_setup",
    "apprenticeship_program_design",
    "tech_talent_strategy",
    "skills_framework_design",
    "proof_of_work_implementation",
];

pub const VERDICTS: &[&str] = &["approve", "approve_with_concerns", "concerns", "reject"];

/// What Skilluv keeps on a consultation.
///
/// Lower on advisory, where the product is an introduction and an hour in a
/// calendar. Higher on a review, where Skilluv assembles the panel, holds the
/// deadline against several people at once and writes the synthesis the
/// client actually bought — the experts' comments are the working.
pub fn commission_for(kind: &str) -> f64 {
    match kind {
        "architecture_review" => 40.0,
        // An implementation is weeks of somebody else's work with Skilluv
        // holding the shape of it. Between the two, closer to the review.
        "implementation" => 35.0,
        _ => 25.0,
    }
}

/// The rank floor for giving advice for money under Skilluv's name.
pub const EXPERT_MIN_RANK: &str = "maitre";

/// What each expert on a review is paid.
///
/// The fee less the platform's share, divided between the people who actually
/// submitted. Somebody who was invited and did not write anything is not in
/// the division — the fee buys the opinion, not the availability — and the
/// last person absorbs the rounding rather than the platform.
pub fn split_between_experts(
    fee: &BigDecimal,
    commission_percent: &BigDecimal,
    submitters: usize,
) -> (BigDecimal, Vec<BigDecimal>) {
    let commission = (fee * commission_percent / BigDecimal::from(100))
        .with_scale_round(2, bigdecimal::RoundingMode::Down);
    let pot = fee - &commission;

    if submitters == 0 {
        return (commission, vec![]);
    }

    let each = (&pot / BigDecimal::from(submitters as i64))
        .with_scale_round(2, bigdecimal::RoundingMode::Down);

    let mut shares = Vec::with_capacity(submitters);
    let mut given = BigDecimal::from(0);
    for _ in 0..submitters - 1 {
        given += &each;
        shares.push(each.clone());
    }
    shares.push(&pot - &given);

    (commission, shares)
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Consultation {
    pub id: Uuid,
    pub enterprise_id: Uuid,
    pub kind: String,
    pub topic: String,
    pub question_md: String,
    pub skill_domain: String,
    pub orientation_slug: Option<String>,
    pub duration_minutes: Option<i16>,
    pub scheduled_at: Option<chrono::DateTime<chrono::Utc>>,
    pub document_url: Option<String>,
    pub review_deadline: Option<chrono::DateTime<chrono::Utc>>,
    pub reviewers_wanted: Option<i16>,
    pub synthesis_md: Option<String>,
    pub fee: BigDecimal,
    pub currency: String,
    pub commission_percent: BigDecimal,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

const CONSULTATION_SELECT: &str = r#"
    SELECT id, enterprise_id, kind, topic, question_md, skill_domain,
           orientation_slug, duration_minutes, scheduled_at, document_url,
           review_deadline, reviewers_wanted, synthesis_md, fee, currency,
           commission_percent, status, created_at
      FROM consultations
"#;

#[derive(Debug, Clone, Deserialize)]
pub struct ConsultationInput {
    pub kind: String,
    pub topic: String,
    pub question_md: String,
    pub skill_domain: String,
    #[serde(default)]
    pub orientation_slug: Option<String>,
    #[serde(default)]
    pub duration_minutes: Option<i16>,
    #[serde(default)]
    pub document_url: Option<String>,
    #[serde(default)]
    pub review_deadline: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub reviewers_wanted: Option<i16>,
    #[serde(default)]
    pub implementation_type: Option<String>,
    #[serde(default)]
    pub duration_weeks: Option<i16>,
    pub fee: BigDecimal,
    #[serde(default = "eur")]
    pub currency: String,
}

fn eur() -> String {
    "EUR".into()
}

pub async fn request(
    db: &PgPool,
    enterprise_id: Uuid,
    author: Uuid,
    input: ConsultationInput,
) -> Result<Consultation, AppError> {
    if !KINDS.contains(&input.kind.as_str()) {
        return Err(AppError::Validation(format!(
            "kind must be one of: {}",
            KINDS.join(", ")
        )));
    }
    if input.question_md.trim().len() < 30 {
        return Err(AppError::Validation(
            "write the question out. A consultation with no stated question is an hour \
             of both people working out what the hour is for."
                .into(),
        ));
    }
    crate::validators::check_max_len(&input.topic, "topic", 200)?;
    crate::validators::check_max_len(&input.question_md, "question_md", 20_000)?;

    if let Some(slug) = &input.orientation_slug {
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

    if let Some(kind) = &input.implementation_type
        && !IMPLEMENTATION_TYPES.contains(&kind.as_str())
    {
        return Err(AppError::Validation(format!(
            "implementation_type must be one of: {}",
            IMPLEMENTATION_TYPES.join(", ")
        )));
    }

    let commission = commission_for(&input.kind);

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO consultations
            (enterprise_id, kind, topic, question_md, skill_domain, orientation_slug,
             duration_minutes, document_url, review_deadline, reviewers_wanted,
             implementation_type, duration_weeks, fee, currency, commission_percent,
             created_by)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)
         RETURNING id",
    )
    .bind(enterprise_id)
    .bind(&input.kind)
    .bind(input.topic.trim())
    .bind(input.question_md.trim())
    .bind(&input.skill_domain)
    .bind(input.orientation_slug.as_deref())
    .bind(input.duration_minutes)
    .bind(input.document_url.as_deref())
    .bind(input.review_deadline)
    .bind(input.reviewers_wanted)
    .bind(input.implementation_type.as_deref())
    .bind(input.duration_weeks)
    .bind(&input.fee)
    .bind(&input.currency)
    .bind(BigDecimal::try_from(commission).unwrap_or_default())
    .bind(author)
    .fetch_one(db)
    .await
    .map_err(|e| {
        let m = e.to_string();
        if m.contains("an_advisory_is_a_call_of_a_stated_length") {
            AppError::Validation(
                "say how long the call is: 30, 60 or 120 minutes. The expert is pricing \
                 their afternoon on it."
                    .into(),
            )
        } else if m.contains("an_implementation_says_what_and_how_long") {
            AppError::Validation(
                "say what is being built and over how many weeks. An implementation is \
                 weeks of work, and the type decides which experts belong on it."
                    .into(),
            )
        } else if m.contains("a_review_has_a_document_and_a_deadline") {
            AppError::Validation(
                "a review needs the document, a deadline and how many reviewers you \
                 want. Nobody can read what has not been sent."
                    .into(),
            )
        } else {
            AppError::from(e)
        }
    })?;

    consultation(db, id).await
}

pub async fn consultation(db: &PgPool, id: Uuid) -> Result<Consultation, AppError> {
    let sql = format!("{CONSULTATION_SELECT} WHERE id = $1");
    sqlx::query_as::<_, Consultation>(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NotFound("consultation not found".into()))
}

pub async fn for_enterprise(
    db: &PgPool,
    enterprise_id: Uuid,
) -> Result<Vec<Consultation>, AppError> {
    let sql = format!("{CONSULTATION_SELECT} WHERE enterprise_id = $1 ORDER BY created_at DESC");
    let rows = sqlx::query_as::<_, Consultation>(sqlx::AssertSqlSafe(sql))
        .bind(enterprise_id)
        .fetch_all(db)
        .await?;
    Ok(rows)
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Expert {
    pub expert_user_id: Uuid,
    pub username: String,
    pub accepted_at: Option<chrono::DateTime<chrono::Utc>>,
    pub declined_at: Option<chrono::DateTime<chrono::Utc>>,
    pub comment_md: Option<String>,
    pub verdict: Option<String>,
    pub submitted_at: Option<chrono::DateTime<chrono::Utc>>,
    pub share: Option<BigDecimal>,
}

pub async fn experts(db: &PgPool, consultation_id: Uuid) -> Result<Vec<Expert>, AppError> {
    let rows = sqlx::query_as::<_, Expert>(
        "SELECT e.expert_user_id, u.username, e.accepted_at, e.declined_at,
                e.comment_md, e.verdict, e.submitted_at, e.share
           FROM consultation_experts e
           JOIN users u ON u.id = e.expert_user_id
          WHERE e.consultation_id = $1
          ORDER BY e.invited_at",
    )
    .bind(consultation_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// Invite an expert. Checks the rank, and stops there — the answer is theirs.
pub async fn invite_expert(
    db: &PgPool,
    consultation_id: Uuid,
    expert_user_id: Uuid,
) -> Result<(), AppError> {
    let rank: Option<String> = sqlx::query_scalar("SELECT rank FROM user_ranks WHERE user_id = $1")
        .bind(expert_user_id)
        .fetch_optional(db)
        .await?;
    let rank = rank.unwrap_or_else(|| "apprenti".into());

    if !crate::services::ambassadors::rank_clears(&rank, EXPERT_MIN_RANK) {
        return Err(AppError::Validation(format!(
            "advising for money under Skilluv's name opens at {EXPERT_MIN_RANK}, and \
             this person is {rank}. The client is buying our judgement about who to \
             put in the room."
        )));
    }

    sqlx::query(
        "INSERT INTO consultation_experts (consultation_id, expert_user_id)
         VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(consultation_id)
    .bind(expert_user_id)
    .execute(db)
    .await?;

    sqlx::query(
        "UPDATE consultations SET status = 'matching'
          WHERE id = $1 AND status = 'requested'",
    )
    .bind(consultation_id)
    .execute(db)
    .await?;

    Ok(())
}

/// The expert answers for themselves.
pub async fn respond(
    db: &PgPool,
    consultation_id: Uuid,
    expert_user_id: Uuid,
    accept: bool,
    reason: Option<&str>,
) -> Result<(), AppError> {
    let done = sqlx::query(
        "UPDATE consultation_experts
            SET accepted_at = CASE WHEN $3 THEN NOW() END,
                declined_at = CASE WHEN $3 THEN NULL ELSE NOW() END,
                declined_reason = CASE WHEN $3 THEN NULL ELSE $4 END
          WHERE consultation_id = $1 AND expert_user_id = $2",
    )
    .bind(consultation_id)
    .bind(expert_user_id)
    .bind(accept)
    .bind(reason.map(str::trim).filter(|r| !r.is_empty()))
    .execute(db)
    .await?;

    if done.rows_affected() == 0 {
        return Err(AppError::NotFound(
            "you were not invited to this one".into(),
        ));
    }
    Ok(())
}

/// Submit an opinion.
pub async fn submit_opinion(
    db: &PgPool,
    consultation_id: Uuid,
    expert_user_id: Uuid,
    comment_md: &str,
    verdict: Option<&str>,
) -> Result<(), AppError> {
    if comment_md.trim().len() < 50 {
        return Err(AppError::Validation(
            "fifty characters is the floor. The client is paying for an opinion, and \
             below that there is not one."
                .into(),
        ));
    }
    if let Some(v) = verdict
        && !VERDICTS.contains(&v)
    {
        return Err(AppError::Validation(format!(
            "verdict must be one of: {}",
            VERDICTS.join(", ")
        )));
    }

    let done = sqlx::query(
        "UPDATE consultation_experts
            SET comment_md = $3, verdict = $4, submitted_at = NOW()
          WHERE consultation_id = $1 AND expert_user_id = $2
            AND accepted_at IS NOT NULL",
    )
    .bind(consultation_id)
    .bind(expert_user_id)
    .bind(comment_md.trim())
    .bind(verdict)
    .execute(db)
    .await?;

    if done.rows_affected() == 0 {
        return Err(AppError::NotFound(
            "you have not accepted this consultation".into(),
        ));
    }
    Ok(())
}

/// Deliver, and pay everyone who actually wrote something.
pub async fn deliver(
    db: &PgPool,
    consultation_id: Uuid,
    synthesis_md: Option<&str>,
) -> Result<(BigDecimal, usize), AppError> {
    let consultation = consultation(db, consultation_id).await?;
    if consultation.status == "delivered" {
        return Err(AppError::Validation("already delivered".into()));
    }

    let submitters: Vec<Uuid> = sqlx::query_scalar(
        "SELECT expert_user_id FROM consultation_experts
          WHERE consultation_id = $1 AND submitted_at IS NOT NULL
          ORDER BY submitted_at",
    )
    .bind(consultation_id)
    .fetch_all(db)
    .await?;

    if submitters.is_empty() {
        return Err(AppError::Validation(
            "nobody has written anything yet. Delivering now would charge the client \
             for an empty document."
                .into(),
        ));
    }

    let (commission, shares) = split_between_experts(
        &consultation.fee,
        &consultation.commission_percent,
        submitters.len(),
    );

    let mut tx = db.begin().await?;
    sqlx::query(
        "UPDATE consultations
            SET status = 'delivered', synthesis_md = COALESCE($2, synthesis_md),
                synthesis_delivered_at = NOW()
          WHERE id = $1",
    )
    .bind(consultation_id)
    .bind(synthesis_md.map(str::trim).filter(|s| !s.is_empty()))
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        if e.to_string()
            .contains("a_delivered_review_has_its_synthesis")
        {
            AppError::Validation(
                "a review is delivered with its synthesis. The comments are the \
                 working; the synthesis is what the client bought."
                    .into(),
            )
        } else {
            AppError::from(e)
        }
    })?;

    for (expert, share) in submitters.iter().zip(shares.iter()) {
        sqlx::query(
            "UPDATE consultation_experts SET share = $3, paid_at = NOW()
              WHERE consultation_id = $1 AND expert_user_id = $2",
        )
        .bind(consultation_id)
        .bind(expert)
        .bind(share)
        .execute(&mut *tx)
        .await?;
    }

    sqlx::query(
        "INSERT INTO platform_revenues
            (source, related_enterprise_id, amount_credits, fee_rate_bps, notes)
         VALUES ('consulting_fee', $1, $2, $3, $4)",
    )
    .bind(consultation.enterprise_id)
    .bind(&commission)
    .bind(ledger::percent_to_bps(&consultation.commission_percent))
    .bind(format!("{} — {}", consultation.kind, consultation.topic))
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    let currency: ledger::Currency = consultation.currency.parse()?;
    for (expert, share) in submitters.iter().zip(shares.iter()) {
        ledger::capture_for_recipient(
            db,
            "stripe",
            format!("consultation:{consultation_id}:{expert}"),
            *expert,
            share.clone(),
            BigDecimal::from(0),
            currency,
            "consultation",
            consultation_id,
        )
        .await?;
    }

    Ok((commission, submitters.len()))
}

// ═══════════════════════════════════════════════════════════════════
// Skill audits
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Audit {
    pub id: Uuid,
    pub enterprise_id: Uuid,
    pub scope: String,
    pub stated_purpose: String,
    pub employees_count: i16,
    pub domains_assessed: Vec<String>,
    pub orientations_assessed: Vec<String>,
    pub methodology: Vec<String>,
    pub duration_weeks: i16,
    pub fee: BigDecimal,
    pub currency: String,
    pub matrix_url: Option<String>,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

const AUDIT_SELECT: &str = r#"
    SELECT id, enterprise_id, scope, stated_purpose, employees_count,
           domains_assessed, orientations_assessed, methodology, duration_weeks,
           fee, currency, matrix_url, status, created_at
      FROM enterprise_skill_audits
"#;

#[derive(Debug, Clone, Deserialize)]
pub struct AuditInput {
    pub scope: String,
    pub stated_purpose: String,
    pub employees_count: i16,
    pub domains_assessed: Vec<String>,
    #[serde(default)]
    pub orientations_assessed: Vec<String>,
    #[serde(default)]
    pub methodology: Option<Vec<String>>,
    #[serde(default)]
    pub duration_weeks: Option<i16>,
    pub fee: BigDecimal,
    #[serde(default = "eur")]
    pub currency: String,
}

pub async fn open_audit(
    db: &PgPool,
    enterprise_id: Uuid,
    author: Uuid,
    input: AuditInput,
) -> Result<Audit, AppError> {
    if input.stated_purpose.trim().len() < 20 {
        return Err(AppError::Validation(
            "say what the audit is for, in a sentence. Every person assessed is shown \
             it, and it is the difference between a development plan and a redundancy \
             list."
                .into(),
        ));
    }
    if input.domains_assessed.is_empty() {
        return Err(AppError::Validation(
            "say which domains are being assessed".into(),
        ));
    }

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO enterprise_skill_audits
            (enterprise_id, scope, stated_purpose, employees_count, domains_assessed,
             orientations_assessed, methodology, duration_weeks, fee, currency,
             created_by)
         VALUES ($1,$2,$3,$4,$5,$6,COALESCE($7,'{challenges,code_review}'),
                 COALESCE($8,3),$9,$10,$11)
         RETURNING id",
    )
    .bind(enterprise_id)
    .bind(input.scope.trim())
    .bind(input.stated_purpose.trim())
    .bind(input.employees_count)
    .bind(&input.domains_assessed)
    .bind(&input.orientations_assessed)
    .bind(input.methodology.as_ref())
    .bind(input.duration_weeks)
    .bind(&input.fee)
    .bind(&input.currency)
    .bind(author)
    .fetch_one(db)
    .await?;

    audit(db, id).await
}

pub async fn audit(db: &PgPool, id: Uuid) -> Result<Audit, AppError> {
    let sql = format!("{AUDIT_SELECT} WHERE id = $1");
    sqlx::query_as::<_, Audit>(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NotFound("audit not found".into()))
}

/// Record that somebody has been told they are being assessed.
///
/// Separate from the assessment itself and required before it, because the
/// order matters: telling somebody afterwards is telling them what was
/// decided, not asking them to take part.
pub async fn inform_employee(
    db: &PgPool,
    audit_id: Uuid,
    employee_email: &str,
    orientation_slug: &str,
    employee_name: Option<&str>,
) -> Result<Uuid, AppError> {
    if !employee_email.contains('@') {
        return Err(AppError::Validation("that is not an email".into()));
    }

    // If they turn out to have an account, the assessment can reach them
    // directly rather than through the employer who commissioned it.
    let matched: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM users WHERE lower(email) = lower($1)")
            .bind(employee_email.trim())
            .fetch_optional(db)
            .await?;

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO enterprise_employee_assessments
            (audit_id, employee_email, employee_name, matched_user_id,
             orientation_slug, informed_at)
         VALUES ($1,$2,$3,$4,$5,NOW())
         ON CONFLICT (audit_id, employee_email, orientation_slug) DO UPDATE
             SET informed_at = COALESCE(enterprise_employee_assessments.informed_at, NOW()),
                 matched_user_id = EXCLUDED.matched_user_id
         RETURNING id",
    )
    .bind(audit_id)
    .bind(employee_email.trim())
    .bind(employee_name.map(str::trim))
    .bind(matched)
    .bind(orientation_slug)
    .fetch_one(db)
    .await?;

    Ok(id)
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssessmentInput {
    pub assessed_level: String,
    #[serde(default)]
    pub strengths: Vec<String>,
    #[serde(default)]
    pub gaps: Vec<String>,
    #[serde(default)]
    pub notes_md: Option<String>,
}

/// Write what was concluded about one person.
pub async fn assess(
    db: &PgPool,
    assessment_id: Uuid,
    assessor: Uuid,
    input: AssessmentInput,
) -> Result<(), AppError> {
    const LEVELS: &[&str] = &["junior", "mid", "senior", "principal"];
    if !LEVELS.contains(&input.assessed_level.as_str()) {
        return Err(AppError::Validation(format!(
            "assessed_level must be one of: {}",
            LEVELS.join(", ")
        )));
    }

    sqlx::query(
        "UPDATE enterprise_employee_assessments
            SET assessed_level = $2, strengths = $3, gaps = $4, notes_md = $5,
                assessed_by = $6, assessed_at = NOW()
          WHERE id = $1",
    )
    .bind(assessment_id)
    .bind(&input.assessed_level)
    .bind(&input.strengths)
    .bind(&input.gaps)
    .bind(input.notes_md.as_deref())
    .bind(assessor)
    .execute(db)
    .await
    .map_err(|e| {
        if e.to_string()
            .contains("nobody_is_assessed_without_being_told")
        {
            AppError::Validation(
                "this person has not been told they are being assessed. Their employer \
                 is the customer; they are not."
                    .into(),
            )
        } else {
            AppError::from(e)
        }
    })?;

    Ok(())
}

/// Show somebody what was written about them.
pub async fn share_with_employee(db: &PgPool, assessment_id: Uuid) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE enterprise_employee_assessments
            SET shared_with_employee_at = NOW()
          WHERE id = $1 AND assessed_at IS NOT NULL",
    )
    .bind(assessment_id)
    .execute(db)
    .await?;
    Ok(())
}

/// Deliver the audit to the client.
pub async fn deliver_audit(
    db: &PgPool,
    audit_id: Uuid,
    matrix_url: &str,
    recommendations_md: Option<&str>,
) -> Result<BigDecimal, AppError> {
    let audit = audit(db, audit_id).await?;
    if audit.status == "delivered" {
        return Err(AppError::Validation("already delivered".into()));
    }
    if !matrix_url.starts_with("https://") {
        return Err(AppError::Validation("the matrix URL must be https".into()));
    }

    let mut tx = db.begin().await?;
    sqlx::query(
        "UPDATE enterprise_skill_audits
            SET matrix_url = $2, recommendations_md = $3, delivered_at = NOW(),
                status = 'delivered'
          WHERE id = $1",
    )
    .bind(audit_id)
    .bind(matrix_url.trim())
    .bind(recommendations_md.map(str::trim).filter(|r| !r.is_empty()))
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        let m = e.to_string();
        if m.contains("have not been shown what was written about them") {
            AppError::Validation(
                m.rsplit("ERROR:")
                    .next()
                    .unwrap_or("somebody has not seen their assessment")
                    .trim()
                    .to_string(),
            )
        } else {
            AppError::from(e)
        }
    })?;

    sqlx::query(
        "INSERT INTO platform_revenues
            (source, related_enterprise_id, amount_credits, fee_rate_bps, notes)
         VALUES ('consulting_fee', $1, $2, 10000, $3)",
    )
    .bind(audit.enterprise_id)
    .bind(&audit.fee)
    .bind(format!("audit de compétences — {}", audit.scope))
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(audit.fee)
}

/// What one person can see about themselves.
pub async fn assessments_for_user(
    db: &PgPool,
    user_id: Uuid,
) -> Result<Vec<serde_json::Value>, AppError> {
    let rows = sqlx::query_scalar(
        "SELECT jsonb_build_object(
                    'assessment_id', a.id,
                    'scope', s.scope,
                    'stated_purpose', s.stated_purpose,
                    'orientation', a.orientation_slug,
                    'assessed_level', a.assessed_level,
                    'strengths', a.strengths,
                    'gaps', a.gaps,
                    'notes', a.notes_md,
                    'assessed_at', a.assessed_at,
                    'your_response', a.employee_response_md
                )
           FROM enterprise_employee_assessments a
           JOIN enterprise_skill_audits s ON s.id = a.audit_id
          WHERE a.matched_user_id = $1 AND a.shared_with_employee_at IS NOT NULL
          ORDER BY a.assessed_at DESC",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// The right of reply. A conclusion with none is a verdict.
pub async fn respond_to_assessment(
    db: &PgPool,
    assessment_id: Uuid,
    user_id: Uuid,
    response_md: &str,
) -> Result<(), AppError> {
    if response_md.trim().is_empty() {
        return Err(AppError::Validation("say something".into()));
    }

    let done = sqlx::query(
        "UPDATE enterprise_employee_assessments
            SET employee_response_md = $3
          WHERE id = $1 AND matched_user_id = $2
            AND shared_with_employee_at IS NOT NULL",
    )
    .bind(assessment_id)
    .bind(user_id)
    .bind(response_md.trim())
    .execute(db)
    .await?;

    if done.rows_affected() == 0 {
        return Err(AppError::NotFound(
            "no assessment of yours has been shared with you here".into(),
        ));
    }
    Ok(())
}

/// How far an audit is from being deliverable.
pub async fn audit_readiness(db: &PgPool, audit_id: Uuid) -> Result<(i64, i64, i64), AppError> {
    let row: (i64, i64, i64) = sqlx::query_as(
        "SELECT count(*) FILTER (WHERE informed_at IS NOT NULL),
                count(*) FILTER (WHERE assessed_at IS NOT NULL),
                count(*) FILTER (WHERE shared_with_employee_at IS NOT NULL)
           FROM enterprise_employee_assessments WHERE audit_id = $1",
    )
    .bind(audit_id)
    .fetch_one(db)
    .await?;
    Ok(row)
}

/// The consultation, if the caller's company bought it.
pub async fn owned_by(
    db: &PgPool,
    consultation_id: Uuid,
    enterprise_id: Uuid,
) -> Result<Consultation, AppError> {
    let consultation = consultation(db, consultation_id).await?;
    if consultation.enterprise_id != enterprise_id {
        return Err(AppError::NotFound("consultation not found".into()));
    }
    Ok(consultation)
}

/// The rating a client leaves. One, on a delivered consultation.
pub async fn rate(db: &PgPool, consultation_id: Uuid, rating: i16) -> Result<(), AppError> {
    if !(1..=5).contains(&rating) {
        return Err(AppError::Validation("a rating runs from 1 to 5".into()));
    }
    let done = sqlx::query(
        "UPDATE consultations SET rating = $2, rated_at = NOW()
          WHERE id = $1 AND status = 'delivered'",
    )
    .bind(consultation_id)
    .bind(rating)
    .execute(db)
    .await?;

    if done.rows_affected() == 0 {
        return Err(AppError::Validation(
            "nothing has been delivered yet".into(),
        ));
    }
    Ok(())
}

/// The share one expert would receive, for showing before they accept.
pub fn expected_share(
    fee: &BigDecimal,
    commission_percent: &BigDecimal,
    expected_submitters: usize,
) -> Option<f64> {
    let (_, shares) = split_between_experts(fee, commission_percent, expected_submitters);
    shares.first().and_then(|s| s.to_f64())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn dec(s: &str) -> BigDecimal {
        BigDecimal::from_str(s).unwrap()
    }

    #[test]
    fn an_implementation_sits_between_the_two() {
        // Weeks of somebody else's work, with Skilluv holding the shape.
        let advisory = commission_for("advisory");
        let implementation = commission_for("implementation");
        let review = commission_for("architecture_review");
        assert!(implementation > advisory);
        assert!(implementation < review);
    }

    #[test]
    fn a_review_keeps_more_than_an_advisory_call() {
        // An advisory is an introduction and an hour in a calendar. A review
        // is a panel assembled, a deadline held and a synthesis written.
        assert!(commission_for("architecture_review") > commission_for("advisory"));
        // And a majority still reaches the people who wrote the opinions.
        assert!(commission_for("architecture_review") < 50.0);
    }

    #[test]
    fn a_fee_always_divides_exactly() {
        for (fee, commission, experts) in [
            ("10000.00", "40.00", 5),
            ("3000.00", "40.00", 3),
            ("999.99", "25.00", 1),
            ("7777.77", "40.00", 7),
        ] {
            let (kept, shares) = split_between_experts(&dec(fee), &dec(commission), experts);
            let total: BigDecimal = shares.iter().fold(kept.clone(), |acc, s| acc + s);
            assert_eq!(total, dec(fee), "{fee} split {experts} ways lost a centime");
            assert_eq!(shares.len(), experts);
        }
    }

    #[test]
    fn nobody_who_wrote_nothing_is_in_the_division() {
        // The fee buys the opinion, not the availability.
        let (kept, shares) = split_between_experts(&dec("10000.00"), &dec("40.00"), 0);
        assert_eq!(kept, dec("4000.00"));
        assert!(shares.is_empty());
    }

    #[test]
    fn the_last_expert_absorbs_the_rounding_rather_than_the_platform() {
        // 6000 split three ways is exact; 6000.01 is not.
        let (kept, shares) = split_between_experts(&dec("10000.01"), &dec("40.00"), 3);
        let total: BigDecimal = shares.iter().fold(kept, |acc, s| acc + s);
        assert_eq!(total, dec("10000.01"));
        assert!(shares[2] >= shares[0]);
    }

    #[test]
    fn an_expert_can_be_shown_their_share_before_accepting() {
        // Ten thousand, 40% kept, three reviewers: two thousand each.
        assert_eq!(
            expected_share(&dec("10000.00"), &dec("40.00"), 3),
            Some(2000.0)
        );
    }

    #[test]
    fn every_kind_and_verdict_is_a_known_one() {
        assert_eq!(KINDS.len(), 3);
        assert_eq!(IMPLEMENTATION_TYPES.len(), 5);
        assert_eq!(VERDICTS.len(), 4);
        assert!(VERDICTS.contains(&"approve_with_concerns"));
    }
}
