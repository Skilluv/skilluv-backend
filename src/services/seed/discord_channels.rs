//! The Discord routing table, restored from the deployment's own configuration.
//!
//! ## The problem this solves
//!
//! `discord_channels` decides which room each announcement lands in. Migration
//! 0257 refuses to seed it, and is right to: every value is a snowflake from
//! one specific server, so a migration carrying them would post this platform's
//! test suite into somebody's real Discord.
//!
//! But that left the rows living only in the database, applied by hand. Drop
//! the database and the routing is gone — every announcement then falls back to
//! the default room, silently, because a missing row is not an error.
//!
//! ## Where they live instead
//!
//! In `SKILLUV_DISCORD_CHANNELS`, next to `DISCORD_BOT_TOKEN` on the
//! deployment. Environment survives a dropped database, which is exactly the
//! property that was missing, and it keeps the snowflakes out of the repository
//! where 0257 did not want them.
//!
//! `scripts/discord-setup.py --env` prints the value, read off the live server.
//!
//! ## The shape
//!
//! A JSON array. `domain` is null for the room an announcement falls back to
//! when its own domain has none:
//!
//! ```json
//! [{"purpose":"general","domain":null,"channel_id":"123"},
//!  {"purpose":"contests","domain":"design","channel_id":"456"}]
//! ```
//!
//! Unset is not an error. A deployment with no Discord server is a deployment
//! with nothing to route, and every other seed step still runs.

use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

pub const ENV_VAR: &str = "SKILLUV_DISCORD_CHANNELS";

#[derive(serde::Deserialize)]
struct Row {
    purpose: String,
    /// Absent or null: the room every domain falls back to.
    #[serde(default)]
    domain: Option<String>,
    channel_id: String,
    #[serde(default)]
    label: Option<String>,
}

/// What the routing looks like right now, for the ledger's version.
///
/// The value itself rather than a hand-maintained number: change the variable
/// and the step re-applies on the next boot, which is the whole point of
/// keeping it in configuration.
pub fn declared() -> Option<String> {
    std::env::var(ENV_VAR).ok().filter(|v| !v.trim().is_empty())
}

/// Apply the declared routing. Idempotent by upsert, like every other step.
pub async fn run(db: &PgPool, _owner: Uuid) -> Result<String, AppError> {
    let Some(raw) = declared() else {
        return Ok(format!("{ENV_VAR} not set; no Discord routing applied"));
    };

    let rows: Vec<Row> = serde_json::from_str(&raw).map_err(|e| {
        // Named rather than swallowed: a malformed variable means every
        // announcement quietly goes to the default room, which is the failure
        // this module exists to end.
        AppError::Internal(format!(
            "{ENV_VAR} is not the JSON array this expects ({e}). \
             Regenerate it with `scripts/discord-setup.py --env`."
        ))
    })?;

    let mut applied = 0usize;
    for row in &rows {
        // ON CONFLICT names the expression migration 0440 indexed, not a
        // column pair: the primary key went when the empty-string sentinel
        // did, and uniqueness moved to COALESCE(skill_domain, '').
        sqlx::query(
            "INSERT INTO discord_channels (purpose, skill_domain, channel_id, label)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (purpose, COALESCE(skill_domain, '')) DO UPDATE
                 SET channel_id = EXCLUDED.channel_id,
                     label      = EXCLUDED.label,
                     updated_at = NOW()",
        )
        .bind(&row.purpose)
        .bind(row.domain.as_deref())
        .bind(&row.channel_id)
        .bind(row.label.as_deref())
        .execute(db)
        .await?;
        applied += 1;
    }

    Ok(format!("{applied} rooms routed"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_may_omit_its_domain_and_its_label() {
        // The fallback room is the one most likely to be written by hand.
        let rows: Vec<Row> =
            serde_json::from_str(r#"[{"purpose":"general","channel_id":"123"}]"#).unwrap();
        assert_eq!(rows.len(), 1);
        assert!(rows[0].domain.is_none());
        assert!(rows[0].label.is_none());
    }

    #[test]
    fn an_explicit_null_domain_is_the_fallback_too() {
        // `r##` rather than `r#`: the label contains a `#`, and `"#` would
        // close a single-hash raw string in the middle of the JSON.
        let rows: Vec<Row> = serde_json::from_str(
            r##"[{"purpose":"general","domain":null,"channel_id":"1","label":"#annonces"}]"##,
        )
        .unwrap();
        assert!(rows[0].domain.is_none());
        assert_eq!(rows[0].label.as_deref(), Some("#annonces"));
    }
}
