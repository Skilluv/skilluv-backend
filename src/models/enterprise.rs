use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Enterprise {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub company_name: String,
    pub slug: String,
    pub description: Option<String>,
    pub website: Option<String>,
    pub logo_url: Option<String>,
    pub industry: Option<String>,
    pub company_size: String,
    pub verified: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct EnterpriseMember {
    pub id: Uuid,
    pub enterprise_id: Uuid,
    pub user_id: Uuid,
    pub role: String,
    pub invited_by: Option<Uuid>,
    pub invited_at: DateTime<Utc>,
    pub accepted_at: Option<DateTime<Utc>>,
    pub status: String,
}

/// Lightweight enterprise info for public display
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct EnterprisePublic {
    pub id: Uuid,
    pub company_name: String,
    pub slug: String,
    pub description: Option<String>,
    pub website: Option<String>,
    pub logo_url: Option<String>,
    pub industry: Option<String>,
    pub company_size: String,
    pub verified: bool,
}
