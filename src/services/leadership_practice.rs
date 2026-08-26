//! The leadership domain: redaction, retrospectives, coordination, cohorts.
//!
//! Four things a leader is judged on that no other domain records, and one
//! rule that runs through all of them: **a claim about other people is
//! checkable, or it is refused.**
//!
//!   * **redaction** — what can be shown of a document written inside an
//!     organisation, declared by its author and confirmed by somebody else;
//!   * **retrospectives** — not the hour in the room, the action items that
//!     were still being closed three months later;
//!   * **coordination** — what a document commits, and whether the people it
//!     commits have said so;
//!   * **cohorts** — who joined, who finished, and the denominator travelling
//!     with the rate.
//!
//! ## Why so much of this is somebody else's act
//!
//! Every other domain's proof can be produced alone: write the code, ship the
//! module, find the defect. Leadership's cannot, and a domain that let it be
//! would be a domain of unfalsifiable claims — which is what leadership
//! credentials mostly are elsewhere.
//!
//! So the confirmations here are all done by a second person: the reviewer
//! confirms the redaction, the steward acknowledges the commitment, the
//! mentee's graduation is recorded by the lead who is not a member. None of
//! them can be self-served, and that is the design rather than an oversight.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

/// The five families of leadership review. Mirrors `orientations.reviewer_group`.
pub const REVIEWER_GROUPS: &[&str] = &["delivery", "technical", "people", "community", "teaching"];

/// What a leadership artefact can be. Mirrors migration 0460's CHECK.
pub const SUBTYPES: &[&str] = &[
    "roadmap",
    "prd",
    "rfc",
    "adr",
    "delivery_plan",
    "retrospective",
    "playbook",
    "career_ladder",
    "hiring_process",
    "team_health_audit",
    "community_strategy",
    "cohort_curriculum",
    "okrs_doc",
];

pub const REDACTION_STATES: &[&str] = &["public", "anonymised", "confidential"];

pub const RETRO_FORMATS: &[&str] = &[
    "start_stop_continue",
    "four_ls",
    "sailboat",
    "mad_sad_glad",
    "timeline",
    "other",
];

pub const LINK_KINDS: &[&str] = &["commits", "depends_on", "coordinates", "references"];

pub const LEAVE_REASONS: &[&str] = &[
    "schedule",
    "level_mismatch",
    "personal",
    "found_work",
    "inactive",
    "other",
];

/// The shortest set of retrospective notes the database will accept.
///
/// Mirrors the CHECK on `leadership_retrospectives.insights_md`, so a refusal
/// is a message somebody can act on rather than a constraint violation.
const MIN_INSIGHTS_LEN: usize = 200;

// ═══════════════════════════════════════════════════════════════════
// Redaction
// ═══════════════════════════════════════════════════════════════════

/// The author saying they have rewritten the document so nobody can be
/// identified.
///
/// A claim, and the attestation waits for a second reader as well. Only the
/// person who holds the slice can declare, because it is a statement about
/// what they did to their own text.
pub async fn declare_redaction(db: &PgPool, user_id: Uuid, slice_id: Uuid) -> Result<(), AppError> {
    let updated = sqlx::query(
        r#"
        UPDATE project_slices
           SET redaction_declared_at = COALESCE(redaction_declared_at, NOW())
         WHERE id = $1
           AND slice_type = 'leadership_artifact'
           AND redaction_state = 'anonymised'
           AND (claimed_by_user_id = $2 OR created_by_user_id = $2)
        "#,
    )
    .bind(slice_id)
    .bind(user_id)
    .execute(db)
    .await?;

    if updated.rows_affected() == 0 {
        return Err(AppError::Validation(
            "declaring a redaction needs an anonymised leadership artefact you hold — \
             a public document has nothing to declare and a confidential one publishes \
             nothing"
                .into(),
        ));
    }
    Ok(())
}

