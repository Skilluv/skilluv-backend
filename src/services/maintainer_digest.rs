//! P26 v2 SKI-120 — maintainer weekly digest.
//!
//! External-repo maintainers can subscribe (double opt-in) to receive a
//! weekly summary of Skilluv activity on their repos. Zero-spam policy:
//!   - self-serve subscribe (no auto-subscription even when we detect
//!     shadow contributions on their repo — see community-first policy)
//!   - confirm email must be clicked
//!   - unsubscribe token in every digest email
//!
//! Background task runs every hour and picks all confirmed subscriptions
//! whose `last_digest_at` is > 7 days old (or NULL). One-hour tick is
//! precise enough — a maintainer subscribed on Wednesday can get their
//! first digest by Wednesday of the following week at the same hour.

use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::EmailService;

pub const DIGEST_PERIOD_DAYS: i32 = 7;

// ─── Data types ───────────────────────────────────────────────────

#[derive(Debug, sqlx::FromRow)]
pub struct Subscription {
    pub id: Uuid,
    pub github_login: String,
    pub email: String,
    pub repos: Vec<String>,
    pub confirm_token: String,
    pub unsubscribe_token: String,
    pub confirmed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub unsubscribed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_digest_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

// ─── URL-safe random tokens ───────────────────────────────────────

/// 32-byte token base64url-encoded (44 chars). Cryptographically random,
/// distinct per confirm / unsubscribe purpose so a leaked confirm link
/// cannot unsubscribe someone else and vice-versa.
pub fn new_token() -> String {
    use base64::Engine;
    let mut bytes = [0u8; 32];
    // `rand_core::OsRng` is gone in 0.10; `getrandom::fill` is the same OS
    // entropy it wrapped, and what the rest of this codebase uses for tokens.
    getrandom::fill(&mut bytes).expect("OS RNG");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

// ─── Subscribe / confirm / unsubscribe ────────────────────────────

pub struct SubscribeInput {
    pub github_login: String,
    pub email: String,
    pub repos: Vec<String>,
}

/// Insert (or re-issue tokens for) a pending subscription and send the
/// confirmation email. Returning the row lets the caller expose the
/// confirm URL in dev logs.
pub async fn subscribe(
    db: &PgPool,
    email_svc: &EmailService,
    base_url: &str,
    input: SubscribeInput,
) -> Result<Subscription, AppError> {
    if input.repos.is_empty() {
        return Err(AppError::Validation("repos must be non-empty".into()));
    }

    let confirm_token = new_token();
    let unsubscribe_token = new_token();

    let row: Subscription = sqlx::query_as(
        r#"
        INSERT INTO maintainer_digest_subscriptions
            (github_login, email, repos, confirm_token, unsubscribe_token)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (email)
            WHERE unsubscribed_at IS NULL
        DO UPDATE SET
            github_login = EXCLUDED.github_login,
            repos = EXCLUDED.repos,
            confirm_token = EXCLUDED.confirm_token,
            updated_at = NOW()
        RETURNING *
        "#,
    )
    .bind(&input.github_login)
    .bind(&input.email)
    .bind(&input.repos)
    .bind(&confirm_token)
    .bind(&unsubscribe_token)
    .fetch_one(db)
    .await?;

    let confirm_url = format!(
        "{}/maintainer-digest/confirm/{}",
        base_url.trim_end_matches('/'),
        row.confirm_token
    );
    let html = format!(
        r#"<p>Hello,</p>
<p>Please confirm your subscription to the Skilluv weekly digest for repos:
{repos}</p>
<p><a href="{url}">Confirm subscription</a></p>
<p>If you did not request this, simply ignore this email.</p>"#,
        repos = row.repos.join(", "),
        url = confirm_url,
    );
    email_svc
        .send_direct(
            &row.email,
            &row.github_login,
            "Confirm your Skilluv weekly digest",
            &html,
        )
        .await?;

    Ok(row)
}

pub async fn confirm(db: &PgPool, token: &str) -> Result<Subscription, AppError> {
    let row: Option<Subscription> = sqlx::query_as(
        r#"
        UPDATE maintainer_digest_subscriptions
           SET confirmed_at = COALESCE(confirmed_at, NOW()),
               updated_at = NOW()
         WHERE confirm_token = $1
           AND unsubscribed_at IS NULL
     RETURNING *
        "#,
    )
    .bind(token)
    .fetch_optional(db)
    .await?;
    row.ok_or_else(|| AppError::NotFound("invalid or expired confirm token".into()))
}

pub async fn unsubscribe(db: &PgPool, token: &str) -> Result<(), AppError> {
    let affected = sqlx::query(
        r#"
        UPDATE maintainer_digest_subscriptions
           SET unsubscribed_at = NOW(), updated_at = NOW()
         WHERE unsubscribe_token = $1
           AND unsubscribed_at IS NULL
        "#,
    )
    .bind(token)
    .execute(db)
    .await?
    .rows_affected();
    if affected == 0 {
        return Err(AppError::NotFound("invalid or already unsubscribed".into()));
    }
    Ok(())
}

// ─── Weekly digest sending ────────────────────────────────────────

pub async fn send_due_digests(
    db: &PgPool,
    email_svc: &EmailService,
    base_url: &str,
) -> Result<usize, AppError> {
    let due: Vec<Subscription> = sqlx::query_as(
        r#"
        SELECT * FROM maintainer_digest_subscriptions
         WHERE confirmed_at IS NOT NULL
           AND unsubscribed_at IS NULL
           AND (last_digest_at IS NULL
                OR last_digest_at < NOW() - ($1 || ' days')::interval)
         ORDER BY last_digest_at ASC NULLS FIRST
         LIMIT 50
        "#,
    )
    .bind(DIGEST_PERIOD_DAYS.to_string())
    .fetch_all(db)
    .await?;

    let mut sent = 0usize;
    for sub in due {
        match render_and_send(db, email_svc, base_url, &sub).await {
            Ok(()) => {
                let _ = sqlx::query(
                    "UPDATE maintainer_digest_subscriptions SET last_digest_at = NOW(), updated_at = NOW() WHERE id = $1",
                )
                .bind(sub.id)
                .execute(db)
                .await;
                sent += 1;
            }
            Err(e) => {
                tracing::warn!(sub_id = %sub.id, error = %e, "digest send failed");
            }
        }
    }
    Ok(sent)
}

async fn render_and_send(
    db: &PgPool,
    email_svc: &EmailService,
    base_url: &str,
    sub: &Subscription,
) -> Result<(), AppError> {
    // For each subscribed repo, count new activity in the last N days.
    let mut sections = Vec::with_capacity(sub.repos.len());
    for repo in &sub.repos {
        let Some((owner, name)) = repo.split_once('/') else {
            continue;
        };
        let stats: (i64, i64, i64) = sqlx::query_as(
            r#"
            SELECT
              COUNT(*) FILTER (WHERE s.claimed_at    > NOW() - ($3 || ' days')::interval)::bigint,
              COUNT(*) FILTER (WHERE s.submitted_at  > NOW() - ($3 || ' days')::interval)::bigint,
              COUNT(*) FILTER (WHERE s.validated_at  > NOW() - ($3 || ' days')::interval)::bigint
              FROM project_slices s
              JOIN projects p ON p.id = s.project_id
             WHERE p.github_repo_owner = $1 AND p.github_repo_name = $2
            "#,
        )
        .bind(owner)
        .bind(name)
        .bind(DIGEST_PERIOD_DAYS.to_string())
        .fetch_one(db)
        .await
        .unwrap_or((0, 0, 0));

        sections.push(format!(
            "<li><strong>{repo}</strong> — {} claimed, {} submitted, {} validated in the last {} days.</li>",
            stats.0, stats.1, stats.2, DIGEST_PERIOD_DAYS,
        ));
    }

    let unsubscribe_url = format!(
        "{}/maintainer-digest/unsubscribe/{}",
        base_url.trim_end_matches('/'),
        sub.unsubscribe_token
    );
    let html = format!(
        r#"<p>Hello {login},</p>
<p>Skilluv community activity on your repos this past week:</p>
<ul>
{sections}
</ul>
<p>Public dashboards: <a href="{base}">skill-uv.com</a></p>
<hr/>
<p style="color:#666;font-size:11px">
Not interested any more? <a href="{unsub}">Unsubscribe with one click</a>.
</p>"#,
        login = sub.github_login,
        sections = sections.join("\n"),
        base = base_url,
        unsub = unsubscribe_url,
    );

    email_svc
        .send_direct(
            &sub.email,
            &sub.github_login,
            "Skilluv weekly digest",
            &html,
        )
        .await
}

// ─── Background task ──────────────────────────────────────────────

pub fn start_maintainer_digest_task(db: PgPool, email_svc: Arc<EmailService>, base_url: String) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(3600));
        loop {
            ticker.tick().await;
            match send_due_digests(&db, &email_svc, &base_url).await {
                Ok(n) if n > 0 => {
                    tracing::info!(sent = n, "SKI-120 digest tick");
                }
                Ok(_) => {}
                Err(e) => tracing::warn!(error = %e, "SKI-120 digest tick failed"),
            }
        }
    });
}
