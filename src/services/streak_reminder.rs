//! The reminder the settings screen has been promising since phase 1.7.
//!
//! `user_email_preferences.streak_reminder` shipped as a checkbox and
//! nothing ever sent one. A toggle that does nothing is worse than an
//! absent feature: someone turns it on, believes they are covered, and
//! loses the streak anyway.
//!
//! ## What "at risk" means
//!
//! A streak breaks when a day passes with no contribution. So the reminder
//! goes to people whose last activity was *yesterday* — they still have a
//! streak, and today is the day it ends. Someone active today needs
//! nothing; someone whose last activity was two days ago has already lost
//! it, and being told so is a notification about a failure, which nobody
//! asked for.
//!
//! ## Why it defaults to push and not email
//!
//! It is time-limited by construction. An email that arrives after the day
//! turns over is not a reminder, it is a reproach.

use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

/// Streaks shorter than this get no reminder.
///
/// A one-day "streak" is a day. Notifying someone about it teaches them to
/// ignore the notification before they ever have a streak worth keeping.
const MIN_STREAK_WORTH_KEEPING: i32 = 3;

/// How many people one daily run will reach.
const MAX_PER_RUN: usize = 100_000;

#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct ReminderReport {
    /// True when the ceiling was hit — people at risk who were not told.
    pub truncated: bool,
    pub at_risk: usize,
    pub sent: usize,
    pub declined: usize,
    pub failures: usize,
}

/// Notify everyone whose streak ends today unless they act.
pub async fn run(db: &PgPool) -> Result<ReminderReport, AppError> {
    let mut report = ReminderReport::default();

    #[derive(sqlx::FromRow)]
    struct AtRisk {
        id: Uuid,
        streak_current: i32,
    }

    // Yesterday in the account's own day, not in UTC: a talent in Cotonou
    // and one in Paris do not agree on when today started, and reminding
    // the wrong one is reminding them after it is too late.
    // `LIMIT 5000` with no cursor meant the 5001st person at risk lost their
    // streak in silence. Walked in pages now, with the ceiling reported.
    let mut walk = crate::services::batch::Walk::new("streak_reminder", MAX_PER_RUN);
    let mut page_len;

    loop {
        let at_risk: Vec<AtRisk> = sqlx::query_as(
            "SELECT id, streak_current
           FROM users
          WHERE is_banned = FALSE
            AND profile_active = TRUE
            AND streak_current >= $1
            AND streak_last_activity IS NOT NULL
            AND (streak_last_activity AT TIME ZONE COALESCE(timezone, 'UTC'))::date
                = ((NOW() AT TIME ZONE COALESCE(timezone, 'UTC'))::date - 1)
            AND ($2::uuid IS NULL OR id > $2)
          ORDER BY id
          LIMIT $3",
        )
        .bind(MIN_STREAK_WORTH_KEEPING)
        .bind(walk.after())
        .bind(walk.page_size())
        .fetch_all(db)
        .await?;

        page_len = at_risk.len();
        let Some(last) = at_risk.last().map(|p| p.id) else {
            break;
        };

        for person in at_risk {
            report.at_risk += 1;

            // Database only: the reminder is a push and an in-app record, and
            // the sweep has no email service. A person who opted the email on
            // explicitly gets it from the delivery below only if a context with
            // one is supplied — which is why this runs from the app, not a
            // standalone binary.
            let outcome = crate::services::notify::send(
                crate::services::notify::Ctx::db_only(db),
                crate::services::notify::Recipient::User(person.id),
                "streak.reminder",
            )
            .arg("days", person.streak_current.to_string())
            .payload(serde_json::json!({ "streak_current": person.streak_current }))
            .execute()
            .await;

            match outcome {
                Ok(delivery) if delivery.push > 0 || delivery.in_app > 0 => report.sent += 1,
                // Declined every channel. The preference working, not a failure.
                Ok(_) => report.declined += 1,
                Err(e) => {
                    tracing::warn!(user = %person.id, error = %e, "streak reminder failed");
                    report.failures += 1;
                }
            }
        }

        walk.advance(page_len, last);
        if !walk.should_continue(page_len) {
            break;
        }
    }

    report.truncated = walk.finish(page_len) == crate::services::batch::Ending::Truncated;
    metrics::counter!("skilluv_streak_reminders_sent_total").increment(report.sent as u64);
    Ok(report)
}

/// Run once a day, at an hour that is morning somewhere useful.
///
/// Hourly ticks with a guard rather than a daily interval: a process
/// restart resets a `tokio::time::interval`, and a daily one would mean a
/// deployment at the wrong moment skips a whole day of reminders.
pub fn start_streak_reminder_task(db: PgPool) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(3600));
        let mut last_run_date: Option<chrono::NaiveDate> = None;
        loop {
            ticker.tick().await;

            let now = chrono::Utc::now();
            let today = now.date_naive();
            // Late enough that a morning contribution already counts, early
            // enough to leave the day usable.
            if now.format("%H").to_string() != "17" || last_run_date == Some(today) {
                continue;
            }

            match run(&db).await {
                Ok(report) => {
                    last_run_date = Some(today);
                    if report.at_risk > 0 {
                        tracing::info!(
                            at_risk = report.at_risk,
                            sent = report.sent,
                            declined = report.declined,
                            "streak reminders"
                        );
                    }
                }
                Err(e) => tracing::warn!(error = %e, "streak reminder run failed"),
            }
        }
    });
}