/// A reviewer saying they have read it and nobody in it is identifiable.
///
/// Never the author. The whole value of this confirmation is that a second
/// person looked, and a self-confirmation would make the state a formality.
pub async fn confirm_redaction(
    db: &PgPool,
    reviewer_id: Uuid,
    slice_id: Uuid,
) -> Result<(), AppError> {
    let updated = sqlx::query(
        r#"
        UPDATE project_slices ps
           SET redaction_confirmed_by = $2, redaction_confirmed_at = NOW()
         WHERE ps.id = $1
           AND ps.slice_type = 'leadership_artifact'
           AND ps.redaction_state = 'anonymised'
           AND ps.redaction_declared_at IS NOT NULL
           AND COALESCE(ps.claimed_by_user_id, ps.created_by_user_id) <> $2
        "#,
    )
    .bind(slice_id)
    .bind(reviewer_id)
    .execute(db)
    .await?;

    if updated.rows_affected() == 0 {
        return Err(AppError::Validation(
            "confirming a redaction needs an anonymised artefact whose author has \
             declared it, and whose author is not you"
                .into(),
        ));
    }
    Ok(())
}

/// Record that an organisation took the proposal up.
///
/// Only on a written decision, and only by the person who holds the slice —
/// they are the one who can point at where it landed. The evidence URL is
/// required unless the artefact is confidential, which the schema enforces
/// and this repeats so the message is readable.
pub async fn record_adoption(
    db: &PgPool,
    user_id: Uuid,
    slice_id: Uuid,
    evidence_url: Option<&str>,
) -> Result<(), AppError> {
    if let Some(url) = evidence_url {
        crate::validators::validate_url(url, "evidence_url", 500)?;
        if !url.starts_with("https://") {
            return Err(AppError::Validation(
                "adoption evidence has to be a public https link".into(),
            ));
        }
    }

    let updated = sqlx::query(
        r#"
        UPDATE project_slices
           SET leadership_adopted_at = COALESCE(leadership_adopted_at, NOW()),
               leadership_adoption_evidence_url =
                   COALESCE($3, leadership_adoption_evidence_url)
         WHERE id = $1
           AND slice_type = 'leadership_artifact'
           AND leadership_subtype IN ('rfc', 'adr')
           AND (claimed_by_user_id = $2 OR created_by_user_id = $2)
           AND ($3 IS NOT NULL
                OR leadership_adoption_evidence_url IS NOT NULL
                OR redaction_state = 'confidential')
        "#,
    )
    .bind(slice_id)
    .bind(user_id)
    .bind(evidence_url)
    .execute(db)
    .await?;

    if updated.rows_affected() == 0 {
        return Err(AppError::Validation(
            "recording an adoption needs a written decision of yours, and — unless it \
             is confidential — a link to where it landed"
                .into(),
        ));
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════
// Retrospectives
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Retrospective {
    pub id: Uuid,
    pub slice_id: Option<Uuid>,
    pub facilitator_user_id: Uuid,
    pub title: String,
    pub format: String,
    pub format_note: Option<String>,
    pub participants_count: i16,
    pub held_on: chrono::NaiveDate,
    pub insights_md: String,
    pub shared_with_participants_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

const RETRO_SELECT: &str = r#"
    SELECT id, slice_id, facilitator_user_id, title, format, format_note,
           participants_count, held_on, insights_md,
           shared_with_participants_at, created_at
      FROM leadership_retrospectives
"#;

#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
pub struct RetrospectiveInput {
    /// The slice this is filed under, when it is being submitted for review.
    /// Absent for a retrospective recorded to carry its actions — somebody
    /// tracking their own team's follow-through, which is a use worth
    /// allowing.
    #[serde(default)]
    pub slice_id: Option<Uuid>,
    pub title: String,
    pub format: String,
    #[serde(default)]
    pub format_note: Option<String>,
    pub participants_count: i16,
    pub held_on: chrono::NaiveDate,
    pub insights_md: String,
}

pub async fn record_retrospective(
    db: &PgPool,
    user_id: Uuid,
    input: RetrospectiveInput,
) -> Result<Retrospective, AppError> {
    if input.title.trim().is_empty() {
        return Err(AppError::Validation("a retrospective needs a title".into()));
    }
    crate::validators::check_max_len(&input.title, "title", 200)?;

    if !RETRO_FORMATS.contains(&input.format.as_str()) {
        return Err(AppError::Validation(format!(
            "format must be one of: {}",
            RETRO_FORMATS.join(", ")
        )));
    }
    if input.format == "other"
        && input
            .format_note
            .as_deref()
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
    {
        return Err(AppError::Validation(
            "an 'other' format says which — the shape decides what the notes can \
             contain, and a synthesis across two shapes compares different things"
                .into(),
        ));
    }
    if !(2..=200).contains(&input.participants_count) {
        return Err(AppError::Validation(
            "participants must be between 2 and 200 — a retrospective of one is a note \
             to self"
                .into(),
        ));
    }
    if input.insights_md.trim().chars().count() < MIN_INSIGHTS_LEN {
        return Err(AppError::Validation(format!(
            "the notes have to be at least {MIN_INSIGHTS_LEN} characters — a heading and \
             three bullet points is a meeting that happened, not a retrospective that \
             was facilitated"
        )));
    }

    // A slice, when given, has to be this person's and a retrospective one.
    if let Some(slice_id) = input.slice_id {
        let ok: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM project_slices
                 WHERE id = $1
                   AND slice_type = 'leadership_artifact'
                   AND leadership_subtype = 'retrospective'
                   AND (claimed_by_user_id = $2 OR created_by_user_id = $2))
            "#,
        )
        .bind(slice_id)
        .bind(user_id)
        .fetch_one(db)
        .await?;

        if !ok {
            return Err(AppError::Validation(
                "that slice is not a retrospective artefact you hold".into(),
            ));
        }
    }

    Ok(sqlx::query_as(
        r#"
        INSERT INTO leadership_retrospectives
            (slice_id, facilitator_user_id, title, format, format_note,
             participants_count, held_on, insights_md)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING id, slice_id, facilitator_user_id, title, format, format_note,
                  participants_count, held_on, insights_md,
                  shared_with_participants_at, created_at
        "#,
    )
    .bind(input.slice_id)
    .bind(user_id)
    .bind(input.title.trim())
    .bind(&input.format)
    .bind(input.format_note.as_deref().map(str::trim))
    .bind(input.participants_count)
    .bind(input.held_on)
    .bind(input.insights_md.trim())
    .fetch_one(db)
    .await?)
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct RetrospectiveAction {
    pub id: Uuid,
    pub retrospective_id: Uuid,
    pub description: String,
    pub owner_user_id: Option<Uuid>,
    pub owner_label: Option<String>,
    pub due_on: Option<chrono::NaiveDate>,
    pub done_at: Option<chrono::DateTime<chrono::Utc>>,
    pub abandoned_reason: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

const ACTION_SELECT: &str = r#"
    SELECT id, retrospective_id, description, owner_user_id, owner_label,
           due_on, done_at, abandoned_reason, created_at
      FROM leadership_retrospective_actions
"#;

#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
pub struct ActionInput {
    pub description: String,
    #[serde(default)]
    pub owner_user_id: Option<Uuid>,
    /// The owner's name when they have no account here. One of the two is
    /// required: an action with nobody on it is an intention.
    #[serde(default)]
    pub owner_label: Option<String>,
    #[serde(default)]
    pub due_on: Option<chrono::NaiveDate>,
}

pub async fn add_action(
    db: &PgPool,
    user_id: Uuid,
    retrospective_id: Uuid,
    input: ActionInput,
) -> Result<RetrospectiveAction, AppError> {
    if input.description.trim().is_empty() {
        return Err(AppError::Validation(
            "an action item says what is to be done".into(),
        ));
    }
    if input.owner_user_id.is_none()
        && input
            .owner_label
            .as_deref()
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
    {
        return Err(AppError::Validation(
            "an action item has an owner — with nobody on it, it is an intention".into(),
        ));
    }

    let owns = facilitates(db, user_id, retrospective_id).await?;
    if !owns {
        return Err(AppError::NotFound(
            "no retrospective of yours under that id".into(),
        ));
    }

    Ok(sqlx::query_as(
        r#"
        INSERT INTO leadership_retrospective_actions
            (retrospective_id, description, owner_user_id, owner_label, due_on)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, retrospective_id, description, owner_user_id, owner_label,
                  due_on, done_at, abandoned_reason, created_at
        "#,
    )
    .bind(retrospective_id)
    .bind(input.description.trim())
    .bind(input.owner_user_id)
    .bind(input.owner_label.as_deref().map(str::trim))
    .bind(input.due_on)
    .fetch_one(db)
    .await?)
}

/// Close an action item, or drop it with a reason.
///
/// Dropping is not a lesser outcome. Deciding not to do something, in
/// writing, with a reason, is a decision — the follow-through view counts it
/// as resolved, because a rule that punished it would teach people to leave
/// action items open forever instead.
pub async fn resolve_action(
    db: &PgPool,
    user_id: Uuid,
    action_id: Uuid,
    abandoned_reason: Option<&str>,
) -> Result<RetrospectiveAction, AppError> {
    let reason = abandoned_reason.map(str::trim).filter(|r| !r.is_empty());

    let updated: Option<RetrospectiveAction> = sqlx::query_as(
        r#"
        UPDATE leadership_retrospective_actions a
           SET done_at = CASE WHEN $3::TEXT IS NULL
                              THEN COALESCE(a.done_at, NOW())
                              ELSE NULL END,
               abandoned_reason = $3
          FROM leadership_retrospectives r
         WHERE a.id = $1
           AND r.id = a.retrospective_id
           AND r.facilitator_user_id = $2
        RETURNING a.id, a.retrospective_id, a.description, a.owner_user_id,
                  a.owner_label, a.due_on, a.done_at, a.abandoned_reason,
                  a.created_at
        "#,
    )
    .bind(action_id)
    .bind(user_id)
    .bind(reason)
    .fetch_optional(db)
    .await?;

    updated.ok_or_else(|| {
        AppError::NotFound("no action item on a retrospective of yours under that id".into())
    })
}

async fn facilitates(db: &PgPool, user_id: Uuid, retrospective_id: Uuid) -> Result<bool, AppError> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM leadership_retrospectives
              WHERE id = $1 AND facilitator_user_id = $2)",
    )
    .bind(retrospective_id)
    .bind(user_id)
    .fetch_one(db)
    .await?)
}

