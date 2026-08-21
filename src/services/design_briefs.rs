//! Where design work comes from.
//!
//! ## Why this exists at all
//!
//! A code challenge arrives on its own: somebody opens an issue, the ingestor
//! reads the label, a slice appears. Design has no such source. Nobody files
//! "the contrast on this settings page is unreadable" as a ticket with a
//! `design` label, and the projects that would benefit most are the ones with
//! no designer to notice.
//!
//! So the source is editorial, and this is the queue: somebody writes a brief,
//! somebody reads it, it becomes a slice.
//!
//! ## Why a published brief becomes a slice
//!
//! Because that is what the review loop runs on. `challenge_templates` has a
//! community-proposal flow already, and the design catalogue seeds a hundred
//! and thirty of them — but those are exercises with a rubric and no critique
//! rounds. A brief needs an orientation, a subtype, a reviewer family and a
//! round count, none of which a template has.
//!
//! ## Who may propose
//!
//! Anybody with a completed profile. Not a new capability: `issue_proposer`
//! already means "has had community proposals published", and it is *earned*
//! by proposing — gating proposals on it would mean nobody could ever earn it.
//!
//! The gate is at publication, where it belongs, and it is a person reading
//! the brief.

use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::models::DesignSubtype;

/// What a published brief pays its author.
///
/// Setting work for other people leaves no deliverable, earns no craft score
/// and is invisible on a profile — which is exactly how a community runs out
/// of briefs. Twenty fragments is not a wage; it is an acknowledgement that
/// the hour existed.
pub const FRAGMENTS_ON_PUBLICATION: i32 = 20;

/// And what it pays when the work it set gets validated.
///
/// Larger than the publication reward on purpose. Anybody can write a brief;
/// writing one that somebody finished is the harder and rarer thing, and it is
/// the only signal that separates a good brief from a plausible one.
pub const FRAGMENTS_ON_FIRST_VALIDATION: i32 = 30;

/// The slug of the project curated briefs land in.
///
/// One project rather than one per brief: a project is a body of work with an
/// owner and a repository, and a brief has neither. This is a shelf, and
/// calling it what it is beats inventing forty empty projects.
pub const CURATED_PROJECT_SLUG: &str = "skilluv-design-briefs";

#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Proposal {
    pub id: Uuid,
    pub proposed_by: Uuid,
    pub author_username: Option<String>,
    pub title: String,
    pub brief_md: String,
    pub orientation_id: Uuid,
    pub orientation_slug: Option<String>,
    pub design_subtype: String,
    pub difficulty: i16,
    pub estimated_hours: Option<i32>,
    pub expected_rounds: Option<i16>,
    pub format: String,
    pub status: String,
    pub review_feedback: Option<String>,
    pub published_slice_id: Option<Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, serde::Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ProposeInput {
    pub title: String,
    /// The brief, following one of the thirteen family templates in
    /// `docs/design/BRIEF-TEMPLATES.md`.
    pub brief_md: String,
    /// The trade this is for, by slug.
    pub orientation_slug: String,
    pub design_subtype: String,
    pub difficulty: i16,
    #[serde(default)]
    pub estimated_hours: Option<i32>,
    #[serde(default)]
    pub expected_rounds: Option<i16>,
    /// `individual` or `contest`.
    #[serde(default = "default_format")]
    pub format: String,
}

fn default_format() -> String {
    "individual".to_string()
}

/// The two formats a brief can be answered in.
pub const FORMATS: &[&str] = &["individual", "contest"];

/// The shortest brief that is worth answering.
///
/// Two hundred characters. Not a style rule: a brief shorter than this cannot
/// carry a context, a constraint and a deliverable list, and answers to it
/// diverge — after which the reviewer is arbitrating on taste, which is the
/// failure the whole grid system exists to prevent.
pub const MIN_BRIEF_CHARS: usize = 200;

