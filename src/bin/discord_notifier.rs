//! skilluv-discord-notifier (SKI-34, Hygiène pré-prod HYG-07)
//!
//! Simple long-poll binary that watches `discord_notifications_queue`,
//! posts pending notifications to Discord channels via webhook URLs,
//! and marks them sent.
//!
//! Deliberately does NOT implement Discord "gateway bot" semantics
//! (welcome DM, slash commands) — those require a persistent WebSocket
//! connection and a heavier framework like `serenity`. Ticket follow-up
//! can add them once we're sure the notification queue works end-to-end.
//!
//! Usage:
//!   DISCORD_PROMOTIONS_WEBHOOK_URL=https://discord.com/api/webhooks/...
//!   DISCORD_ANNONCES_WEBHOOK_URL=https://discord.com/api/webhooks/...
//!   DATABASE_URL=postgres://...
//!   cargo run --bin skilluv-discord-notifier
//!
//! Runs forever, polling every 15 seconds. Deploy as a long-running
//! sidecar on Coolify (same repo, separate Coolify "app" pointing at
//! this binary via a Dockerfile stage).

use anyhow::{Context, Result};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const POLL_INTERVAL_SECONDS: u64 = 15;
const MAX_FAILED_ATTEMPTS: i16 = 10;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .compact()
        .init();

    let db_url = std::env::var("DATABASE_URL").context("DATABASE_URL required")?;
    let db = PgPool::connect(&db_url).await?;

    let promotions_url = std::env::var("DISCORD_PROMOTIONS_WEBHOOK_URL").ok();
    let annonces_url = std::env::var("DISCORD_ANNONCES_WEBHOOK_URL").ok();
    if promotions_url.is_none() && annonces_url.is_none() {
        tracing::warn!(
            "no Discord webhook URLs configured — bot will poll but never post. \
             Set DISCORD_PROMOTIONS_WEBHOOK_URL and/or DISCORD_ANNONCES_WEBHOOK_URL."
        );
    }

    tracing::info!("Discord notifier started, polling every {POLL_INTERVAL_SECONDS}s");
    loop {
        match tick(&db, promotions_url.as_deref(), annonces_url.as_deref()).await {
            Ok(n) if n > 0 => tracing::info!(sent = n, "tick posted notifications"),
            Ok(_) => {}
            Err(e) => tracing::warn!(error = %e, "tick failed"),
        }
        tokio::time::sleep(std::time::Duration::from_secs(POLL_INTERVAL_SECONDS)).await;
    }
}

#[derive(Debug, sqlx::FromRow)]
struct QueueRow {
    id: Uuid,
    event_type: String,
    payload_json: sqlx::types::Json<Value>,
    #[allow(dead_code)]
    target_channel_id: Option<String>,
}

async fn tick(
    db: &PgPool,
    promotions_url: Option<&str>,
    annonces_url: Option<&str>,
) -> Result<usize> {
    let rows: Vec<QueueRow> = sqlx::query_as(
        r#"
        SELECT id, event_type, payload_json, target_channel_id
          FROM discord_notifications_queue
         WHERE sent_at IS NULL AND failed_count < $1
         ORDER BY created_at ASC
         LIMIT 20
        "#,
    )
    .bind(MAX_FAILED_ATTEMPTS)
    .fetch_all(db)
    .await?;

    let mut sent = 0usize;
    for row in rows {
        // Route to a webhook based on event type.
        let webhook_url = match row.event_type.as_str() {
            "rank_promotion" | "badge_earned" => promotions_url,
            "attestation_new" | "slice_validated" => annonces_url,
            _ => None,
        };
        let Some(url) = webhook_url else {
            // Two different reasons to have no webhook, and they must not be
            // treated the same.
            //
            // An event type this binary *knows* and whose webhook the operator
            // has not wired: mark it sent, so the queue does not swell against
            // a channel that will never exist.
            //
            // An event type it does not know — `contest_opened`,
            // `contest_won`, `talent_featured`, `mission_posted`, added by
            // migration 0257 — belongs to `skilluv-discord-bot`, which routes
            // on `target_channel_id`. Marking those sent here silently
            // destroys them: this binary is the v1 fallback, and a fallback
            // that eats the successor's messages is worse than one that does
            // nothing. Left untouched, so whichever consumer can post them
            // still finds them.
            if !HANDLED_EVENTS.contains(&row.event_type.as_str()) {
                tracing::debug!(
                    event_type = %row.event_type,
                    "left for skilluv-discord-bot: this binary cannot route it"
                );
                continue;
            }
            let _ =
                sqlx::query("UPDATE discord_notifications_queue SET sent_at = NOW() WHERE id = $1")
                    .bind(row.id)
                    .execute(db)
                    .await;
            continue;
        };
        match post_to_discord(url, &row.event_type, &row.payload_json.0).await {
            Ok(()) => {
                sqlx::query("UPDATE discord_notifications_queue SET sent_at = NOW() WHERE id = $1")
                    .bind(row.id)
                    .execute(db)
                    .await?;
                sent += 1;
            }
            Err(e) => {
                sqlx::query(
                    r#"
                    UPDATE discord_notifications_queue
                       SET failed_count = failed_count + 1, last_error = $2
                     WHERE id = $1
                    "#,
                )
                .bind(row.id)
                .bind(e.to_string())
                .execute(db)
                .await?;
                tracing::warn!(id = %row.id, error = %e, "discord post failed");
            }
        }
    }
    Ok(sent)
}

