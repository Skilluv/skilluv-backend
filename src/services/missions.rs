//! Paid missions (migration 0192).
//!
//! An enterprise publishes work, people apply, one is selected, the work is
//! delivered and paid. The table holds one domain-agnostic shape and the
//! kinds of work are rows, so a design marketplace later is twelve INSERTs
//! rather than a second half of this file.
//!
//! Two rules are worth stating out loud, because both are easy to get wrong
//! in a way nobody notices until somebody is out of pocket:
//!
//!   * the commission is frozen when the mission is published, not read at
//!     payout time — changing the platform rate must not rewrite terms
//!     somebody already agreed to;
//!   * a rejection carries a reason. Somebody who spent an hour on an
//!     application is owed a sentence.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

/// What Skilluv keeps on a mission, as a percentage.
pub const STANDARD_COMMISSION: f64 = 15.0;

/// The reduced rate, for people who have delivered enough that the platform
/// is no longer taking a risk on them.
pub const FEATURED_COMMISSION: f64 = 10.0;

/// How many delivered missions earn the reduced rate.
pub const FEATURED_THRESHOLD: i64 = 10;

pub const PAYMENT_MODELS: &[&str] = &[
    "fixed_price",
    "per_hour",
    "per_deliverable",
    "retainer_monthly",
    "revenue_share",
];

pub const IP_TERMS: &[&str] = &[
    "full_ownership_client",
    "open_source_output",
    "retain_reusable_components",
    "dual_license",
];

/// What a finished mission is handed over as.
///
/// The first four are code shapes and were the whole list. A design mission
/// delivers none of them: a brand identity is not a pull request, and calling
/// it `consulting_report` would have made every design mission lie about what
/// it produced. The rest are the shapes design actually hands over.
pub const DELIVERABLE_FORMATS: &[&str] = &[
    "github_pr",
    "repository_handover",
    "library_published",
    "consulting_report",
    // Ops (migration 0524). A runbook is the deliverable here rather than
    // documentation accompanying one.
    "iac_repository",
    "runbooks",
    "dashboards",
    "migration_executed",
    // Editable sources plus whatever is needed to reopen them. The default
    // for most design work: a deliverable nobody can reopen is not delivered.
    "design_source_files",
    // Marks, palette, type and the rules for using them.
    "brand_package",
    // A rendered animation and the project behind it.
    "motion_package",
    // A prototype somebody can walk through at a link.
    "prototype_link",
    // Tokens, components and their documentation, handed to a team that will
    // build on them.
    "design_system_handover",
];

