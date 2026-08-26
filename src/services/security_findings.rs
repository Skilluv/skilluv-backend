//! A reported vulnerability, from arrival to public disclosure.
//!
//! ## The one rule the whole module is built around
//!
//! A finding is only a finding if it was in scope. Everything else here —
//! triage, severity, embargo, deduplication — is secondary to that, and it is
//! checked first, at submission, against a list this module owns. A report
//! against something nobody authorised is refused rather than triaged, because
//! accepting it would make this platform the place that received an
//! unauthorised intrusion report and did nothing about it.
//!
//! ## The state machine, and why it is here rather than in the database
//!
//! Postgres enforces what a row may *say* — migration 0547 has fourteen
//! constraints on that. What it cannot enforce is who may change it and in
//! which order, because both depend on capabilities and on the actor. So the
//! transitions live in [`allowed_transition`], one table, with a test that
//! walks it. A `submitted` finding cannot become `published`; a reporter can
//! withdraw and nothing else; only an administrator publishes.
//!
//! ## Triage is skipped for people with a record, and recorded when it is
//!
//! W-03 asked for triage to be mandatory for juniors and optional for seniors.
//! Implemented, with the reason stored on the row rather than recomputed:
//! `triage_skipped_reason` says whether it was the rank or the track record,
//! because the rule will change and old findings should still say which way it
//! went for them.
//!
//! ## What happens on confirmation
//!
//! Four things, in one transaction: the status moves, a `deliverables` row is
//! created, fragments are credited, and the embargo clock starts. The
//! deliverable is the part that matters structurally — it is what makes a
//! vulnerability count towards a rank exactly as a merged pull request does,
//! which is what one cross-domain rank means (F-06).
//!
//! ## What deliberately does not happen automatically
//!
//! A duplicate is never merged by a machine. The similarity scan flags
//! candidates and a person decides, because a merge decides who is paid and a
//! trigram score does not get to.

use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::cvss;

/// The hosts a report may name, when the target is this platform.
///
/// This list is the authoritative scope. `docs/security/SCOPE.md` quotes it and
/// says so; if the two ever disagree, this one is what refuses a submission and
/// the document is wrong.
///
/// Overridable with `SKILLUV_SECURITY_SCOPE_HOSTS` (comma-separated) so that a
/// staging deployment can widen or narrow it without a release — the same
/// mechanism the rate-limit allow-list uses.
const DEFAULT_SCOPE_HOSTS: &[&str] = &[
    "api.skill-uv.com",
    "skill-uv.com",
    "admin.skill-uv.com",
    "staging.skill-uv.com",
    "ctf.skill-uv.com",
];

/// Fragments for a confirmed finding, by the severity a validator settled.
///
/// The scale F-05 proposed, and the ratios are the argument: a critical is
/// worth two hundred informationals, so there is no volume strategy. Nothing
/// is awarded before confirmation, because a submission is a claim.
pub fn fragments_for(severity_tier: &str) -> i32 {
    match severity_tier {
        "critical" => 1000,
        "high" => 300,
        "medium" => 80,
        "low" => 20,
        _ => 5,
    }
}

/// Ranks that do not need a junior triage pass before a reviewer sees the
/// report. W-03's rule, with the reason recorded on the row.
fn triage_skip_reason(rank: Option<&str>, confirmed_findings: i64) -> Option<&'static str> {
    match rank {
        Some("maitre" | "doyen") => Some("reporter_rank"),
        // An artisan with a record of confirmed findings has earned the same.
        // Five, because that is enough to have been wrong once and corrected.
        Some("artisan") if confirmed_findings >= 5 => Some("reporter_track_record"),
        _ => None,
    }
}

/// The hosts in scope right now.
pub fn scope_hosts() -> Vec<String> {
    parse_scope(std::env::var("SKILLUV_SECURITY_SCOPE_HOSTS").ok().as_deref())
}

/// The parsing, separated from the environment so a test can exercise it
/// without mutating process state that every other test shares.
fn parse_scope(override_list: Option<&str>) -> Vec<String> {
    match override_list {
        Some(raw) if !raw.trim().is_empty() => raw
            .split(',')
            .map(|h| h.trim().to_ascii_lowercase())
            .filter(|h| !h.is_empty())
            .collect(),
        _ => DEFAULT_SCOPE_HOSTS.iter().map(|h| h.to_string()).collect(),
    }
}

/// Which transitions are legal, and who may make them.
///
/// `Actor` is the coarsest thing that decides: the reporter, somebody with the
/// triage capability, somebody who can review the domain, or an administrator.
/// A finer model would be a permission per transition, and there are eleven of
/// them — the cost of that precision is a table nobody reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Actor {
    Reporter,
    Triager,
    Reviewer,
    Admin,
}

/// Whether this actor may move a finding from `from` to `to`.
pub fn allowed_transition(actor: Actor, from: &str, to: &str) -> bool {
    use Actor::*;
    match (from, to) {
        // The reporter's only move, and available until somebody has acted.
        ("submitted" | "triaged", "withdrawn") => actor == Reporter,

        // Triage: worth a reviewer's time, or not.
        ("submitted", "triaged") => matches!(actor, Triager | Reviewer | Admin),
        ("submitted" | "triaged", "not_applicable") => {
            matches!(actor, Triager | Reviewer | Admin)
        }

        // Reproduction. A triager may not confirm: confirming asserts publicly
        // that a vulnerability is real, and that is the reviewer's judgement.
        ("triaged", "confirmed") => matches!(actor, Reviewer | Admin),
        // Marking a duplicate decides who is paid. Reviewer or above.
        ("submitted" | "triaged" | "confirmed", "duplicate") => {
            matches!(actor, Reviewer | Admin)
        }

        // The owner shipped something.
        ("confirmed", "fixed") => matches!(actor, Reviewer | Admin),

        // Publication is the last door and only an administrator opens it: it
        // is irreversible in the way that matters, because the internet keeps
        // a copy.
        ("confirmed" | "fixed", "published") => actor == Admin,

        _ => false,
    }
}

/// Which notification a status change earns the reporter.
///
/// `None` for the two nobody needs telling about: a withdrawal, which they did
/// themselves, and a status that is not a transition.
///
/// The kinds are rows in `notification_kinds` (migration 0547) and every one of
/// them is transactional: a reporter cannot opt out of learning what happened
/// to a report they filed. Not being told is the single most common way a
/// disclosure programme dies, and it is the one failure this table exists to
/// prevent.
pub fn notification_for(status: &str) -> Option<&'static str> {
    match status {
        "triaged" => Some("security.finding_triaged"),
        "confirmed" => Some("security.finding_confirmed"),
        "duplicate" => Some("security.finding_duplicate"),
        "not_applicable" => Some("security.finding_rejected"),
        "fixed" => Some("security.finding_fixed"),
        "published" => Some("security.finding_published"),
        _ => None,
    }
}

/// Who to tell, and what about.
#[derive(Debug, sqlx::FromRow)]
pub struct NotifiableFinding {
    pub reporter_user_id: Uuid,
    pub title: String,
    pub severity_tier: String,
}

