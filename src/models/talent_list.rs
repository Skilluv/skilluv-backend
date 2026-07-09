use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TalentList {
    pub id: Uuid,
    pub enterprise_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct EnterpriseBookmark {
    pub enterprise_id: Uuid,
    pub talent_id: Uuid,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
}
