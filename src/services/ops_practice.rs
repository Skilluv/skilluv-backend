//! The ops domain: service objectives, incidents, cost work.
//!
//! Three things an ops contributor is actually judged on, and none of them is
//! a pull request. Each is recorded as a claim somebody else can dispute:
//!
//!   * an objective states a target and a window, and closing it states what
//!     was achieved and where the figure came from;
//!   * an incident carries the two durations every review starts from, and a
//!     post-mortem with a body rather than a heading;
//!   * a cost reduction states both figures and whether the service still
//!     works, because a saving that broke it is an outage with a spreadsheet.
//!
//! ## Blameless is a constraint, not a value
//!
//! There is no column for who caused an incident, and the service offers no
//! way to add one. A post-mortem that names a person is one nobody writes
//! honestly the second time, and the honest second one is the whole point.

use bigdecimal::BigDecimal;
use bigdecimal::num_traits::ToPrimitive;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

/// The five families of ops review. Mirrors `orientations.reviewer_group`.
pub const REVIEWER_GROUPS: &[&str] = &["infra", "reliability", "cloud", "observability", "data"];

pub const SUBTYPES: &[&str] = &[
    "iac_terraform",
    "kubernetes_manifests",
    "cicd_pipeline",
    "observability_config",
    "runbook_incident",
    "db_migration_scheme",
];

pub const SEVERITIES: &[&str] = &["sev1", "sev2", "sev3", "sev4"];

/// Whether a closed window met what was promised.
///
/// Compared at the stated precision rather than rounded: 99.94 against a
/// 99.95 target is a miss, and rounding it to two figures would turn every
/// near miss into a pass.
pub fn objective_met(target: &BigDecimal, achieved: &BigDecimal) -> bool {
    achieved >= target
}

/// How much of an error budget a window consumed, as a percentage of the
/// budget rather than of the total time.
///
/// The number an ops team actually steers on: 100% means the budget is gone,
/// and above 100% means the objective was missed. Uptime alone hides how
/// close a pass was.
pub fn error_budget_consumed(target: &BigDecimal, achieved: &BigDecimal) -> Option<f64> {
    let target = target.to_f64()?;
    let achieved = achieved.to_f64()?;
    let budget = 100.0 - target;
    if budget <= 0.0 {
        // A hundred per cent target has no budget. Any failure exhausts it.
        return Some(if achieved >= 100.0 {
            0.0
        } else {
            f64::INFINITY
        });
    }
    Some(((100.0 - achieved) / budget * 100.0).max(0.0))
}

/// What a cost change actually saved, over a year.
///
/// Annual rather than monthly because that is the figure a decision was made
/// against, and stating the monthly one alone makes a large piece of work
/// look small.
pub fn annual_saving(before: &BigDecimal, after: &BigDecimal) -> BigDecimal {
    ((before - after) * BigDecimal::from(12)).with_scale_round(2, bigdecimal::RoundingMode::Down)
}

/// The share of a bill removed.
pub fn reduction_percent(before: &BigDecimal, after: &BigDecimal) -> Option<f64> {
    let before = before.to_f64()?;
    let after = after.to_f64()?;
    if before <= 0.0 {
        return None;
    }
    Some(((before - after) / before * 100.0).clamp(0.0, 100.0))
}

// ═══════════════════════════════════════════════════════════════════
// Service objectives
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Objective {
    pub id: Uuid,
    pub owner_user_id: Uuid,
    pub service_name: String,
    pub target_percent: BigDecimal,
    pub window_days: i16,
    pub achieved_percent: Option<BigDecimal>,
    pub evidence_url: Option<String>,
    pub started_on: chrono::NaiveDate,
    pub closed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub verified_at: Option<chrono::DateTime<chrono::Utc>>,
}

const OBJECTIVE_SELECT: &str = r#"
    SELECT id, owner_user_id, service_name, target_percent, window_days,
           achieved_percent, evidence_url, started_on, closed_at, verified_at
      FROM ops_service_objectives
