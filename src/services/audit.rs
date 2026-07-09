//! Generic audit log (Phase 1.18).
//!
//! Use from any handler. Best-effort: failures are logged but never propagated to the user.

use axum::http::HeaderMap;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::middleware::extract_ip;

#[derive(Debug, Clone, Copy)]
pub enum ActorType {
    User,
    Admin,
    System,
    Enterprise,
    Anonymous,
}

impl ActorType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Admin => "admin",
            Self::System => "system",
            Self::Enterprise => "enterprise",
            Self::Anonymous => "anonymous",
        }
    }
}

pub struct AuditEntry<'a> {
    pub actor_type: ActorType,
    pub actor_id: Option<Uuid>,
    pub action: &'a str,
    pub target_type: Option<&'a str>,
    pub target_id: Option<Uuid>,
    pub metadata: Option<Value>,
    pub headers: Option<&'a HeaderMap>,
}

pub async fn record(db: &PgPool, entry: AuditEntry<'_>) {
    let (ip, user_agent) = match entry.headers {
        Some(h) => (
            Some(extract_ip(h)),
            h.get("user-agent")
                .and_then(|v| v.to_str().ok())
                .map(String::from),
        ),
        None => (None, None),
    };

    let result = sqlx::query(
        r#"
        INSERT INTO audit_log (actor_type, actor_id, action, target_type, target_id, metadata, ip, user_agent)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
    )
    .bind(entry.actor_type.as_str())
    .bind(entry.actor_id)
    .bind(entry.action)
    .bind(entry.target_type)
    .bind(entry.target_id)
    .bind(entry.metadata)
    .bind(ip)
    .bind(user_agent)
    .execute(db)
    .await;

    if let Err(err) = result {
        tracing::warn!(error = %err, action = entry.action, "audit log insert failed");
    }
}