/// The statuses a mission can be moved to, from each status.
///
/// Written as a table rather than as a chain of `if`s because the shape of
/// the workflow is the thing worth reading: a mission goes forward, or it is
/// cancelled, and it never goes back.
fn allowed_transitions(from: &str) -> &'static [&'static str] {
    match from {
        "draft" => &["published", "cancelled"],
        "published" => &["applications_closed", "in_progress", "cancelled"],
        "applications_closed" => &["in_progress", "published", "cancelled"],
        "in_progress" => &["delivered", "cancelled"],
        // Closing is the client accepting delivery. Reopening a delivered
        // mission would mean disputing it, which is a different flow.
        "delivered" => &["closed"],
        _ => &[],
    }
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Mission {
    pub id: Uuid,
    pub slug: String,
    pub enterprise_id: Uuid,
    pub mission_type_slug: String,
    pub skill_domain: String,
    pub title: String,
    pub description: String,
    pub acceptance_criteria: String,
    pub target_languages: Vec<String>,
    pub target_frameworks: Vec<String>,
    pub orientation_slug: Option<String>,
    pub deliverable_format: String,
    pub nda_required: bool,
    pub ip_terms: String,
    pub payment_model: String,
    pub budget_eur: Option<bigdecimal::BigDecimal>,
    pub hourly_rate_eur: Option<bigdecimal::BigDecimal>,
    pub revenue_share_percent: Option<bigdecimal::BigDecimal>,
    pub commission_percent: bigdecimal::BigDecimal,
    pub remote_only: bool,
    pub urgency: String,
    pub estimated_days: Option<i16>,
    pub status: String,
    pub assigned_user_id: Option<Uuid>,
    pub applications_close_at: Option<chrono::DateTime<chrono::Utc>>,
    pub published_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Absent on the public listing, present for the enterprise that owns it.
    pub application_count: Option<i64>,

    pub target_platforms: Vec<String>,
    pub includes_oncall: bool,
    pub oncall_window: Option<String>,
    pub oncall_response_minutes: Option<i16>,
    pub oncall_has_backup: bool,
    pub production_access_required: bool,
    pub compliance_frameworks: Vec<String>,

    /// What the client may do with the delivered work. Orthogonal to
    /// `ip_terms`, which says who owns it (migration 0413). Compulsory for
    /// audio and communication, where the licence is where the disputes are.
    pub licensing_scope: Option<String>,
    /// Education: who is being taught. Not a level of the person doing the
    /// work, which is what every other level field here means.
    pub target_audience: Option<String>,
    pub target_learners: Option<i32>,
    /// Education: hours in front of people. Distinct from `estimated_days`,
    /// which says how long the work takes.
    pub delivery_hours: Option<i32>,
}

const MISSION_SELECT: &str = r#"
    SELECT m.id, m.slug, m.enterprise_id,
           mt.slug AS mission_type_slug,
           m.skill_domain, m.title, m.description, m.acceptance_criteria,
           m.target_languages, m.target_frameworks,
           o.slug AS orientation_slug,
           m.deliverable_format, m.nda_required, m.ip_terms, m.payment_model,
           m.budget_eur, m.hourly_rate_eur, m.revenue_share_percent,
           m.commission_percent, m.remote_only, m.urgency, m.estimated_days,
           m.status, m.assigned_user_id, m.applications_close_at,
           m.published_at, m.created_at,
           (SELECT count(*) FROM mission_applications a WHERE a.mission_id = m.id)
               AS application_count,
           m.target_platforms, m.includes_oncall, m.oncall_window,
           m.oncall_response_minutes, m.oncall_has_backup,
           m.production_access_required, m.compliance_frameworks,
           m.licensing_scope,
           m.target_audience, m.target_learners, m.delivery_hours
      FROM missions m
      JOIN mission_types mt ON mt.id = m.mission_type_id
      LEFT JOIN orientations o ON o.id = m.orientation_id
"#;

#[derive(Debug, Clone, Deserialize)]
pub struct CreateMissionInput {
    pub slug: String,
    pub mission_type_slug: String,
    pub title: String,
    pub description: String,
    pub acceptance_criteria: String,
    #[serde(default)]
    pub target_languages: Vec<String>,
    #[serde(default)]
    pub target_frameworks: Vec<String>,
    #[serde(default)]
    pub orientation_slug: Option<String>,
    pub deliverable_format: String,
    #[serde(default)]
    pub nda_required: bool,
    #[serde(default)]
    pub ip_terms: Option<String>,
    #[serde(default)]
    pub payment_model: Option<String>,
    #[serde(default)]
    pub budget_eur: Option<bigdecimal::BigDecimal>,
    #[serde(default)]
    pub hourly_rate_eur: Option<bigdecimal::BigDecimal>,
    #[serde(default)]
    pub revenue_share_percent: Option<bigdecimal::BigDecimal>,
    #[serde(default = "yes")]
    pub remote_only: bool,
    #[serde(default)]
    pub urgency: Option<String>,
    #[serde(default)]
    pub estimated_days: Option<i16>,
    #[serde(default)]
    pub applications_close_at: Option<chrono::DateTime<chrono::Utc>>,
    /// The licence the deliverable will derive from, as an SPDX identifier.
    /// Optional, and checked against the IP terms when given (migration 0202).
    #[serde(default)]
    pub upstream_license_spdx: Option<String>,

    /// `aws`, `gcp`, `azure`, `on-prem`. Empty means the work does not depend
    /// on one.
    #[serde(default)]
    pub target_platforms: Vec<String>,
    /// Whether the person is expected to be reachable. True requires a
    /// window, a response time and a monthly retainer — the schema refuses
    /// anything else, because unpaid availability is the most common way this
    /// trade is exploited.
    #[serde(default)]
    pub includes_oncall: bool,
    #[serde(default)]
    pub oncall_window: Option<String>,
    /// Time to acknowledge, not to resolve.
    #[serde(default)]
    pub oncall_response_minutes: Option<i16>,
    #[serde(default)]
    pub oncall_has_backup: bool,
    /// Whether the work needs credentials on the client's live estate.
    #[serde(default)]
    pub production_access_required: bool,
    /// `soc2`, `iso27001`, `hipaa`. Named so an applicant learns about the
    /// background check before applying rather than after being refused.
    #[serde(default)]
    pub compliance_frameworks: Vec<String>,

    /// One of `mission_licensing_scopes` (migration 0413): what the client may
    /// do with the work, as distinct from who owns it.
    ///
    /// This field was missing until communication opened, and its absence was
    /// a live bug rather than a gap: migration 0413 made the column compulsory
    /// for audio missions, so from that migration until this one an audio
    /// mission could not be created through the API at all — the insert was
    /// refused by a constraint naming a column the request had no way to set.
    #[serde(default)]
    pub licensing_scope: Option<String>,

    /// Education: `beginner`, `junior`, `mid`, `senior` or `mixed`.
    /// Compulsory for an education mission — without it an applicant cannot
    /// tell whether they are the right trainer.
    #[serde(default)]
    pub target_audience: Option<String>,
    /// Education: how many people. Twelve and two hundred are different work.
    #[serde(default)]
    pub target_learners: Option<i32>,
    /// Education: hours in front of people, which is not the same question as
    /// how long the preparation takes.
    #[serde(default)]
    pub delivery_hours: Option<i32>,
}

fn yes() -> bool {
    true
}

pub async fn create(
    db: &PgPool,
    enterprise_id: Uuid,
    author: Uuid,
    input: CreateMissionInput,
) -> Result<Mission, AppError> {
    let slug = input.slug.trim().to_lowercase();
    if slug.len() < 3
        || slug.len() > 120
        || !slug.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err(AppError::Validation(
            "slug must be 3-120 lowercase alphanumeric characters or dashes".into(),
        ));
    }
    if input.title.trim().is_empty() {
        return Err(AppError::Validation("title is required".into()));
    }
    crate::validators::check_max_len(&input.title, "title", 200)?;
    if input.description.trim().is_empty() {
        return Err(AppError::Validation("description is required".into()));
    }
    crate::validators::check_max_len(&input.description, "description", 20_000)?;
    if input.acceptance_criteria.trim().is_empty() {
        return Err(AppError::Validation(
            "acceptance_criteria is required — a mission without one ends in an argument about scope"
                .into(),
        ));
    }
    crate::validators::check_max_len(&input.acceptance_criteria, "acceptance_criteria", 10_000)?;

    if !DELIVERABLE_FORMATS.contains(&input.deliverable_format.as_str()) {
        return Err(AppError::Validation(format!(
            "deliverable_format must be one of: {}",
            DELIVERABLE_FORMATS.join(", ")
        )));
    }
    let ip_terms = input
        .ip_terms
        .clone()
        .unwrap_or_else(|| "full_ownership_client".into());
    if !IP_TERMS.contains(&ip_terms.as_str()) {
        return Err(AppError::Validation(format!(
            "ip_terms must be one of: {}",
            IP_TERMS.join(", ")
        )));
    }
    let payment_model = input
        .payment_model
        .clone()
        .unwrap_or_else(|| "fixed_price".into());
    if !PAYMENT_MODELS.contains(&payment_model.as_str()) {
        return Err(AppError::Validation(format!(
            "payment_model must be one of: {}",
            PAYMENT_MODELS.join(", ")
        )));
    }
    let urgency = input.urgency.clone().unwrap_or_else(|| "normal".into());
    if !matches!(urgency.as_str(), "normal" | "soon" | "urgent") {
        return Err(AppError::Validation(
            "urgency must be normal, soon or urgent".into(),
        ));
    }

    // The constraint would catch this, but with a message about a check
    // called `mission_price_matches_its_model`, which helps nobody.
    let priced = match payment_model.as_str() {
        "per_hour" => input.hourly_rate_eur.is_some(),
        "revenue_share" => input.revenue_share_percent.is_some(),
        _ => input.budget_eur.is_some(),
    };
    if !priced {
        return Err(AppError::Validation(match payment_model.as_str() {
            "per_hour" => "a per_hour mission needs an hourly_rate_eur".into(),
            "revenue_share" => "a revenue_share mission needs a revenue_share_percent".into(),
            other => format!("a {other} mission needs a budget_eur"),
        }));
    }

    let type_row: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT id, skill_domain FROM mission_types WHERE slug = $1 AND is_active = TRUE",
    )
    .bind(&input.mission_type_slug)
    .fetch_optional(db)
    .await?;
    let (mission_type_id, skill_domain) = type_row.ok_or_else(|| {
        AppError::NotFound(format!(
            "no active mission type '{}'",
            input.mission_type_slug
        ))
    })?;

    // Follows a rename, like everywhere else a trade is named by slug.
    let orientation_id: Option<Uuid> = match &input.orientation_slug {
        Some(slug) => {
            let resolved: Option<Uuid> = sqlx::query_scalar("SELECT resolve_orientation($1)")
                .bind(slug)
                .fetch_one(db)
                .await?;
            Some(
                resolved
                    .ok_or_else(|| AppError::NotFound(format!("orientation '{slug}' not found")))?,
            )
        }
        None => None,
    };

    let id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO missions
            (slug, enterprise_id, mission_type_id, skill_domain, title, description,
             acceptance_criteria, target_languages, target_frameworks, orientation_id,
             deliverable_format, nda_required, ip_terms, payment_model,
             budget_eur, hourly_rate_eur, revenue_share_percent,
             remote_only, urgency, estimated_days, applications_close_at, created_by,
             upstream_license_spdx,
             target_platforms, includes_oncall, oncall_window,
             oncall_response_minutes, oncall_has_backup,
             production_access_required, compliance_frameworks,
             licensing_scope, target_audience, target_learners, delivery_hours)
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,
                $23,$24,$25,$26,$27,$28,$29,$30,$31,$32,$33,$34)
        RETURNING id
        "#,
    )
    .bind(&slug)
    .bind(enterprise_id)
    .bind(mission_type_id)
    .bind(&skill_domain)
    .bind(input.title.trim())
    .bind(input.description.trim())
    .bind(input.acceptance_criteria.trim())
    .bind(&input.target_languages)
    .bind(&input.target_frameworks)
    .bind(orientation_id)
    .bind(&input.deliverable_format)
    .bind(input.nda_required)
    .bind(&ip_terms)
    .bind(&payment_model)
    .bind(input.budget_eur.as_ref())
    .bind(input.hourly_rate_eur.as_ref())
    .bind(input.revenue_share_percent.as_ref())
    .bind(input.remote_only)
    .bind(&urgency)
    .bind(input.estimated_days)
    .bind(input.applications_close_at)
    .bind(author)
    .bind(input.upstream_license_spdx.as_deref())
    .bind(&input.target_platforms)
    .bind(input.includes_oncall)
    .bind(input.oncall_window.as_deref())
    .bind(input.oncall_response_minutes)
    .bind(input.oncall_has_backup)
    .bind(input.production_access_required)
    .bind(&input.compliance_frameworks)
    .bind(input.licensing_scope.as_deref())
    .bind(input.target_audience.as_deref())
    .bind(input.target_learners)
    .bind(input.delivery_hours)
    .fetch_one(db)
    .await
    .map_err(|e| {
        if is_unique_violation(&e) {
            return AppError::Validation(format!("a mission already uses the slug '{slug}'"));
        }
        let message = e.to_string();

        // The two ops constraints from migration 0524 are positions rather
        // than data errors, and a constraint name in a 500 would hide the
        // position behind a stack trace.
        if message.contains("oncall_missions_state_their_terms") {
            return AppError::Validation(
                "a mission that includes on-call states the window, the response \
                 time to acknowledge, and a monthly retainer. Being reachable is \
                 work whether or not anything happens, and this platform does not \
                 publish it unpaid."
                    .into(),
            );
        }
        if message.contains("production_access_requires_an_nda") {
            return AppError::Validation(
                "a mission needing credentials on your live estate requires an \
                 NDA. Set nda_required."
                    .into(),
            );
        }

        // The licence trigger from migration 0202 raises text written for a
        // human already; passing it through beats replacing it with
        // something vaguer.
        for marker in [
            "cannot promise client ownership",
            "must be released under it",
        ] {
            if message.contains(marker) {
                // The whole exception line, which already names the licence
                // and carries its caveat.
                let start = message.find("ERROR:").map(|i| i + 6).unwrap_or(0);
                let sentence = message[start..].lines().next().unwrap_or(&message).trim();
                return AppError::Validation(sentence.to_string());
            }
        }
        AppError::from(e)
    })?;

    by_id(db, id).await
}

