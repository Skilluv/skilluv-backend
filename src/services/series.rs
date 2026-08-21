//! Contests read as one event.
//!
//! ## What this replaced
//!
//! Two backlog items that looked like two formats: an annual awards edition
//! with thirteen categories, and a weekend design sprint with an imposed
//! theme. Neither is a format — an edition is thirteen contests judged in
//! parallel, a sprint is a contest with a very short window run again every
//! few weeks. What both needed was a way to say *these contests are one
//! thing*, and that is all this is.
//!
//! Built separately they would have shipped two tables, two sets of routes,
//! and two definitions of "who won overall".
//!
//! ## Why there is no overall winner
//!
//! An awards edition has thirteen podiums and no fourteenth. Summing places
//! across categories would rank a designer who entered all thirteen above one
//! who won the only category they work in, which is the opposite of what the
//! edition is for.
//!
//! What [`standings`] returns instead is every category and its podium. A
//! reader draws their own conclusion, which is the honest amount of
//! conclusion available.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

/// The kinds a series can be. Mirrors the CHECK on `tournament_series.kind`.
pub const KINDS: &[&str] = &["awards_edition", "sprint", "programme"];

#[derive(Debug, Clone, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Series {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub kind: String,
    pub skill_domain: Option<String>,
    pub starts_at: chrono::DateTime<chrono::Utc>,
    pub ends_at: chrono::DateTime<chrono::Utc>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateSeries {
    pub slug: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub kind: String,
    #[serde(default)]
    pub skill_domain: Option<String>,
    pub starts_at: chrono::DateTime<chrono::Utc>,
    pub ends_at: chrono::DateTime<chrono::Utc>,
}

pub async fn create(db: &PgPool, input: CreateSeries, by: Uuid) -> Result<Series, AppError> {
    if !KINDS.contains(&input.kind.as_str()) {
        return Err(AppError::Validation(format!(
            "kind must be one of: {}",
            KINDS.join(", ")
        )));
    }
    if let Some(domain) = input.skill_domain.as_deref() {
        crate::validators::validate_skill_domain(domain, "skill_domain")?;
    }
    crate::validators::check_max_len(&input.slug, "slug", 80)?;
    crate::validators::check_max_len(&input.name, "name", 160)?;
    if input.ends_at <= input.starts_at {
        return Err(AppError::Validation("a series ends after it starts".into()));
    }

    let series = sqlx::query_as::<_, Series>(
        r#"
        INSERT INTO tournament_series
            (slug, name, description, kind, skill_domain, starts_at, ends_at, created_by)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING id, slug, name, description, kind, skill_domain,
                  starts_at, ends_at, created_at
        "#,
    )
    .bind(input.slug.trim())
    .bind(input.name.trim())
    .bind(input.description.as_deref().map(str::trim))
    .bind(&input.kind)
    .bind(input.skill_domain.as_deref())
    .bind(input.starts_at)
    .bind(input.ends_at)
    .bind(by)
    .fetch_one(db)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(ref db_err) if db_err.is_unique_violation() => {
            AppError::Conflict(format!("a series already uses the slug '{}'", input.slug))
        }
        other => AppError::from(other),
    })?;
    Ok(series)
}

/// Put a contest in a series, under a category.
///
/// The category is what the contest is *for* inside the series — a family for
/// an awards edition, an editorial axis for a programme. A sprint's contest
/// carries none, because it is the whole of its series.
///
/// Refuses a second contest in the same category, which the unique index also
/// refuses: two "best motion" categories in one edition is a mistake nobody
/// notices until the results page shows two winners of the same thing.
pub async fn attach(
    db: &PgPool,
    series_id: Uuid,
    tournament_id: Uuid,
    category: Option<&str>,
) -> Result<(), AppError> {
    if let Some(category) = category {
        crate::validators::check_max_len(category, "series_category", 60)?;
    }

    let updated = sqlx::query(
        "UPDATE tournaments SET series_id = $1, series_category = $2, updated_at = NOW()
          WHERE id = $3",
    )
    .bind(series_id)
    .bind(category.map(str::trim).filter(|c| !c.is_empty()))
    .bind(tournament_id)
    .execute(db)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(ref db_err) if db_err.is_unique_violation() => {
            AppError::Conflict("this series already has a contest in that category".into())
        }
        other => AppError::from(other),
    })?;

    if updated.rows_affected() == 0 {
        return Err(AppError::NotFound("tournament not found".into()));
    }
    Ok(())
}