pub async fn retrospectives_for(
    db: &PgPool,
    user_id: Uuid,
) -> Result<Vec<Retrospective>, AppError> {
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{RETRO_SELECT} WHERE facilitator_user_id = $1 ORDER BY held_on DESC LIMIT 100"
    )))
    .bind(user_id)
    .fetch_all(db)
    .await?)
}

pub async fn actions_for(
    db: &PgPool,
    retrospective_id: Uuid,
) -> Result<Vec<RetrospectiveAction>, AppError> {
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{ACTION_SELECT} WHERE retrospective_id = $1 ORDER BY due_on NULLS LAST, created_at"
    )))
    .bind(retrospective_id)
    .fetch_all(db)
    .await?)
}

/// Whether a retrospective's actions actually landed, with the figures.
pub async fn followthrough_for(
    db: &PgPool,
    retrospective_id: Uuid,
) -> Result<Option<serde_json::Value>, AppError> {
    Ok(sqlx::query_scalar(
        "SELECT jsonb_build_object(
                    'actions_total', actions_total,
                    'actions_resolved', actions_resolved,
                    'actions_resolved_in_window', actions_resolved_in_window,
                    'followed_through', followed_through)
           FROM leadership_retrospective_followthrough
          WHERE retrospective_id = $1",
    )
    .bind(retrospective_id)
    .fetch_optional(db)
    .await?)
}

