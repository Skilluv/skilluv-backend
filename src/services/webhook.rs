use hmac::{Hmac, Mac};
use sha2::Sha256;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, sqlx::FromRow)]
struct WebhookRow {
    id: Uuid,
    url: String,
    secret: String,
    events: Vec<String>,
    #[allow(dead_code)]
    active: bool,
}

pub struct WebhookService;

impl WebhookService {
    /// Trigger webhooks for a given event. Finds all active webhooks subscribed to this event
    /// and delivers the payload asynchronously.
    pub async fn trigger(db: &PgPool, event: &str, payload: serde_json::Value) {
        let webhooks: Vec<WebhookRow> = sqlx::query_as(
            "SELECT id, url, secret, events, active FROM webhooks WHERE active = TRUE AND $1 = ANY(events)",
        )
        .bind(event)
        .fetch_all(db)
        .await
        .unwrap_or_default();

        for webhook in webhooks {
            let db = db.clone();
            let event = event.to_string();
            let payload = payload.clone();

            tokio::spawn(async move {
                Self::deliver(
                    &db,
                    webhook.id,
                    &webhook.url,
                    &webhook.secret,
                    &event,
                    &payload,
                )
                .await;
            });
        }
    }

    /// Deliver a webhook payload to a URL with HMAC signature.
    async fn deliver(
        db: &PgPool,
        webhook_id: Uuid,
        url: &str,
        secret: &str,
        event: &str,
        payload: &serde_json::Value,
    ) {
        let body = serde_json::json!({
            "event": event,
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "data": payload,
        });

        let body_str = serde_json::to_string(&body).unwrap_or_default();
        let signature = Self::sign(secret, &body_str);

        let client = reqwest::Client::new();
        let result = client
            .post(url)
            .header("Content-Type", "application/json")
            .header("X-Skilluv-Signature", format!("sha256={signature}"))
            .header("X-Skilluv-Event", event)
            .body(body_str.clone())
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await;

        let (status, response_body, success) = match result {
            Ok(resp) => {
                let status = resp.status().as_u16() as i32;
                let body = resp.text().await.unwrap_or_default();
                let ok = (200..300).contains(&(status as u16 as usize));
                (Some(status), Some(body), ok)
            }
            Err(e) => (None, Some(format!("Error: {e}")), false),
        };

        // Log delivery
        let _ = sqlx::query(
            "INSERT INTO webhook_deliveries (webhook_id, event, payload, response_status, response_body, success) VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(webhook_id)
        .bind(event)
        .bind(&body)
        .bind(status)
        .bind(&response_body)
        .bind(success)
        .execute(db)
        .await;

        // Update webhook stats
        if success {
            let _ = sqlx::query(
                "UPDATE webhooks SET last_triggered_at = NOW(), failure_count = 0 WHERE id = $1",
            )
            .bind(webhook_id)
            .execute(db)
            .await;
        } else {
            let _ =
                sqlx::query("UPDATE webhooks SET failure_count = failure_count + 1 WHERE id = $1")
                    .bind(webhook_id)
                    .execute(db)
                    .await;

            // Disable after 10 consecutive failures
            let _ = sqlx::query(
                "UPDATE webhooks SET active = FALSE WHERE id = $1 AND failure_count >= 10",
            )
            .bind(webhook_id)
            .execute(db)
            .await;
        }
    }

    /// HMAC-SHA256 signature.
    pub fn sign(secret: &str, body: &str) -> String {
        let mut mac =
            HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC can take key of any size");
        mac.update(body.as_bytes());
        hex::encode(mac.finalize().into_bytes())
    }

    /// Send a test event to a webhook.
    pub async fn send_test(db: &PgPool, webhook_id: Uuid) -> Result<bool, AppError> {
        let webhook: WebhookRow =
            sqlx::query_as("SELECT id, url, secret, events, active FROM webhooks WHERE id = $1")
                .bind(webhook_id)
                .fetch_optional(db)
                .await?
                .ok_or(AppError::NotFound("Webhook not found".to_string()))?;

        let payload = serde_json::json!({
            "test": true,
            "message": "This is a test event from Skilluv"
        });

        let event = webhook.events.first().map(|s| s.as_str()).unwrap_or("test");

        Self::deliver(
            db,
            webhook.id,
            &webhook.url,
            &webhook.secret,
            event,
            &payload,
        )
        .await;

        Ok(true)
    }
}