"#;

#[derive(Debug, Clone, Deserialize)]
pub struct ObjectiveInput {
    pub service_name: String,
    pub target_percent: BigDecimal,
    pub window_days: i16,
    #[serde(default)]
    pub slice_id: Option<Uuid>,
    #[serde(default)]
    pub project_id: Option<Uuid>,
    #[serde(default)]
    pub started_on: Option<chrono::NaiveDate>,
}

pub async fn declare_objective(
    db: &PgPool,
    owner: Uuid,
    input: ObjectiveInput,
) -> Result<Objective, AppError> {
    if input.slice_id.is_none() && input.project_id.is_none() {
        return Err(AppError::Validation(
            "an objective belongs to a slice or a project — a target floating on its \
             own is a promise about nothing"
                .into(),
        ));
    }

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO ops_service_objectives
            (slice_id, project_id, owner_user_id, service_name, target_percent,
             window_days, started_on)
         VALUES ($1,$2,$3,$4,$5,$6,COALESCE($7, CURRENT_DATE))
         RETURNING id",
    )
    .bind(input.slice_id)
    .bind(input.project_id)
    .bind(owner)
    .bind(input.service_name.trim())
    .bind(&input.target_percent)
    .bind(input.window_days)
    .bind(input.started_on)
    .fetch_one(db)
    .await?;

    objective(db, id).await
}

pub async fn objective(db: &PgPool, id: Uuid) -> Result<Objective, AppError> {
    let sql = format!("{OBJECTIVE_SELECT} WHERE id = $1");
    sqlx::query_as::<_, Objective>(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NotFound("objective not found".into()))
}

pub async fn objectives_for(db: &PgPool, owner: Uuid) -> Result<Vec<Objective>, AppError> {
    let sql = format!("{OBJECTIVE_SELECT} WHERE owner_user_id = $1 ORDER BY started_on DESC");
    let rows = sqlx::query_as::<_, Objective>(sqlx::AssertSqlSafe(sql))
        .bind(owner)
        .fetch_all(db)
        .await?;
    Ok(rows)
}

/// Close a window with what actually happened.
///
/// Both the figure and its source, together. A closed window with one and not
/// the other produces an attestation resting on nothing.
pub async fn close_objective(
    db: &PgPool,
    id: Uuid,
    owner: Uuid,
    achieved: BigDecimal,
    evidence_url: &str,
) -> Result<(Objective, bool), AppError> {
    if !evidence_url.starts_with("https://") {
        return Err(AppError::Validation(
            "point at where the figure comes from, over https. A number with no source \
             is a claim."
                .into(),
        ));
    }

    let done = sqlx::query(
        "UPDATE ops_service_objectives
            SET achieved_percent = $3, evidence_url = $4, closed_at = NOW()
          WHERE id = $1 AND owner_user_id = $2 AND closed_at IS NULL",
    )
    .bind(id)
    .bind(owner)
    .bind(&achieved)
    .bind(evidence_url.trim())
    .execute(db)
    .await?;

    if done.rows_affected() == 0 {
        return Err(AppError::NotFound(
            "no open objective of yours with that id".into(),
        ));
    }

    let objective = objective(db, id).await?;
    let met = objective_met(&objective.target_percent, &achieved);
    Ok((objective, met))
}

