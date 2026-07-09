use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct InterestRequest {
    pub id: Uuid,
    pub enterprise_id: Uuid,
    pub sender_id: Uuid,
    pub talent_id: Uuid,
    pub status: String,
    pub initial_message: String,
    pub declined_at: Option<DateTime<Utc>>,
    pub accepted_at: Option<DateTime<Utc>>,
    pub cooldown_until: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Conversation {
    pub id: Uuid,
    pub interest_request_id: Uuid,
    pub enterprise_id: Uuid,
    pub talent_id: Uuid,
    pub closed: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Message {
    pub id: Uuid,
    pub conversation_id: Uuid,
    pub sender_id: Uuid,
    pub content: String,
    pub read_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}
