//! The talent posts, and companies come to them.
//!
//! ## Why the rank threshold is here and not a second bar
//!
//! A posting is a claim on the attention of every company on the platform,
//! and the argument for reversing the direction is that the person's work
//! speaks for itself — which requires that some of it exists.
//!
//! The rank is the platform's existing answer to "has this person done enough
//! to be taken at their word". Inventing a separate threshold would mean two
//! definitions of the same judgement, drifting apart.

use bigdecimal::BigDecimal;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

/// The minimum rank to post. `artisan` is the first rank that requires a
/// verified attestation, which is exactly the bar this needs.
pub const MIN_RANK: &str = "artisan";

/// Ranks at or above the threshold, in order.
pub const RANKS_ALLOWED: &[&str] = &["artisan", "maitre", "doyen"];

/// What a company spends to send one. Higher than an ordinary contact: the
/// opportunity is rarer, and the monthly ceiling makes each one scarce.
pub const PITCH_COST_CREDITS: i16 = 4;

/// Shorter than this and the company is asking the person to do the
/// persuading after all.
pub const MIN_PITCH_LENGTH: usize = 200;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Posting {
    pub id: Uuid,
    pub talent_user_id: Uuid,
    pub username: String,
    pub title: String,
    pub desired_role: String,
    pub desired_domain: String,
    pub desired_orientations: Vec<String>,
    pub desired_salary_range: Option<serde_json::Value>,
    pub remote_only: bool,
    pub preferred_countries: Vec<String>,
    pub available_from: chrono::NaiveDate,
    pub not_looking_for: Option<String>,
    pub status: String,
    /// How many pitches are left this month. What a company needs to know
    /// before writing four hundred words.
    pub pitches_left_this_month: i64,
    /// Their score in the domain they are looking for work in.
    pub craft_score: Option<i32>,
    pub craft_tier: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

const POSTING_SELECT: &str = r#"
    SELECT p.id, p.talent_user_id, u.username, p.title, p.desired_role,
           p.desired_domain, p.desired_orientations, p.desired_salary_range,
           p.remote_only, p.preferred_countries, p.available_from,
           p.not_looking_for, p.status,
           GREATEST(0, p.max_pitches_per_month - (
               SELECT count(*) FROM reverse_recruitment_pitches x
                WHERE x.posting_id = p.id
                  AND x.created_at > date_trunc('month', NOW())
           )) AS pitches_left_this_month,
           cs.score AS craft_score, cs.tier_slug AS craft_tier,
           p.created_at
      FROM reverse_recruitment_postings p
      JOIN users u ON u.id = p.talent_user_id
      LEFT JOIN craft_scores cs
             ON cs.user_id = p.talent_user_id AND cs.skill_domain = p.desired_domain
"#;

#[derive(Debug, Clone, Deserialize)]
pub struct PostingInput {
    pub title: String,
    pub desired_role: String,
    pub desired_domain: String,
    #[serde(default)]
    pub desired_orientations: Vec<String>,
    #[serde(default)]
    pub desired_salary_range: Option<serde_json::Value>,
    #[serde(default)]
    pub remote_only: bool,
    #[serde(default)]
    pub preferred_countries: Vec<String>,
    pub available_from: chrono::NaiveDate,
    #[serde(default)]
    pub not_looking_for: Option<String>,
    #[serde(default = "default_ceiling")]
    pub max_pitches_per_month: i16,
}

fn default_ceiling() -> i16 {
    10
}

/// Post, or replace the posting already there.
pub async fn post(
    db: &PgPool,
    talent_user_id: Uuid,
    input: PostingInput,
) -> Result<Posting, AppError> {
    // Absent means nothing has been computed yet, which is `apprenti` — the
    // rank everybody starts at, and below the bar either way.
    let rank: Option<String> = sqlx::query_scalar("SELECT rank FROM user_ranks WHERE user_id = $1")
        .bind(talent_user_id)
        .fetch_optional(db)
        .await?;

    let rank = rank.unwrap_or_else(|| "apprenti".into());
    if !RANKS_ALLOWED.contains(&rank.as_str()) {
        return Err(AppError::Validation(format!(
            "posting here needs the rank of {MIN_RANK} — the argument for companies \
             coming to you is that your work speaks for itself, which needs some of it \
             to exist first. You are {rank}."
        )));
    }

    if input.title.trim().is_empty() || input.desired_role.trim().is_empty() {
        return Err(AppError::Validation(
            "say what you are looking for — a posting with no role is a posting nobody \
             can answer"
                .into(),
        ));
    }
    crate::validators::check_max_len(&input.title, "title", 200)?;
    crate::validators::check_max_len(&input.desired_role, "desired_role", 200)?;
    if !(1..=50).contains(&input.max_pitches_per_month) {
        return Err(AppError::Validation(
            "max_pitches_per_month must be between 1 and 50".into(),
        ));
    }

    for slug in &input.desired_orientations {
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

    let id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO reverse_recruitment_postings
            (talent_user_id, title, desired_role, desired_domain,
             desired_orientations, desired_salary_range, remote_only,
             preferred_countries, available_from, not_looking_for,
             max_pitches_per_month)
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
        ON CONFLICT (talent_user_id) DO UPDATE
            SET title = EXCLUDED.title,
                desired_role = EXCLUDED.desired_role,
                desired_domain = EXCLUDED.desired_domain,
                desired_orientations = EXCLUDED.desired_orientations,
                desired_salary_range = EXCLUDED.desired_salary_range,
                remote_only = EXCLUDED.remote_only,
                preferred_countries = EXCLUDED.preferred_countries,
                available_from = EXCLUDED.available_from,
                not_looking_for = EXCLUDED.not_looking_for,
                max_pitches_per_month = EXCLUDED.max_pitches_per_month,
                status = 'active',
                closed_reason = NULL
        RETURNING id
        "#,
    )
    .bind(talent_user_id)
    .bind(input.title.trim())
    .bind(input.desired_role.trim())
    .bind(&input.desired_domain)
    .bind(&input.desired_orientations)
    .bind(input.desired_salary_range.as_ref())
    .bind(input.remote_only)
    .bind(&input.preferred_countries)
    .bind(input.available_from)
    .bind(input.not_looking_for.as_deref().map(str::trim))
    .bind(input.max_pitches_per_month)
    .fetch_one(db)
    .await?;

    by_id(db, id).await
}

pub async fn by_id(db: &PgPool, id: Uuid) -> Result<Posting, AppError> {
    let sql = format!("{POSTING_SELECT} WHERE p.id = $1");
    sqlx::query_as::<_, Posting>(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NotFound("posting not found".into()))
}

pub async fn mine(db: &PgPool, talent_user_id: Uuid) -> Result<Option<Posting>, AppError> {
    let sql = format!("{POSTING_SELECT} WHERE p.talent_user_id = $1");
    let row = sqlx::query_as::<_, Posting>(sqlx::AssertSqlSafe(sql))
        .bind(talent_user_id)
        .fetch_optional(db)
        .await?;
    Ok(row)
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct BrowseFilter {
    pub domain: Option<String>,
    pub orientation: Option<String>,
    pub remote_only: Option<bool>,
    pub country: Option<String>,
}

/// The postings a company can answer.
///
/// Postings with no pitches left this month are excluded: showing one would
/// invite four hundred words that the database will refuse.
pub async fn browse(
    db: &PgPool,
    filter: &BrowseFilter,
    limit: i64,
) -> Result<Vec<Posting>, AppError> {
    let orientation_id: Option<Uuid> = match &filter.orientation {
        Some(slug) => {
            sqlx::query_scalar("SELECT resolve_orientation($1)")
                .bind(slug)
                .fetch_one(db)
                .await?
        }
        None => None,
    };
    if filter.orientation.is_some() && orientation_id.is_none() {
        return Ok(vec![]);
    }

    let sql = format!(
        "{POSTING_SELECT}
         WHERE p.status = 'active'
           AND ($1::TEXT IS NULL OR p.desired_domain = $1)
           AND ($2::TEXT IS NULL OR $2 = ANY(p.desired_orientations))
           AND ($3::BOOLEAN IS NULL OR p.remote_only = $3)
           AND ($4::TEXT IS NULL
                OR cardinality(p.preferred_countries) = 0
                OR $4 = ANY(p.preferred_countries))
           AND (SELECT count(*) FROM reverse_recruitment_pitches x
                 WHERE x.posting_id = p.id
                   AND x.created_at > date_trunc('month', NOW()))
               < p.max_pitches_per_month
         ORDER BY cs.score DESC NULLS LAST, p.available_from ASC
         LIMIT $5"
    );

    let rows = sqlx::query_as::<_, Posting>(sqlx::AssertSqlSafe(sql))
        .bind(filter.domain.as_deref())
        .bind(filter.orientation.as_deref())
        .bind(filter.remote_only)
        .bind(filter.country.as_deref())
        .bind(limit.clamp(1, 100))
        .fetch_all(db)
        .await?;
    Ok(rows)
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Pitch {
    pub id: Uuid,
    pub posting_id: Uuid,
    pub enterprise_id: Uuid,
    pub company_name: String,
    pub pitch_md: String,
    pub offered_salary: Option<BigDecimal>,
    pub currency: Option<String>,
    pub status: String,
    pub decline_reason: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PitchInput {
    pub pitch_md: String,
    #[serde(default)]
    pub offered_salary: Option<BigDecimal>,
    #[serde(default)]
    pub currency: Option<String>,
}

/// Argue for yourself.
pub async fn pitch(
    db: &PgPool,
    posting_id: Uuid,
    enterprise_id: Uuid,
    sent_by: Uuid,
    input: PitchInput,
) -> Result<Uuid, AppError> {
    let pitch_md = input.pitch_md.trim();
    if pitch_md.chars().count() < MIN_PITCH_LENGTH {
        return Err(AppError::Validation(format!(
            "a pitch is at least {MIN_PITCH_LENGTH} characters. The premise here is that \
             the company does the persuading, and two lines is asking the person to do it \
             instead."
        )));
    }
    crate::validators::check_max_len(pitch_md, "pitch_md", 8000)?;

    if input.offered_salary.is_some() != input.currency.is_some() {
        return Err(AppError::Validation(
            "a figure needs its currency, and a currency needs its figure".into(),
        ));
    }

    // Charged before the row exists, so a refused pitch does not cost
    // anything: the trigger on the ceiling runs after this and rolls the
    // whole thing back.
    let mut tx = db.begin().await?;

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO reverse_recruitment_pitches
            (posting_id, enterprise_id, sent_by, pitch_md, offered_salary,
             currency, credits_spent)
         VALUES ($1,$2,$3,$4,$5,$6,$7)
         RETURNING id",
    )
    .bind(posting_id)
    .bind(enterprise_id)
    .bind(sent_by)
    .bind(pitch_md)
    .bind(input.offered_salary.as_ref())
    .bind(input.currency.as_deref())
    .bind(PITCH_COST_CREDITS)
    .fetch_one(&mut *tx)
    .await
    .map_err(pitch_error)?;

    tx.commit().await?;
    Ok(id)
}

/// The database speaks in constraint names; this says the same in words the
/// company can act on.
fn pitch_error(e: sqlx::Error) -> AppError {
    let message = e.to_string();
    if message.contains("has taken its") || message.contains("not taking pitches") {
        let start = message.find("ERROR:").map(|i| i + 6).unwrap_or(0);
        let sentence = message[start..].lines().next().unwrap_or(&message).trim();
        return AppError::Validation(sentence.to_string());
    }
    if matches!(&e, sqlx::Error::Database(db) if db.code().as_deref() == Some("23505")) {
        return AppError::Validation(
            "you have already pitched for this posting — a second one is a follow-up, \
             which belongs in a conversation rather than in a new pitch"
                .into(),
        );
    }
    if message.contains("pitch_md_check") {
        return AppError::Validation(format!("a pitch is at least {MIN_PITCH_LENGTH} characters"));
    }
    AppError::from(e)
}

/// The pitches somebody has received.
pub async fn pitches_for(db: &PgPool, posting_id: Uuid) -> Result<Vec<Pitch>, AppError> {
    let rows = sqlx::query_as::<_, Pitch>(
        "SELECT p.id, p.posting_id, p.enterprise_id, e.company_name, p.pitch_md,
                p.offered_salary, p.currency, p.status, p.decline_reason, p.created_at
           FROM reverse_recruitment_pitches p
           JOIN enterprises e ON e.id = p.enterprise_id
          WHERE p.posting_id = $1
          ORDER BY p.created_at DESC",
    )
    .bind(posting_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// The talent answers.
pub async fn respond(
    db: &PgPool,
    pitch_id: Uuid,
    talent_user_id: Uuid,
    interested: bool,
    reason: Option<&str>,
) -> Result<(), AppError> {
    let done = sqlx::query(
        "UPDATE reverse_recruitment_pitches p
            SET status = CASE WHEN $3 THEN 'interested' ELSE 'declined' END,
                responded_at = NOW(),
                decline_reason = CASE WHEN $3 THEN NULL ELSE $4 END
           FROM reverse_recruitment_postings o
          WHERE p.id = $1 AND p.posting_id = o.id
            AND o.talent_user_id = $2
            AND p.status IN ('sent', 'read')",
    )
    .bind(pitch_id)
    .bind(talent_user_id)
    .bind(interested)
    .bind(reason.map(str::trim).filter(|s| !s.is_empty()))
    .execute(db)
    .await?;

    if done.rows_affected() == 0 {
        return Err(AppError::NotFound(
            "that pitch is not yours, or you have already answered it".into(),
        ));
    }
    Ok(())
}

/// Mark a pitch read, once.
///
/// Recorded because a company that spent credits is owed the knowledge that
/// their argument was at least opened — which is not the same as an answer,
/// and is deliberately not presented as one.
pub async fn mark_read(db: &PgPool, pitch_id: Uuid, talent_user_id: Uuid) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE reverse_recruitment_pitches p
            SET status = 'read', read_at = NOW()
           FROM reverse_recruitment_postings o
          WHERE p.id = $1 AND p.posting_id = o.id
            AND o.talent_user_id = $2 AND p.status = 'sent'",
    )
    .bind(pitch_id)
    .bind(talent_user_id)
    .execute(db)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_threshold_is_the_platforms_existing_one() {
        // Not a second bar. Two definitions of "has done enough to be taken
        // at their word" would drift apart.
        assert!(RANKS_ALLOWED.contains(&MIN_RANK));
        // The two below the bar, as migration 0092 spells them.
        assert!(!RANKS_ALLOWED.contains(&"apprenti"));
        assert!(!RANKS_ALLOWED.contains(&"ranger"));
    }

    // The opportunity is rarer than an ordinary contact and the ceiling makes
    // each one scarce, so a pitch costs more. Compile-time: both sides are
    // constants.
    const _: () = assert!(PITCH_COST_CREDITS > 1);

    // Two hundred characters is roughly three sentences: short enough to
    // write, long enough that a template shows.
    const _: () = assert!(MIN_PITCH_LENGTH >= 150);
}