pub async fn by_id(db: &PgPool, id: Uuid) -> Result<Mission, AppError> {
    let sql = format!("{MISSION_SELECT} WHERE m.id = $1");
    sqlx::query_as::<_, Mission>(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NotFound("mission not found".into()))
}

pub async fn by_slug(db: &PgPool, slug: &str) -> Result<Mission, AppError> {
    let sql = format!("{MISSION_SELECT} WHERE m.slug = $1");
    sqlx::query_as::<_, Mission>(sqlx::AssertSqlSafe(sql))
        .bind(slug)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NotFound("mission not found".into()))
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct MissionFilter {
    pub skill_domain: Option<String>,
    pub mission_type: Option<String>,
    pub language: Option<String>,
    pub framework: Option<String>,
    pub orientation: Option<String>,
    pub ip_terms: Option<String>,
    pub payment_model: Option<String>,
    pub min_budget_eur: Option<bigdecimal::BigDecimal>,
    pub remote_only: Option<bool>,
    pub urgency: Option<String>,
    /// Education: who the mission teaches. A trainer filtering for beginners
    /// and one filtering for senior engineers are looking for different work.
    pub target_audience: Option<String>,
}

/// Missions anybody can apply to.
///
/// Drafts, assigned and closed missions are absent: a public board listing
/// work nobody can take is a board people stop reading.
pub async fn list_open(
    db: &PgPool,
    filter: &MissionFilter,
    limit: i64,
    offset: i64,
) -> Result<Vec<Mission>, AppError> {
    let sql = format!(
        "{MISSION_SELECT}
         WHERE m.status = 'published'
           AND (m.applications_close_at IS NULL OR m.applications_close_at > NOW())
           AND ($1::TEXT IS NULL OR m.skill_domain = $1)
           AND ($2::TEXT IS NULL OR mt.slug = $2)
           AND ($3::TEXT IS NULL OR $3 = ANY(m.target_languages))
           AND ($4::TEXT IS NULL OR $4 = ANY(m.target_frameworks))
           AND ($5::UUID IS NULL OR m.orientation_id = $5)
           AND ($6::TEXT IS NULL OR m.ip_terms = $6)
           AND ($7::TEXT IS NULL OR m.payment_model = $7)
           AND ($8::NUMERIC IS NULL
                OR COALESCE(m.budget_eur, m.hourly_rate_eur) >= $8)
           AND ($9::BOOLEAN IS NULL OR m.remote_only = $9)
           AND ($10::TEXT IS NULL OR m.urgency = $10)
           AND ($11::TEXT IS NULL OR m.target_audience = $11)
         ORDER BY CASE m.urgency WHEN 'urgent' THEN 0 WHEN 'soon' THEN 1 ELSE 2 END,
                  m.published_at DESC NULLS LAST
         LIMIT $12 OFFSET $13"
    );

    let orientation_id: Option<Uuid> = match &filter.orientation {
        Some(slug) => {
            sqlx::query_scalar("SELECT resolve_orientation($1)")
                .bind(slug)
                .fetch_one(db)
                .await?
        }
        None => None,
    };
    // A filter that resolves to nothing must answer nothing, not everything.
    if filter.orientation.is_some() && orientation_id.is_none() {
        return Ok(vec![]);
    }

    let rows = sqlx::query_as::<_, Mission>(sqlx::AssertSqlSafe(sql))
        .bind(filter.skill_domain.as_deref())
        .bind(filter.mission_type.as_deref())
        .bind(filter.language.as_deref())
        .bind(filter.framework.as_deref())
        .bind(orientation_id)
        .bind(filter.ip_terms.as_deref())
        .bind(filter.payment_model.as_deref())
        .bind(filter.min_budget_eur.as_ref())
        .bind(filter.remote_only)
        .bind(filter.urgency.as_deref())
        .bind(filter.target_audience.as_deref())
        .bind(limit)
        .bind(offset)
        .fetch_all(db)
        .await?;
    Ok(rows)
}

