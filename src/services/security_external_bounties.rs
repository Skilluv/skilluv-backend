//! Bounties earned somewhere else, claimed here.
//!
//! ## What checking one can and cannot establish
//!
//! A reviewer opens the public disclosure and checks that it exists, that it
//! names this person, and that its severity is roughly what was claimed. That
//! is everything anybody can check from outside, and the attestation says so:
//! `security_external_bounty_confirmed`, worth less than a finding this platform
//! reproduced itself, because one of the two we reproduced.
//!
//! ## Why an undisclosed report is refused rather than left pending
//!
//! Most bounty reports are never published. A queue that accepts them fills up
//! with claims nobody can ever act on, and a queue nobody can work is a queue
//! nobody opens — after which the ones that *could* have been verified sit in it
//! too. So the link is required at submission and a claim whose page has gone
//! is refused with that as the reason.

use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

/// What somebody claims.
#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ClaimInput {
    /// One of the curated platforms, or `self_hosted` for a programme an
    /// organisation runs itself.
    pub platform: String,
    pub organisation_name: String,
    /// The public disclosure. Without it nothing can be checked.
    pub report_url: String,
    pub claimed_severity: String,
    #[serde(default)]
    pub cwe_id: Option<String>,
    pub summary_md: String,
    #[serde(default)]
    pub disclosed_on: Option<chrono::NaiveDate>,
    /// The curated programme, when this platform lists it.
    #[serde(default)]
    pub program_id: Option<Uuid>,
}

/// File a claim.
pub async fn claim(db: &PgPool, user_id: Uuid, input: ClaimInput) -> Result<Uuid, AppError> {
    if !matches!(
        input.platform.as_str(),
        "hackerone" | "bugcrowd" | "intigriti" | "yeswehack" | "self_hosted"
    ) {
        return Err(AppError::Validation(format!(
            "'{}' is not a platform this claim can name",
            input.platform
        )));
    }
    if !matches!(
        input.claimed_severity.as_str(),
        "critical" | "high" | "medium" | "low" | "informational"
    ) {
        return Err(AppError::Validation("not a severity".into()));
    }
    if !input.report_url.starts_with("https://") {
        return Err(AppError::Validation(
            "a link to the public disclosure. A report nobody outside the \
             programme can read cannot be checked by anybody here, and a claim \
             that cannot be checked is not worth filing"
                .into(),
        ));
    }
    if input.summary_md.trim().chars().count() < 40 {
        return Err(AppError::Validation(
            "two sentences on what it was. The report is at the link; this is \
             what a reader sees on your profile"
                .into(),
        ));
    }
    crate::validators::check_max_len(&input.organisation_name, "organisation_name", 160)?;
    crate::validators::check_max_len(&input.report_url, "report_url", 500)?;

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO external_bounty_claims
             (user_id, program_id, platform, organisation_name, report_url,
              claimed_severity, cwe_id, summary_md, disclosed_on)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         ON CONFLICT (user_id, report_url) DO UPDATE SET
             summary_md = EXCLUDED.summary_md,
             claimed_severity = EXCLUDED.claimed_severity,
             cwe_id = EXCLUDED.cwe_id,
             disclosed_on = EXCLUDED.disclosed_on,
             updated_at = NOW()
         RETURNING id",
    )
    .bind(user_id)
    .bind(input.program_id)
    .bind(&input.platform)
    .bind(input.organisation_name.trim())
    .bind(input.report_url.trim())
    .bind(&input.claimed_severity)
    .bind(input.cwe_id.as_deref())
    .bind(input.summary_md.trim())
    .bind(input.disclosed_on)
    .fetch_one(db)
    .await?;

    Ok(id)
}

