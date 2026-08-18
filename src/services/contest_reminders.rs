//! Deadlines, and telling people about them before they pass.
//!
//! ## Why this is a sweep and not a hook
//!
//! Every other contest notification answers something somebody did — an entry
//! was handed in, a panel was invited, a ranking was published. A deadline is
//! the opposite: it is the absence of an action, and nothing calls a function
//! when nothing happens. So a job asks the question on a clock.
//!
//! ## Why the reminders are per person
//!
//! A flag on the contest would answer "was the warning sent" rather than "to
//! whom", and those differ the moment somebody enters an hour before the
//! deadline: they have still never been warned. `contest_reminders_sent`
//! records the pair, and the sweep is an anti-join against it — which also
//! makes running it twice harmless, the property that lets it be run by hand
//! after an outage.
//!
//! ## Why nothing here raises
//!
//! A reminder that could not be delivered is a reminder to try again in an
//! hour. Failing the sweep would stop every later contest in the same pass
//! from being warned at all, which turns one mail problem into a missed
//! deadline for everybody.

use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::notify::{Ctx, Recipient, send};

/// How long before a deadline the warning goes out.
///
/// Forty-eight hours: long enough to finish something, short enough that the
/// reminder is about this weekend rather than a date in the abstract. The
/// sweep runs hourly, so what stops a second warning is the anti-join, not
/// the arithmetic.
pub const WARNING_HOURS: i64 = 48;

/// The moments this sweep announces. Recorded on the row so a later pass
/// knows what it has already said.
const SUBMISSION_DEADLINE: &str = "submission_deadline";
const JURY_DEADLINE: &str = "jury_deadline";
const CLOSED: &str = "closed";

/// What one pass did, for the log line and for the tests.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SweepReport {
    pub deadline_warnings: u64,
    pub jury_warnings: u64,
    pub closures_announced: u64,
}

impl SweepReport {
    pub fn total(&self) -> u64 {
        self.deadline_warnings + self.jury_warnings + self.closures_announced
    }
}

/// One pass over every open contest.
pub async fn sweep(db: &PgPool) -> Result<SweepReport, AppError> {
    Ok(SweepReport {
        deadline_warnings: warn_entrants(db).await?,
        jury_warnings: warn_jurors(db).await?,
        closures_announced: announce_closures(db).await?,
    })
}

/// Contests closing inside the window, and who has entered them.
async fn warn_entrants(db: &PgPool) -> Result<u64, AppError> {
    let rows: Vec<(Uuid, String, String, Uuid, i64)> = sqlx::query_as(
        r#"
        SELECT t.id, t.slug, t.name, p.participant_id,
               GREATEST(0, EXTRACT(EPOCH FROM (t.ends_at - NOW()))::BIGINT / 3600)
          FROM tournaments t
          JOIN tournament_participants p ON p.tournament_id = t.id
         WHERE t.status IN ('registration', 'active')
           AND t.ends_at > NOW()
           AND t.ends_at <= NOW() + make_interval(hours => $1::INT)
           AND p.participant_type = 'user'
           AND NOT EXISTS (
                 SELECT 1 FROM contest_reminders_sent r
                  WHERE r.tournament_id = t.id
                    AND r.user_id = p.participant_id
                    AND r.moment = $2
           )
        "#,
    )
    .bind(WARNING_HOURS as i32)
    .bind(SUBMISSION_DEADLINE)
    .fetch_all(db)
    .await?;

    let mut sent = 0;
    for (tournament_id, slug, name, user_id, hours) in rows {
        let delivered = send(
            Ctx::db_only(db),
            Recipient::User(user_id),
            "contest.deadline_soon",
        )
        .arg("contest", name)
        .arg("hours", hours.to_string())
        .payload(serde_json::json!({
            "tournament_id": tournament_id,
            "tournament_slug": slug,
        }))
        .execute()
        .await;

        if let Err(e) = delivered {
            tracing::warn!(%tournament_id, %user_id, error = %e, "deadline warning not delivered");
            continue;
        }
        mark(db, tournament_id, user_id, SUBMISSION_DEADLINE).await;
        sent += 1;
    }
    Ok(sent)
}

