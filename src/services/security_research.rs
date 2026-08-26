//! Research tokens: raising the rate limit for somebody who is testing us on
//! purpose.
//!
//! ## What the token buys
//!
//! A multiplier on the rate limit, nothing else. It grants no capability, no
//! extra route, no data. A holder can do exactly what an anonymous visitor can
//! do, more times per hour.
//!
//! That is deliberately unexciting, because the alternative was worse: the
//! published scope invites people to attack the staging deployment, and the
//! limiter refuses them after thirty seconds. A programme that invites research
//! and then blocks it has not published a scope, it has published a decoration.
//!
//! ## Why it is not an exemption
//!
//! Denial of service is out of scope in the policy. A token that removed the
//! limit would make that sentence unenforceable, so the limit is multiplied and
//! not removed: two hundred registrations an hour instead of twenty. Somebody
//! who needs more than that is running a stress test, which is the thing the
//! policy forbids.
//!
//! ## The abuse rule, and why it is a rule rather than a judgement
//!
//! Over five hundred requests a minute under one token revokes it
//! automatically, with `abnormal_volume` recorded. Not because five hundred is
//! a magic number, but because the decision has to be made in the middle of the
//! night by something that is not a person, and a documented threshold that
//! occasionally revokes an enthusiastic fuzzing run is better than an
//! undocumented one that never fires.
//!
//! Revocation is one statement and re-issue is one request, so the cost of a
//! false positive is a minute of somebody's evening.

use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

/// How much the ceiling is multiplied by.
///
/// Ten. A hundred payloads against one form is ordinary manual testing and is
/// refused at the default limits; a thousand a minute is not testing.
pub const RATE_LIMIT_MULTIPLIER: u64 = 10;

/// Requests a minute under one token before it revokes itself.
pub const ABNORMAL_REQUESTS_PER_MINUTE: i64 = 500;

/// How many uses accumulate in Redis before the count is written down.
///
/// The point of the token is to permit a great many requests; a write per
/// request would make the audit trail the bottleneck.
const FLUSH_EVERY: i64 = 25;

/// The prefix, so a token is recognisable in a log or a support message.
const PREFIX: &str = "srt_";

/// A token that has been checked and found live.
#[derive(Debug, Clone, Copy)]
pub struct ResearchMode {
    pub token_id: Uuid,
    pub user_id: Uuid,
}

/// A token as it is shown back to its holder. Never contains the secret except
/// in the one response that issues it.
#[derive(Debug, serde::Serialize)]
pub struct TokenView {
    pub id: Uuid,
    pub token_prefix: String,
    pub label: Option<String>,
    pub issued_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub last_used_at: Option<chrono::DateTime<chrono::Utc>>,
    pub requests_seen: i64,
}

fn hash(plaintext: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(plaintext.as_bytes());
    hex::encode(hasher.finalize())
}

/// A new secret: the prefix, then thirty-two bytes of OS entropy in hex.
fn mint() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("OS RNG");
    format!("{PREFIX}{}", hex::encode(bytes))
}

/// Issue a token, superseding whatever the holder had.
///
/// Superseding rather than refusing: somebody who has lost a token needs a new
/// one, and telling them to revoke the old one first is a step that exists only
/// to make the code simpler. The partial unique index in migration 0548 is what
/// makes "one live token" true, so the old one has to go in the same
/// transaction as the new one arrives.
///
/// Returns the plaintext, which is the only time it exists outside the
/// holder's own notes.
pub async fn issue(
    db: &PgPool,
    user_id: Uuid,
    label: Option<&str>,
    days: i64,
) -> Result<(String, TokenView), AppError> {
    if !(1..=365).contains(&days) {
        return Err(AppError::Validation(
            "a research token lasts between one and three hundred and sixty-five days".into(),
        ));
    }
    if let Some(l) = label {
        crate::validators::check_max_len(l, "label", 80)?;
    }

    let plaintext = mint();
    let token_hash = hash(&plaintext);
    let token_prefix = plaintext.chars().take(12).collect::<String>();

    let mut tx = db.begin().await?;

    sqlx::query(
        "UPDATE security_research_tokens
            SET revoked_at = NOW(), revoked_reason = 'superseded'
          WHERE user_id = $1 AND revoked_at IS NULL",
    )
    .bind(user_id)
    .execute(&mut *tx)
    .await?;

    let view: TokenView = sqlx::query_as::<
        _,
        (
            Uuid,
            String,
            Option<String>,
            chrono::DateTime<chrono::Utc>,
            chrono::DateTime<chrono::Utc>,
            Option<chrono::DateTime<chrono::Utc>>,
            i64,
        ),
    >(
        "INSERT INTO security_research_tokens
             (user_id, token_hash, token_prefix, label, expires_at)
         VALUES ($1, $2, $3, $4, NOW() + make_interval(days => $5::INT))
         RETURNING id, token_prefix, label, issued_at, expires_at, last_used_at,
                   requests_seen",
    )
    .bind(user_id)
    .bind(&token_hash)
    .bind(&token_prefix)
    .bind(label)
    .bind(days as i32)
    .fetch_one(&mut *tx)
    .await
    .map(
        |(id, token_prefix, label, issued_at, expires_at, last_used_at, requests_seen)| TokenView {
            id,
            token_prefix,
            label,
            issued_at,
            expires_at,
            last_used_at,
            requests_seen,
        },
    )?;

    tx.commit().await?;
    Ok((plaintext, view))
}

/// The columns behind a `TokenView`, in the order the query selects them.
/// Seven positional fields is where a tuple stops documenting itself.
type TokenRow = (
    Uuid,
    String,
    Option<String>,
    chrono::DateTime<chrono::Utc>,
    chrono::DateTime<chrono::Utc>,
    Option<chrono::DateTime<chrono::Utc>>,
    i64,
);