/// Somebody's own claims.
pub async fn mine(db: &PgPool, user_id: Uuid) -> Result<Vec<serde_json::Value>, AppError> {
    Ok(sqlx::query_scalar(
        "SELECT jsonb_build_object(
                    'id', id, 'platform', platform,
                    'organisation', organisation_name,
                    'report_url', report_url,
                    'claimed_severity', claimed_severity,
                    'severity', severity, 'cwe_id', cwe_id,
                    'summary_md', summary_md,
                    'disclosed_on', disclosed_on,
                    'state', CASE
                        WHEN verified_at IS NOT NULL THEN 'confirmed'
                        WHEN refused_at IS NOT NULL THEN 'refused'
                        ELSE 'waiting' END,
                    'refused_reason', refused_reason,
                    'created_at', created_at)
           FROM external_bounty_claims
          WHERE user_id = $1
          ORDER BY created_at DESC
          LIMIT 200",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?)
}

/// The review queue.
pub async fn awaiting_review(db: &PgPool, limit: i64) -> Result<Vec<serde_json::Value>, AppError> {
    Ok(sqlx::query_scalar(
        "SELECT jsonb_build_object(
                    'id', c.id, 'username', u.username,
                    'platform', c.platform,
                    'organisation', c.organisation_name,
                    'report_url', c.report_url,
                    'claimed_severity', c.claimed_severity,
                    'cwe_id', c.cwe_id, 'summary_md', c.summary_md,
                    'disclosed_on', c.disclosed_on,
                    'created_at', c.created_at,
                    'other_claims_by_this_person', (
                        SELECT count(*) FROM external_bounty_claims o
                         WHERE o.user_id = c.user_id
                           AND o.verified_at IS NOT NULL))
           FROM external_bounty_claims c
           JOIN users u ON u.id = c.user_id
          WHERE c.verified_at IS NULL AND c.refused_at IS NULL
          ORDER BY c.created_at
          LIMIT $1",
    )
    .bind(limit)
    .fetch_all(db)
    .await?)
}

/// Accept a claim, at the severity the reviewer settled on.
///
/// The attestation is issued here rather than by the sweep, because this is the
/// only moment anybody knows the claim is good — there is no row anywhere else
/// for a later pass to read.
pub async fn verify(
    db: &PgPool,
    reviewer: Uuid,
    claim_id: Uuid,
    severity: &str,
) -> Result<String, AppError> {
    if !matches!(
        severity,
        "critical" | "high" | "medium" | "low" | "informational"
    ) {
        return Err(AppError::Validation("not a severity".into()));
    }

    let row: Option<(Uuid, String, String, String)> = sqlx::query_as(
        "UPDATE external_bounty_claims
            SET verified_at = NOW(), verified_by_user_id = $2, severity = $3
          WHERE id = $1 AND verified_at IS NULL AND refused_at IS NULL
        RETURNING user_id, platform, organisation_name, report_url",
    )
    .bind(claim_id)
    .bind(reviewer)
    .bind(severity)
    .fetch_optional(db)
    .await?;

    let Some((user_id, platform, organisation, report_url)) = row else {
        return Err(AppError::Conflict(
            "that claim has already been decided".into(),
        ));
    };

    let basis = "security_external_bounty_confirmed";
    let (title, description) = crate::services::attestations::basis_wording(db, basis).await;

    let issued = crate::services::artefact_attestations::issue(
        db,
        user_id,
        basis,
        &crate::services::artefact_attestations::Evidence {
            url: report_url,
            title,
            description: format!("{description}\n\n{organisation} — {platform}, {severity}"),
            deliverable_id: None,
            project_id: None,
            skill_node_ids: Vec::new(),
        },
        &crate::services::security_attestations::SECURITY,
    )
    .await?;

    Ok(issued.verification_code)
}

/// Refuse it, with the reason the person will read.
pub async fn refuse(
    db: &PgPool,
    _reviewer: Uuid,
    claim_id: Uuid,
    reason: &str,
) -> Result<(), AppError> {
    if reason.trim().chars().count() < 10 {
        return Err(AppError::Validation(
            "say why. A refusal with no reason is a refusal that gets filed \
             again next week"
                .into(),
        ));
    }
    let affected = sqlx::query(
        "UPDATE external_bounty_claims
            SET refused_at = NOW(), refused_reason = $2
          WHERE id = $1 AND verified_at IS NULL AND refused_at IS NULL",
    )
    .bind(claim_id)
    .bind(reason.trim())
    .execute(db)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(AppError::Conflict(
            "that claim has already been decided".into(),
        ));
    }
    Ok(())
}