/// The three things a notification about a finding needs.
pub async fn notifiable(db: &PgPool, finding_id: Uuid) -> Result<NotifiableFinding, AppError> {
    sqlx::query_as(
        "SELECT reporter_user_id, title, severity_tier
           FROM security_findings WHERE id = $1",
    )
    .bind(finding_id)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| AppError::NotFound("no such finding".into()))
}

// ═══════════════════════════════════════════════════════════════════
// Submission
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SubmitInput {
    pub title: String,
    pub description_md: String,
    pub reproduction_steps_md: String,
    #[serde(default)]
    pub impact_md: Option<String>,
    #[serde(default)]
    pub proposed_fix_md: Option<String>,
    /// `platform`, `mission` or `project`.
    pub target_kind: String,
    /// Required when `target_kind` is `platform`: which host.
    #[serde(default)]
    pub target_host: Option<String>,
    /// Required when the target is a mission or a project.
    #[serde(default)]
    pub mission_slug: Option<String>,
    #[serde(default)]
    pub project_slug: Option<String>,
    #[serde(default)]
    pub affected_endpoint: Option<String>,
    /// A CVSS 3.1 vector. When present the severity is computed from it and
    /// `severity_tier` is ignored — a vector is an argument and a tier is an
    /// assertion.
    #[serde(default)]
    pub cvss_vector: Option<String>,
    /// The claimed severity, for a report with no vector.
    #[serde(default)]
    pub severity_tier: Option<String>,
    #[serde(default)]
    pub cwe_id: Option<String>,
    /// Keys returned by the proof upload endpoint. Not URLs: the download
    /// endpoint signs them per request and checks who is asking.
    #[serde(default)]
    pub proof_keys: Vec<String>,
    /// Credit without a name on it.
    #[serde(default)]
    pub anonymous: bool,
}

/// What a submission returns.
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct Submitted {
    pub id: Uuid,
    /// Echoed back, trimmed as stored. The acknowledgement quotes it, and a
    /// reporter who filed three reports in an afternoon needs to know which
    /// one this is.
    pub title: String,
    pub status: String,
    pub severity_tier: String,
    pub cvss_score: Option<f64>,
    /// When the reporter can expect a first answer. The published triage
    /// commitment, computed rather than promised in prose.
    pub triage_due_by: chrono::DateTime<chrono::Utc>,
    /// True when the reporter's record meant the report went straight to a
    /// reviewer.
    pub triage_skipped: bool,
}

/// Days a reporter waits for triage, from the published policy.
pub const TRIAGE_SLA_DAYS: i64 = 7;

/// Take a report.
pub async fn submit(
    db: &PgPool,
    reporter: Uuid,
    input: SubmitInput,
) -> Result<Submitted, AppError> {
    // ── Shape ───────────────────────────────────────────────────────
    let title = input.title.trim();
    if title.chars().count() < 5 {
        return Err(AppError::Validation(
            "a title of at least five characters — 'bug' is not one".into(),
        ));
    }
    crate::validators::check_max_len(title, "title", 200)?;
    if input.description_md.trim().chars().count() < 50 {
        return Err(AppError::Validation(
            "a description of at least fifty characters. What it is, where, and \
             why it matters"
                .into(),
        ));
    }
    if input.reproduction_steps_md.trim().chars().count() < 30 {
        return Err(AppError::Validation(
            "reproduction steps a stranger could follow. Without them there is \
             nothing for a reviewer to reproduce"
                .into(),
        ));
    }
    if input.proof_keys.len() > 10 {
        return Err(AppError::Validation(
            "at most ten proof files on one report".into(),
        ));
    }
    for key in &input.proof_keys {
        if !key.starts_with("security-proofs/") {
            return Err(AppError::Validation(
                "a proof has to be a key from the proof upload endpoint. A link \
                 to somewhere else is a link that can change after review"
                    .into(),
            ));
        }
    }
    if let Some(cwe) = input.cwe_id.as_deref() {
        let ok = cwe.starts_with("CWE-")
            && cwe.len() > 4
            && cwe[4..].chars().all(|c| c.is_ascii_digit())
            && cwe[4..].len() <= 5;
        if !ok {
            return Err(AppError::Validation(
                "a weakness class looks like CWE-89".into(),
            ));
        }
    }

    // ── Severity ────────────────────────────────────────────────────
    //
    // A vector wins over a tier whenever both arrive: one is an argument that
    // can be checked metric by metric, the other is an adjective.
    let (vector, score, tier) = match input.cvss_vector.as_deref() {
        Some(raw) => {
            let scored = cvss::score_vector(raw).map_err(AppError::Validation)?;
            (Some(scored.vector), Some(scored.score), scored.tier.to_string())
        }
        None => {
            let tier = input.severity_tier.as_deref().unwrap_or("medium");
            if !matches!(
                tier,
                "critical" | "high" | "medium" | "low" | "informational"
            ) {
                return Err(AppError::Validation(
                    "severity is one of critical, high, medium, low, informational \
                     — or send a CVSS vector instead"
                        .into(),
                ));
            }
            (None, None, tier.to_string())
        }
    };

    // ── Target ──────────────────────────────────────────────────────
    let (mission_id, project_id, host) = resolve_target(db, &input).await?;

    // ── Triage ──────────────────────────────────────────────────────
    let rank: Option<String> =
        sqlx::query_scalar("SELECT rank FROM user_ranks WHERE user_id = $1")
            .bind(reporter)
            .fetch_optional(db)
            .await?;
    let confirmed: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM security_findings
          WHERE reporter_user_id = $1
            AND status IN ('confirmed', 'fixed', 'published')",
    )
    .bind(reporter)
    .fetch_one(db)
    .await?;
    let skip = triage_skip_reason(rank.as_deref(), confirmed);

    // ── Write ───────────────────────────────────────────────────────
    let mut tx = db.begin().await?;

    let id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO security_findings (
            reporter_user_id, reporter_is_anonymous,
            target_kind, mission_id, project_id, target_host, affected_endpoint,
            title, description_md, reproduction_steps_md, impact_md,
            proposed_fix_md, proof_keys,
            cvss_vector, cvss_score,
            severity_reported_tier, severity_tier, cwe_id,
            status, triage_skipped_reason
        ) VALUES (
            $1, $2,
            $3, $4, $5, $6, $7,
            $8, $9, $10, $11,
            $12, $13,
            $14, $15,
            $16, $16, $17,
            'submitted', $18
        )
        RETURNING id
        "#,
    )
    .bind(reporter)
    .bind(input.anonymous)
    .bind(&input.target_kind)
    .bind(mission_id)
    .bind(project_id)
    .bind(&host)
    .bind(input.affected_endpoint.as_deref())
    .bind(title)
    .bind(input.description_md.trim())
    .bind(input.reproduction_steps_md.trim())
    .bind(input.impact_md.as_deref())
    .bind(input.proposed_fix_md.as_deref())
    .bind(&input.proof_keys)
    .bind(vector.as_deref())
    .bind(score.map(|s| bigdecimal::BigDecimal::try_from(s).unwrap_or_default()))
    .bind(&tier)
    .bind(input.cwe_id.as_deref())
    .bind(skip)
    .fetch_one(&mut *tx)
    .await?;

    record_event(&mut tx, id, Some(reporter), "submitted", None, Some("submitted"), None, None)
        .await?;

    tx.commit().await?;

    metrics::counter!("skilluv_security_findings_submitted_total",
        "severity" => tier.clone())
    .increment(1);

    Ok(Submitted {
        id,
        title: title.to_string(),
        status: "submitted".into(),
        severity_tier: tier,
        cvss_score: score,
        triage_due_by: chrono::Utc::now() + chrono::Duration::days(TRIAGE_SLA_DAYS),
        triage_skipped: skip.is_some(),
    })
}

