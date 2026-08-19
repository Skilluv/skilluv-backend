//! Announcing something to a room.
//!
//! ## Why this is not part of `notify`
//!
//! `notify` delivers to a person, across three channels each of which they
//! can switch off, and it reads a catalogue of per-kind defaults to decide
//! what to try. None of that applies here: a Discord post goes into a public
//! room, there is no recipient to hold a preference, and a preference toggle
//! would let one person silence a channel for everybody.
//!
//! ## What was there before
//!
//! `discord_notifications_queue` has existed since migration 0135 with a
//! consumer that polls it every fifteen seconds. It had no producer at all —
//! no trigger, no call site. This is that producer.
//!
//! ## Never fatal
//!
//! Every function here returns `()`. A Discord announcement is the least
//! important thing happening at any of its call sites: a contest is still
//! concluded, a featuring still awarded and a mission still published if the
//! queue insert fails. Failing the caller for it would trade a real outcome
//! for a chat message.

use serde_json::Value;
use sqlx::PgPool;

/// What kind of room an announcement belongs in.
///
/// Named for the job, not for the channel: `#design-contests` is Discord's
/// name to change, and the routing has to survive somebody renaming it on a
/// Tuesday.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    Contests,
    Winners,
    Missions,
    General,
    Promotions,
}

impl Purpose {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Contests => "contests",
            Self::Winners => "winners",
            Self::Missions => "missions",
            Self::General => "general",
            Self::Promotions => "promotions",
        }
    }
}

/// Which room, for this purpose and this domain.
///
/// The domain's own room first, then the domain-blind one. A server with a
/// single `#contests` channel configures one row; a server that splits them
/// per domain configures seven, and both work without a code change.
///
/// `None` when neither exists. The announcement is still enqueued, and the
/// consumer posts it in its default channel — a message in the wrong room is
/// recoverable, a message nobody sent is not.
pub async fn resolve_channel(db: &PgPool, purpose: Purpose, domain: Option<&str>) -> Option<String> {
    sqlx::query_scalar(
        r#"
        SELECT channel_id FROM discord_channels
         WHERE purpose = $1
           AND skill_domain IN (COALESCE($2, ''), '')
         ORDER BY (skill_domain <> '') DESC
         LIMIT 1
        "#,
    )
    .bind(purpose.as_str())
    .bind(domain)
    .fetch_optional(db)
    .await
    .unwrap_or_default()
}