// ═══════════════════════════════════════════════════════════════════
// Coordination
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ArtifactLink {
    pub id: Uuid,
    pub leadership_slice_id: Uuid,
    pub linked_project_id: Uuid,
    pub link_kind: String,
    pub note: Option<String>,
    pub acknowledged_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
pub struct LinkInput {
    pub linked_project_id: Uuid,
    pub link_kind: String,
    #[serde(default)]
    pub note: Option<String>,
}

/// Record what a leadership document coordinates.
pub async fn link_project(
    db: &PgPool,
    user_id: Uuid,
    slice_id: Uuid,
    input: LinkInput,
) -> Result<ArtifactLink, AppError> {
    if !LINK_KINDS.contains(&input.link_kind.as_str()) {
        return Err(AppError::Validation(format!(
            "link_kind must be one of: {}",
            LINK_KINDS.join(", ")
        )));
    }

    let note = input
        .note
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty());
    if matches!(input.link_kind.as_str(), "commits" | "depends_on") && note.is_none() {
        return Err(AppError::Validation(
            "say what is being committed or depended on — a commitment nobody wrote \
             down is a commitment nobody can dispute"
                .into(),
        ));
    }

    let holds: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM project_slices
             WHERE id = $1
               AND slice_type = 'leadership_artifact'
               AND (claimed_by_user_id = $2 OR created_by_user_id = $2))
        "#,
    )
    .bind(slice_id)
    .bind(user_id)
    .fetch_one(db)
    .await?;

    if !holds {
        return Err(AppError::Validation(
            "linking projects needs a leadership artefact you hold".into(),
        ));
    }

    Ok(sqlx::query_as(
        r#"
        INSERT INTO leadership_artifact_links
            (leadership_slice_id, linked_project_id, link_kind, note, created_by)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (leadership_slice_id, linked_project_id, link_kind) DO UPDATE
            SET note = EXCLUDED.note
        RETURNING id, leadership_slice_id, linked_project_id, link_kind, note,
                  acknowledged_at, created_at
        "#,
    )
    .bind(slice_id)
    .bind(input.linked_project_id)
    .bind(&input.link_kind)
    .bind(note)
    .bind(user_id)
    .fetch_one(db)
    .await?)
}