pub async fn by_slug(db: &PgPool, slug: &str) -> Result<Series, AppError> {
    sqlx::query_as::<_, Series>(
        "SELECT id, slug, name, description, kind, skill_domain,
                starts_at, ends_at, created_at
           FROM tournament_series WHERE slug = $1",
    )
    .bind(slug)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("no series '{slug}'")))
}

pub async fn list(db: &PgPool, kind: Option<&str>, limit: i64) -> Result<Vec<Series>, AppError> {
    let rows = sqlx::query_as::<_, Series>(
        "SELECT id, slug, name, description, kind, skill_domain,
                starts_at, ends_at, created_at
           FROM tournament_series
          WHERE ($1::text IS NULL OR kind = $1)
          ORDER BY starts_at DESC
          LIMIT $2",
    )
    .bind(kind)
    .bind(limit.clamp(1, 100))
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// One category of a series, and who is on its podium.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct CategoryStanding {
    pub category: Option<String>,
    pub tournament_id: Uuid,
    pub tournament_slug: String,
    pub tournament_name: String,
    pub status: String,
    pub ends_at: chrono::DateTime<chrono::Utc>,
    /// The top three, when there are three. Empty while the contest runs.
    pub podium: Vec<PodiumLine>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct PodiumLine {
    pub rank: i32,
    pub participant_type: String,
    pub participant_id: Uuid,
    pub username: Option<String>,
    pub display_name: Option<String>,
    pub score: i32,
}

/// One contest of a series, as the query returns it: id, slug, name, status,
/// deadline, category.
type ContestRow = (
    Uuid,
    String,
    String,
    String,
    chrono::DateTime<chrono::Utc>,
    Option<String>,
);

/// Every category of a series and its podium.
///
/// No overall ranking, deliberately: see the module documentation. Summing
/// places across thirteen categories would rank somebody who entered all of
/// them above somebody who won the only one they work in.
pub async fn standings(db: &PgPool, series_id: Uuid) -> Result<Vec<CategoryStanding>, AppError> {
    let contests: Vec<ContestRow> = sqlx::query_as(
        "SELECT id, slug, name, status, ends_at, series_category
               FROM tournaments
              WHERE series_id = $1
              ORDER BY series_category NULLS FIRST, starts_at ASC",
    )
    .bind(series_id)
    .fetch_all(db)
    .await?;

    let mut out = Vec::with_capacity(contests.len());
    for (id, slug, name, status, ends_at, category) in contests {
        let podium = sqlx::query_as::<_, PodiumLine>(
            r#"
            SELECT p.rank, p.participant_type, p.participant_id,
                   COALESCE(u.username, g.slug) AS username,
                   COALESCE(u.display_name, g.name) AS display_name,
                   p.score
              FROM tournament_participants p
              LEFT JOIN users u
                     ON p.participant_type = 'user' AND u.id = p.participant_id
              LEFT JOIN guilds g
                     ON p.participant_type = 'guild' AND g.id = p.participant_id
             WHERE p.tournament_id = $1 AND p.rank IS NOT NULL AND p.rank <= 3
             ORDER BY p.rank ASC
            "#,
        )
        .bind(id)
        .fetch_all(db)
        .await?;

        out.push(CategoryStanding {
            category,
            tournament_id: id,
            tournament_slug: slug,
            tournament_name: name,
            status,
            ends_at,
            podium,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_kinds_are_the_ones_the_schema_accepts() {
        // Migration 0249 carries the same three. Drifting apart means a
        // request refused by the database as a 500 instead of by the service
        // as a 400.
        assert_eq!(KINDS.len(), 3);
        assert!(KINDS.contains(&"awards_edition"));
        assert!(KINDS.contains(&"sprint"));
        assert!(KINDS.contains(&"programme"));
    }
}
