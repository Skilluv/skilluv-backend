//! Drip email sequences — onboarding and retention.
//!
//! Triggered hourly by a background task. Each send is recorded in
//! `email_log`, so the same sequence never reaches the same person twice.
//!
//! What is here is *when*: which accounts are at the right age, and whether
//! they have done the thing the message is about. Everything else — the
//! words, the language, the theme, the button, the unsubscribe link and
//! whether this person consented at all — belongs to
//! [`crate::services::notify`].
//!
//! It did not, until now. Each sequence carried a French subject line and a
//! slab of hand-written HTML with the accent colour typed in, and the
//! targeting query filtered on `user_email_preferences.marketing` while
//! `notify` read the catalogue. Two answers to "may we write to this
//! person", and the one that won was whichever code path ran.

use std::time::Duration;

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::EmailService;
use std::sync::Arc;

/// How many accounts one sequence walks per run.
///
/// Each sequence targets a one-day-wide signup window and runs hourly, so
/// this is only reachable on a day with more signups than that — which is
/// a good problem, and one worth being told about rather than truncated
/// through.
const MAX_PER_SEQUENCE: usize = 50_000;

#[derive(Debug, Clone, Serialize)]
pub struct DripRunReport {
    pub sequences_evaluated: usize,
    pub emails_sent: usize,
    pub emails_skipped_already_sent: usize,
    pub emails_skipped_no_match: usize,
    pub failures: usize,
}

/// What building an email needs beyond the database.
///
/// Carried rather than read from the environment at the point of use: a
/// background task holds these for its whole life, and a sequence that
/// silently sent links to `localhost` because a variable was missing is
/// the kind of failure nobody sees until a recipient reports it.
#[derive(Clone, Copy)]
pub struct Site<'a> {
    /// Where a human clicks. Every button in every sequence starts here.
    pub frontend_url: &'a str,
    /// Signs the one-click unsubscribe link. Marketing mail without one is
    /// not acceptable, and for a bulk sender it is not legal either.
    pub jwt_secret: &'a str,
}

pub fn start_drip_task(
    db: PgPool,
    email: Arc<EmailService>,
    frontend_url: String,
    jwt_secret: String,
) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(3600));
        loop {
            ticker.tick().await;
            let site = Site {
                frontend_url: &frontend_url,
                jwt_secret: &jwt_secret,
            };
            if let Err(err) = run_all(&db, &email, site).await {
                tracing::warn!(error = %err, "drip sequences run failed");
            }
        }
    });
}

/// Send one sequence message, or report that it went out before.
///
/// The whole delivery is `notify`'s: it asks the catalogue whether this
/// person consented, resolves their language and theme, renders the shared
/// template and logs the send under the same kind this sequence
/// deduplicates on.
async fn deliver(
    db: &PgPool,
    email: &EmailService,
    site: Site<'_>,
    user_id: Uuid,
    kind: &str,
) -> Result<bool, AppError> {
    let delivery = crate::services::notify::send(
        crate::services::notify::Ctx {
            db,
            redis: None,
            ws: None,
            email: Some(email),
            frontend_url: Some(site.frontend_url),
            jwt_secret: Some(site.jwt_secret),
        },
        crate::services::notify::Recipient::User(user_id),
        kind,
    )
    .execute()
    .await?;

    Ok(delivery.email > 0)
}

pub async fn run_all(
    db: &PgPool,
    email: &EmailService,
    site: Site<'_>,
) -> Result<DripRunReport, AppError> {
    let mut report = DripRunReport {
        sequences_evaluated: 0,
        emails_sent: 0,
        emails_skipped_already_sent: 0,
        emails_skipped_no_match: 0,
        failures: 0,
    };

    for seq in talent_sequences() {
        report.sequences_evaluated += 1;
        if let Err(err) = run_sequence(db, email, site, &seq, &mut report).await {
            tracing::warn!(seq = seq.kind, error = %err, "drip sequence failed");
            report.failures += 1;
        }
    }
    for seq in enterprise_sequences() {
        report.sequences_evaluated += 1;
        if let Err(err) = run_enterprise_sequence(db, email, site, &seq, &mut report).await {
            tracing::warn!(seq = seq.kind, error = %err, "drip sequence failed");
            report.failures += 1;
        }
    }
    Ok(report)
}

/// One message in a sequence: when it fires, and to whom.
///
/// `kind` is the catalogue kind, which is also the `email_log` key and so
/// the deduplication key. One name for one message, everywhere.
struct TalentSeq {
    kind: &'static str,
    delay_min_days: i64,
    delay_max_days: i64,
    require_inactive: bool,
}

fn talent_sequences() -> Vec<TalentSeq> {
    vec![
        // Signed up yesterday, has not tried anything yet.
        TalentSeq {
            kind: "lifecycle.activate",
            delay_min_days: 1,
            delay_max_days: 2,
            require_inactive: true,
        },
        // Active, and alone. A guild is the difference between a tool and
        // a place, which is the whole thesis.
        TalentSeq {
            kind: "lifecycle.join_guild",
            delay_min_days: 3,
            delay_max_days: 4,
            require_inactive: false,
        },
        TalentSeq {
            kind: "lifecycle.silent",
            delay_min_days: 14,
            delay_max_days: 15,
            require_inactive: true,
        },
        // The last one. It says so, and it is the last one — after this the
        // sequence stops rather than nagging forever.
        TalentSeq {
            kind: "lifecycle.last_chance",
            delay_min_days: 30,
            delay_max_days: 31,
            require_inactive: true,
        },
    ]
}