/// A project's steward accepting what a document commits them to.
///
/// The one act in this domain that turns a plan written *about* somebody into
/// a plan agreed *with* them, and the only leadership score term that cannot
/// be produced alone.
///
/// The author cannot acknowledge their own link, for the obvious reason.
pub async fn acknowledge_link(
    db: &PgPool,
    steward_id: Uuid,
    link_id: Uuid,
) -> Result<ArtifactLink, AppError> {
    let updated: Option<ArtifactLink> = sqlx::query_as(
        r#"
        UPDATE leadership_artifact_links l
           SET acknowledged_by = $2, acknowledged_at = NOW()
          FROM project_slices ps
         WHERE l.id = $1
           AND ps.id = l.leadership_slice_id
           AND l.link_kind IN ('commits', 'depends_on')
           AND COALESCE(ps.claimed_by_user_id, ps.created_by_user_id) <> $2
        RETURNING l.id, l.leadership_slice_id, l.linked_project_id, l.link_kind,
                  l.note, l.acknowledged_at, l.created_at
        "#,
    )
    .bind(link_id)
    .bind(steward_id)
    .fetch_optional(db)
    .await?;

    updated.ok_or_else(|| {
        AppError::Validation(
            "acknowledging needs a commitment somebody else's document makes — a weak \
             link asks nothing of you, and you cannot acknowledge your own"
                .into(),
        )
    })
}

/// How far a document reaches, and how much of that reach was agreed.
pub async fn coordination_reach(
    db: &PgPool,
    slice_id: Uuid,
) -> Result<Option<serde_json::Value>, AppError> {
    Ok(sqlx::query_scalar(
        "SELECT jsonb_build_object(
                    'title', title,
                    'projects_linked', projects_linked,
                    'projects_committed', projects_committed,
                    'commitments_acknowledged', commitments_acknowledged,
                    'commitments_outstanding', commitments_outstanding)
           FROM leadership_coordination_reach
          WHERE leadership_slice_id = $1",
    )
    .bind(slice_id)
    .fetch_optional(db)
    .await?)
}

// ═══════════════════════════════════════════════════════════════════
// Cohorts
// ═══════════════════════════════════════════════════════════════════

