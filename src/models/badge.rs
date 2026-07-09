use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Badge {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub category: String,
    pub condition_type: String,
    pub condition_value: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct UserBadge {
    pub user_id: Uuid,
    pub badge_id: Uuid,
    pub earned_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct BadgeWithEarnedAt {
    pub slug: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub category: String,
    pub earned_at: DateTime<Utc>,
}