/// The live token of this person, if there is one.
pub async fn current(db: &PgPool, user_id: Uuid) -> Result<Option<TokenView>, AppError> {
    let row: Option<TokenRow> = sqlx::query_as(
        "SELECT id, token_prefix, label, issued_at, expires_at, last_used_at,
                requests_seen
           FROM security_research_tokens
          WHERE user_id = $1 AND revoked_at IS NULL",
    )
    .bind(user_id)
    .fetch_optional(db)
    .await?;

    Ok(row.map(
        |(id, token_prefix, label, issued_at, expires_at, last_used_at, requests_seen)| TokenView {
            id,
            token_prefix,
            label,
            issued_at,
            expires_at,
            last_used_at,
            requests_seen,
        },
    ))
}

/// Revoke the holder's live token.
///
/// `NotFound` when there is none, rather than success: a holder who thinks they
/// have revoked something and had not is exactly the person this endpoint
/// exists for.
pub async fn revoke(db: &PgPool, user_id: Uuid, reason: &str) -> Result<(), AppError> {
    let affected = sqlx::query(
        "UPDATE security_research_tokens
            SET revoked_at = NOW(), revoked_reason = $2
          WHERE user_id = $1 AND revoked_at IS NULL",
    )
    .bind(user_id)
    .bind(reason)
    .execute(db)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(AppError::NotFound("no live research token".into()));
    }
    Ok(())
}

/// Revoke by token id, for an operator or for the abuse rule.
pub async fn revoke_by_id(db: &PgPool, token_id: Uuid, reason: &str) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE security_research_tokens
            SET revoked_at = NOW(), revoked_reason = $2
          WHERE id = $1 AND revoked_at IS NULL",
    )
    .bind(token_id)
    .bind(reason)
    .execute(db)
    .await?;
    Ok(())
}

/// Look a presented token up.
///
/// `None` for anything that is not a live token — wrong prefix, unknown hash,
/// expired, revoked. Never an error: a bad token means the request is treated
/// as ordinary traffic, and answering 401 to it would turn this header into an
/// oracle for whether a token exists.
pub async fn verify(db: &PgPool, presented: &str) -> Option<ResearchMode> {
    if !presented.starts_with(PREFIX) || presented.len() != PREFIX.len() + 64 {
        return None;
    }

    let row: Option<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT id, user_id
           FROM security_research_tokens
          WHERE token_hash = $1
            AND revoked_at IS NULL
            AND expires_at > NOW()",
    )
    .bind(hash(presented))
    .fetch_optional(db)
    .await
    .ok()
    .flatten();

    row.map(|(token_id, user_id)| ResearchMode { token_id, user_id })
}

/// Count a use, flush the count occasionally, and revoke on abnormal volume.
///
/// Redis rather than the database for the counting, because this runs on every
/// request the token covers and the whole point of the token is that there are
/// a lot of them. Both counters are best-effort: a Redis that is down costs the
/// audit trail its precision and must not cost the request its answer.
pub async fn record_use(
    db: &PgPool,
    redis: &mut redis::aio::ConnectionManager,
    mode: ResearchMode,
) {
    use redis::AsyncCommands;

    // Per-minute counter, for the abuse rule.
    let rate_key = format!("research:rate:{}", mode.token_id);
    if let Ok(count) = redis.incr::<_, _, i64>(&rate_key, 1).await {
        if count == 1 {
            let _: Result<(), _> = redis.expire(&rate_key, 60).await;
        }
        if count > ABNORMAL_REQUESTS_PER_MINUTE {
            tracing::warn!(
                token = %mode.token_id, user = %mode.user_id, count,
                "research token revoked: volume past the published threshold"
            );
            if let Err(e) = revoke_by_id(db, mode.token_id, "abnormal_volume").await {
                tracing::error!(token = %mode.token_id, error = %e,
                    "could not revoke a research token that tripped the volume rule");
            }
            return;
        }
    }

    // Pending-uses counter, flushed to the row every FLUSH_EVERY requests.
    let pending_key = format!("research:pending:{}", mode.token_id);
    let Ok(pending) = redis.incr::<_, _, i64>(&pending_key, 1).await else {
        return;
    };
    if pending < FLUSH_EVERY {
        return;
    }

    // `GETDEL` so two workers cannot both flush the same batch.
    let flushed: i64 = redis::cmd("GETDEL")
        .arg(&pending_key)
        .query_async(redis)
        .await
        .unwrap_or(0);
    if flushed == 0 {
        return;
    }

    if let Err(e) = sqlx::query(
        "UPDATE security_research_tokens
            SET last_used_at = NOW(), requests_seen = requests_seen + $2
          WHERE id = $1",
    )
    .bind(mode.token_id)
    .bind(flushed)
    .execute(db)
    .await
    {
        tracing::warn!(token = %mode.token_id, error = %e,
            "research token use count not written down");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_minted_token_is_recognisable_and_long_enough() {
        let t = mint();
        assert!(t.starts_with(PREFIX));
        // Thirty-two bytes in hex, plus the prefix.
        assert_eq!(t.len(), PREFIX.len() + 64);
        assert_ne!(t, mint(), "two mints must not collide");
    }

    #[test]
    fn hashing_is_stable_and_hides_the_secret() {
        let t = mint();
        assert_eq!(hash(&t), hash(&t));
        assert_eq!(hash(&t).len(), 64);
        assert!(!hash(&t).contains(&t[PREFIX.len()..PREFIX.len() + 8]));
    }
}