/// Take responsibility for a cohort.
///
/// Distinct from creating it and from being an organiser: this is the row an
/// attestation rests on, and the person who holds it is the one the outcome
/// is attributed to.
pub async fn lead_cohort(
    db: &PgPool,
    user_id: Uuid,
    cohort_id: Uuid,
    curriculum_slice_id: Option<Uuid>,
    target_domain: Option<&str>,
) -> Result<(), AppError> {
    if let Some(domain) = target_domain {
        crate::validators::check_skill_domain(domain, "target_domain")?;
    }

    // A curriculum, when named, is this person's leadership artefact.
    if let Some(slice_id) = curriculum_slice_id {
        let ok: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM project_slices
                 WHERE id = $1
                   AND slice_type = 'leadership_artifact'
                   AND leadership_subtype = 'cohort_curriculum'
                   AND (claimed_by_user_id = $2 OR created_by_user_id = $2))
            "#,
        )
        .bind(slice_id)
        .bind(user_id)
        .fetch_one(db)
        .await?;

        if !ok {
            return Err(AppError::Validation(
                "that slice is not a curriculum artefact you hold".into(),
            ));
        }
    }

    let updated = sqlx::query(
        r#"
        UPDATE cohorts
           SET led_by_user_id = $2,
               curriculum_slice_id = COALESCE($3, curriculum_slice_id),
               target_domain = COALESCE($4, target_domain)
         WHERE id = $1
           AND (led_by_user_id IS NULL OR led_by_user_id = $2)
           AND (created_by = $2 OR led_by_user_id = $2)
           AND archived_at IS NULL
        "#,
    )
    .bind(cohort_id)
    .bind(user_id)
    .bind(curriculum_slice_id)
    .bind(target_domain)
    .execute(db)
    .await?;

    if updated.rows_affected() == 0 {
        return Err(AppError::Validation(
            "leading a cohort needs one you created that nobody else already leads".into(),
        ));
    }
    Ok(())
}

/// Record that somebody finished.
///
/// Set by the lead, who is not a member. A self-declared graduation would
/// make `leadership_cohort_completed` mean nothing.
pub async fn graduate_member(
    db: &PgPool,
    lead_id: Uuid,
    cohort_id: Uuid,
    member_id: Uuid,
) -> Result<(), AppError> {
    if lead_id == member_id {
        return Err(AppError::Validation(
            "a lead does not graduate themselves".into(),
        ));
    }

    let updated = sqlx::query(
        r#"
        UPDATE cohort_members m
           SET graduated_at = COALESCE(m.graduated_at, NOW())
          FROM cohorts c
         WHERE m.cohort_id = $1 AND m.user_id = $3
           AND c.id = m.cohort_id AND c.led_by_user_id = $2
           AND m.role = 'member'
           AND m.left_at IS NULL
        "#,
    )
    .bind(cohort_id)
    .bind(lead_id)
    .bind(member_id)
    .execute(db)
    .await?;

    if updated.rows_affected() == 0 {
        return Err(AppError::Validation(
            "graduating needs a member of a cohort you lead who has not left it".into(),
        ));
    }
    Ok(())
}

/// Record that somebody left, and why.
///
/// The reason is required and is not there to police anybody: four people
/// leaving because the schedule did not work and four leaving because the
/// curriculum assumed knowledge they did not have are different facts about
/// the lead, and only the second is theirs to act on.
pub async fn record_departure(
    db: &PgPool,
    lead_id: Uuid,
    cohort_id: Uuid,
    member_id: Uuid,
    reason: &str,
    note: Option<&str>,
) -> Result<(), AppError> {
    if !LEAVE_REASONS.contains(&reason) {
        return Err(AppError::Validation(format!(
            "reason must be one of: {}",
            LEAVE_REASONS.join(", ")
        )));
    }
    let note = note.map(str::trim).filter(|n| !n.is_empty());
    if reason == "other" && note.is_none() {
        return Err(AppError::Validation("an 'other' reason says which".into()));
    }

    let updated = sqlx::query(
        r#"
        UPDATE cohort_members m
           SET left_at = COALESCE(m.left_at, NOW()),
               leave_reason = $4,
               leave_note = $5
          FROM cohorts c
         WHERE m.cohort_id = $1 AND m.user_id = $3
           AND c.id = m.cohort_id AND c.led_by_user_id = $2
           AND m.role = 'member'
           AND m.graduated_at IS NULL
        "#,
    )
    .bind(cohort_id)
    .bind(lead_id)
    .bind(member_id)
    .bind(reason)
    .bind(note)
    .execute(db)
    .await?;

    if updated.rows_affected() == 0 {
        return Err(AppError::Validation(
            "recording a departure needs a member of a cohort you lead who has not \
             graduated"
                .into(),
        ));
    }
    Ok(())
}

