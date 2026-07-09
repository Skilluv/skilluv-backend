use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Notification {
    pub id: Uuid,
    pub user_id: Uuid,
    pub notification_type: String,
    pub title: String,
    pub body: Option<String>,
    pub data: Option<serde_json::Value>,
    pub read: bool,
    pub created_at: DateTime<Utc>,
}