/// Who is asking for the status change.
///
/// The transition table cannot answer this on its own, and for one transition
/// it has to. An arbiter deciding against a delivery must be able to end the
/// mission, which means `delivered -> cancelled`; the client must not, because
/// cancelling is what returns the escrow and the client is who it returns to.
/// Opening that edge for everybody would let somebody accept the work, cancel
/// the mission and take the money back — with the refund this codebase has
/// just gained doing the taking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decider {
    /// The enterprise or the assignee, moving their own mission along.
    Party,
    /// Somebody outside, deciding a case the two sides would not end. Reaches
    /// this through `POST /api/admin/missions/{slug}/arbitrate`, which checks
    /// the capability before calling.
    Arbiter,
}

/// Move a mission along its workflow.
///
/// Publishing is where the commission is frozen, so it is the one transition
/// that does more than change a word.
pub async fn set_status(
    db: &PgPool,
    mission_id: Uuid,
    to: &str,
    reason: Option<&str>,
) -> Result<Mission, AppError> {
    set_status_as(db, mission_id, to, reason, Decider::Party).await
}

/// As [`set_status`], for a caller whose authority has already been checked.
pub async fn set_status_as(
    db: &PgPool,
    mission_id: Uuid,
    to: &str,
    reason: Option<&str>,
    who: Decider,
) -> Result<Mission, AppError> {
    let current: Option<(String, Option<Uuid>)> =
        sqlx::query_as("SELECT status, assigned_user_id FROM missions WHERE id = $1")
            .bind(mission_id)
            .fetch_optional(db)
            .await?;
    let (from, assigned) = current.ok_or_else(|| AppError::NotFound("mission not found".into()))?;

    if from == to {
        return by_id(db, mission_id).await;
    }
    // The one edge the table does not carry, because it depends on who asks.
    let arbitrated_end = who == Decider::Arbiter && from == "delivered" && to == "cancelled";
    if !arbitrated_end && !allowed_transitions(&from).contains(&to) {
        return Err(AppError::Validation(format!(
            "a {from} mission cannot become {to}"
        )));
    }
    if matches!(to, "in_progress" | "delivered") && assigned.is_none() {
        return Err(AppError::Validation(
            "select an applicant before moving the mission forward".into(),
        ));
    }
    let reason = reason.map(str::trim).filter(|s| !s.is_empty());
    if to == "cancelled" && reason.is_none() {
        return Err(AppError::Validation(
            "cancelling requires a reason the applicants can read".into(),
        ));
    }

    sqlx::query(
        r#"
        UPDATE missions
           SET status = $2,
               cancellation_reason = COALESCE($3, cancellation_reason),
               published_at = CASE WHEN $2 = 'published' AND published_at IS NULL
                                   THEN NOW() ELSE published_at END,
               delivered_at = CASE WHEN $2 = 'delivered' THEN NOW() ELSE delivered_at END,
               closed_at = CASE WHEN $2 IN ('closed', 'cancelled')
                                THEN NOW() ELSE closed_at END
         WHERE id = $1
        "#,
    )
    .bind(mission_id)
    .bind(to)
    .bind(reason)
    .execute(db)
    .await?;

    // Closing is the client accepting delivery, and that is what makes the
    // money withdrawable. Doing it here rather than in the route means it
    // happens whichever way the mission is closed.
    // A mission opening for applications is the one status change the room
    // wants to hear about — the rest are between the client and one person.
    if to == "published"
        && let Ok(Some((domain, slug, title))) = sqlx::query_as::<_, (String, String, String)>(
            "SELECT skill_domain, slug, title FROM missions WHERE id = $1",
        )
        .bind(mission_id)
        .fetch_optional(db)
        .await
    {
        crate::services::discord_announce::mission_posted(db, &domain, &slug, &title).await;
    }

    // Cancelling gives back what was captured and never released. Here rather
    // than in the route for the same reason the release is: it has to happen
    // whichever way the mission is cancelled, and it did not — a mission
    // cancelled from `in_progress` with paid invoices left the escrow with
    // nothing to release it and nobody to notice, which is the shape this
    // codebase keeps finding and removing.
    if to == "cancelled" {
        let why = reason.unwrap_or("mission annulée");
        crate::services::mission_billing::refund_all(db, mission_id, why).await?;
    }

    if to == "closed" {
        crate::services::mission_billing::release_all(db, mission_id).await?;

        // A delivered mission on the public feed, with no figure: the work
        // happening is public, what it paid is not. Best-effort — a feed
        // line must never fail a closure that genuinely happened.
        if let Err(err) = announce_delivery(db, mission_id).await {
            tracing::warn!(%err, mission = %mission_id, "mission not announced on the public feed");
        }
    }

    by_id(db, mission_id).await
}