async fn run_sequence(
    db: &PgPool,
    email: &EmailService,
    site: Site<'_>,
    seq: &TalentSeq,
    report: &mut DripRunReport,
) -> Result<(), AppError> {
    let since_min = Utc::now() - ChronoDuration::days(seq.delay_max_days);
    let since_max = Utc::now() - ChronoDuration::days(seq.delay_min_days);

    // This selected `LIMIT 500` with no cursor. The 501st eligible account
    // never received the message — not late, never — and nothing said so.
    // The window is one day wide, so the ceiling is only reachable on a day
    // with more than that many signups, and hitting it is now logged.
    let mut walk = crate::services::batch::Walk::new("drip_talent", MAX_PER_SEQUENCE);
    let mut page_len;

    loop {
        let candidates: Vec<(Uuid, Option<DateTime<Utc>>)> = sqlx::query_as(
            r#"
            SELECT u.id,
                   (SELECT MAX(evaluated_at) FROM challenge_submissions cs WHERE cs.user_id = u.id) AS last_activity
            FROM users u
            WHERE u.email_disabled = FALSE
              AND u.is_banned = FALSE
              AND u.created_at BETWEEN $1 AND $2
              AND NOT EXISTS (
                  SELECT 1 FROM email_log el WHERE el.user_id = u.id AND el.kind = $3
              )
              AND ($4::uuid IS NULL OR u.id > $4)
            ORDER BY u.id
            LIMIT $5
            "#,
        )
        .bind(since_min)
        .bind(since_max)
        .bind(seq.kind)
        .bind(walk.after())
        .bind(walk.page_size())
        .fetch_all(db)
        .await?;

        page_len = candidates.len();
        let Some(last) = candidates.last().map(|(id, _)| *id) else {
            break;
        };

        for (user_id, last_activity) in candidates {
            if seq.require_inactive {
                let recently_active = last_activity
                    .map(|d| (Utc::now() - d) < ChronoDuration::days(seq.delay_min_days))
                    .unwrap_or(false);
                if recently_active {
                    report.emails_skipped_no_match += 1;
                    continue;
                }
            }
            match deliver(db, email, site, user_id, seq.kind).await {
                Ok(true) => report.emails_sent += 1,
                Ok(false) => report.emails_skipped_already_sent += 1,
                Err(_) => report.failures += 1,
            }
        }

        walk.advance(page_len, last);
        if !walk.should_continue(page_len) {
            break;
        }
    }

    walk.finish(page_len);
    Ok(())
}

struct EntSeq {
    kind: &'static str,
    delay_min_days: i64,
    delay_max_days: i64,
    require_no_credit_use: bool,
}

fn enterprise_sequences() -> Vec<EntSeq> {
    vec![
        EntSeq {
            kind: "lifecycle.enterprise_welcome",
            delay_min_days: 1,
            delay_max_days: 2,
            require_no_credit_use: true,
        },
        EntSeq {
            kind: "lifecycle.enterprise_demo",
            delay_min_days: 3,
            delay_max_days: 4,
            require_no_credit_use: true,
        },
        EntSeq {
            kind: "lifecycle.enterprise_value",
            delay_min_days: 7,
            delay_max_days: 8,
            require_no_credit_use: false,
        },
    ]
}

async fn run_enterprise_sequence(
    db: &PgPool,
    email: &EmailService,
    site: Site<'_>,
    seq: &EntSeq,
    report: &mut DripRunReport,
) -> Result<(), AppError> {
    let since_min = Utc::now() - ChronoDuration::days(seq.delay_max_days);
    let since_max = Utc::now() - ChronoDuration::days(seq.delay_min_days);
    let mut walk = crate::services::batch::Walk::new("drip_enterprise", MAX_PER_SEQUENCE);
    let mut page_len;

    loop {
        // Every active member of the enterprise, not only the founder: the
        // person who signed up is often not the one who does the hiring.
        let candidates: Vec<(Uuid, i32)> = sqlx::query_as(
            r#"
            SELECT u.id, COALESCE(ec.total_used, 0)::INT AS credits_used_count
            FROM enterprises e
            JOIN enterprise_members em ON em.enterprise_id = e.id AND em.status = 'active'
            JOIN users u ON u.id = em.user_id
            LEFT JOIN enterprise_credits ec ON ec.enterprise_id = e.id
            WHERE u.email_disabled = FALSE
              AND u.is_banned = FALSE
              AND e.created_at BETWEEN $1 AND $2
              AND NOT EXISTS (
                  SELECT 1 FROM email_log el WHERE el.user_id = u.id AND el.kind = $3
              )
              AND ($4::uuid IS NULL OR u.id > $4)
            ORDER BY u.id
            LIMIT $5
            "#,
        )
        .bind(since_min)
        .bind(since_max)
        .bind(seq.kind)
        .bind(walk.after())
        .bind(walk.page_size())
        .fetch_all(db)
        .await?;

        page_len = candidates.len();
        let Some(last) = candidates.last().map(|(id, _)| *id) else {
            break;
        };

        for (user_id, credits_used) in candidates {
            if seq.require_no_credit_use && credits_used > 0 {
                report.emails_skipped_no_match += 1;
                continue;
            }
            match deliver(db, email, site, user_id, seq.kind).await {
                Ok(true) => report.emails_sent += 1,
                Ok(false) => report.emails_skipped_already_sent += 1,
                Err(_) => report.failures += 1,
            }
        }

        walk.advance(page_len, last);
        if !walk.should_continue(page_len) {
            break;
        }
    }

    walk.finish(page_len);
    Ok(())
}
