use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::websocket::{WsManager, WsMessage};

pub struct NotificationService;

/// Payload d'une notification à envoyer via [`NotificationService::send`].
///
/// Regroupe le destinataire + contenu pour rester sous le seuil clippy
/// `too_many_arguments` (les 3 canaux infra db/redis/ws sont passés à côté).
#[derive(Debug, Clone)]
pub struct NotificationPayload<'a> {
    pub user_id: Uuid,
    pub notification_type: &'a str,
    pub title: &'a str,
    pub body: Option<&'a str>,
    pub data: Option<serde_json::Value>,
}

impl NotificationService {
    /// Persist notification to DB, push via WebSocket, increment Redis counter.
    pub async fn send(
        db: &PgPool,
        redis: &mut ConnectionManager,
        ws: &WsManager,
        payload: NotificationPayload<'_>,
    ) -> Result<Uuid, AppError> {
        let NotificationPayload {
            user_id,
            notification_type,
            title,
            body,
            data,
        } = payload;
        let notification_id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO notifications (user_id, notification_type, title, body, data)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id
            "#,
        )
        .bind(user_id)
        .bind(notification_type)
        .bind(title)
        .bind(body)
        .bind(&data)
        .fetch_one(db)
        .await?;

        // Increment Redis unread counter
        let counter_key = format!("notifications:unread:{user_id}");
        let _: i64 = redis.incr(&counter_key, 1).await?;

        // Push via WebSocket
        ws.send_to_user(
            user_id,
            WsMessage {
                event: "notification".to_string(),
                room: None,
                payload: serde_json::json!({
                    "id": notification_id,
                    "type": notification_type,
                    "title": title,
                    "body": body,
                    "data": data,
                }),
            },
        )
        .await;

        // P15.1 : push mobile best-effort (FCM/APNS). Ne fail pas la notif si
        // les push échouent — la ligne DB + WS reste la source de vérité.
        let msg = crate::services::mobile_push::MobilePushMessage {
            title,
            body: body.unwrap_or(""),
            data: data.clone(),
        };
        if let Err(e) = crate::services::mobile_push::push_to_user_mobile(db, user_id, msg).await {
            tracing::debug!(
                error = %e, user_id = %user_id,
                "P15.1 mobile push best-effort failed (notif still persisted)"
            );
        }

        Ok(notification_id)
    }

    /// Get unread count from Redis (fallback to DB if not cached).
    pub async fn unread_count(
        db: &PgPool,
        redis: &mut ConnectionManager,
        user_id: Uuid,
    ) -> Result<i64, AppError> {
        let counter_key = format!("notifications:unread:{user_id}");
        let cached: Option<i64> = redis.get(&counter_key).await?;

        match cached {
            Some(count) => Ok(count),
            None => {
                let count: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM notifications WHERE user_id = $1 AND read = FALSE",
                )
                .bind(user_id)
                .fetch_one(db)
                .await?;

                let () = redis.set(&counter_key, count).await?;
                Ok(count)
            }
        }
    }

    /// Reset the unread counter in Redis (after marking all as read).
    pub async fn reset_counter(
        redis: &mut ConnectionManager,
        user_id: Uuid,
    ) -> Result<(), AppError> {
        let counter_key = format!("notifications:unread:{user_id}");
        let () = redis.set(&counter_key, 0i64).await?;
        Ok(())
    }

    /// Decrement the unread counter by 1 (after marking one as read).
    pub async fn decrement_counter(
        redis: &mut ConnectionManager,
        user_id: Uuid,
    ) -> Result<(), AppError> {
        let counter_key = format!("notifications:unread:{user_id}");
        let current: i64 = redis.get(&counter_key).await.unwrap_or(0);
        if current > 0 {
            let _: i64 = redis.decr(&counter_key, 1).await?;
        }
        Ok(())
    }
}
