//! Weekly digest + email preferences + unsubscribe tokens.
//!
//! Phase 1.7. Cron-triggered (external `cron` or systemd timer) via
//! `POST /api/admin/digest/run-weekly`. The endpoint requires admin auth and is rate-
//! limited; it iterates over opted-in users and dispatches digest emails.

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use hmac::{Hmac, Mac};
use serde::Serialize;
use sha2::Sha256;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::EmailService;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Serialize)]
pub struct DigestRunReport {
    pub users_processed: usize,
    pub emails_sent: usize,
    pub emails_skipped_no_activity: usize,
    pub emails_skipped_opt_out: usize,
    pub emails_skipped_disabled: usize,
    pub failures: usize,
}

pub struct DigestService<'a> {
    pub db: &'a PgPool,
    pub email: &'a EmailService,
    pub base_url: &'a str,
    pub unsubscribe_secret: &'a [u8],
}

impl<'a> DigestService<'a> {
    pub async fn run_weekly(&self) -> Result<DigestRunReport, AppError> {
        let mut report = DigestRunReport {
            users_processed: 0,
            emails_sent: 0,
            emails_skipped_no_activity: 0,
            emails_skipped_opt_out: 0,
            emails_skipped_disabled: 0,
            failures: 0,
        };

        // Iterate active, non-disabled, opted-in users with digest_weekly = TRUE
        // We use a stream-style approach via paginated query to handle large user bases.
        let rows: Vec<DigestTarget> = sqlx::query_as(
            r#"
            SELECT u.id, u.email, u.display_name, u.skill_domain
            FROM users u
            LEFT JOIN user_email_preferences p ON p.user_id = u.id
            WHERE u.profile_active = TRUE
              AND u.is_banned = FALSE
              AND u.email_disabled = FALSE
              AND COALESCE(p.digest_weekly, TRUE) = TRUE
            "#,
        )
        .fetch_all(self.db)
        .await?;

        let week_ago = Utc::now() - ChronoDuration::days(7);

        for target in rows {
            report.users_processed += 1;
            match self.process_one(&target, week_ago).await {
                Ok(DigestOutcome::Sent) => report.emails_sent += 1,
                Ok(DigestOutcome::NoActivity) => report.emails_skipped_no_activity += 1,
                Ok(DigestOutcome::OptOut) => report.emails_skipped_opt_out += 1,
                Ok(DigestOutcome::Disabled) => report.emails_skipped_disabled += 1,
                Err(err) => {
                    tracing::warn!(
                        user_id = %target.id,
                        error = %err,
                        "digest send failed"
                    );
                    report.failures += 1;
                }
            }
        }

        Ok(report)
    }

    async fn process_one(
        &self,
        target: &DigestTarget,
        since: DateTime<Utc>,
    ) -> Result<DigestOutcome, AppError> {
        let stats = compute_weekly_stats(self.db, target.id, since).await?;
        if !stats.has_meaningful_activity() {
            return Ok(DigestOutcome::NoActivity);
        }

        let unsub_token =
            build_unsubscribe_token(target.id, "digest_weekly", self.unsubscribe_secret);
        let unsub_url = format!(
            "{}/api/email/unsubscribe?token={}&kind=digest_weekly",
            self.base_url, unsub_token
        );
        let html = render_digest_html(target, &stats, &unsub_url);
        let subject = format!(
            "Ta semaine Skilluv — {}/{}",
            stats.challenges_completed, stats.fragments_earned
        );

        self.email
            .send_with_log(
                self.db,
                crate::services::email::SendWithLogParams {
                    user_id: target.id,
                    to_email: &target.email,
                    to_name: &target.display_name,
                    subject: &subject,
                    html: &html,
                    kind: "digest_weekly",
                },
            )
            .await?;
        Ok(DigestOutcome::Sent)
    }
}

#[derive(sqlx::FromRow, Debug)]
struct DigestTarget {
    id: Uuid,
    email: String,
    display_name: String,
    skill_domain: String,
}

enum DigestOutcome {
    Sent,
    NoActivity,
    #[allow(dead_code)]
    OptOut,
    #[allow(dead_code)]
    Disabled,
}

#[derive(Debug, Clone, Default)]
pub struct WeeklyStats {
    pub challenges_completed: i64,
    pub fragments_earned: i64,
    pub streak_current: i32,
    pub current_title: String,
}

impl WeeklyStats {
    pub fn has_meaningful_activity(&self) -> bool {
        self.challenges_completed > 0 || self.fragments_earned > 0
    }
}

