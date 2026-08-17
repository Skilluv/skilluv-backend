//! The Skilluv Code Awards (migration 0190).
//!
//! An edition runs through four states — draft, nominations, voting,
//! concluded — and each one permits exactly one thing. The rules that matter
//! (a nominee must be shortlisted, a vote must land in the right category, an
//! edition must be open) live in triggers, because they are true regardless
//! of which code path reaches the table. What lives here is the part a
//! trigger cannot say well: which door the caller came through, and what to
//! answer when the answer is no.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

pub const EDITION_STATUSES: &[&str] = &["draft", "nominations", "voting", "concluded"];

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Edition {
    pub id: Uuid,
    pub year: i16,
    pub status: String,
    pub community_weight: i16,
    pub jury_weight: i16,
    pub nominations_close_at: Option<chrono::DateTime<chrono::Utc>>,
    pub voting_closes_at: Option<chrono::DateTime<chrono::Utc>>,
    pub prize_amount_eur: Option<bigdecimal::BigDecimal>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Category {
    pub slug: String,
    pub name: String,
    pub description: String,
    pub subject_type: String,
    pub sort_order: i16,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Nominee {
    pub id: Uuid,
    pub category_slug: String,
    pub subject_type: String,
    pub subject_id: Uuid,
    /// The name to print: a username, a project name, or the title of the
    /// slice a deliverable answered.
    pub subject_label: Option<String>,
    pub citation: String,
    pub shortlisted: bool,
    pub community_votes: i64,
    pub jury_votes: i64,
    /// Only meaningful once voting has started. Zero before then, which is
    /// the truth rather than a placeholder.
    pub weighted_score: bigdecimal::BigDecimal,
}

pub async fn categories(db: &PgPool) -> Result<Vec<Category>, AppError> {
    let rows = sqlx::query_as::<_, Category>(
        "SELECT slug, name, description, subject_type, sort_order
           FROM award_categories WHERE is_active = TRUE
          ORDER BY sort_order, slug",
    )
    .fetch_all(db)
    .await?;
    Ok(rows)
}

pub async fn edition_of_year(db: &PgPool, year: i16) -> Result<Edition, AppError> {
    sqlx::query_as::<_, Edition>(
        "SELECT id, year, status, community_weight, jury_weight,
                nominations_close_at, voting_closes_at, prize_amount_eur
           FROM award_editions WHERE year = $1",
    )
    .bind(year)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("no Code Awards edition for {year}")))
}

/// The standings of one edition, category by category.
///
/// The subject label is resolved here rather than left to the caller: a
/// nominee list that answers with thirty UUIDs is not a shortlist anybody can
/// read.
pub async fn nominees(db: &PgPool, edition_id: Uuid) -> Result<Vec<Nominee>, AppError> {
    let rows = sqlx::query_as::<_, Nominee>(
        r#"
        SELECT n.id,
               c.slug AS category_slug,
               n.subject_type,
               n.subject_id,
               CASE n.subject_type
                   WHEN 'user' THEN (SELECT u.username FROM users u WHERE u.id = n.subject_id)
                   WHEN 'project' THEN (SELECT p.name FROM projects p WHERE p.id = n.subject_id)
                   WHEN 'deliverable' THEN (
                       SELECT COALESCE(s.title, ct.title)
                         FROM deliverables d
                         LEFT JOIN project_slices s ON s.id = d.slice_id
                         LEFT JOIN challenge_templates ct ON ct.id = d.challenge_id
                        WHERE d.id = n.subject_id
                   )
               END AS subject_label,
               n.citation,
               (n.shortlisted_at IS NOT NULL) AS shortlisted,
               COALESCE(r.community_votes, 0) AS community_votes,
               COALESCE(r.jury_votes, 0) AS jury_votes,
               COALESCE(r.weighted_score, 0) AS weighted_score
          FROM award_nominees n
          JOIN award_categories c ON c.id = n.category_id
          LEFT JOIN award_results r ON r.nominee_id = n.id
         WHERE n.edition_id = $1
         ORDER BY c.sort_order, COALESCE(r.weighted_score, 0) DESC, n.created_at ASC
        "#,
    )
    .bind(edition_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

#[derive(Debug, Clone, Deserialize)]
pub struct NominateInput {
    pub category_slug: String,
    pub subject_id: Uuid,
    pub citation: String,
}

/// Put a piece of work forward.
///
/// Open to anybody with an account, including for their own work: an award
/// that only counts what somebody else noticed rewards visibility, which is
/// the failure these awards exist to correct.
pub async fn nominate(
    db: &PgPool,
    edition_id: Uuid,
    nominator: Uuid,
    input: NominateInput,
) -> Result<Uuid, AppError> {
    let status: String = sqlx::query_scalar("SELECT status FROM award_editions WHERE id = $1")
        .bind(edition_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NotFound("edition not found".into()))?;
    if status != "nominations" {
        return Err(AppError::Validation(format!(
            "this edition is {status} — nominations are closed"
        )));
    }

    let citation = input.citation.trim();
    if citation.is_empty() {
        return Err(AppError::Validation(
            "a nomination must say why — voters cannot weigh a name".into(),
        ));
    }
    crate::validators::check_max_len(citation, "citation", 2000)?;

    let category: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT id, subject_type FROM award_categories
          WHERE slug = $1 AND is_active = TRUE",
    )
    .bind(&input.category_slug)
    .fetch_optional(db)
    .await?;
    let (category_id, subject_type) = category.ok_or_else(|| {
        AppError::NotFound(format!("no active category '{}'", input.category_slug))
    })?;

    let id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO award_nominees
            (edition_id, category_id, subject_type, subject_id, nominated_by, citation)
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (edition_id, category_id, subject_type, subject_id) DO UPDATE
            -- A second nomination of the same work is a second voice for it,
            -- not a second entry. The newer citation replaces the older one:
            -- somebody took the trouble to make the case again.
            SET citation = EXCLUDED.citation
        RETURNING id
        "#,
    )
    .bind(edition_id)
    .bind(category_id)
    .bind(&subject_type)
    .bind(input.subject_id)
    .bind(nominator)
    .bind(citation)
    .fetch_one(db)
    .await
    .map_err(subject_error)?;

    Ok(id)
}