/// Which of the three targets this is, checked.
///
/// The scope check is here rather than at the edge because it is the rule the
/// module exists to enforce, and a caller that forgot it would have created an
/// unauthorised report.
async fn resolve_target(
    db: &PgPool,
    input: &SubmitInput,
) -> Result<(Option<Uuid>, Option<Uuid>, Option<String>), AppError> {
    match input.target_kind.as_str() {
        "platform" => {
            let host = input
                .target_host
                .as_deref()
                .map(|h| h.trim().to_ascii_lowercase())
                .ok_or_else(|| {
                    AppError::Validation("a report against the platform names a host".into())
                })?;
            if !scope_hosts().contains(&host) {
                return Err(AppError::Validation(format!(
                    "'{host}' is not in the published scope. The scope is at \
                     /security, and a report against something outside it \
                     cannot be accepted — that is what the safe harbour \
                     covers and what it does not"
                )));
            }
            Ok((None, None, Some(host)))
        }
        "mission" => {
            let slug = input.mission_slug.as_deref().ok_or_else(|| {
                AppError::Validation("a report against a mission names the mission".into())
            })?;
            let row: Option<(Uuid, Option<String>)> = sqlx::query_as(
                "SELECT id, rules_of_engagement_url FROM missions
                  WHERE slug = $1 AND skill_domain = 'security'",
            )
            .bind(slug)
            .fetch_optional(db)
            .await?;
            let (mission_id, roe) =
                row.ok_or_else(|| AppError::NotFound("no such security mission".into()))?;
            if roe.is_none() {
                return Err(AppError::Validation(
                    "that mission has no rules of engagement recorded. Nothing \
                     can be reported against it until it does"
                        .into(),
                ));
            }
            Ok((Some(mission_id), None, input.target_host.clone()))
        }
        "project" => {
            let slug = input.project_slug.as_deref().ok_or_else(|| {
                AppError::Validation("a report against a project names the project".into())
            })?;
            let row: Option<(Uuid, bool)> =
                sqlx::query_as("SELECT id, bug_bounty_open FROM projects WHERE slug = $1")
                    .bind(slug)
                    .fetch_optional(db)
                    .await?;
            let (project_id, open) =
                row.ok_or_else(|| AppError::NotFound("no such project".into()))?;
            if !open {
                return Err(AppError::Validation(
                    "that project has not opened itself to security reports. \
                     `bug_bounty_open` is how a project says it accepts them"
                        .into(),
                ));
            }
            Ok((None, Some(project_id), input.target_host.clone()))
        }
        other => Err(AppError::Validation(format!(
            "'{other}' is not a target kind — platform, mission or project"
        ))),
    }
}

// ═══════════════════════════════════════════════════════════════════
// The history
// ═══════════════════════════════════════════════════════════════════

#[allow(clippy::too_many_arguments)]
async fn record_event(
    tx: &mut Transaction<'_, Postgres>,
    finding_id: Uuid,
    actor: Option<Uuid>,
    event: &str,
    from_status: Option<&str>,
    to_status: Option<&str>,
    reason: Option<&str>,
    detail: Option<serde_json::Value>,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO security_finding_events
             (finding_id, actor_user_id, event, from_status, to_status, reason, detail)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(finding_id)
    .bind(actor)
    .bind(event)
    .bind(from_status)
    .bind(to_status)
    .bind(reason)
    .bind(detail)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════
// Transitions
// ═══════════════════════════════════════════════════════════════════

/// What a transition may carry with it.
#[derive(Debug, Default, serde::Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct TransitionInput {
    pub to: String,
    #[serde(default)]
    pub reason: Option<String>,
    /// Required to move to `fixed`.
    #[serde(default)]
    pub fix_url: Option<String>,
    /// Required to move to `published`.
    #[serde(default)]
    pub writeup_url: Option<String>,
    /// Required to move to `duplicate`.
    #[serde(default)]
    pub duplicate_of: Option<Uuid>,
    /// Recorded on a triage.
    #[serde(default)]
    pub triage_notes_md: Option<String>,
}

