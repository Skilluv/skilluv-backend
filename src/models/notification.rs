use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Notification {
    pub id: Uuid,
    pub user_id: Uuid,
    /// Legacy name, kept because the front reads it. Equal to `kind` for
    /// anything written since the catalogue existed.
    pub notification_type: String,
    /// Dotted catalogue identifier — `social.mention`, `payout.sent`. What
    /// a client switches on to pick an icon or a destination. `None` only
    /// on rows written before the catalogue.
    pub kind: Option<String>,
    pub title: String,
    pub body: Option<String>,
    pub data: Option<serde_json::Value>,
    pub read: bool,

    /// How many events this row stands for. Always at least one; more than
    /// one means several things happened in the same context and were
    /// folded into this line rather than filling the list.
    pub group_count: i32,
    /// The most recent distinct people involved, newest first, capped at
    /// four. Rendered as "Fatou and 2 others" — the count comes from
    /// `group_count`, not from the length of this list, because only the
    /// first few names are kept.
    pub group_actors: serde_json::Value,

    pub created_at: DateTime<Utc>,
    /// When this row last absorbed an event. Lists order by this rather
    /// than `created_at`, so a conversation that just moved comes back to
    /// the top instead of staying where it started.
    pub updated_at: Option<DateTime<Utc>>,
}