/// Turn a database exception raised by the trigger into something the person
/// who hit the endpoint can act on.
fn subject_error(e: sqlx::Error) -> AppError {
    let message = e.to_string();
    for marker in [
        "does not exist",
        "this category nominates",
        "not on the shortlist",
        "does not match the nominee",
        "not voting",
    ] {
        if message.contains(marker) {
            // The exception text is written for a human already; passing it
            // through beats replacing it with something vaguer.
            if let Some(start) = message.find(marker) {
                let sentence: String = message[start..].lines().next().unwrap_or("").into();
                return AppError::Validation(sentence);
            }
        }
    }
    AppError::from(e)
}

/// Fix the shortlist for a category. Idempotent.
pub async fn shortlist(db: &PgPool, nominee_ids: &[Uuid]) -> Result<u64, AppError> {
    if nominee_ids.is_empty() {
        return Err(AppError::Validation(
            "a shortlist of nobody is not a shortlist".into(),
        ));
    }
    let done = sqlx::query(
        "UPDATE award_nominees SET shortlisted_at = COALESCE(shortlisted_at, NOW())
          WHERE id = ANY($1)",
    )
    .bind(nominee_ids)
    .execute(db)
    .await?;
    Ok(done.rows_affected())
}

/// Cast a vote.
///
/// The ballot is decided here, from the voter's capabilities at this moment,
/// and then frozen on the row. A juror votes on both ballots — they are also
/// a member of the community, and pretending otherwise would silently remove
/// eight people from the community count.
pub async fn vote(
    db: &PgPool,
    nominee_id: Uuid,
    voter: Uuid,
    as_jury: bool,
) -> Result<(), AppError> {
    if as_jury {
        crate::middleware::capabilities::require_capability(db, voter, "jury_tournament").await?;
    }

    let nominee: Option<(Uuid, Uuid)> =
        sqlx::query_as("SELECT edition_id, category_id FROM award_nominees WHERE id = $1")
            .bind(nominee_id)
            .fetch_optional(db)
            .await?;
    let (edition_id, category_id) =
        nominee.ok_or_else(|| AppError::NotFound("nominee not found".into()))?;

    let ballot = if as_jury { "jury" } else { "community" };
    let result = sqlx::query(
        "INSERT INTO award_votes (nominee_id, voter_id, ballot, edition_id, category_id)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(nominee_id)
    .bind(voter)
    .bind(ballot)
    .bind(edition_id)
    .bind(category_id)
    .execute(db)
    .await;

    match result {
        Ok(_) => Ok(()),
        Err(e) if is_unique_violation(&e) => Err(AppError::Validation(
            "you have already voted in this category".into(),
        )),
        Err(e) => Err(subject_error(e)),
    }
}

fn is_unique_violation(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::Database(db) if db.code().as_deref() == Some("23505"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_four_states_are_the_four_states() {
        assert_eq!(EDITION_STATUSES.len(), 4);
        assert!(EDITION_STATUSES.contains(&"nominations"));
        assert!(EDITION_STATUSES.contains(&"voting"));
    }
}
