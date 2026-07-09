//! Product analytics via PostHog (self-hosted or cloud).
//!
//! Fire-and-forget: each `track` call spawns a tokio task that POSTs to PostHog.
//! If PostHog is unreachable or unconfigured, the call is a no-op — never blocks the
//! request and never propagates errors to the user.

use std::sync::Arc;
use std::time::Duration;

use reqwest::Client;
use serde::Serialize;
use serde_json::Value;
use tracing::{debug, warn};
use uuid::Uuid;

#[derive(Clone)]
pub struct AnalyticsService {
    inner: Arc<Inner>,
}

struct Inner {
    api_key: Option<String>,
    host: String,
    client: Client,
}

impl AnalyticsService {
    /// Build a service from env. If `POSTHOG_API_KEY` is unset, returns a disabled
    /// service that silently no-ops on every call.
    pub fn from_env() -> Self {
        let api_key = std::env::var("POSTHOG_API_KEY")
            .ok()
            .filter(|s| !s.is_empty());
        let host = std::env::var("POSTHOG_HOST").unwrap_or_else(|_| "https://eu.posthog.com".into());
        let client = Client::builder()
            .timeout(Duration::from_secs(3))
            .build()
            .expect("reqwest client");
        Self {
            inner: Arc::new(Inner {
                api_key,
                host,
                client,
            }),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.inner.api_key.is_some()
    }

    /// Track an event for an authenticated user (distinct_id = user UUID).
    pub fn track(&self, user_id: Uuid, event: &str, properties: Value) {
        self.dispatch(event, &user_id.to_string(), properties);
    }

    /// Track an event for an anonymous visitor (distinct_id = a session-stable opaque id).
    pub fn track_anonymous(&self, distinct_id: &str, event: &str, properties: Value) {
        self.dispatch(event, distinct_id, properties);
    }

    fn dispatch(&self, event: &str, distinct_id: &str, properties: Value) {
        let Some(api_key) = self.inner.api_key.clone() else {
            return; // disabled
        };
        let payload = CapturePayload {
            api_key,
            event: event.to_string(),
            distinct_id: distinct_id.to_string(),
            properties,
            timestamp: chrono::Utc::now().to_rfc3339(),
        };
        let url = format!("{}/capture/", self.inner.host);
        let client = self.inner.client.clone();
        let event_name = event.to_string();
        tokio::spawn(async move {
            match client.post(&url).json(&payload).send().await {
                Ok(resp) if resp.status().is_success() => {
                    debug!(event = %event_name, "posthog event sent");
                }
                Ok(resp) => {
                    warn!(
                        event = %event_name,
                        status = resp.status().as_u16(),
                        "posthog rejected event"
                    );
                }
                Err(err) => {
                    warn!(event = %event_name, error = %err, "posthog send failed");
                }
            }
        });
    }
}

#[derive(Serialize)]
struct CapturePayload {
    api_key: String,
    event: String,
    distinct_id: String,
    properties: Value,
    timestamp: String,
}

/// Centralised list of event names — keep in sync with PostHog dashboards.
/// Adding a new event ? Add it here first so all sites use the same string.
pub mod events {
    pub const USER_SIGNUP: &str = "user_signup";
    pub const USER_LOGIN: &str = "user_login";
    pub const EMAIL_VERIFIED: &str = "email_verified";
    pub const PROFILE_ACTIVATED: &str = "profile_activated";
    pub const CHALLENGE_STARTED: &str = "challenge_started";
    pub const CHALLENGE_COMPLETED: &str = "challenge_completed";
    pub const STREAK_MILESTONE: &str = "streak_milestone_reached";
    pub const TITLE_CHANGED: &str = "title_changed";
    pub const BADGE_EARNED: &str = "badge_earned";
    pub const ENTERPRISE_SIGNUP: &str = "enterprise_signup";
    pub const TALENT_SEARCHED: &str = "talent_searched";
    pub const INTEREST_REQUEST_SENT: &str = "interest_request_sent";
    pub const INTEREST_REQUEST_ACCEPTED: &str = "interest_request_accepted";
    pub const INTEREST_REQUEST_REJECTED: &str = "interest_request_rejected";
    pub const ACCOUNT_DELETED: &str = "account_deleted";
    // Phase 2 Sprint 1 — social primitives
    pub const COMMENT_POSTED: &str = "comment_posted";
    pub const REACTION_ADDED: &str = "reaction_added";
    pub const MENTION_RECEIVED: &str = "mention_received";
    // Phase 2 Sprint 2 — DM + feed + notifs
    pub const DM_SENT: &str = "dm_sent";
    pub const DM_CONVERSATION_OPENED: &str = "dm_conversation_opened";
    pub const FEED_VIEWED: &str = "feed_viewed";
    pub const NOTIFICATION_CLICKED: &str = "notification_clicked";
    pub const USER_BLOCKED: &str = "user_blocked";
    // Phase 2 Sprint 4 — guilds
    pub const GUILD_CREATED: &str = "guild_created";
    pub const GUILD_JOINED: &str = "guild_joined";
    pub const GUILD_LEFT: &str = "guild_left";
    pub const GUILD_INVITE_SENT: &str = "guild_invite_sent";
    pub const GUILD_APPLICATION_SUBMITTED: &str = "guild_application_submitted";
    pub const GUILD_APPLICATION_DECIDED: &str = "guild_application_decided";
    pub const GUILD_MEMBER_PROMOTED: &str = "guild_member_promoted";
    pub const GUILD_WAR_PROPOSED: &str = "guild_war_proposed";
    pub const GUILD_WAR_ACCEPTED: &str = "guild_war_accepted";
    pub const GUILD_WAR_CONCLUDED: &str = "guild_war_concluded";
    // Phase 2 Sprint 5 — GitHub + projects
    pub const GITHUB_CONNECTED: &str = "github_connected";
    pub const GITHUB_SYNC_TRIGGERED: &str = "github_sync_triggered";
    pub const PROJECT_CREATED: &str = "project_created";
    pub const PROJECT_CONTRIBUTION_SYNCED: &str = "project_contribution_synced";
    pub const CV_VIEWED: &str = "cv_viewed";
}

/// Helper to build a JSON properties object more ergonomically.
pub fn props(pairs: &[(&str, Value)]) -> Value {
    let map: serde_json::Map<String, Value> =
        pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect();
    Value::Object(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn disabled_when_api_key_missing() {
        unsafe {
            std::env::remove_var("POSTHOG_API_KEY");
        }
        let svc = AnalyticsService::from_env();
        assert!(!svc.is_enabled());
        // Should not panic / not block
        svc.track(Uuid::new_v4(), events::USER_LOGIN, json!({}));
    }

    #[test]
    fn props_helper() {
        let p = props(&[("a", json!(1)), ("b", json!("x"))]);
        assert_eq!(p["a"], json!(1));
        assert_eq!(p["b"], json!("x"));
    }
}