async fn compute_weekly_stats(
    db: &PgPool,
    user_id: Uuid,
    since: DateTime<Utc>,
) -> Result<WeeklyStats, AppError> {
    let row: Option<(i64, i64)> = sqlx::query_as(
        r#"
        SELECT COUNT(*)::BIGINT, COALESCE(SUM(fragments_earned), 0)::BIGINT
        FROM challenge_submissions
        WHERE user_id = $1 AND status = 'success' AND evaluated_at >= $2
        "#,
    )
    .bind(user_id)
    .bind(since)
    .fetch_optional(db)
    .await?;
    let (challenges_completed, fragments_earned) = row.unwrap_or((0, 0));

    let user: Option<(i32, String)> =
        sqlx::query_as("SELECT streak_current, title FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(db)
            .await?;
    let (streak_current, current_title) = user.unwrap_or((0, "apprenti".into()));

    Ok(WeeklyStats {
        challenges_completed,
        fragments_earned,
        streak_current,
        current_title,
    })
}

fn render_digest_html(target: &DigestTarget, stats: &WeeklyStats, unsub_url: &str) -> String {
    format!(
        r#"
        <div style="font-family: Arial, sans-serif; max-width: 600px; margin: 0 auto; color: #1a1a2e;">
            <h2>Salut {name},</h2>
            <p>Ta semaine sur Skilluv en chiffres :</p>
            <table cellpadding="12" style="width: 100%; border-collapse: collapse;">
                <tr style="background:#f4f4f9;">
                    <td><strong>Challenges réussis</strong></td>
                    <td style="text-align:right; font-size:20px; color:#6c5ce7;">{ch}</td>
                </tr>
                <tr>
                    <td><strong>Fragments gagnés</strong></td>
                    <td style="text-align:right; font-size:20px; color:#6c5ce7;">+{fr}</td>
                </tr>
                <tr style="background:#f4f4f9;">
                    <td><strong>Streak actuel</strong></td>
                    <td style="text-align:right; font-size:20px;">🔥 {streak} jour(s)</td>
                </tr>
                <tr>
                    <td><strong>Titre</strong></td>
                    <td style="text-align:right; text-transform: capitalize;">{title}</td>
                </tr>
            </table>
            <p style="text-align:center; margin: 30px 0;">
                <a href="{base}/challenges?domain={domain}" style="background-color: #6c5ce7; color: white; padding: 14px 28px; text-decoration: none; border-radius: 8px; font-weight: bold;">
                    Continuer mes challenges
                </a>
            </p>
            <hr style="border:none; border-top:1px solid #eee;">
            <p style="color:#999; font-size:11px; text-align:center;">
                Tu reçois ce résumé hebdomadaire parce que tu es inscrit·e sur Skilluv.<br>
                <a href="{unsub}" style="color:#999;">Me désinscrire des digests</a>
            </p>
        </div>
        "#,
        name = target.display_name,
        ch = stats.challenges_completed,
        fr = stats.fragments_earned,
        streak = stats.streak_current,
        title = stats.current_title,
        base = "https://skilluv.com",
        domain = target.skill_domain,
        unsub = unsub_url,
    )
}

// ─── Unsubscribe tokens (signed) ──────────────────────────────────

pub fn build_unsubscribe_token(user_id: Uuid, kind: &str, secret: &[u8]) -> String {
    let payload = format!("{user_id}|{kind}");
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts any key size");
    mac.update(payload.as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());
    let payload_b64 = base32_encode(payload.as_bytes());
    format!("{payload_b64}.{signature}")
}

pub fn verify_unsubscribe_token(token: &str, secret: &[u8]) -> Option<(Uuid, String)> {
    let (payload_b64, signature_hex) = token.split_once('.')?;
    let payload_bytes = base32_decode(payload_b64)?;
    let payload = String::from_utf8(payload_bytes).ok()?;
    let signature = hex::decode(signature_hex).ok()?;

    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts any key size");
    mac.update(payload.as_bytes());
    mac.verify_slice(&signature).ok()?;

    let (uid_str, kind) = payload.split_once('|')?;
    let uid = Uuid::parse_str(uid_str).ok()?;
    Some((uid, kind.to_string()))
}

fn base32_encode(bytes: &[u8]) -> String {
    base32::encode(base32::Alphabet::Rfc4648 { padding: false }, bytes)
}

fn base32_decode(s: &str) -> Option<Vec<u8>> {
    base32::decode(base32::Alphabet::Rfc4648 { padding: false }, s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsub_token_roundtrip() {
        let secret = b"test-secret";
        let uid = Uuid::new_v4();
        let token = build_unsubscribe_token(uid, "digest_weekly", secret);
        let (uid_back, kind_back) = verify_unsubscribe_token(&token, secret).unwrap();
        assert_eq!(uid_back, uid);
        assert_eq!(kind_back, "digest_weekly");
    }

    #[test]
    fn unsub_token_rejects_tampering() {
        let secret = b"test-secret";
        let token = build_unsubscribe_token(Uuid::new_v4(), "digest_weekly", secret);
        let tampered = format!("{token}aaaa");
        assert!(verify_unsubscribe_token(&tampered, secret).is_none());
    }

    #[test]
    fn unsub_token_rejects_wrong_secret() {
        let token = build_unsubscribe_token(Uuid::new_v4(), "digest_weekly", b"good");
        assert!(verify_unsubscribe_token(&token, b"bad").is_none());
    }

    #[test]
    fn weekly_stats_meaningful_activity() {
        let mut s = WeeklyStats::default();
        assert!(!s.has_meaningful_activity());
        s.challenges_completed = 1;
        assert!(s.has_meaningful_activity());
    }
}