/// Put a delivered mission on the public feed.
///
/// One writer, in code that already knows the wording, so this is emitted
/// here rather than by a trigger — unlike the two kinds that are written from
/// half a dozen places.
///
/// The figure is deliberately absent. That the work happened is public; what
/// it paid is the contractor's business, and `mission_delivered` is barred
/// from carrying an amount at all.
async fn announce_delivery(db: &PgPool, mission_id: Uuid) -> Result<(), AppError> {
    let row: Option<(Uuid, String, String, String)> = sqlx::query_as(
        "SELECT m.assigned_user_id, u.username, m.title, mt.name
           FROM missions m
           JOIN users u ON u.id = m.assigned_user_id
           JOIN mission_types mt ON mt.id = m.mission_type_id
          WHERE m.id = $1 AND m.assigned_user_id IS NOT NULL",
    )
    .bind(mission_id)
    .fetch_optional(db)
    .await?;
    let Some((user_id, username, title, kind_name)) = row else {
        return Ok(());
    };

    // The mission's public page. A line with nowhere to go is a claim, and
    // the feed refuses those.
    let url = format!(
        "{}/missions/{}",
        std::env::var("SKILLUV_FRONTEND_URL")
            .unwrap_or_else(|_| "https://skill-uv.com".into())
            .trim_end_matches('/'),
        by_id(db, mission_id).await?.slug
    );

    crate::services::public_feed::emit(
        db,
        crate::services::public_feed::Emission {
            kind: "mission_delivered",
            subject_type: "user",
            subject_id: user_id,
            subject_label: &username,
            headline: format!("mission livrée — {kind_name} : {title}"),
            artifact_url: url,
            repository: None,
            amount: None,
            currency: None,
            source_type: "mission",
            source_id: mission_id,
        },
    )
    .await?;

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════
// Commission
// ═══════════════════════════════════════════════════════════════════

/// How many missions somebody has delivered and had closed.
pub async fn delivered_missions_of(db: &PgPool, user_id: Uuid) -> Result<i64, AppError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM missions
          WHERE assigned_user_id = $1 AND status = 'closed'",
    )
    .bind(user_id)
    .fetch_one(db)
    .await?;
    Ok(count)
}

/// A rate, and the rule that produced it.
///
/// The reason travels with the number because a rate with nothing to point at
/// is a rate somebody will eventually argue about.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Commission {
    pub percent: f64,
    /// `standard`, `charity_brief` or `loyalty_discount`.
    pub reason: &'static str,
}