/// Bring the run to an end.
///
/// Distinct from `ends_at` passing: that is the planned window closing, which
/// is not the same as somebody having concluded it. The attestation waits for
/// this, so a cohort abandoned in week three never earns one.
pub async fn conclude_cohort(
    db: &PgPool,
    lead_id: Uuid,
    cohort_id: Uuid,
    note: Option<&str>,
) -> Result<serde_json::Value, AppError> {
    let updated = sqlx::query(
        r#"
        UPDATE cohorts
           SET concluded_at = COALESCE(concluded_at, NOW()),
               conclusion_note = COALESCE($3, conclusion_note)
         WHERE id = $1 AND led_by_user_id = $2
        "#,
    )
    .bind(cohort_id)
    .bind(lead_id)
    .bind(note.map(str::trim).filter(|n| !n.is_empty()))
    .execute(db)
    .await?;

    if updated.rows_affected() == 0 {
        return Err(AppError::Validation(
            "concluding needs a cohort you lead".into(),
        ));
    }

    outcomes_for_cohort(db, cohort_id)
        .await?
        .ok_or_else(|| AppError::Internal("cohort outcomes disappeared".into()))
}

/// The numbers behind a cohort, with the denominator.
pub async fn outcomes_for_cohort(
    db: &PgPool,
    cohort_id: Uuid,
) -> Result<Option<serde_json::Value>, AppError> {
    Ok(sqlx::query_scalar(
        "SELECT jsonb_build_object(
                    'cohort_id', cohort_id,
                    'slug', slug,
                    'joined_total', joined_total,
                    'graduated_total', graduated_total,
                    'left_for_work', left_for_work,
                    'concluded_at', concluded_at,
                    'led_to_the_end', led_to_the_end)
           FROM cohort_outcomes
          WHERE cohort_id = $1",
    )
    .bind(cohort_id)
    .fetch_optional(db)
    .await?)
}

/// Which trade a leadership artefact has to be reviewed by.
pub async fn reviewer_orientation_for_slice(
    db: &PgPool,
    slice_id: Uuid,
) -> Result<Option<String>, AppError> {
    Ok(sqlx::query_scalar(
        r#"
        SELECT o.slug
          FROM project_slices ps
          JOIN orientations o ON o.id = ps.orientation_id
         WHERE ps.id = $1 AND o.primary_domain = 'leadership'
        "#,
    )
    .bind(slice_id)
    .fetch_optional(db)
    .await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_subtype_the_schema_allows_is_listed_here() {
        // `SUBTYPES` is what the reference endpoint publishes. A subtype in
        // the schema and not here is one no client offers, so nobody files
        // it — and one here that earns nothing is worse.
        for subtype in SUBTYPES {
            assert!(
                crate::services::leadership_attestations::basis_for_subtype(subtype).is_some(),
                "{subtype} is offered and earns nothing"
            );
        }
    }

    #[test]
    fn the_reviewer_families_are_the_five_the_catalogue_declares() {
        assert_eq!(REVIEWER_GROUPS.len(), 5);
        // `delivery` covers two trades. The others cover one each, and the
        // grouping is in migration 0460.
        assert!(REVIEWER_GROUPS.contains(&"delivery"));
        assert!(REVIEWER_GROUPS.contains(&"teaching"));
    }

    #[test]
    fn leaving_because_it_worked_is_one_of_the_reasons() {
        // `found_work` is removed from the graduation denominator rather than
        // counted as a loss. Losing it from this list would make that branch
        // in `cohort_outcomes` unreachable.
        assert!(LEAVE_REASONS.contains(&"found_work"));
    }
}
