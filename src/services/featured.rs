//! Putting somebody forward, once a week.
//!
//! ## What this is worth, and why
//!
//! One person per domain per week, with a written reason, kept on the record.
//! The scarcity is the whole value: a featuring that happened to forty people
//! this month is a newsletter section, not a distinction, and the attestation
//! it produces would say nothing.
//!
//! ## Why nothing is posted for you
//!
//! The backlog asked for automatic publication to social networks. That would
//! publish somebody's name and work to a third-party platform on a schedule
//! with no human between the decision and the post — and it needs credentials
//! for accounts that do not exist. What this produces instead is [`Card`]:
//! everything a post needs, ready for a person to send.
//!
//! ## Why it is not design-only
//!
//! `featured_coder`, `featured_ai_researcher` and `featured_designer` were
//! already three attestation bases. The domain is a parameter here for the
//! same reason it is one in the recommender: three copies of one rule diverge
//! within a year.

use chrono::{Datelike, NaiveDate, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

/// How long before the same person can be put forward again, in the same
/// domain.
///
/// Thirteen weeks. Long enough that a featuring is not a rotation among the
/// same four people, short enough that somebody who was outstanding twice in a
/// year can be told so twice.
pub const COOLDOWN_WEEKS: i64 = 13;

/// The Monday of the week containing `day`, in UTC.
///
/// Weeks are stored as a date rather than a week number because ISO week
/// numbering disagrees with itself across new year boundaries — week 1 of a
/// year can start in December — and a date is unambiguous everywhere.
pub fn week_of(day: NaiveDate) -> NaiveDate {
    let weekday = day.weekday().num_days_from_monday() as i64;
    day - chrono::Duration::days(weekday)
}

/// This week.
pub fn current_week() -> NaiveDate {
    week_of(Utc::now().date_naive())
}

/// A featuring, as it is read back.
#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Featured {
    pub skill_domain: String,
    pub week_of: NaiveDate,
    pub user_id: Uuid,
    pub username: Option<String>,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub reason_md: String,
    pub deliverable_id: Option<Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Everything a post needs, for a person to send.
///
/// Returned rather than published. Composed here rather than in a client so
/// that whoever posts it does not have to rebuild the profile URL, and so the
/// same words go to every network.
#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct Card {
    pub headline: String,
    pub body: String,
    pub profile_url: String,
    pub deliverable_url: Option<String>,
    /// The image a network will show, when there is one to show.
    pub avatar_url: Option<String>,
}

/// Put somebody forward for a week.
///
/// Refuses three things, each for a stated reason:
///
/// * a week already awarded in this domain — two people featured in one week
///   means neither was;
/// * somebody featured in the same domain inside the cooldown;
/// * somebody with no verified deliverable in the domain — being put forward
///   for work nobody has checked is exactly the claim this platform exists
///   not to make.
#[allow(clippy::too_many_arguments)]
pub async fn feature(
    db: &PgPool,
    domain: &str,
    week: NaiveDate,
    user_id: Uuid,
    reason_md: &str,
    deliverable_id: Option<Uuid>,
    chosen_by: Uuid,
    frontend_url: &str,
) -> Result<Featured, AppError> {
    crate::validators::validate_skill_domain(domain, "skill_domain")?;

    let reason = reason_md.trim();
    if reason.chars().count() < 40 {
        return Err(AppError::Validation(
            "say why in at least forty characters: a featuring with no stated reason is a popularity contest".into(),
        ));
    }
    crate::validators::check_max_len(reason, "reason_md", 4000)?;

    if week != week_of(week) {
        return Err(AppError::Validation(
            "week_of must be the Monday of the week being awarded".into(),
        ));
    }

    let proven: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM deliverables d
              LEFT JOIN challenge_templates ct ON ct.id = d.challenge_id
              LEFT JOIN project_slices ps ON ps.id = d.slice_id
             WHERE d.user_id = $1
               AND d.verification_status = 'verified'
               AND d.revoked_at IS NULL
               AND (ct.skill_domain = $2 OR ps.primary_domain = $2)
        )
        "#,
    )
    .bind(user_id)
    .bind(domain)
    .fetch_one(db)
    .await?;
    if !proven {
        return Err(AppError::Validation(format!(
            "this account has no verified {domain} deliverable: there is nothing to put forward"
        )));
    }

    let recently: Option<NaiveDate> = sqlx::query_scalar(
        "SELECT max(week_of) FROM featured_talents
          WHERE skill_domain = $1 AND user_id = $2",
    )
    .bind(domain)
    .bind(user_id)
    .fetch_optional(db)
    .await?
    .flatten();
    if let Some(last) = recently
        && (week - last).num_weeks() < COOLDOWN_WEEKS
    {
        return Err(AppError::Conflict(format!(
            "already featured in {domain} on {last}: the same person comes back after {COOLDOWN_WEEKS} weeks"
        )));
    }

    sqlx::query(
        "INSERT INTO featured_talents
             (skill_domain, week_of, user_id, reason_md, deliverable_id, chosen_by_user_id)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(domain)
    .bind(week)
    .bind(user_id)
    .bind(reason)
    .bind(deliverable_id)
    .bind(chosen_by)
    .execute(db)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(ref db_err) if db_err.is_unique_violation() => AppError::Conflict(
            format!("a {domain} talent is already featured for the week of {week}"),
        ),
        other => AppError::from(other),
    })?;

    // The attestation and the notification are both after the row, and both
    // are logged rather than raised: the featuring is a fact once written, and
    // failing it because a mail server was down would leave the week awarded
    // and the response an error.
    let evidence_url = evidence_url_of(db, user_id, deliverable_id, frontend_url).await;
    if let Err(e) = issue_attestation(db, domain, user_id, week, reason, &evidence_url).await {
        tracing::warn!(%user_id, domain, error = %e, "featured attestation not issued");
    }
    if let Err(e) = crate::services::notify::send(
        crate::services::notify::Ctx::db_only(db),
        crate::services::notify::Recipient::User(user_id),
        "talent.featured",
    )
    .payload(serde_json::json!({ "skill_domain": domain, "week_of": week }))
    .execute()
    .await
    {
        tracing::warn!(%user_id, error = %e, "featured notification not delivered");
    }

    // And in the room, where the point of a featuring is that other people
    // see it. Best-effort, like everything else after the row is written.
    if let Ok(Some(username)) =
        sqlx::query_scalar::<_, String>("SELECT username FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(db)
            .await
    {
        crate::services::discord_announce::talent_featured(db, domain, &username, week).await;
    }

    of_week(db, domain, week)
        .await?
        .ok_or_else(|| AppError::Internal("the featuring was written and cannot be read".into()))
}