/// Put one announcement in the queue.
///
/// The channel is resolved now rather than at post time: which room this
/// belonged in is a fact about the moment it happened, and re-resolving later
/// would move a backlog into whatever the channel has since been repurposed
/// for.
pub async fn announce(
    db: &PgPool,
    event_type: &str,
    purpose: Purpose,
    domain: Option<&str>,
    payload: Value,
) {
    let channel = resolve_channel(db, purpose, domain).await;

    let result = sqlx::query(
        "INSERT INTO discord_notifications_queue
             (event_type, payload_json, target_channel_id, skill_domain)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(event_type)
    .bind(&payload)
    .bind(channel.as_deref())
    .bind(domain)
    .execute(db)
    .await;

    if let Err(e) = result {
        // Logged, never raised. See the module note.
        tracing::warn!(event_type, error = %e, "discord announcement not enqueued");
    }
}

/// A contest opened for entries.
pub async fn contest_opened(
    db: &PgPool,
    domain: &str,
    slug: &str,
    title: &str,
    ends_at: chrono::DateTime<chrono::Utc>,
    prize: Option<String>,
) {
    announce(
        db,
        "contest_opened",
        Purpose::Contests,
        Some(domain),
        serde_json::json!({
            "slug": slug,
            "title": title,
            "skill_domain": domain,
            "ends_at": ends_at.to_rfc3339(),
            // Absent rather than zero when there is no cash prize. A contest
            // announced as "0 €" reads as a mistake; one announced without a
            // figure reads as what it is.
            "prize": prize,
        }),
    )
    .await;
}

/// A contest concluded, and somebody won it.
pub async fn contest_won(db: &PgPool, domain: &str, slug: &str, title: &str, winner: &str) {
    announce(
        db,
        "contest_won",
        Purpose::Winners,
        Some(domain),
        serde_json::json!({
            "slug": slug,
            "title": title,
            "skill_domain": domain,
            "username": winner,
        }),
    )
    .await;
}

/// The week's editorial featuring.
pub async fn talent_featured(
    db: &PgPool,
    domain: &str,
    username: &str,
    week_of: chrono::NaiveDate,
) {
    announce(
        db,
        "talent_featured",
        Purpose::General,
        Some(domain),
        serde_json::json!({
            "username": username,
            "skill_domain": domain,
            "week_of": week_of.to_string(),
        }),
    )
    .await;
}

/// An enterprise published a paid mission.
pub async fn mission_posted(db: &PgPool, domain: &str, slug: &str, title: &str) {
    announce(
        db,
        "mission_posted",
        Purpose::Missions,
        Some(domain),
        serde_json::json!({
            "slug": slug,
            "title": title,
            "skill_domain": domain,
        }),
    )
    .await;
}

/// What the consumer posts, for one queued row.
///
/// Rendering lives here rather than in the bot binary so it can be tested
/// without a Discord connection. The bot calls it.
pub fn render(event_type: &str, payload: &Value, frontend: &str) -> String {
    let s = |key: &str| payload[key].as_str().unwrap_or_default().to_string();

    match event_type {
        "rank_promotion" => {
            let username = payload["username"].as_str().unwrap_or("quelqu'un");
            let rank = payload["new_rank"].as_str().unwrap_or("");
            format!("**{username}** vient d'atteindre le rang **{rank}**.")
        }
        "badge_earned" => {
            let username = payload["username"].as_str().unwrap_or("quelqu'un");
            let badge = payload["badge_name"].as_str().unwrap_or("un badge");
            format!("**{username}** a obtenu le badge **{badge}**.")
        }
        "attestation_new" => {
            let username = payload["username"].as_str().unwrap_or("quelqu'un");
            let title = payload["challenge_title"].as_str().unwrap_or("un défi");
            let hash = s("attestation_hash");
            format!("**{username}** a validé **{title}** — vérifier : {frontend}/verify/{hash}")
        }
        "slice_validated" => {
            let username = payload["username"].as_str().unwrap_or("quelqu'un");
            let repo = payload["repo"].as_str().unwrap_or("un dépôt");
            format!("**{username}** a livré une contribution validée sur **{repo}**.")
        }

        "contest_opened" => {
            let title = s("title");
            let slug = s("slug");
            // The prize is only mentioned when there is one.
            let prize = match payload["prize"].as_str() {
                Some(p) if !p.is_empty() => format!(" — {p}"),
                _ => String::new(),
            };
            let deadline = payload["ends_at"]
                .as_str()
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|d| format!(" — jusqu'au {}", d.format("%d/%m/%Y")))
                .unwrap_or_default();
            format!("Nouveau concours : **{title}**{prize}{deadline}\n{frontend}/contests/{slug}")
        }
        "contest_won" => {
            let username = s("username");
            let title = s("title");
            let slug = s("slug");
            format!("**{username}** remporte **{title}** — {frontend}/contests/{slug}")
        }
        "talent_featured" => {
            let username = s("username");
            let domain = s("skill_domain");
            format!(
                "Cette semaine, mis en avant en **{domain}** : **{username}** — \
                 {frontend}/@{username}"
            )
        }
        "mission_posted" => {
            let title = s("title");
            let slug = s("slug");
            format!("Nouvelle mission rémunérée : **{title}** — {frontend}/missions/{slug}")
        }

        // An event type the bot does not know how to render is still posted,
        // named. Silence would make a producer look broken when the consumer
        // is the one that is behind.
        other => format!("Événement Skilluv : {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_contest_without_a_prize_does_not_announce_one() {
        let with = render(
            "contest_opened",
            &json!({"title": "Identité", "slug": "identite", "prize": "500 €"}),
            "https://skill-uv.com",
        );
        assert!(with.contains("500 €"), "{with}");

        // Not "0 €", and not an empty dash. A contest with no cash prize is a
        // contest, and saying so badly makes it look like a mistake.
        let without = render(
            "contest_opened",
            &json!({"title": "Identité", "slug": "identite", "prize": null}),
            "https://skill-uv.com",
        );
        assert!(!without.contains('€'), "{without}");
        assert!(without.contains("Identité"));
    }

    #[test]
    fn a_missing_field_does_not_print_the_word_null() {
        // Payloads are written by four call sites and read by one renderer.
        // A missing key has to degrade to nothing, not to a literal.
        let out = render("contest_won", &json!({}), "https://skill-uv.com");
        assert!(!out.contains("null"), "{out}");
        assert!(!out.to_lowercase().contains("none"), "{out}");
    }

    #[test]
    fn an_unknown_event_is_named_rather_than_swallowed() {
        let out = render("something_new", &json!({}), "https://skill-uv.com");
        assert!(out.contains("something_new"), "{out}");
    }

    #[test]
    fn a_deadline_is_a_date_and_not_a_timestamp() {
        let out = render(
            "contest_opened",
            &json!({
                "title": "Identité",
                "slug": "identite",
                "ends_at": "2026-09-30T23:59:00+00:00"
            }),
            "https://skill-uv.com",
        );
        assert!(out.contains("30/09/2026"), "{out}");
        assert!(!out.contains("23:59"), "nobody needs the minute: {out}");
    }
}