/// The rate that applies to this mission, taken by this person, today.
///
/// Called once, at selection, and written onto the mission. Reading it at
/// payout time would mean somebody's tenth delivery retroactively re-rated
/// the nine before it.
///
/// Charity wins over loyalty rather than stacking: zero is already the floor,
/// and "zero, and then ten percent off" is a rule somebody implements wrongly
/// one day.
pub async fn commission_for(
    db: &PgPool,
    user_id: Uuid,
    charity_brief: bool,
) -> Result<Commission, AppError> {
    if charity_brief {
        // Skilluv does not take a cut of work given away.
        return Ok(Commission {
            percent: 0.0,
            reason: "charity_brief",
        });
    }

    let delivered = delivered_missions_of(db, user_id).await?;
    Ok(if delivered >= FEATURED_THRESHOLD {
        Commission {
            percent: FEATURED_COMMISSION,
            reason: "loyalty_discount",
        }
    } else {
        Commission {
            percent: STANDARD_COMMISSION,
            reason: "standard",
        }
    })
}

// ═══════════════════════════════════════════════════════════════════
// Applications
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Application {
    pub id: Uuid,
    pub mission_id: Uuid,
    pub user_id: Uuid,
    pub username: String,
    pub cover_letter: String,
    pub portfolio_urls: Vec<String>,
    pub expertise: serde_json::Value,
    pub past_similar_missions: Option<String>,
    pub availability_hours_per_week: Option<i16>,
    pub status: String,
    pub decision_reason: Option<String>,
    /// How many attestations the platform has issued this person in the
    /// mission's domain. Sits next to the self-declared expertise, and is the
    /// half of it nobody typed themselves.
    pub verified_attestations: i64,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

const APPLICATION_SELECT: &str = r#"
    SELECT a.id, a.mission_id, a.user_id, u.username,
           a.cover_letter, a.portfolio_urls, a.expertise,
           a.past_similar_missions, a.availability_hours_per_week,
           a.status, a.decision_reason,
           (SELECT count(*) FROM attestations at
             WHERE at.user_id = a.user_id AND at.revoked_at IS NULL)
               AS verified_attestations,
           a.created_at
      FROM mission_applications a
      JOIN users u ON u.id = a.user_id
"#;

#[derive(Debug, Clone, Deserialize)]
pub struct ApplyInput {
    pub cover_letter: String,
    #[serde(default)]
    pub portfolio_urls: Vec<String>,
    /// `[{"name": "rust", "years": 3}]`.
    #[serde(default)]
    pub expertise: Option<serde_json::Value>,
    #[serde(default)]
    pub past_similar_missions: Option<String>,
    #[serde(default)]
    pub availability_hours_per_week: Option<i16>,
    /// Whether this person can hold the rotation the mission describes.
    /// Answered before selection, because answering after is how somebody
    /// agrees to a rotation they cannot hold.
    #[serde(default)]
    pub oncall_available: Option<bool>,
    #[serde(default)]
    pub oncall_experience: Option<String>,
}

pub async fn apply(
    db: &PgPool,
    mission_id: Uuid,
    user_id: Uuid,
    input: ApplyInput,
) -> Result<Application, AppError> {
    if input.cover_letter.trim().is_empty() {
        return Err(AppError::Validation(
            "a cover letter is required — an application with nothing in it cannot be compared"
                .into(),
        ));
    }
    crate::validators::check_max_len(&input.cover_letter, "cover_letter", 8000)?;

    for url in &input.portfolio_urls {
        if !url.starts_with("https://") {
            return Err(AppError::Validation("portfolio URLs must be https".into()));
        }
    }
    if input.portfolio_urls.len() > 20 {
        return Err(AppError::Validation(
            "twenty portfolio links is already more than anybody will open".into(),
        ));
    }

    let expertise = input
        .expertise
        .clone()
        .unwrap_or_else(|| serde_json::json!([]));
    if !expertise.is_array() {
        return Err(AppError::Validation(
            "expertise must be a list of {name, years}".into(),
        ));
    }

    // An enterprise applying to its own mission would be selecting itself.
    let own: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM missions m
               JOIN enterprises e ON e.id = m.enterprise_id
              WHERE m.id = $1 AND e.owner_id = $2)",
    )
    .bind(mission_id)
    .bind(user_id)
    .fetch_one(db)
    .await?;
    if own {
        return Err(AppError::Validation(
            "you cannot apply to your own mission".into(),
        ));
    }

    if let Some(experience) = input.oncall_experience.as_deref()
        && !crate::services::ops_onboarding::ONCALL_EXPERIENCE.contains(&experience)
    {
        return Err(AppError::Validation(format!(
            "'{experience}' is not an on-call answer"
        )));
    }

    // A mission that includes on-call cannot be applied to without answering
    // whether the rotation is holdable. Asked here rather than left to the
    // enterprise to ask later, because "later" is after selection, when
    // saying no costs the applicant the mission.
    let includes_oncall: bool =
        sqlx::query_scalar("SELECT includes_oncall FROM missions WHERE id = $1")
            .bind(mission_id)
            .fetch_optional(db)
            .await?
            .ok_or_else(|| AppError::NotFound("mission not found".into()))?;

    if includes_oncall && input.oncall_available.is_none() {
        return Err(AppError::Validation(
            "this mission includes on-call: say whether you can hold the rotation \
             it describes. Answering after selection is how somebody agrees to a \
             rotation they cannot hold."
                .into(),
        ));
    }

    // The three gates a mission may set, checked here rather than left to the
    // enterprise to notice after selection.
    //
    // All three are generic — any domain may set them — and all three arrived
    // with security, which is where a mission that says "OSCP, artisan or
    // above, sign the NDA first" is ordinary rather than exotic.
    check_application_gates(db, mission_id, user_id).await?;

    let id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO mission_applications
            (mission_id, user_id, cover_letter, portfolio_urls, expertise,
             past_similar_missions, availability_hours_per_week,
             oncall_available, oncall_experience)
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
        ON CONFLICT (mission_id, user_id) DO UPDATE
            SET cover_letter = EXCLUDED.cover_letter,
                portfolio_urls = EXCLUDED.portfolio_urls,
                expertise = EXCLUDED.expertise,
                past_similar_missions = EXCLUDED.past_similar_missions,
                availability_hours_per_week = EXCLUDED.availability_hours_per_week,
                oncall_available = EXCLUDED.oncall_available,
                oncall_experience = EXCLUDED.oncall_experience,
                status = 'submitted'
        RETURNING id
        "#,
    )
    .bind(mission_id)
    .bind(user_id)
    .bind(input.cover_letter.trim())
    .bind(&input.portfolio_urls)
    .bind(&expertise)
    .bind(input.past_similar_missions.as_deref().map(str::trim))
    .bind(input.availability_hours_per_week)
    .bind(input.oncall_available)
    .bind(input.oncall_experience.as_deref())
    .fetch_one(db)
    .await
    .map_err(open_mission_error)?;

    let sql = format!("{APPLICATION_SELECT} WHERE a.id = $1");
    sqlx::query_as::<_, Application>(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_one(db)
        .await
        .map_err(AppError::from)
}