/// Write a brief and put it in the queue.
pub async fn propose(
    db: &PgPool,
    author_id: Uuid,
    input: ProposeInput,
) -> Result<Proposal, AppError> {
    let title = input.title.trim();
    if title.chars().count() < 8 {
        return Err(AppError::Validation(
            "un titre de brief fait au moins huit caractères".into(),
        ));
    }
    crate::validators::check_max_len(title, "title", 160)?;

    let brief = input.brief_md.trim();
    if brief.chars().count() < MIN_BRIEF_CHARS {
        return Err(AppError::Validation(format!(
            "un brief fait au moins {MIN_BRIEF_CHARS} caractères : en dessous, il ne porte ni \
             contexte, ni contrainte, ni liste de livrables, et les réponses divergent"
        )));
    }
    crate::validators::check_max_len(brief, "brief_md", 20_000)?;

    if DesignSubtype::parse(&input.design_subtype).is_none() {
        return Err(AppError::Validation(format!(
            "'{}' n'est pas un sous-type design",
            input.design_subtype
        )));
    }
    if !FORMATS.contains(&input.format.as_str()) {
        return Err(AppError::Validation(format!(
            "format must be one of: {}",
            FORMATS.join(", ")
        )));
    }
    if !(1..=5).contains(&input.difficulty) {
        return Err(AppError::Validation("difficulty goes from 1 to 5".into()));
    }

    // The trade has to be a live design one. An archived slug would produce a
    // brief nobody can be routed to, which is the one thing a brief must do.
    let orientation_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM orientations
          WHERE slug = $1 AND primary_domain = 'design' AND is_archived = FALSE",
    )
    .bind(&input.orientation_slug)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| {
        AppError::Validation(format!(
            "'{}' n'est pas un métier design ouvert",
            input.orientation_slug
        ))
    })?;

    let id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO design_brief_proposals
            (proposed_by, title, brief_md, orientation_id, design_subtype,
             difficulty, estimated_hours, expected_rounds, format)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        RETURNING id
        "#,
    )
    .bind(author_id)
    .bind(title)
    .bind(brief)
    .bind(orientation_id)
    .bind(&input.design_subtype)
    .bind(input.difficulty)
    .bind(input.estimated_hours)
    .bind(input.expected_rounds)
    .bind(&input.format)
    .fetch_one(db)
    .await?;

    load(db, id).await
}