/// The same warning for the panel, which is working to the same clock.
///
/// A juror who declined is not reminded: they said no, and reminding them is
/// how an invitation becomes nagging. One who has not answered still is —
/// silence is usually a missed notification, not a refusal.
async fn warn_jurors(db: &PgPool) -> Result<u64, AppError> {
    let rows: Vec<(Uuid, String, String, Uuid, i64)> = sqlx::query_as(
        r#"
        SELECT t.id, t.slug, t.name, j.juror_user_id,
               GREATEST(0, EXTRACT(EPOCH FROM (t.ends_at - NOW()))::BIGINT / 3600)
          FROM tournaments t
          JOIN tournament_juries j ON j.tournament_id = t.id
         WHERE t.status IN ('registration', 'active')
           AND t.ends_at > NOW()
           AND t.ends_at <= NOW() + make_interval(hours => $1::INT)
           AND j.declined_at IS NULL
           AND NOT EXISTS (
                 SELECT 1 FROM contest_reminders_sent r
                  WHERE r.tournament_id = t.id
                    AND r.user_id = j.juror_user_id
                    AND r.moment = $2
           )
        "#,
    )
    .bind(WARNING_HOURS as i32)
    .bind(JURY_DEADLINE)
    .fetch_all(db)
    .await?;

    let mut sent = 0;
    for (tournament_id, slug, name, user_id, hours) in rows {
        let delivered = send(
            Ctx::db_only(db),
            Recipient::User(user_id),
            "contest.jury_deadline_soon",
        )
        .arg("contest", name)
        .arg("hours", hours.to_string())
        .payload(serde_json::json!({
            "tournament_id": tournament_id,
            "tournament_slug": slug,
        }))
        .execute()
        .await;

        if let Err(e) = delivered {
            tracing::warn!(%tournament_id, %user_id, error = %e, "jury warning not delivered");
            continue;
        }
        mark(db, tournament_id, user_id, JURY_DEADLINE).await;
        sent += 1;
    }
    Ok(sent)
}

/// Contests whose deadline has passed and which are not concluded yet.
///
/// This announces the moment and deliberately does not change the status:
/// concluding a contest ranks it and pays it, and a background job doing that
/// on a clock would publish a ranking nobody had judged. The status is moved
/// by whoever runs the contest; this only stops entrants wondering whether
/// their entry arrived.
///
/// On a blind contest it is also the moment the whole field becomes readable,
/// which is worth being told rather than discovered.
async fn announce_closures(db: &PgPool) -> Result<u64, AppError> {
    let rows: Vec<(Uuid, String, String, Uuid)> = sqlx::query_as(
        r#"
        SELECT t.id, t.slug, t.name, p.participant_id
          FROM tournaments t
          JOIN tournament_participants p ON p.tournament_id = t.id
         WHERE t.status IN ('registration', 'active')
           AND t.ends_at <= NOW()
           AND p.participant_type = 'user'
           AND NOT EXISTS (
                 SELECT 1 FROM contest_reminders_sent r
                  WHERE r.tournament_id = t.id
                    AND r.user_id = p.participant_id
                    AND r.moment = $1
           )
        "#,
    )
    .bind(CLOSED)
    .fetch_all(db)
    .await?;

    let mut sent = 0;
    for (tournament_id, slug, name, user_id) in rows {
        let delivered = send(Ctx::db_only(db), Recipient::User(user_id), "contest.closed")
            .arg("contest", name)
            .payload(serde_json::json!({
                "tournament_id": tournament_id,
                "tournament_slug": slug,
            }))
            .execute()
            .await;

        if let Err(e) = delivered {
            tracing::warn!(%tournament_id, %user_id, error = %e, "closure notice not delivered");
            continue;
        }
        mark(db, tournament_id, user_id, CLOSED).await;
        sent += 1;
    }
    Ok(sent)
}

/// Record that this person has been told.
///
/// Written after the delivery, so a failure means the next pass tries again.
/// The reverse order would lose a reminder permanently to one bad minute.
async fn mark(db: &PgPool, tournament_id: Uuid, user_id: Uuid, moment: &str) {
    if let Err(e) = sqlx::query(
        "INSERT INTO contest_reminders_sent (tournament_id, user_id, moment)
         VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
    )
    .bind(tournament_id)
    .bind(user_id)
    .bind(moment)
    .execute(db)
    .await
    {
        // Worth a line: the consequence is a duplicate reminder next hour,
        // which is annoying rather than dangerous — but it is also the first
        // sign of a table nobody can write to.
        tracing::warn!(%tournament_id, %user_id, moment, error = %e, "reminder not recorded");
    }
}