/// The rank, the credentials and the confidentiality agreement.
///
/// Refused here, at application, and never later. A gate checked after
/// selection is a gate that costs the applicant the mission — which is the
/// argument the on-call question above already makes.
///
/// ## Why a declared credential is enough
///
/// The check is that the applicant has *declared* the credential, not that
/// somebody verified it. Verification is a separate act with its own queue, and
/// refusing every applicant whose OSCP nobody has got round to checking would
/// make the filter a filter on this platform's own backlog. What the enterprise
/// sees on the application is which of the two it is — declared or verified —
/// so the decision stays theirs.
async fn check_application_gates(
    db: &PgPool,
    mission_id: Uuid,
    user_id: Uuid,
) -> Result<(), AppError> {
    #[derive(sqlx::FromRow)]
    struct Gates {
        min_rank: Option<String>,
        required_credentials: Vec<String>,
        nda_required: bool,
    }

    let gates: Gates = sqlx::query_as(
        "SELECT min_rank, required_credentials, nda_required
           FROM missions WHERE id = $1",
    )
    .bind(mission_id)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| AppError::NotFound("mission not found".into()))?;

    if let Some(required) = gates.min_rank.as_deref() {
        let held: Option<String> =
            sqlx::query_scalar("SELECT rank FROM user_ranks WHERE user_id = $1")
                .bind(user_id)
                .fetch_optional(db)
                .await?;
        let ladder = ["apprenti", "ranger", "artisan", "maitre", "doyen"];
        let position = |r: &str| ladder.iter().position(|x| *x == r);
        let (Some(need), Some(have)) = (
            position(required),
            held.as_deref().and_then(position).or(Some(0)),
        ) else {
            // A rank nobody recognises is a seeding mistake, and refusing every
            // applicant because of it would be worse than letting them through
            // to a human.
            tracing::warn!(mission = %mission_id, required,
                "a mission requires a rank that is not on the ladder");
            return Ok(());
        };
        if have < need {
            return Err(AppError::Validation(format!(
                "this mission is open from {required} upwards. What raises a rank                  is verified work, and the mission board is not the place it                  starts"
            )));
        }
    }

    if !gates.required_credentials.is_empty() {
        // Matched on the name, case-insensitively, because an enterprise types
        // "OSCP" and a holder typed "oscp".
        let missing: Vec<String> = sqlx::query_scalar(
            "SELECT r FROM unnest($2::TEXT[]) AS r
              WHERE NOT EXISTS (
                  SELECT 1 FROM external_credentials c
                   WHERE c.user_id = $1
                     AND (lower(c.name) = lower(r) OR lower(c.level) = lower(r))
                     AND (c.expires_on IS NULL OR c.expires_on >= CURRENT_DATE))",
        )
        .bind(user_id)
        .bind(&gates.required_credentials)
        .fetch_all(db)
        .await?;

        if !missing.is_empty() {
            return Err(AppError::Validation(format!(
                "this mission asks for {}. Declare it on your profile first —                  declared is enough to apply, and the enterprise is told which                  of your credentials anybody has checked",
                missing.join(", ")
            )));
        }
    }

    if gates.nda_required {
        let signed: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                 SELECT 1 FROM mission_nda_signatures
                  WHERE mission_id = $1 AND signer_user_id = $2
                    AND released_at IS NULL)",
        )
        .bind(mission_id)
        .bind(user_id)
        .fetch_one(db)
        .await?;
        if !signed {
            return Err(AppError::Validation(
                "this mission requires a confidentiality agreement. Sign it                  first: most of what makes an engagement like this describable                  is what the client has agreed you may say"
                    .into(),
            ));
        }
    }

    Ok(())
}

/// The trigger speaks SQL; this says the same in words the applicant can act
/// on.
fn open_mission_error(e: sqlx::Error) -> AppError {
    let message = e.to_string();
    for marker in [
        "not open to applications",
        "applications for this mission closed",
    ] {
        if let Some(start) = message.find(marker) {
            let sentence: String = message[start..].lines().next().unwrap_or("").into();
            return AppError::Validation(sentence);
        }
    }
    AppError::from(e)
}