/// The event types this binary can route, and the only ones it may mark sent.
///
/// Anything else is a later addition — migration 0257's contest and mission
/// announcements — and belongs to `skilluv-discord-bot`, which routes on
/// `target_channel_id` rather than on two hard-coded webhooks. Both binaries
/// poll the same queue, so this list is what keeps the v1 fallback from
/// consuming rows only v2 can deliver.
const HANDLED_EVENTS: &[&str] = &[
    "rank_promotion",
    "badge_earned",
    "attestation_new",
    "slice_validated",
];

async fn post_to_discord(webhook_url: &str, event_type: &str, payload: &Value) -> Result<()> {
    let content = render_message(event_type, payload);
    let body = json!({ "content": content });
    let resp = reqwest::Client::new()
        .post(webhook_url)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .context("http POST failed")?;
    if !resp.status().is_success() {
        anyhow::bail!("discord returned {}", resp.status());
    }
    Ok(())
}

/// Format a queue row into the message text posted to Discord. Pure fn —
/// unit tests below cover each event type.
fn render_message(event_type: &str, payload: &Value) -> String {
    match event_type {
        "rank_promotion" => {
            let username = payload["username"].as_str().unwrap_or("someone");
            let rank = payload["new_rank"].as_str().unwrap_or("");
            format!("**{username}** just reached rank **{rank}** on Skilluv.")
        }
        "badge_earned" => {
            let username = payload["username"].as_str().unwrap_or("someone");
            let badge = payload["badge_name"].as_str().unwrap_or("a new badge");
            format!("**{username}** earned the **{badge}** badge.")
        }
        "attestation_new" => {
            let username = payload["username"].as_str().unwrap_or("someone");
            let title = payload["challenge_title"].as_str().unwrap_or("a challenge");
            let hash = payload["attestation_hash"].as_str().unwrap_or("");
            format!(
                "**{username}** just validated **{title}** — verify: https://skill-uv.com/verify/{hash}"
            )
        }
        "slice_validated" => {
            let username = payload["username"].as_str().unwrap_or("someone");
            let repo = payload["repo"].as_str().unwrap_or("a repo");
            format!("**{username}** shipped a validated PR on **{repo}**.")
        }
        _ => format!("Skilluv event: {event_type}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_rank_promotion() {
        let msg = render_message(
            "rank_promotion",
            &json!({"username": "amina", "new_rank": "artisan"}),
        );
        assert!(msg.contains("amina"));
        assert!(msg.contains("artisan"));
    }

    #[test]
    fn render_attestation_includes_verify_url() {
        let msg = render_message(
            "attestation_new",
            &json!({
                "username": "amina",
                "challenge_title": "Refactor form validation",
                "attestation_hash": "abc123"
            }),
        );
        assert!(msg.contains("verify/abc123"));
    }

    #[test]
    fn render_unknown_event_fallback() {
        let msg = render_message("mystery", &json!({}));
        assert!(msg.contains("mystery"));
    }
}