/// A reviewer confirms a closed window, and a met objective earns its
/// attestation.
pub async fn verify_objective(db: &PgPool, id: Uuid, reviewer: Uuid) -> Result<bool, AppError> {
    let objective = objective(db, id).await?;
    let Some(achieved) = objective.achieved_percent.clone() else {
        return Err(AppError::Validation(
            "this window has not been closed yet".into(),
        ));
    };

    sqlx::query(
        "UPDATE ops_service_objectives SET verified_by = $2, verified_at = NOW()
          WHERE id = $1",
    )
    .bind(id)
    .bind(reviewer)
    .execute(db)
    .await?;

    let met = objective_met(&objective.target_percent, &achieved);
    if !met {
        // A missed objective is still worth recording — it is what an error
        // budget is for — and it earns nothing.
        return Ok(false);
    }

    issue_ops_attestation(
        db,
        objective.owner_user_id,
        "ops_uptime_achievement",
        &format!("Objectif tenu — {}", objective.service_name),
        &format!(
            "{} a tenu {}% sur {} jours, pour un objectif de {}%.",
            objective.service_name, achieved, objective.window_days, objective.target_percent
        ),
    )
    .await?;

    Ok(true)
}

// ═══════════════════════════════════════════════════════════════════
// Incidents
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Incident {
    pub id: Uuid,
    pub project_id: Option<Uuid>,
    pub commander_user_id: Uuid,
    pub title: String,
    pub severity: String,
    pub time_to_detect_minutes: Option<i32>,
    pub time_to_resolve_minutes: Option<i32>,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub resolved_at: Option<chrono::DateTime<chrono::Utc>>,
    pub postmortem_published_at: Option<chrono::DateTime<chrono::Utc>>,
}

const INCIDENT_SELECT: &str = r#"
    SELECT id, project_id, commander_user_id, title, severity,
           time_to_detect_minutes, time_to_resolve_minutes, started_at,
           resolved_at, postmortem_published_at
      FROM ops_incidents
"#;

#[derive(Debug, Clone, Deserialize)]
pub struct IncidentInput {
    pub title: String,
    pub severity: String,
    pub started_at: chrono::DateTime<chrono::Utc>,
    #[serde(default)]
    pub project_id: Option<Uuid>,
}

pub async fn open_incident(
    db: &PgPool,
    commander: Uuid,
    input: IncidentInput,
) -> Result<Incident, AppError> {
    if !SEVERITIES.contains(&input.severity.as_str()) {
        return Err(AppError::Validation(format!(
            "severity must be one of: {}",
            SEVERITIES.join(", ")
        )));
    }

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO ops_incidents
            (project_id, commander_user_id, title, severity, started_at)
         VALUES ($1,$2,$3,$4,$5)
         RETURNING id",
    )
    .bind(input.project_id)
    .bind(commander)
    .bind(input.title.trim())
    .bind(&input.severity)
    .bind(input.started_at)
    .fetch_one(db)
    .await?;

    incident(db, id).await
}

pub async fn incident(db: &PgPool, id: Uuid) -> Result<Incident, AppError> {
    let sql = format!("{INCIDENT_SELECT} WHERE id = $1");
    sqlx::query_as::<_, Incident>(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NotFound("incident not found".into()))
}

pub async fn incidents_for(db: &PgPool, commander: Uuid) -> Result<Vec<Incident>, AppError> {
    let sql = format!("{INCIDENT_SELECT} WHERE commander_user_id = $1 ORDER BY started_at DESC");
    let rows = sqlx::query_as::<_, Incident>(sqlx::AssertSqlSafe(sql))
        .bind(commander)
        .fetch_all(db)
        .await?;
    Ok(rows)
}

/// Resolve it, with the two durations every review starts from.
pub async fn resolve_incident(
    db: &PgPool,
    id: Uuid,
    commander: Uuid,
    detect_minutes: Option<i32>,
    resolve_minutes: Option<i32>,
) -> Result<Incident, AppError> {
    let done = sqlx::query(
        "UPDATE ops_incidents
            SET resolved_at = NOW(), time_to_detect_minutes = $3,
                time_to_resolve_minutes = $4
          WHERE id = $1 AND commander_user_id = $2 AND resolved_at IS NULL",
    )
    .bind(id)
    .bind(commander)
    .bind(detect_minutes)
    .bind(resolve_minutes)
    .execute(db)
    .await?;

    if done.rows_affected() == 0 {
        return Err(AppError::NotFound(
            "no open incident of yours with that id".into(),
        ));
    }
    incident(db, id).await
}