/// Move a finding, and do whatever the new state entails.
///
/// Returns the new status. The caller has already established which `Actor` the
/// requester is; this function decides whether that actor may make this move.
pub async fn transition(
    db: &PgPool,
    actor_id: Uuid,
    actor: Actor,
    finding_id: Uuid,
    input: TransitionInput,
) -> Result<String, AppError> {
    // The row is locked for the whole decision, not read and then locked: two
    // reviewers pressing confirm at the same moment would otherwise both see
    // `triaged`, both pass the transition check, and both insert.
    let mut tx = db.begin().await?;

    let current: Option<(String, Uuid, String)> = sqlx::query_as(
        "SELECT status, reporter_user_id, severity_tier
           FROM security_findings WHERE id = $1 FOR UPDATE",
    )
    .bind(finding_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((from, reporter, severity)) = current else {
        return Err(AppError::NotFound("no such finding".into()));
    };

    // A reporter is only ever the reporter of their own report.
    if actor == Actor::Reporter && actor_id != reporter {
        return Err(AppError::Forbidden);
    }

    if !allowed_transition(actor, &from, &input.to) {
        return Err(AppError::Conflict(format!(
            "a finding cannot go from {from} to {} here",
            input.to
        )));
    }

    match input.to.as_str() {
        "triaged" => {
            sqlx::query(
                "UPDATE security_findings
                    SET status = 'triaged', triaged_by_user_id = $2,
                        triaged_at = NOW(), triage_notes_md = $3
                  WHERE id = $1",
            )
            .bind(finding_id)
            .bind(actor_id)
            .bind(input.triage_notes_md.as_deref())
            .execute(&mut *tx)
            .await?;
        }
        "not_applicable" => {
            let reason = input.reason.as_deref().ok_or_else(|| {
                AppError::Validation(
                    "a refusal says why. A reporter who is told 'no' and not why \
                     files the same report again"
                        .into(),
                )
            })?;
            sqlx::query(
                "UPDATE security_findings
                    SET status = 'not_applicable',
                        triaged_by_user_id = COALESCE(triaged_by_user_id, $2),
                        triaged_at = COALESCE(triaged_at, NOW()),
                        triage_notes_md = COALESCE(triage_notes_md, $3)
                  WHERE id = $1",
            )
            .bind(finding_id)
            .bind(actor_id)
            .bind(reason)
            .execute(&mut *tx)
            .await?;
        }
        "withdrawn" => {
            sqlx::query("UPDATE security_findings SET status = 'withdrawn' WHERE id = $1")
                .bind(finding_id)
                .execute(&mut *tx)
                .await?;
        }
        "confirmed" => {
            confirm(&mut tx, finding_id, reporter, actor_id, &severity).await?;
        }
        "duplicate" => {
            let original = input.duplicate_of.ok_or_else(|| {
                AppError::Validation("a duplicate names the finding it duplicates".into())
            })?;
            if original == finding_id {
                return Err(AppError::Validation(
                    "a finding cannot duplicate itself".into(),
                ));
            }
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM security_findings WHERE id = $1)",
            )
            .bind(original)
            .fetch_one(&mut *tx)
            .await?;
            if !exists {
                return Err(AppError::NotFound("no such original finding".into()));
            }
            sqlx::query(
                "UPDATE security_findings
                    SET status = 'duplicate', dedup_state = 'duplicate_confirmed',
                        duplicate_of_finding_id = $2,
                        dedup_reviewed_by_user_id = $3, dedup_reviewed_at = NOW()
                  WHERE id = $1",
            )
            .bind(finding_id)
            .bind(original)
            .bind(actor_id)
            .execute(&mut *tx)
            .await?;
        }
        "fixed" => {
            let url = input.fix_url.as_deref().ok_or_else(|| {
                AppError::Validation("a fix says where it landed".into())
            })?;
            if !url.starts_with("https://") {
                return Err(AppError::Validation(
                    "the fix link has to be an https link somebody can open".into(),
                ));
            }
            sqlx::query(
                "UPDATE security_findings
                    SET status = 'fixed', fix_url = $2, fixed_at = NOW(),
                        vendor_patch_confirmed_at =
                            COALESCE(vendor_patch_confirmed_at, NOW())
                  WHERE id = $1",
            )
            .bind(finding_id)
            .bind(url)
            .execute(&mut *tx)
            .await?;
        }
        "published" => {
            let url = input.writeup_url.as_deref().ok_or_else(|| {
                AppError::Validation(
                    "publication needs a write-up. The point of the last \
                     transition is that somebody can read what happened"
                        .into(),
                )
            })?;
            sqlx::query(
                "UPDATE security_findings
                    SET status = 'published', published_at = NOW(),
                        writeup_url = $2, disclosure_stage = 'public'
                  WHERE id = $1",
            )
            .bind(finding_id)
            .bind(url)
            .execute(&mut *tx)
            .await?;
        }
        other => {
            return Err(AppError::Validation(format!("'{other}' is not a status")));
        }
    }

    record_event(
        &mut tx,
        finding_id,
        Some(actor_id),
        "transition",
        Some(&from),
        Some(&input.to),
        input.reason.as_deref(),
        None,
    )
    .await?;

    tx.commit().await?;

    // Attestations after the commit: the transaction above is the one that
    // must not fail, and issuing is idempotent so a failure here is recovered
    // by the next sweep rather than losing the transition.
    if let Err(e) = crate::services::security_attestations::issue_for_finding(db, finding_id).await
    {
        tracing::warn!(finding = %finding_id, error = %e,
            "finding moved but its attestation was not issued");
    }

    metrics::counter!("skilluv_security_finding_transitions_total",
        "to" => input.to.clone())
    .increment(1);

    Ok(input.to)
}