/// Briefs waiting for somebody to read them, oldest first.
pub async fn queue(db: &PgPool, limit: i64) -> Result<Vec<Proposal>, AppError> {
    // The projection is written out in each of the three readers rather than
    // composed from a constant: sqlx refuses a query built with `format!`, and
    // it is right to — a projection that can be interpolated is a projection
    // that will be, eventually, with something that came from a request.
    let rows = sqlx::query_as::<_, Proposal>(
        r#"
    SELECT p.id, p.proposed_by, u.username AS author_username, p.title, p.brief_md,
           p.orientation_id, o.slug AS orientation_slug, p.design_subtype,
           p.difficulty, p.estimated_hours, p.expected_rounds, p.format,
           p.status, p.review_feedback, p.published_slice_id, p.created_at
      FROM design_brief_proposals p
      LEFT JOIN users u ON u.id = p.proposed_by
      LEFT JOIN orientations o ON o.id = p.orientation_id
     WHERE p.status = 'pending'
     ORDER BY p.created_at ASC
     LIMIT $1
        "#,
    )
    .bind(limit.clamp(1, 100))
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// Somebody's own briefs, whatever became of them.
pub async fn mine(db: &PgPool, author_id: Uuid) -> Result<Vec<Proposal>, AppError> {
    let rows = sqlx::query_as::<_, Proposal>(
        r#"
    SELECT p.id, p.proposed_by, u.username AS author_username, p.title, p.brief_md,
           p.orientation_id, o.slug AS orientation_slug, p.design_subtype,
           p.difficulty, p.estimated_hours, p.expected_rounds, p.format,
           p.status, p.review_feedback, p.published_slice_id, p.created_at
      FROM design_brief_proposals p
      LEFT JOIN users u ON u.id = p.proposed_by
      LEFT JOIN orientations o ON o.id = p.orientation_id
     WHERE p.proposed_by = $1
     ORDER BY p.created_at DESC
     LIMIT 100
        "#,
    )
    .bind(author_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

async fn load(db: &PgPool, id: Uuid) -> Result<Proposal, AppError> {
    sqlx::query_as::<_, Proposal>(
        r#"
    SELECT p.id, p.proposed_by, u.username AS author_username, p.title, p.brief_md,
           p.orientation_id, o.slug AS orientation_slug, p.design_subtype,
           p.difficulty, p.estimated_hours, p.expected_rounds, p.format,
           p.status, p.review_feedback, p.published_slice_id, p.created_at
      FROM design_brief_proposals p
      LEFT JOIN users u ON u.id = p.proposed_by
      LEFT JOIN orientations o ON o.id = p.orientation_id
     WHERE p.id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| AppError::NotFound("no such brief".into()))
}

/// What `publish` needs off a pending row: author, title, brief, trade,
/// subtype, difficulty, hours, rounds.
type PendingBrief = (
    Uuid,
    String,
    String,
    Uuid,
    String,
    i16,
    Option<i32>,
    Option<i16>,
);

/// Accept a brief and turn it into work somebody can claim.
///
/// Everything happens in one transaction: a brief marked published whose slice
/// was never created is a brief that looks done and produces nothing, and it
/// would be discovered by a designer clicking a dead link.
pub async fn publish(db: &PgPool, id: Uuid, reviewer_id: Uuid) -> Result<Proposal, AppError> {
    let mut tx = db.begin().await?;

    let proposal: Option<PendingBrief> = sqlx::query_as(
        "SELECT proposed_by, title, brief_md, orientation_id, design_subtype,
                    difficulty, estimated_hours, expected_rounds
               FROM design_brief_proposals
              WHERE id = $1 AND status = 'pending'
                FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;

    let (author, title, brief, orientation_id, subtype, difficulty, hours, rounds) = proposal
        .ok_or_else(|| {
            AppError::Conflict("this brief is not waiting to be read any more".into())
        })?;

    let project_id = curated_project(&mut tx, reviewer_id).await?;

    let slice_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO project_slices
            (project_id, slice_type, title, description, primary_domain, difficulty,
             estimated_hours, status, design_subtype, design_expected_rounds,
             orientation_id, created_by_user_id, ingested_from)
        VALUES ($1, 'design_artifact', $2, $3, 'design', $4, $5, 'open', $6, $7, $8, $9, 'manual')
        RETURNING id
        "#,
    )
    .bind(project_id)
    .bind(&title)
    .bind(&brief)
    .bind(difficulty)
    .bind(hours)
    .bind(&subtype)
    .bind(rounds)
    .bind(orientation_id)
    .bind(author)
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query(
        "UPDATE design_brief_proposals
            SET status = 'published', published_slice_id = $2,
                reviewed_by = $3, reviewed_at = NOW(), updated_at = NOW()
          WHERE id = $1",
    )
    .bind(id)
    .bind(slice_id)
    .bind(reviewer_id)
    .execute(&mut *tx)
    .await?;

    // The author's acknowledgement, inside the transaction: fragments awarded
    // for a publication that then rolled back would be fragments for nothing.
    sqlx::query("UPDATE users SET total_fragments = total_fragments + $2 WHERE id = $1")
        .bind(author)
        .bind(FRAGMENTS_ON_PUBLICATION)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    // Outside: the badge engine reads committed rows, and a failure here is a
    // recompute away rather than a lost publication.
    let db_clone = db.clone();
    tokio::spawn(async move {
        if let Err(e) =
            crate::services::badge_engine::recompute_badges_for_user(&db_clone, author).await
        {
            tracing::warn!(user_id = %author, error = %e, "badge recompute after a brief failed");
        }
    });

    if let Err(e) = crate::services::notify::send(
        crate::services::notify::Ctx::db_only(db),
        crate::services::notify::Recipient::User(author),
        "design.brief_published",
    )
    .arg("brief", title)
    .payload(serde_json::json!({ "slice_id": slice_id, "proposal_id": id }))
    .execute()
    .await
    {
        tracing::warn!(%id, error = %e, "brief publication notice not delivered");
    }

    load(db, id).await
}

/// Refuse a brief, saying why.
pub async fn reject(
    db: &PgPool,
    id: Uuid,
    reviewer_id: Uuid,
    feedback: &str,
) -> Result<Proposal, AppError> {
    let feedback = feedback.trim();
    if feedback.chars().count() < 20 {
        return Err(AppError::Validation(
            "dis pourquoi en vingt caractères au moins : un refus sans raison est un refus qui \
             revient"
                .into(),
        ));
    }
    crate::validators::check_max_len(feedback, "review_feedback", 4000)?;

    let updated = sqlx::query(
        "UPDATE design_brief_proposals
            SET status = 'rejected', review_feedback = $2,
                reviewed_by = $3, reviewed_at = NOW(), updated_at = NOW()
          WHERE id = $1 AND status = 'pending'",
    )
    .bind(id)
    .bind(feedback)
    .bind(reviewer_id)
    .execute(db)
    .await?;

    if updated.rows_affected() == 0 {
        return Err(AppError::Conflict(
            "this brief is not waiting to be read any more".into(),
        ));
    }

    let proposal = load(db, id).await?;

    if let Err(e) = crate::services::notify::send(
        crate::services::notify::Ctx::db_only(db),
        crate::services::notify::Recipient::User(proposal.proposed_by),
        "design.brief_rejected",
    )
    .arg("brief", proposal.title.clone())
    .payload(serde_json::json!({ "proposal_id": id }))
    .execute()
    .await
    {
        tracing::warn!(%id, error = %e, "brief rejection notice not delivered");
    }

    Ok(proposal)
}

/// Take back a brief nobody has read yet.
pub async fn withdraw(db: &PgPool, id: Uuid, author_id: Uuid) -> Result<(), AppError> {
    let updated = sqlx::query(
        "UPDATE design_brief_proposals
            SET status = 'withdrawn', updated_at = NOW()
          WHERE id = $1 AND proposed_by = $2 AND status = 'pending'",
    )
    .bind(id)
    .bind(author_id)
    .execute(db)
    .await?;

    if updated.rows_affected() == 0 {
        return Err(AppError::Conflict(
            "only a brief of yours that nobody has read yet can be withdrawn".into(),
        ));
    }
    Ok(())
}

/// Pay a brief's author the first time the work it set is validated.
///
/// Called from the design review loop. Idempotent by construction: it pays
/// only when the slice has exactly one verified deliverable, which is true
/// once.
pub async fn reward_author_on_first_validation(
    db: &PgPool,
    slice_id: Uuid,
) -> Result<(), AppError> {
    let author: Option<Uuid> = sqlx::query_scalar(
        r#"
        SELECT p.proposed_by
          FROM design_brief_proposals p
         WHERE p.published_slice_id = $1
           AND (SELECT count(*) FROM deliverables d
                 WHERE d.slice_id = $1 AND d.verification_status = 'verified') = 1
        "#,
    )
    .bind(slice_id)
    .fetch_optional(db)
    .await?;

    let Some(author) = author else {
        return Ok(());
    };

    sqlx::query("UPDATE users SET total_fragments = total_fragments + $2 WHERE id = $1")
        .bind(author)
        .bind(FRAGMENTS_ON_FIRST_VALIDATION)
        .execute(db)
        .await?;
    Ok(())
}

/// The shelf curated briefs land on, created the first time one is published.
///
/// One project rather than one per brief: a project is a body of work with an
/// owner and a repository, and a brief has neither.
async fn curated_project(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    owner_id: Uuid,
) -> Result<Uuid, AppError> {
    if let Some(id) = sqlx::query_scalar::<_, Uuid>("SELECT id FROM projects WHERE slug = $1")
        .bind(CURATED_PROJECT_SLUG)
        .fetch_optional(&mut **tx)
        .await?
    {
        return Ok(id);
    }

    let id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO projects (slug, name, description, owner_type, owner_id,
                              skill_domains, curated_by_admin, looking_for_contributors)
        VALUES ($1, 'Briefs design Skilluv',
                'Les briefs écrits par la communauté et retenus par la curation. Design n''a pas
de source d''ingestion comme le code en a avec GitHub : cette étagère est la source.',
                'user', $2, ARRAY['design'], TRUE, TRUE)
        RETURNING id
        "#,
    )
    .bind(CURATED_PROJECT_SLUG)
    .bind(owner_id)
    .fetch_one(&mut **tx)
    .await?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setting_work_pays_less_than_the_work_being_finished() {
        // Anybody can write a brief; writing one somebody finished is the
        // harder and rarer thing, and the only signal that separates a good
        // brief from a plausible one.
        const { assert!(FRAGMENTS_ON_FIRST_VALIDATION > FRAGMENTS_ON_PUBLICATION) };
    }

    #[test]
    fn the_two_formats_are_the_two_weeks() {
        // A brief claimed by one person and a brief answered by many are
        // different weeks, and the brief has to say which before anybody
        // starts.
        assert_eq!(FORMATS, ["individual", "contest"]);
        assert_eq!(default_format(), "individual");
    }
}