/// Add an action item.
pub async fn add_action(
    db: &PgPool,
    incident_id: Uuid,
    description: &str,
    owner: Option<Uuid>,
    due_on: Option<chrono::NaiveDate>,
) -> Result<Uuid, AppError> {
    if description.trim().is_empty() {
        return Err(AppError::Validation("say what will be done".into()));
    }

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO ops_incident_actions (incident_id, description, owner_user_id, due_on)
         VALUES ($1,$2,$3,$4)
         RETURNING id",
    )
    .bind(incident_id)
    .bind(description.trim())
    .bind(owner)
    .bind(due_on)
    .fetch_one(db)
    .await?;

    Ok(id)
}

/// Publish the post-mortem, and earn the attestation.
///
/// Refused for an incident with no action items: a post-mortem that concludes
/// nothing needs doing has either found a system that cannot fail again, or
/// has not looked.
pub async fn publish_postmortem(
    db: &PgPool,
    id: Uuid,
    commander: Uuid,
    postmortem_md: &str,
    url: Option<&str>,
) -> Result<(), AppError> {
    if postmortem_md.trim().len() < 200 {
        return Err(AppError::Validation(
            "two hundred characters is the floor. A post-mortem shorter than that is a \
             heading, and the second occurrence of the same incident is what it costs."
                .into(),
        ));
    }
    if postmortem_md.to_lowercase().contains("blame") {
        // A soft check, not a filter: the word usually appears in "blameless"
        // and this is not the place to police prose. The real guard is that
        // there is no column to name anybody in.
        tracing::debug!(incident = %id, "post-mortem mentions blame");
    }

    let actions: i64 =
        sqlx::query_scalar("SELECT count(*) FROM ops_incident_actions WHERE incident_id = $1")
            .bind(id)
            .fetch_one(db)
            .await?;

    if actions == 0 {
        return Err(AppError::Validation(
            "no action items. A post-mortem that concludes nothing needs doing has \
             either found a system that cannot fail again, or has not looked."
                .into(),
        ));
    }

    let done = sqlx::query(
        "UPDATE ops_incidents
            SET postmortem_md = $3, postmortem_url = $4,
                postmortem_published_at = NOW()
          WHERE id = $1 AND commander_user_id = $2 AND resolved_at IS NOT NULL",
    )
    .bind(id)
    .bind(commander)
    .bind(postmortem_md.trim())
    .bind(url.map(str::trim).filter(|u| u.starts_with("https://")))
    .execute(db)
    .await?;

    if done.rows_affected() == 0 {
        return Err(AppError::NotFound(
            "no resolved incident of yours with that id".into(),
        ));
    }

    let incident = incident(db, id).await?;
    issue_ops_attestation(
        db,
        commander,
        "ops_incident_led",
        &format!("Incident conduit — {}", incident.title),
        &format!(
            "Incident {} conduit et post-mortem publié, avec {actions} action(s) de \
             suivi enregistrée(s).",
            incident.severity
        ),
    )
    .await?;

    Ok(())
}

/// Action items that were promised and are late.
///
/// The query that makes the difference between a post-mortem practice and a
/// post-mortem archive.
pub async fn overdue_actions(db: &PgPool) -> Result<Vec<serde_json::Value>, AppError> {
    let rows = sqlx::query_scalar(
        "SELECT jsonb_build_object(
                    'incident_id', a.incident_id,
                    'incident', i.title,
                    'severity', i.severity,
                    'action', a.description,
                    'due_on', a.due_on,
                    'owner', a.owner_user_id
                )
           FROM ops_incident_actions a
           JOIN ops_incidents i ON i.id = a.incident_id
          WHERE a.done_at IS NULL AND a.abandoned_reason IS NULL
            AND a.due_on IS NOT NULL AND a.due_on < CURRENT_DATE
          ORDER BY a.due_on",
    )
    .fetch_all(db)
    .await?;
    Ok(rows)
}