pub async fn applications_for(db: &PgPool, mission_id: Uuid) -> Result<Vec<Application>, AppError> {
    let sql = format!(
        "{APPLICATION_SELECT} WHERE a.mission_id = $1
         ORDER BY CASE a.status
                      WHEN 'selected' THEN 0
                      WHEN 'shortlisted' THEN 1
                      WHEN 'submitted' THEN 2
                      ELSE 3
                  END,
                  a.created_at ASC"
    );
    let rows = sqlx::query_as::<_, Application>(sqlx::AssertSqlSafe(sql))
        .bind(mission_id)
        .fetch_all(db)
        .await?;
    Ok(rows)
}

/// Shortlist, select or reject an application.
///
/// Selecting does three things at once, and they belong in one transaction:
/// the applicant is marked, the mission is assigned to them, and the
/// commission that will apply is frozen at the rate they qualify for today.
pub async fn decide(
    db: &PgPool,
    application_id: Uuid,
    decider: Uuid,
    status: &str,
    reason: Option<&str>,
) -> Result<Application, AppError> {
    if !matches!(status, "shortlisted" | "selected" | "rejected") {
        return Err(AppError::Validation(
            "status must be shortlisted, selected or rejected".into(),
        ));
    }
    let reason = reason.map(str::trim).filter(|s| !s.is_empty());
    if status == "rejected" && reason.is_none() {
        return Err(AppError::Validation(
            "a rejection carries a reason — somebody spent an hour on this".into(),
        ));
    }

    let row: Option<(Uuid, Uuid, String)> = sqlx::query_as(
        "SELECT a.mission_id, a.user_id, m.status
           FROM mission_applications a
           JOIN missions m ON m.id = a.mission_id
          WHERE a.id = $1",
    )
    .bind(application_id)
    .fetch_optional(db)
    .await?;
    let (mission_id, applicant, mission_status) =
        row.ok_or_else(|| AppError::NotFound("application not found".into()))?;

    if status == "selected"
        && !matches!(mission_status.as_str(), "published" | "applications_closed")
    {
        return Err(AppError::Validation(format!(
            "this mission is {mission_status} — somebody is already on it"
        )));
    }

    let mut tx = db.begin().await?;
    sqlx::query(
        "UPDATE mission_applications
            SET status = $2, decision_reason = $3, decided_by = $4, decided_at = NOW()
          WHERE id = $1",
    )
    .bind(application_id)
    .bind(status)
    .bind(reason)
    .bind(decider)
    .execute(&mut *tx)
    .await?;

    if status == "selected" {
        let charity: bool = sqlx::query_scalar("SELECT charity_brief FROM missions WHERE id = $1")
            .bind(mission_id)
            .fetch_one(&mut *tx)
            .await?;
        let commission = commission_for(db, applicant, charity).await?;
        sqlx::query(
            "UPDATE missions
                SET assigned_user_id = $2,
                    assigned_at = NOW(),
                    status = 'in_progress',
                    commission_percent = $3,
                    commission_reason = $4
              WHERE id = $1",
        )
        .bind(mission_id)
        .bind(applicant)
        .bind(bigdecimal::BigDecimal::try_from(commission.percent).unwrap_or_default())
        .bind(commission.reason)
        .execute(&mut *tx)
        .await?;

        // Everybody else's application is over. Told plainly rather than left
        // reading "submitted" forever.
        sqlx::query(
            "UPDATE mission_applications
                SET status = 'rejected',
                    decision_reason = COALESCE(decision_reason,
                        'un autre candidat a été retenu pour cette mission'),
                    decided_by = $3,
                    decided_at = NOW()
              WHERE mission_id = $1 AND id <> $2
                AND status IN ('submitted', 'shortlisted')",
        )
        .bind(mission_id)
        .bind(application_id)
        .bind(decider)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;

    // The whole schedule of instalments, after the commit and only for the
    // model that has one. A designer starting work needs to see how much each
    // round releases and how much is held to the end.
    if status == "selected"
        && let Err(err) =
            crate::services::mission_milestones::schedule_on_assignment(db, mission_id).await
    {
        tracing::warn!(%err, mission = %mission_id, "milestone schedule not raised");
    }

    let sql = format!("{APPLICATION_SELECT} WHERE a.id = $1");
    sqlx::query_as::<_, Application>(sqlx::AssertSqlSafe(sql))
        .bind(application_id)
        .fetch_one(db)
        .await
        .map_err(AppError::from)
}

pub fn is_unique_violation(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::Database(db) if db.code().as_deref() == Some("23505"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mission_never_goes_backwards() {
        assert!(allowed_transitions("draft").contains(&"published"));
        assert!(!allowed_transitions("published").contains(&"draft"));
        assert!(!allowed_transitions("in_progress").contains(&"published"));
        assert!(!allowed_transitions("closed").contains(&"in_progress"));
    }

    #[test]
    fn everything_open_can_still_be_cancelled() {
        for status in ["draft", "published", "applications_closed", "in_progress"] {
            assert!(
                allowed_transitions(status).contains(&"cancelled"),
                "{status} must be cancellable"
            );
        }
        // Except once delivered: disputing delivered work is a different flow
        // from cancelling a mission nobody started.
        //
        // This is a money rule before it is a workflow rule. Cancelling is
        // what returns the escrow, and it returns it to the client — so a
        // client who could cancel after delivery would accept the work, cancel
        // the mission and take the payment back. An arbiter can, through
        // `Decider::Arbiter`; nobody reaching this table can.
        assert!(!allowed_transitions("delivered").contains(&"cancelled"));
    }

    #[test]
    fn applications_can_reopen_but_work_cannot() {
        assert!(allowed_transitions("applications_closed").contains(&"published"));
        assert!(!allowed_transitions("delivered").contains(&"in_progress"));
    }
}
