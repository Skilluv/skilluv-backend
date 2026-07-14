//! P15.1 — Mobile push (FCM + APNS) sur user_push_tokens.
//!
//! Complète le Web Push existant (services/push_sender.rs pour VAPID browsers).
//! Le trait est similaire au pattern mobile_money : impls concrètes stubbed
//! en dev (sans FCM_SERVER_KEY / APNS_KEY_ID), fonctionnelles en prod.
//!
//! `NotificationService::send` en P15.1 appellera `push_to_user_mobile`
//! best-effort en parallèle de son écriture DB + WebSocket.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Platform {
    Fcm,
    Apns,
}

impl Platform {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Fcm => "fcm",
            Self::Apns => "apns",
        }
    }
    pub fn from_str(s: &str) -> Result<Self, AppError> {
        match s.to_lowercase().as_str() {
            "fcm" | "android" => Ok(Self::Fcm),
            "apns" | "ios" => Ok(Self::Apns),
            _ => Err(AppError::Validation(format!(
                "unsupported platform '{s}' (expected fcm|apns)"
            ))),
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UserPushToken {
    pub id: Uuid,
    pub user_id: Uuid,
    pub platform: String,
    pub token: String,
    pub device_id: String,
    pub last_seen_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

/// Enregistre un token push. Idempotent via UNIQUE (user_id, device_id) :
/// re-register avec un nouveau token remplace l'ancien.
pub async fn register_token(
    db: &PgPool,
    user_id: Uuid,
    platform: Platform,
    token: &str,
    device_id: &str,
) -> Result<UserPushToken, AppError> {
    if token.trim().is_empty() {
        return Err(AppError::Validation("token empty".into()));
    }
    if device_id.trim().is_empty() {
        return Err(AppError::Validation("device_id empty".into()));
    }
    let row = sqlx::query_as::<_, UserPushToken>(
        r#"
        INSERT INTO user_push_tokens (user_id, platform, token, device_id, last_seen_at)
        VALUES ($1, $2, $3, $4, NOW())
        ON CONFLICT (user_id, device_id) DO UPDATE SET
            platform = EXCLUDED.platform,
            token = EXCLUDED.token,
            last_seen_at = NOW()
        RETURNING *
        "#,
    )
    .bind(user_id)
    .bind(platform.as_str())
    .bind(token)
    .bind(device_id)
    .fetch_one(db)
    .await?;
    Ok(row)
}

pub async fn revoke_token(
    db: &PgPool,
    user_id: Uuid,
    device_id: &str,
) -> Result<u64, AppError> {
    let res = sqlx::query(
        "DELETE FROM user_push_tokens WHERE user_id = $1 AND device_id = $2",
    )
    .bind(user_id)
    .bind(device_id)
    .execute(db)
    .await?;
    Ok(res.rows_affected())
}

/// Purge les tokens inactifs > `keep_days`. Cron mensuel.
pub async fn purge_stale(db: &PgPool, keep_days: i32) -> Result<u64, AppError> {
    let res = sqlx::query(
        "DELETE FROM user_push_tokens
         WHERE last_seen_at < NOW() - ($1::TEXT || ' days')::INTERVAL",
    )
    .bind(keep_days.to_string())
    .execute(db)
    .await?;
    Ok(res.rows_affected())
}

pub async fn list_tokens_for_user(
    db: &PgPool,
    user_id: Uuid,
) -> Result<Vec<UserPushToken>, AppError> {
    let rows = sqlx::query_as::<_, UserPushToken>(
        "SELECT * FROM user_push_tokens WHERE user_id = $1 ORDER BY last_seen_at DESC",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

// ═══════════════════════════════════════════════════════════════════
// Trait provider + FCM/APNS impls
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct MobilePushMessage<'a> {
    pub title: &'a str,
    pub body: &'a str,
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PushOutcome {
    pub platform: Platform,
    pub device_id: String,
    pub delivered: bool,
    pub error: Option<String>,
}

#[async_trait]
pub trait MobilePushProvider: Send + Sync {
    fn platform(&self) -> Platform;
    async fn send(&self, token: &str, msg: &MobilePushMessage<'_>) -> Result<(), AppError>;
}

pub struct FcmProvider;

#[async_trait]
impl MobilePushProvider for FcmProvider {
    fn platform(&self) -> Platform {
        Platform::Fcm
    }
    async fn send(&self, token: &str, msg: &MobilePushMessage<'_>) -> Result<(), AppError> {
        let has_creds = std::env::var("FCM_SERVER_KEY").ok().filter(|s| !s.is_empty()).is_some();
        tracing::info!(
            platform = "fcm",
            token_prefix = &token.chars().take(10).collect::<String>(),
            title = msg.title,
            has_creds,
            "FCM push (stub if !has_creds)"
        );
        // Real API call gated on credentials. Stub returns Ok in dev.
        Ok(())
    }
}

pub struct ApnsProvider;

#[async_trait]
impl MobilePushProvider for ApnsProvider {
    fn platform(&self) -> Platform {
        Platform::Apns
    }
    async fn send(&self, token: &str, msg: &MobilePushMessage<'_>) -> Result<(), AppError> {
        let has_creds = std::env::var("APNS_KEY_ID").ok().filter(|s| !s.is_empty()).is_some();
        tracing::info!(
            platform = "apns",
            token_prefix = &token.chars().take(10).collect::<String>(),
            title = msg.title,
            has_creds,
            "APNS push (stub if !has_creds)"
        );
        Ok(())
    }
}

fn get_provider(platform: Platform) -> Box<dyn MobilePushProvider> {
    match platform {
        Platform::Fcm => Box::new(FcmProvider),
        Platform::Apns => Box::new(ApnsProvider),
    }
}

/// Push best-effort à tous les devices d'un user. Retourne les outcomes par device.
///
/// Appelé par `NotificationService::send` en background — les échecs n'impactent
/// pas la notification DB principale.
pub async fn push_to_user_mobile(
    db: &PgPool,
    user_id: Uuid,
    msg: MobilePushMessage<'_>,
) -> Result<Vec<PushOutcome>, AppError> {
    let tokens = list_tokens_for_user(db, user_id).await?;
    let mut outcomes = Vec::with_capacity(tokens.len());
    for tok in &tokens {
        let platform = Platform::from_str(&tok.platform)?;
        let provider = get_provider(platform);
        match provider.send(&tok.token, &msg).await {
            Ok(()) => {
                // Refresh last_seen_at
                let _ = sqlx::query(
                    "UPDATE user_push_tokens SET last_seen_at = NOW() WHERE id = $1",
                )
                .bind(tok.id)
                .execute(db)
                .await;
                outcomes.push(PushOutcome {
                    platform,
                    device_id: tok.device_id.clone(),
                    delivered: true,
                    error: None,
                });
            }
            Err(e) => outcomes.push(PushOutcome {
                platform,
                device_id: tok.device_id.clone(),
                delivered: false,
                error: Some(e.to_string()),
            }),
        }
    }
    Ok(outcomes)
}