// ═══════════════════════════════════════════════════════════════════
// Cost work
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct CostWork {
    pub id: Uuid,
    pub owner_user_id: Uuid,
    pub scope: String,
    pub monthly_before: BigDecimal,
    pub monthly_after: BigDecimal,
    pub currency: String,
    pub measured_over_days: i16,
    pub service_still_meets_slo: Option<bool>,
    pub verified_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CostInput {
    pub scope: String,
    pub monthly_before: BigDecimal,
    pub monthly_after: BigDecimal,
    pub change_md: String,
    #[serde(default)]
    pub currency: Option<String>,
    #[serde(default)]
    pub evidence_url: Option<String>,
    #[serde(default)]
    pub measured_over_days: Option<i16>,
    #[serde(default)]
    pub slice_id: Option<Uuid>,
    #[serde(default)]
    pub project_id: Option<Uuid>,
}

pub async fn record_cost_work(
    db: &PgPool,
    owner: Uuid,
    input: CostInput,
) -> Result<CostWork, AppError> {
    if input.change_md.trim().len() < 100 {
        return Err(AppError::Validation(
            "say what was changed, properly. A saving with no explanation is a saving \
             somebody made by turning off something that was needed."
                .into(),
        ));
    }
    if input.slice_id.is_none() && input.project_id.is_none() {
        return Err(AppError::Validation(
            "attach it to a slice or a project".into(),
        ));
    }

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO ops_cost_optimisations
            (slice_id, project_id, owner_user_id, scope, monthly_before, monthly_after,
             currency, change_md, evidence_url, measured_over_days)
         VALUES ($1,$2,$3,$4,$5,$6,COALESCE($7,'USD'),$8,$9,COALESCE($10,30))
         RETURNING id",
    )
    .bind(input.slice_id)
    .bind(input.project_id)
    .bind(owner)
    .bind(input.scope.trim())
    .bind(&input.monthly_before)
    .bind(&input.monthly_after)
    .bind(input.currency.as_deref())
    .bind(input.change_md.trim())
    .bind(input.evidence_url.as_deref())
    .bind(input.measured_over_days)
    .fetch_one(db)
    .await
    .map_err(|e| {
        if e.to_string().contains("a_reduction_reduces") {
            AppError::Validation("the figure after is not smaller than the figure before".into())
        } else {
            AppError::from(e)
        }
    })?;

    cost_work(db, id).await
}

pub async fn cost_work(db: &PgPool, id: Uuid) -> Result<CostWork, AppError> {
    sqlx::query_as::<_, CostWork>(
        "SELECT id, owner_user_id, scope, monthly_before, monthly_after, currency,
                measured_over_days, service_still_meets_slo, verified_at
           FROM ops_cost_optimisations WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| AppError::NotFound("cost record not found".into()))
}

/// Verify a cost reduction — both halves.
///
/// Somebody has to say the service still works. Verifying the saving alone
/// would certify an outage with a spreadsheet.
pub async fn verify_cost_work(
    db: &PgPool,
    id: Uuid,
    reviewer: Uuid,
    service_still_meets_slo: bool,
) -> Result<bool, AppError> {
    let work = cost_work(db, id).await?;

    sqlx::query(
        "UPDATE ops_cost_optimisations
            SET service_still_meets_slo = $3, verified_by = $2, verified_at = NOW()
          WHERE id = $1",
    )
    .bind(id)
    .bind(reviewer)
    .bind(service_still_meets_slo)
    .execute(db)
    .await?;

    if !service_still_meets_slo {
        return Ok(false);
    }

    let saved = annual_saving(&work.monthly_before, &work.monthly_after);
    issue_ops_attestation(
        db,
        work.owner_user_id,
        "ops_cost_optimization",
        &format!("Réduction de coûts — {}", work.scope),
        &format!(
            "{} : {} {} par an économisés, service toujours conforme à son objectif.",
            work.scope, saved, work.currency
        ),
    )
    .await?;

    Ok(true)
}