/// Issue the editorial attestation, where the domain has one.
///
/// Seven domains do, and the list is `attestation_bases` filtered to
/// `featured\_%` rather than anything written here — which is the point of the
/// fallthrough below. Game, security and soft_skills declare none and are
/// featured without one.
///
/// Each domain's own service issues it, because each knows its own bases and
/// its own rules about what evidence may look like.
async fn issue_attestation(
    db: &PgPool,
    domain: &str,
    user_id: Uuid,
    week: NaiveDate,
    reason: &str,
    profile_url: &str,
) -> Result<(), AppError> {
    let citation = format!("Semaine du {week}. {reason}");
    match domain {
        "code" => {
            crate::services::code_attestations::featured_coder(db, user_id, profile_url, &citation)
                .await?;
        }
        "design" => {
            crate::services::design_attestations::featured_designer(
                db,
                user_id,
                profile_url,
                &citation,
            )
            .await?;
        }
        "ai" => {
            crate::services::ai_attestations::featured_ai_researcher(
                db,
                user_id,
                profile_url,
                &citation,
            )
            .await?;
        }
        "quality" => {
            crate::services::quality_attestations::featured_quality_engineer(
                db,
                user_id,
                profile_url,
                &citation,
            )
            .await?;
        }
        "leadership" => {
            crate::services::leadership_attestations::featured_leader(
                db,
                user_id,
                profile_url,
                &citation,
            )
            .await?;
        }
        "ops" => {
            crate::services::ops_practice::featured_ops_engineer(
                db,
                user_id,
                profile_url,
                &citation,
            )
            .await?;
        }
        "audio" => {
            crate::services::audio_attestations::featured_audio_creator(
                db,
                user_id,
                profile_url,
                &citation,
            )
            .await?;
        }
        "communication" => {
            crate::services::communication_attestations::featured_communicator(
                db,
                user_id,
                profile_url,
                &citation,
            )
            .await?;
        }
        "education" => {
            crate::services::education_attestations::featured_educator(
                db,
                user_id,
                profile_url,
                &citation,
            )
            .await?;
        }
        "security" => {
            crate::services::security_attestations::featured_security_researcher(
                db,
                user_id,
                profile_url,
                &citation,
            )
            .await?;
        }
        // Game and soft_skills declare no `featured_*` basis, and
        // silence is the right answer for them: somebody is put forward, the
        // announcement goes out, and there is nothing to attest.
        //
        // A domain that *does* declare one and reaches this arm is a different
        // thing entirely, and it is the bug that hid here for two domains —
        // ops and audio each had a basis, a profile term counting it, and no
        // arm. The featuring was recorded, nothing was issued, and the only
        // symptom was a number stuck at zero on somebody else's profile.
        //
        // So the fallthrough asks the catalogue rather than assuming. It logs
        // and does not fail: an insert that failed here would lose the
        // featuring itself, which is the thing worth keeping.
        other => warn_if_the_domain_expected_one(db, other).await,
    }
    Ok(())
}

/// Complain when a domain declares a featuring basis that nothing issues.
///
/// The guard the match arms cannot be: a missing arm is not a compile error,
/// it is a person whose profile quietly says zero.
async fn warn_if_the_domain_expected_one(db: &PgPool, domain: &str) {
    let basis: Result<Option<String>, _> = sqlx::query_scalar(
        r"SELECT basis FROM attestation_bases
           WHERE skill_domain = $1 AND basis LIKE 'featured\_%'",
    )
    .bind(domain)
    .fetch_optional(db)
    .await;

    if let Ok(Some(basis)) = basis {
        tracing::error!(
            domain, %basis,
            "this domain declares a featuring basis and no generator issues it —              the featuring was recorded and the attestation was not, which shows              up only as a profile term stuck at zero"
        );
    }
}