/// Confirmation: the four things that happen when somebody reproduces a
/// finding.
async fn confirm(
    tx: &mut Transaction<'_, Postgres>,
    finding_id: Uuid,
    reporter: Uuid,
    confirmer: Uuid,
    severity: &str,
) -> Result<(), AppError> {
    let policy_days: i16 = sqlx::query_scalar(
        "UPDATE security_findings
            SET status = 'confirmed',
                disclosure_stage = COALESCE(disclosure_stage, 'embargoed'),
                embargo_ends_at = COALESCE(
                    embargo_ends_at,
                    NOW() + make_interval(days => disclosure_policy_days::INT))
          WHERE id = $1
        RETURNING disclosure_policy_days",
    )
    .bind(finding_id)
    .fetch_one(&mut **tx)
    .await?;
    let _ = policy_days;

    let fragments = fragments_for(severity);

    // The deliverable. This is what makes a vulnerability count towards a rank
    // exactly as a merged contribution does — see the module header, and F-06.
    let inserted: Option<Uuid> = sqlx::query_scalar(
        r#"
        INSERT INTO deliverables (
            security_finding_id, user_id, artifact_type, artifact_url,
            verifiable_by, verification_status, verified_at, verified_by_user_id,
            fragments_awarded, credits_awarded, public, submitted_at, created_at
        )
        SELECT $1, $2, 'disclosure',
               $3 || '/security/findings/' || $1::TEXT,
               'human_review', 'verified', NOW(), $4,
               $5, 0, TRUE, sf.created_at, NOW()
          FROM security_findings sf WHERE sf.id = $1
        ON CONFLICT DO NOTHING
        RETURNING id
        "#,
    )
    .bind(finding_id)
    .bind(reporter)
    .bind(crate::config::PUBLIC_SITE_URL)
    .bind(confirmer)
    .bind(fragments)
    .fetch_optional(&mut **tx)
    .await?;

    // Fragments only when the deliverable is new. A second confirmation — a
    // status corrected and re-applied — must not pay twice.
    if inserted.is_some() && fragments > 0 {
        sqlx::query(
            "UPDATE users SET total_fragments = total_fragments + $1, updated_at = NOW()
              WHERE id = $2",
        )
        .bind(fragments)
        .bind(reporter)
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════
// Severity
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SeverityOverride {
    /// A vector, preferred: it says which metric the reviewer disagrees about.
    #[serde(default)]
    pub cvss_vector: Option<String>,
    #[serde(default)]
    pub severity_tier: Option<String>,
    pub reason: String,
}

/// Change the severity of a finding, on the record.
///
/// The reported tier is never overwritten — migration 0547 keeps it — so the
/// disagreement stays readable. A reason is required by the database as well as
/// here: an unexplained override is what researchers leave a platform over.
pub async fn override_severity(
    db: &PgPool,
    actor_id: Uuid,
    finding_id: Uuid,
    input: SeverityOverride,
) -> Result<String, AppError> {
    if input.reason.trim().chars().count() < 20 {
        return Err(AppError::Validation(
            "say why, in at least twenty characters. A severity changed without \
             an argument is the thing this whole flow exists to avoid"
                .into(),
        ));
    }

    let (vector, score, tier) = match input.cvss_vector.as_deref() {
        Some(raw) => {
            let scored = cvss::score_vector(raw).map_err(AppError::Validation)?;
            (Some(scored.vector), Some(scored.score), scored.tier.to_string())
        }
        None => {
            let tier = input.severity_tier.as_deref().ok_or_else(|| {
                AppError::Validation("a vector or a tier — one of the two".into())
            })?;
            if !matches!(
                tier,
                "critical" | "high" | "medium" | "low" | "informational"
            ) {
                return Err(AppError::Validation("not a severity tier".into()));
            }
            (None, None, tier.to_string())
        }
    };

    let mut tx = db.begin().await?;

    let before: Option<String> = sqlx::query_scalar(
        "SELECT severity_tier FROM security_findings WHERE id = $1 FOR UPDATE",
    )
    .bind(finding_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(before) = before else {
        return Err(AppError::NotFound("no such finding".into()));
    };

    sqlx::query(
        "UPDATE security_findings
            SET severity_tier = $2,
                cvss_vector = COALESCE($3, cvss_vector),
                cvss_score = COALESCE($4, cvss_score),
                severity_final_by_user_id = $5,
                severity_override_reason = $6
          WHERE id = $1",
    )
    .bind(finding_id)
    .bind(&tier)
    .bind(vector.as_deref())
    .bind(score.map(|s| bigdecimal::BigDecimal::try_from(s).unwrap_or_default()))
    .bind(actor_id)
    .bind(input.reason.trim())
    .execute(&mut *tx)
    .await?;

    record_event(
        &mut tx,
        finding_id,
        Some(actor_id),
        "severity_changed",
        None,
        None,
        Some(input.reason.trim()),
        Some(serde_json::json!({ "from": before, "to": tier })),
    )
    .await?;

    tx.commit().await?;
    Ok(tier)
}

// ═══════════════════════════════════════════════════════════════════
// Rounds
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RoundRequest {
    /// A slug from `revision_round_kinds` — `sec_repro_insufficient` and the
    /// five others migration 0547 seeded.
    pub kind: String,
    pub notes_md: String,
}

/// Ask the researcher for something before deciding.
///
/// Capped at five rounds by the database. After the fifth somebody decides,
/// which is the point of the cap: a report that has been iterated five times
/// and is still not reproducible is a decision, not another round.
pub async fn open_round(
    db: &PgPool,
    actor_id: Uuid,
    finding_id: Uuid,
    input: RoundRequest,
) -> Result<i16, AppError> {
    if input.notes_md.trim().chars().count() < 20 {
        return Err(AppError::Validation(
            "say what is missing, in at least twenty characters".into(),
        ));
    }

    let open: Option<i16> = sqlx::query_scalar(
        "SELECT round_no FROM security_finding_rounds
          WHERE finding_id = $1 AND resolved_at IS NULL
          ORDER BY round_no DESC LIMIT 1",
    )
    .bind(finding_id)
    .fetch_optional(db)
    .await?;
    if let Some(n) = open {
        return Err(AppError::Conflict(format!(
            "round {n} is still open on this finding"
        )));
    }

    // `::SMALLINT` on the way out, because `max(smallint) + 1` is an integer in
    // PostgreSQL and sqlx does not narrow: without the cast the decode fails and
    // the endpoint answers 500 to every first round.
    let next: i16 = sqlx::query_scalar(
        "SELECT (COALESCE(max(round_no), 0) + 1)::SMALLINT
           FROM security_finding_rounds WHERE finding_id = $1",
    )
    .bind(finding_id)
    .fetch_one(db)
    .await?;
    if next > 5 {
        return Err(AppError::Conflict(
            "five rounds is the limit. Decide: confirmed, duplicate, or not \
             applicable"
                .into(),
        ));
    }

    let mut tx = db.begin().await?;
    sqlx::query(
        "INSERT INTO security_finding_rounds
             (finding_id, round_no, kind, requested_by, notes_md)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(finding_id)
    .bind(next)
    .bind(&input.kind)
    .bind(actor_id)
    .bind(input.notes_md.trim())
    .execute(&mut *tx)
    .await?;

    record_event(
        &mut tx,
        finding_id,
        Some(actor_id),
        "round_opened",
        None,
        None,
        Some(input.notes_md.trim()),
        Some(serde_json::json!({ "round": next, "kind": input.kind })),
    )
    .await?;
    tx.commit().await?;

    Ok(next)
}

/// The researcher's answer to the open round.
pub async fn answer_round(
    db: &PgPool,
    reporter: Uuid,
    finding_id: Uuid,
    answer_md: &str,
) -> Result<i16, AppError> {
    if answer_md.trim().is_empty() {
        return Err(AppError::Validation("an answer with something in it".into()));
    }

    let round: Option<(Uuid, i16)> = sqlx::query_as(
        "SELECT r.id, r.round_no
           FROM security_finding_rounds r
           JOIN security_findings f ON f.id = r.finding_id
          WHERE r.finding_id = $1 AND r.resolved_at IS NULL
            AND f.reporter_user_id = $2
          ORDER BY r.round_no DESC LIMIT 1",
    )
    .bind(finding_id)
    .bind(reporter)
    .fetch_optional(db)
    .await?;

    let Some((round_id, round_no)) = round else {
        return Err(AppError::NotFound("no open round of yours".into()));
    };

    let mut tx = db.begin().await?;
    sqlx::query(
        "UPDATE security_finding_rounds
            SET answered_at = NOW(), answer_md = $2 WHERE id = $1",
    )
    .bind(round_id)
    .bind(answer_md.trim())
    .execute(&mut *tx)
    .await?;
    record_event(
        &mut tx,
        finding_id,
        Some(reporter),
        "round_answered",
        None,
        None,
        None,
        Some(serde_json::json!({ "round": round_no })),
    )
    .await?;
    tx.commit().await?;
    Ok(round_no)
}

/// Close the round: satisfied, or not.
pub async fn resolve_round(
    db: &PgPool,
    actor_id: Uuid,
    finding_id: Uuid,
    resolution: &str,
    note: Option<&str>,
) -> Result<(), AppError> {
    if !matches!(resolution, "satisfied" | "insufficient") {
        return Err(AppError::Validation(
            "a round is resolved as satisfied or insufficient".into(),
        ));
    }
    let affected = sqlx::query(
        "UPDATE security_finding_rounds
            SET resolved_at = NOW(), resolved_by = $2, resolution = $3
          WHERE finding_id = $1 AND resolved_at IS NULL AND answered_at IS NOT NULL",
    )
    .bind(finding_id)
    .bind(actor_id)
    .bind(resolution)
    .execute(db)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(AppError::Conflict(
            "no answered round waiting on this finding".into(),
        ));
    }

    let mut tx = db.begin().await?;
    record_event(
        &mut tx,
        finding_id,
        Some(actor_id),
        "round_resolved",
        None,
        None,
        note,
        Some(serde_json::json!({ "resolution": resolution })),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════
// Deduplication
// ═══════════════════════════════════════════════════════════════════

/// Look for findings that resemble this one, and record what was found.
///
/// Two signals, both cheap:
///
///   * the same weakness class on the same endpoint of the same target — which
///     in practice is most real duplicates;
///   * a similar title, by trigram similarity, which catches the case where two
///     people described the same thing in different words.
///
/// Nothing is decided. The candidates are written to the row and a person reads
/// them, because a merge decides who is paid.
pub async fn scan_similar(db: &PgPool, finding_id: Uuid) -> Result<usize, AppError> {
    let rows: Vec<(Uuid, f32)> = sqlx::query_as(
        r#"
        WITH target AS (
            SELECT id, title, cwe_id, affected_endpoint, target_kind,
                   target_host, mission_id, project_id
              FROM security_findings WHERE id = $1
        )
        SELECT f.id,
               GREATEST(
                   similarity(f.title, t.title),
                   CASE WHEN t.cwe_id IS NOT NULL
                             AND f.cwe_id = t.cwe_id
                             AND t.affected_endpoint IS NOT NULL
                             AND f.affected_endpoint = t.affected_endpoint
                        THEN 0.95 ELSE 0 END
               )::REAL AS score
          FROM security_findings f, target t
         WHERE f.id <> t.id
           AND f.status NOT IN ('withdrawn', 'not_applicable')
           AND f.target_kind = t.target_kind
           AND f.target_host IS NOT DISTINCT FROM t.target_host
           AND f.mission_id IS NOT DISTINCT FROM t.mission_id
           AND f.project_id IS NOT DISTINCT FROM t.project_id
           AND (
               similarity(f.title, t.title) > 0.45
               OR (t.cwe_id IS NOT NULL AND f.cwe_id = t.cwe_id
                   AND t.affected_endpoint IS NOT NULL
                   AND f.affected_endpoint = t.affected_endpoint)
           )
         ORDER BY score DESC
         LIMIT 5
        "#,
    )
    .bind(finding_id)
    .fetch_all(db)
    .await?;

    let ids: Vec<Uuid> = rows.iter().map(|(id, _)| *id).collect();
    let scores: Vec<f32> = rows.iter().map(|(_, s)| *s).collect();
    let found = ids.len();

    sqlx::query(
        "UPDATE security_findings
            SET similar_finding_ids = $2, similarity_scores = $3,
                similarity_scanned_at = NOW(),
                dedup_state = CASE
                    WHEN dedup_state = 'original' AND cardinality($2::UUID[]) > 0
                        THEN 'suspected' ELSE dedup_state END
          WHERE id = $1",
    )
    .bind(finding_id)
    .bind(&ids)
    .bind(&scores)
    .execute(db)
    .await?;

    Ok(found)
}

/// Scan whatever has not been scanned. The worker's entry point.
pub async fn sweep_similarity(db: &PgPool, limit: i64) -> Result<usize, AppError> {
    let pending: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM security_findings
          WHERE similarity_scanned_at IS NULL
            AND status IN ('submitted', 'triaged')
          ORDER BY created_at
          LIMIT $1",
    )
    .bind(limit)
    .fetch_all(db)
    .await?;

    let mut done = 0;
    for id in pending {
        match scan_similar(db, id).await {
            Ok(_) => done += 1,
            Err(e) => tracing::warn!(finding = %id, error = %e,
                "similarity scan failed on one finding"),
        }
    }
    Ok(done)
}

// ═══════════════════════════════════════════════════════════════════
// Disclosure
// ═══════════════════════════════════════════════════════════════════

/// Record that the owner of the system has been told.
pub async fn notify_vendor(db: &PgPool, actor_id: Uuid, finding_id: Uuid) -> Result<(), AppError> {
    let mut tx = db.begin().await?;
    let affected = sqlx::query(
        "UPDATE security_findings
            SET vendor_notified_at = COALESCE(vendor_notified_at, NOW())
          WHERE id = $1 AND status IN ('triaged', 'confirmed')",
    )
    .bind(finding_id)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if affected == 0 {
        return Err(AppError::Conflict(
            "only a triaged or confirmed finding is sent to the owner".into(),
        ));
    }
    record_event(
        &mut tx,
        finding_id,
        Some(actor_id),
        "vendor_notified",
        None,
        None,
        None,
        None,
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// The owner asks for more time.
///
/// Requesting and granting are separate calls on purpose: an extension that
/// applied itself would make the embargo a suggestion.
pub async fn request_extension(
    db: &PgPool,
    actor_id: Uuid,
    finding_id: Uuid,
    reason: &str,
) -> Result<(), AppError> {
    let mut tx = db.begin().await?;
    sqlx::query(
        "UPDATE security_findings
            SET disclosure_stage = 'extension_requested',
                extension_requested_at = NOW()
          WHERE id = $1 AND disclosure_stage = 'embargoed'",
    )
    .bind(finding_id)
    .execute(&mut *tx)
    .await?;
    record_event(
        &mut tx,
        finding_id,
        Some(actor_id),
        "extension_requested",
        None,
        None,
        Some(reason),
        None,
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Grant the extension, moving the clock.
pub async fn grant_extension(
    db: &PgPool,
    actor_id: Uuid,
    finding_id: Uuid,
    days: i16,
) -> Result<(), AppError> {
    if !(1..=365).contains(&days) {
        return Err(AppError::Validation(
            "an extension between one and three hundred and sixty-five days".into(),
        ));
    }
    let mut tx = db.begin().await?;
    let affected = sqlx::query(
        "UPDATE security_findings
            SET disclosure_stage = 'embargoed',
                extension_granted_days = COALESCE(extension_granted_days, 0) + $2,
                embargo_ends_at = embargo_ends_at
                    + make_interval(days => $2::INT)
          WHERE id = $1 AND disclosure_stage = 'extension_requested'",
    )
    .bind(finding_id)
    .bind(days)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if affected == 0 {
        return Err(AppError::Conflict(
            "no extension has been requested on this finding".into(),
        ));
    }
    record_event(
        &mut tx,
        finding_id,
        Some(actor_id),
        "extension_granted",
        None,
        None,
        None,
        Some(serde_json::json!({ "days": days })),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Never publish this one, and say why.
pub async fn withhold(
    db: &PgPool,
    actor_id: Uuid,
    finding_id: Uuid,
    reason: &str,
) -> Result<(), AppError> {
    if reason.trim().chars().count() < 20 {
        return Err(AppError::Validation(
            "withholding a disclosure is a decision. Twenty characters of \
             reasoning, minimum"
                .into(),
        ));
    }
    let mut tx = db.begin().await?;
    sqlx::query(
        "UPDATE security_findings
            SET disclosure_stage = 'withheld', withheld_reason = $2
          WHERE id = $1",
    )
    .bind(finding_id)
    .bind(reason.trim())
    .execute(&mut *tx)
    .await?;
    record_event(
        &mut tx,
        finding_id,
        Some(actor_id),
        "withheld",
        None,
        None,
        Some(reason.trim()),
        None,
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// What the embargo worker found.
#[derive(Debug, Default, serde::Serialize)]
pub struct EmbargoSweep {
    /// Findings whose clock has run out. Not published — flagged for an
    /// administrator, because publication is a decision somebody signs.
    pub expired: Vec<Uuid>,
    /// Findings whose clock runs out soon, by how soon.
    pub reminded: Vec<(Uuid, i64)>,
}

/// Days before the end of an embargo that a reminder goes out.
pub const REMINDER_DAYS: [i64; 3] = [30, 7, 1];

/// Walk the embargo clocks.
///
/// Deliberately does **not** publish anything. W-02 proposed an automatic
/// transition to public when the clock expires and the owner has gone quiet.
/// Refused: publishing a vulnerability is irreversible, the internet keeps a
/// copy, and a cron job is the wrong thing to be holding that decision. What
/// expiry produces is an item on an administrator's list, which is the same
/// outcome one working day later and cannot go wrong at three in the morning.
pub async fn sweep_embargoes(db: &PgPool) -> Result<EmbargoSweep, AppError> {
    let mut out = EmbargoSweep::default();

    let expired: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM security_findings
          WHERE disclosure_stage IN ('embargoed', 'extension_requested')
            AND embargo_ends_at <= NOW()
          ORDER BY embargo_ends_at
          LIMIT 200",
    )
    .fetch_all(db)
    .await?;

    for id in &expired {
        let mut tx = db.begin().await?;
        // `partially_disclosed` rather than `public`: the existence and the
        // severity become quotable, the reproduction does not. That is a state
        // a rule may enter; `public` is not.
        sqlx::query(
            "UPDATE security_findings
                SET disclosure_stage = 'partially_disclosed'
              WHERE id = $1 AND disclosure_stage IN ('embargoed', 'extension_requested')",
        )
        .bind(id)
        .execute(&mut *tx)
        .await?;
        record_event(
            &mut tx,
            *id,
            None,
            "embargo_expired",
            None,
            None,
            Some("the embargo ran out; publication is an administrator's decision"),
            None,
        )
        .await?;
        tx.commit().await?;
    }
    out.expired = expired;

    for days in REMINDER_DAYS {
        let due: Vec<Uuid> = sqlx::query_scalar(
            "SELECT id FROM security_findings
              WHERE disclosure_stage = 'embargoed'
                AND embargo_ends_at > NOW()
                AND embargo_ends_at <= NOW() + make_interval(days => $1::INT)
                AND embargo_ends_at > NOW() + make_interval(days => ($1 - 1)::INT)
              LIMIT 200",
        )
        .bind(days as i32)
        .fetch_all(db)
        .await?;
        for id in due {
            out.reminded.push((id, days));
        }
    }

    Ok(out)
}

// ═══════════════════════════════════════════════════════════════════
// Reading
// ═══════════════════════════════════════════════════════════════════

/// The reporter's own list.
pub async fn mine(db: &PgPool, user_id: Uuid) -> Result<Vec<serde_json::Value>, AppError> {
    Ok(sqlx::query_scalar(
        r#"
        SELECT jsonb_build_object(
                   'id', f.id, 'title', f.title, 'status', f.status,
                   'severity_tier', f.severity_tier,
                   'severity_reported_tier', f.severity_reported_tier,
                   'cvss_score', f.cvss_score, 'cwe_id', f.cwe_id,
                   'target_kind', f.target_kind, 'target_host', f.target_host,
                   'disclosure_stage', f.disclosure_stage,
                   'embargo_ends_at', f.embargo_ends_at,
                   'created_at', f.created_at, 'writeup_url', f.writeup_url,
                   'open_round', (
                       SELECT jsonb_build_object('round_no', r.round_no,
                                                 'kind', r.kind,
                                                 'notes_md', r.notes_md)
                         FROM security_finding_rounds r
                        WHERE r.finding_id = f.id AND r.resolved_at IS NULL
                        ORDER BY r.round_no DESC LIMIT 1)
               )
          FROM security_findings f
         WHERE f.reporter_user_id = $1
         ORDER BY f.created_at DESC
         LIMIT 200
        "#,
    )
    .bind(user_id)
    .fetch_all(db)
    .await?)
}

/// The public card: what a stranger may read about a finding.
///
/// Everything that identifies the defect is withheld until publication — no
/// reproduction, no endpoint, no proof. What is shown is what a coordinated
/// disclosure shows from outside: that somebody found something of this
/// severity, in this weakness class, on this date. That is the claim an
/// attestation on this finding is making, so it has to be readable.
pub async fn public_card(db: &PgPool, finding_id: Uuid) -> Result<serde_json::Value, AppError> {
    let card: Option<serde_json::Value> = sqlx::query_scalar(
        r#"
        SELECT jsonb_build_object(
                   'id', f.id,
                   'title', CASE WHEN f.status = 'published' THEN f.title
                                 ELSE NULL END,
                   'status', f.status,
                   'severity_tier', f.severity_tier,
                   'cvss_score', f.cvss_score,
                   'cwe_id', f.cwe_id,
                   'confirmed_at', (SELECT min(e.occurred_at)
                                      FROM security_finding_events e
                                     WHERE e.finding_id = f.id
                                       AND e.to_status = 'confirmed'),
                   'published_at', f.published_at,
                   'writeup_url', f.writeup_url,
                   'disclosure_stage', f.disclosure_stage,
                   'reporter', CASE
                       WHEN f.reporter_is_anonymous
                           THEN jsonb_build_object('alias', 'anonymous-' ||
                                    substr(md5(f.reporter_user_id::TEXT), 1, 6))
                       ELSE jsonb_build_object('username', u.username,
                                               'display_name', u.display_name)
                       END,
                   'description_md', CASE WHEN f.status = 'published'
                                          THEN f.description_md ELSE NULL END
               )
          FROM security_findings f
          JOIN users u ON u.id = f.reporter_user_id
         WHERE f.id = $1
           AND f.status IN ('confirmed', 'fixed', 'published')
        "#,
    )
    .bind(finding_id)
    .fetch_optional(db)
    .await?;

    card.ok_or_else(|| AppError::NotFound("no such published finding".into()))
}

/// The hall of fame (T-05), and the trust-centre figures (T-10) it also
/// answers.
///
/// One query set rather than two endpoints reading the same rows differently,
/// which is how two pages come to quote different numbers.
pub async fn hall_of_fame(db: &PgPool) -> Result<serde_json::Value, AppError> {
    let contributors: Vec<serde_json::Value> = sqlx::query_scalar(
        r#"
        SELECT jsonb_build_object(
                   'reporter', CASE
                       WHEN bool_and(f.reporter_is_anonymous)
                           THEN jsonb_build_object('alias', 'anonymous-' ||
                                    substr(md5(f.reporter_user_id::TEXT), 1, 6))
                       ELSE jsonb_build_object('username', u.username,
                                               'display_name', u.display_name,
                                               'avatar_url', u.avatar_url)
                       END,
                   'findings', count(*),
                   'top_severity', max(CASE f.severity_tier
                                          WHEN 'critical' THEN 5
                                          WHEN 'high' THEN 4
                                          WHEN 'medium' THEN 3
                                          WHEN 'low' THEN 2 ELSE 1 END),
                   'first_finding_at', min(f.created_at),
                   'rank', (SELECT r.rank FROM user_ranks r
                             WHERE r.user_id = f.reporter_user_id)
               )
          FROM security_findings f
          JOIN users u ON u.id = f.reporter_user_id
         WHERE f.status IN ('confirmed', 'fixed', 'published')
           AND f.dedup_state <> 'duplicate_confirmed'
         GROUP BY f.reporter_user_id, u.username, u.display_name, u.avatar_url
         ORDER BY count(*) DESC, min(f.created_at) ASC
         LIMIT 50
        "#,
    )
    .fetch_all(db)
    .await?;

    let recent: Vec<serde_json::Value> = sqlx::query_scalar(
        r#"
        SELECT jsonb_build_object(
                   'id', f.id, 'title', f.title,
                   'severity_tier', f.severity_tier,
                   'published_at', f.published_at,
                   'writeup_url', f.writeup_url,
                   'reporter', CASE WHEN f.reporter_is_anonymous
                       THEN jsonb_build_object('alias', 'anonymous-' ||
                                substr(md5(f.reporter_user_id::TEXT), 1, 6))
                       ELSE jsonb_build_object('username', u.username) END
               )
          FROM security_findings f
          JOIN users u ON u.id = f.reporter_user_id
         WHERE f.status = 'published'
         ORDER BY f.published_at DESC
         LIMIT 20
        "#,
    )
    .fetch_all(db)
    .await?;

    let stats: serde_json::Value = sqlx::query_scalar(
        r#"
        SELECT jsonb_build_object(
                   'confirmed', count(*) FILTER (
                       WHERE status IN ('confirmed', 'fixed', 'published')),
                   'published', count(*) FILTER (WHERE status = 'published'),
                   'fixed', count(*) FILTER (WHERE status IN ('fixed', 'published')),
                   'by_severity', (
                       SELECT jsonb_object_agg(severity_tier, n) FROM (
                           SELECT severity_tier, count(*) AS n
                             FROM security_findings
                            WHERE status IN ('confirmed', 'fixed', 'published')
                            GROUP BY severity_tier) s),
                   'median_days_to_publication', (
                       SELECT round(percentile_cont(0.5) WITHIN GROUP (
                                  ORDER BY EXTRACT(EPOCH FROM
                                      (published_at - created_at)) / 86400)::NUMERIC,
                              1)
                         FROM security_findings WHERE published_at IS NOT NULL),
                   'reporters', (
                       SELECT count(DISTINCT reporter_user_id)
                         FROM security_findings
                        WHERE status IN ('confirmed', 'fixed', 'published'))
               )
          FROM security_findings
        "#,
    )
    .fetch_one(db)
    .await?;

    Ok(serde_json::json!({
        "top_contributors": contributors,
        "recent_findings": recent,
        "stats": stats,
        "scope": scope_hosts(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scope_defaults_to_the_published_hosts() {
        let hosts = parse_scope(None);
        assert!(hosts.contains(&"staging.skill-uv.com".to_string()));
        assert!(hosts.contains(&"api.skill-uv.com".to_string()));
        // And nothing that is not ours: a scope that reached somebody else's
        // host would be an invitation this platform cannot issue.
        assert!(hosts.iter().all(|h| h.ends_with("skill-uv.com")));
    }

    #[test]
    fn an_overridden_scope_is_normalised() {
        let hosts = parse_scope(Some(" Staging.Example.COM , , other.example.com "));
        assert_eq!(hosts, vec!["staging.example.com", "other.example.com"]);
        // An empty override falls back rather than emptying the scope, which
        // would refuse every report.
        assert_eq!(parse_scope(Some("   ")), parse_scope(None));
    }

    #[test]
    fn fragments_make_volume_pointless() {
        // The editorial position of the scale: no number of informational
        // findings adds up to one critical.
        assert!(fragments_for("critical") > fragments_for("high") * 3);
        assert!(fragments_for("high") > fragments_for("medium") * 3);
        assert_eq!(fragments_for("something_else"), fragments_for("informational"));
    }

    #[test]
    fn a_reporter_can_withdraw_and_nothing_else() {
        assert!(allowed_transition(Actor::Reporter, "submitted", "withdrawn"));
        assert!(allowed_transition(Actor::Reporter, "triaged", "withdrawn"));
        assert!(!allowed_transition(Actor::Reporter, "submitted", "triaged"));
        assert!(!allowed_transition(Actor::Reporter, "triaged", "confirmed"));
        assert!(!allowed_transition(Actor::Reporter, "confirmed", "published"));
    }

    #[test]
    fn a_triager_does_not_confirm() {
        // Triage decides whether something is worth a reviewer's afternoon.
        // Confirming asserts publicly that a vulnerability is real.
        assert!(allowed_transition(Actor::Triager, "submitted", "triaged"));
        assert!(allowed_transition(Actor::Triager, "submitted", "not_applicable"));
        assert!(!allowed_transition(Actor::Triager, "triaged", "confirmed"));
        assert!(!allowed_transition(Actor::Triager, "triaged", "duplicate"));
    }

    #[test]
    fn only_an_administrator_publishes() {
        assert!(allowed_transition(Actor::Admin, "fixed", "published"));
        assert!(allowed_transition(Actor::Admin, "confirmed", "published"));
        assert!(!allowed_transition(Actor::Reviewer, "fixed", "published"));
        assert!(!allowed_transition(Actor::Triager, "fixed", "published"));
    }

    #[test]
    fn nothing_skips_the_middle() {
        for actor in [Actor::Reporter, Actor::Triager, Actor::Reviewer, Actor::Admin] {
            assert!(
                !allowed_transition(actor, "submitted", "published"),
                "{actor:?} must not publish an untriaged report"
            );
            assert!(!allowed_transition(actor, "submitted", "confirmed"));
            assert!(!allowed_transition(actor, "submitted", "fixed"));
        }
    }

    #[test]
    fn a_finished_finding_does_not_move_again() {
        for from in ["published", "withdrawn", "not_applicable", "duplicate"] {
            for to in ["triaged", "confirmed", "fixed", "published"] {
                assert!(
                    !allowed_transition(Actor::Admin, from, to),
                    "{from} -> {to} should be closed"
                );
            }
        }
    }

    #[test]
    fn triage_is_skipped_by_record_not_by_seniority_alone() {
        assert_eq!(triage_skip_reason(Some("doyen"), 0), Some("reporter_rank"));
        assert_eq!(triage_skip_reason(Some("maitre"), 0), Some("reporter_rank"));
        // An artisan needs the record.
        assert_eq!(triage_skip_reason(Some("artisan"), 4), None);
        assert_eq!(
            triage_skip_reason(Some("artisan"), 5),
            Some("reporter_track_record")
        );
        // And a beginner always goes through triage, however many they file.
        assert_eq!(triage_skip_reason(Some("apprenti"), 50), None);
        assert_eq!(triage_skip_reason(None, 50), None);
    }
}