// ═══════════════════════════════════════════════════════════════════
// Attestations
// ═══════════════════════════════════════════════════════════════════

/// The three ops bases that rest on a delivered artefact, and the artefact
/// subtypes each one accepts.
///
/// This pairing is the whole check. Without it, `ops_migration_completed`
/// could be issued from a Grafana dashboard and nobody reading the
/// attestation later would be able to tell. The database constraint from
/// migration 0243 only says these three name a deliverable; which deliverable
/// is a question only the domain can answer.
const ARTEFACT_BASES: &[(&str, &[&str])] = &[
    (
        "ops_infra_shipped",
        &["iac_terraform", "kubernetes_manifests", "cicd_pipeline"],
    ),
    ("ops_observability_stack_shipped", &["observability_config"]),
    ("ops_migration_completed", &["db_migration_scheme"]),
];

/// Issue one of the three artefact attestations from a verified deliverable.
///
/// Three conditions, and each one exists because of a way this could
/// otherwise be wrong: the deliverable is this person's and verified, so an
/// attestation cannot be built on somebody else's work or on work still under
/// review; it is not revoked, so a withdrawn artefact does not keep paying;
/// and its slice is an ops artefact of a subtype the basis accepts, so the
/// statement matches the thing.
pub async fn attest_artefact(
    db: &PgPool,
    user_id: Uuid,
    basis: &str,
    deliverable_id: Uuid,
    title: &str,
    evidence_url: &str,
) -> Result<(), AppError> {
    let accepted = ARTEFACT_BASES
        .iter()
        .find(|(b, _)| *b == basis)
        .map(|(_, subtypes)| *subtypes)
        .ok_or_else(|| {
            AppError::Validation(format!(
                "'{basis}' is not one of the ops bases that rest on an artefact"
            ))
        })?;

    if title.trim().is_empty() {
        return Err(AppError::Validation("an attestation needs a title".into()));
    }
    crate::validators::check_max_len(title, "title", 200)?;
    if !evidence_url.trim().starts_with("https://") {
        return Err(AppError::Validation(
            "the evidence URL must be a public https link — an attestation \
             nobody can open is worth nothing"
                .into(),
        ));
    }

    let subtype: Option<Option<String>> = sqlx::query_scalar(
        "SELECT s.ops_subtype
           FROM deliverables d
           JOIN project_slices s ON s.id = d.slice_id
          WHERE d.id = $1 AND d.user_id = $2
            AND d.verification_status = 'verified'
            AND d.revoked_at IS NULL
            AND s.slice_type = 'ops_artifact'",
    )
    .bind(deliverable_id)
    .bind(user_id)
    .fetch_optional(db)
    .await?;

    let subtype = subtype
        .ok_or_else(|| {
            AppError::Validation(
                "that deliverable is not a verified ops artefact of this \
                 person's"
                    .into(),
            )
        })?
        .unwrap_or_default();

    if !accepted.contains(&subtype.as_str()) {
        return Err(AppError::Validation(format!(
            "a {basis} attestation rests on one of {accepted:?}, and that \
             artefact is a '{subtype}'"
        )));
    }

    let code = crate::services::attestations::AttestationsService::generate_verification_code();
    sqlx::query(
        "INSERT INTO attestations
            (user_id, attestation_type, basis, title, description,
             linked_deliverable_ids, verification_code)
         VALUES ($1, 'artefact', $2, $3, $4, ARRAY[$5]::UUID[], $6)",
    )
    .bind(user_id)
    .bind(basis)
    .bind(title.trim())
    .bind(evidence_url.trim())
    .bind(deliverable_id)
    .bind(&code)
    .execute(db)
    .await?;

    metrics::counter!(
        "skilluv_ops_attestations_total",
        "basis" => basis.to_string()
    )
    .increment(1);
    Ok(())
}