/// What the attestation points at.
///
/// The work first, the profile second. "An attestation nobody can open is
/// worth nothing" is the rule the issuing service enforces, and a featuring
/// that pointed at a profile page would send a reader to a list of things
/// rather than to the thing that earned it.
///
/// The profile remains the answer when nobody named a piece of work: somebody
/// can be put forward for a body of work, and a link to the body of work is a
/// profile.
/// Public because two paths reach a featuring: the weekly one below, and an
/// administrator issuing one by hand through `/admin/ops/attestations/featured`.
/// Both have to point at the same thing, and a second convention invented in a
/// route handler is how they stop.
pub async fn evidence_url_of(
    db: &PgPool,
    user_id: Uuid,
    deliverable_id: Option<Uuid>,
    frontend_url: &str,
) -> String {
    if let Some(deliverable_id) = deliverable_id {
        let url: Option<String> = sqlx::query_scalar(
            "SELECT artifact_url FROM deliverables
              WHERE id = $1 AND user_id = $2 AND public
                AND verification_status = 'verified' AND revoked_at IS NULL",
        )
        .bind(deliverable_id)
        .bind(user_id)
        .fetch_optional(db)
        .await
        .ok()
        .flatten();
        if let Some(url) = url {
            return url;
        }
    }

    let username: Option<String> = sqlx::query_scalar("SELECT username FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(db)
        .await
        .ok()
        .flatten();
    match username {
        Some(username) => format!("{frontend_url}/u/{username}"),
        None => frontend_url.to_string(),
    }
}

/// Who was featured in a domain for a given week.
pub async fn of_week(
    db: &PgPool,
    domain: &str,
    week: NaiveDate,
) -> Result<Option<Featured>, AppError> {
    let row = sqlx::query_as::<_, Featured>(
        r#"
        SELECT f.skill_domain, f.week_of, f.user_id,
               u.username, u.display_name, u.avatar_url,
               f.reason_md, f.deliverable_id, f.created_at
          FROM featured_talents f
          LEFT JOIN users u ON u.id = f.user_id
         WHERE f.skill_domain = $1 AND f.week_of = $2
        "#,
    )
    .bind(domain)
    .bind(week)
    .fetch_optional(db)
    .await?;
    Ok(row)
}

/// The last few weeks of a domain, newest first.
pub async fn recent(db: &PgPool, domain: &str, limit: i64) -> Result<Vec<Featured>, AppError> {
    let rows = sqlx::query_as::<_, Featured>(
        r#"
        SELECT f.skill_domain, f.week_of, f.user_id,
               u.username, u.display_name, u.avatar_url,
               f.reason_md, f.deliverable_id, f.created_at
          FROM featured_talents f
          LEFT JOIN users u ON u.id = f.user_id
         WHERE f.skill_domain = $1
         ORDER BY f.week_of DESC
         LIMIT $2
        "#,
    )
    .bind(domain)
    .bind(limit.clamp(1, 52))
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// Compose the post a person will send.
pub async fn card(db: &PgPool, featured: &Featured, frontend_url: &str) -> Result<Card, AppError> {
    let deliverable_url: Option<String> = match featured.deliverable_id {
        None => None,
        Some(id) => {
            sqlx::query_scalar("SELECT artifact_url FROM deliverables WHERE id = $1 AND public")
                .bind(id)
                .fetch_optional(db)
                .await?
        }
    };

    let name = featured
        .display_name
        .clone()
        .or_else(|| featured.username.clone())
        .unwrap_or_else(|| "Un talent Skilluv".to_string());

    Ok(Card {
        headline: format!("{name} — mis en avant cette semaine"),
        body: featured.reason_md.clone(),
        profile_url: match &featured.username {
            Some(username) => format!("{frontend_url}/u/{username}"),
            None => frontend_url.to_string(),
        },
        deliverable_url,
        avatar_url: featured.avatar_url.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_week_is_its_monday() {
        // Every day of one week maps to the same Monday, including the
        // Sunday — which is the day a naive implementation gets wrong.
        let monday = NaiveDate::from_ymd_opt(2026, 8, 17).unwrap();
        for offset in 0..7 {
            let day = monday + chrono::Duration::days(offset);
            assert_eq!(week_of(day), monday, "{day}");
        }
        assert_eq!(
            week_of(NaiveDate::from_ymd_opt(2026, 8, 24).unwrap()),
            NaiveDate::from_ymd_opt(2026, 8, 24).unwrap()
        );
    }

    #[test]
    fn a_week_across_a_new_year_still_has_one_monday() {
        // The case ISO week numbers get wrong: this week starts in one year
        // and ends in the next, and a date says so without argument.
        let thursday = NaiveDate::from_ymd_opt(2027, 1, 1).unwrap();
        assert_eq!(
            week_of(thursday),
            NaiveDate::from_ymd_opt(2026, 12, 28).unwrap()
        );
    }
}