/// The community one. It rests on nobody's artefact and everybody's opinion,
/// which is why it is issued by hand and carries no deliverable.
pub async fn attest_featured(db: &PgPool, user_id: Uuid, reason: &str) -> Result<(), AppError> {
    if reason.trim().len() < 40 {
        return Err(AppError::Validation(
            "say why in a sentence somebody outside the decision would \
             understand — at least forty characters"
                .into(),
        ));
    }
    issue_ops_attestation(
        db,
        user_id,
        "featured_ops_engineer",
        "Mise en avant par la communauté ops",
        reason.trim(),
    )
    .await
}

/// One attestation resting on an ops fact.
async fn issue_ops_attestation(
    db: &PgPool,
    user_id: Uuid,
    basis: &str,
    title: &str,
    description: &str,
) -> Result<(), AppError> {
    let code = crate::services::attestations::AttestationsService::generate_verification_code();

    sqlx::query(
        "INSERT INTO attestations
            (user_id, attestation_type, basis, title, description, verification_code)
         VALUES ($1, 'artefact', $2, $3, $4, $5)",
    )
    .bind(user_id)
    .bind(basis)
    .bind(title)
    .bind(description)
    .bind(&code)
    .execute(db)
    .await?;

    metrics::counter!(
        "skilluv_ops_attestations_total",
        "basis" => basis.to_string()
    )
    .increment(1);
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
    fn a_near_miss_is_a_miss() {
        // Rounding to two figures would turn every near miss into a pass.
        assert!(!objective_met(&dec("99.95"), &dec("99.94")));
        assert!(objective_met(&dec("99.95"), &dec("99.95")));
        assert!(objective_met(&dec("99.9"), &dec("99.99")));
    }

    #[test]
    fn an_error_budget_says_how_close_a_pass_was() {
        // A 99.9 target leaves 0.1 of budget. Achieving 99.95 spends half.
        let consumed = error_budget_consumed(&dec("99.9"), &dec("99.95")).unwrap();
        assert!((consumed - 50.0).abs() < 0.001);

        // Exactly on target spends all of it.
        let consumed = error_budget_consumed(&dec("99.9"), &dec("99.9")).unwrap();
        assert!((consumed - 100.0).abs() < 0.001);
    }

    #[test]
    fn a_missed_objective_reports_over_a_hundred() {
        let consumed = error_budget_consumed(&dec("99.9"), &dec("99.8")).unwrap();
        assert!(consumed > 100.0);
    }

    #[test]
    fn a_perfect_target_has_no_budget_to_spend() {
        assert_eq!(error_budget_consumed(&dec("100"), &dec("100")), Some(0.0));
        assert_eq!(
            error_budget_consumed(&dec("100"), &dec("99.99")),
            Some(f64::INFINITY)
        );
    }

    #[test]
    fn a_saving_is_stated_annually() {
        // A hundred a month looks small; twelve hundred a year is the figure
        // the decision was made against.
        assert_eq!(
            annual_saving(&dec("500.00"), &dec("400.00")),
            dec("1200.00")
        );
    }

    #[test]
    fn a_reduction_percentage_is_bounded() {
        assert_eq!(reduction_percent(&dec("100"), &dec("40")), Some(60.0));
        assert_eq!(reduction_percent(&dec("100"), &dec("0")), Some(100.0));
        // Nothing to reduce.
        assert_eq!(reduction_percent(&dec("0"), &dec("0")), None);
    }

    #[test]
    fn every_group_severity_and_subtype_is_a_known_one() {
        assert_eq!(REVIEWER_GROUPS.len(), 5);
        assert_eq!(SUBTYPES.len(), 6);
        assert_eq!(SEVERITIES.len(), 4);
        assert!(SUBTYPES.contains(&"runbook_incident"));
        assert!(REVIEWER_GROUPS.contains(&"observability"));
    }
}
