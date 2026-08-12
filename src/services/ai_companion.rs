//! SKI-44 (Post-MVP T3-01) — disclosed AI learning companion.
//!
//! See migration 0149 for why disclosure lives in its own table rather
//! than in `ai_call_log`.
//!
//! ## Cost control
//!
//! LLM spend is the stated risk on this ticket, so it is capped in three
//! independent ways, each covering a case the others miss:
//!
//!   * a **daily quota** per user ([`DAILY_QUOTA`]), counted from
//!     `ai_interactions` — the durable record, so a Redis flush cannot
//!     hand someone a fresh allowance;
//!   * a **burst limit** via the Redis rate limiter, so the daily quota
//!     cannot be spent in ten seconds by a script;
//!   * a **response cache** keyed on a hash of the normalized request, so
//!     the same question asked twice is billed once. Cache hits do not
//!     consume quota: charging a user for an answer we did not pay for
//!     would be arbitrary.
//!
//! ## Availability
//!
//! `skilluv-ia` is a separate service and is a stub in some environments.
//! Every failure mode — worker not connected, `Unimplemented`, timeout —
//! is reported as [`AppError::ServiceUnavailable`] with the interaction
//! still recorded, so the reason a learner got no answer is visible in the
//! ledger rather than only in a log line.

use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

pub const INTERACTION_TYPES: &[&str] =
    &["explain", "generate_exercises", "pre_review", "debug_help"];

/// Companion calls allowed per user per rolling 24 hours.
pub const DAILY_QUOTA: i64 = 10;

/// Burst limit: calls per window, enforced in Redis on top of the quota.
pub const BURST_MAX: u64 = 3;
pub const BURST_WINDOW_SECS: u64 = 60;

/// Cached answers are reused for this long.
pub const CACHE_TTL_SECS: u64 = 7 * 24 * 3600;

/// Maximum prompt length accepted.
pub const MAX_PROMPT_CHARS: usize = 4000;
/// Maximum code payload accepted, in characters.
pub const MAX_CODE_CHARS: usize = 20_000;

/// How far back the disclosure sweep looks when a deliverable is
/// submitted.
///
/// Seven days: long enough to cover the work session that produced the
/// artifact, short enough that unrelated help from last month is not
/// dragged in. Attaching everything ever asked would make the disclosure
/// noise; attaching nothing would make it a lie.
pub const DISCLOSURE_WINDOW_DAYS: i64 = 7;

// Compile-time coherence checks on the cost-control constants. Assertions
// rather than a test: these are constants, so a violation is a build error
// and cannot reach a running binary.
const _: () = assert!(DAILY_QUOTA > 0);
const _: () = assert!(DISCLOSURE_WINDOW_DAYS > 0);
// The burst limit has to bite before the daily quota, or it is dead code.
const _: () = assert!((BURST_MAX as i64) < DAILY_QUOTA);

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AiInteraction {
    pub id: Uuid,
    pub user_id: Uuid,
    pub interaction_type: String,
    pub prompt: String,
    pub skill_slug: Option<String>,
    pub status: String,
    pub disclosure_label: String,
    pub model_version: Option<String>,
    pub tokens_used: i32,
    pub disclosed_on_deliverable_id: Option<Uuid>,
    pub disclosed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub request_hash: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// The answer handed back to the caller.
#[derive(Debug, Clone, Serialize)]
pub struct CompanionAnswer {
    pub interaction_id: Uuid,
    pub answer_markdown: String,
    pub items: Vec<CompanionItem>,
    pub disclosure_label: String,
    pub model_version: Option<String>,
    /// True when served from cache — no LLM call was made and no quota was
    /// consumed.
    pub cached: bool,
    /// Companion calls left in the current 24h window.
    pub quota_remaining: i64,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct CompanionItem {
    pub title: String,
    pub body_markdown: String,
    pub kind: String,
    pub priority: i32,
}

/// Cached payload, as stored in Redis.
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
struct CachedAnswer {
    answer_markdown: String,
    items: Vec<CompanionItem>,
    disclosure_label: String,
    model_version: Option<String>,
}

pub fn validate_interaction_type(t: &str) -> Result<(), AppError> {
    if INTERACTION_TYPES.contains(&t) {
        return Ok(());
    }
    Err(AppError::Validation(format!(
        "interaction_type must be one of: {}",
        INTERACTION_TYPES.join(", ")
    )))
}

/// Stable hash of a normalized request.
///
/// Whitespace is collapsed and the text lowercased so trivially different
/// phrasings of the same question share a cache entry. The user id is NOT
/// part of the key: two learners asking the same thing should get the same
/// answer, which is the whole saving.
pub fn request_hash(
    interaction_type: &str,
    prompt: &str,
    code: &str,
    language: &str,
    skill_slug: &str,
    locale: &str,
) -> String {
    fn normalize(s: &str) -> String {
        s.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    }

    let mut hasher = Sha256::new();
    // Length-prefixed so that moving text across field boundaries cannot
    // produce the same digest.
    for part in [
        interaction_type,
        &normalize(prompt),
        // Code keeps its casing and layout: those are semantically
        // meaningful, unlike in prose.
        code.trim(),
        language,
        skill_slug,
        locale,
    ] {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part.as_bytes());
    }
    hex::encode(hasher.finalize())
}

/// Companion calls used in the last 24 hours.
///
/// Counts only interactions that actually reached the worker: a call that
/// failed because the worker was down must not cost the learner an
/// allowance they never got value from.
pub async fn used_today(db: &PgPool, user_id: Uuid) -> Result<i64, AppError> {
    let used: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM ai_interactions
          WHERE user_id = $1
            AND status = 'ok'
            AND created_at >= NOW() - INTERVAL '24 hours'",
    )
    .bind(user_id)
    .fetch_one(db)
    .await?;
    Ok(used)
}

/// Record an interaction in the disclosure ledger. Best-effort on the
/// caller's side is not acceptable here — an undisclosed AI interaction is
/// exactly what this table exists to prevent — so errors propagate.
#[allow(clippy::too_many_arguments)]
pub async fn record(
    db: &PgPool,
    user_id: Uuid,
    interaction_type: &str,
    prompt: &str,
    skill_slug: Option<&str>,
    status: &str,
    disclosure_label: &str,
    model_version: Option<&str>,
    tokens_used: i32,
    request_hash: Option<&str>,
) -> Result<AiInteraction, AppError> {
    let truncated: String = prompt.chars().take(MAX_PROMPT_CHARS).collect();
    let interaction: AiInteraction = sqlx::query_as(
        r#"
        INSERT INTO ai_interactions
            (user_id, interaction_type, prompt, skill_slug, status,
             disclosure_label, model_version, tokens_used, request_hash)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        RETURNING *
        "#,
    )
    .bind(user_id)
    .bind(interaction_type)
    .bind(&truncated)
    .bind(skill_slug)
    .bind(status)
    .bind(disclosure_label)
    .bind(model_version)
    .bind(tokens_used.max(0))
    .bind(request_hash)
    .fetch_one(db)
    .await?;
    Ok(interaction)
}

/// Summary of a disclosure sweep.
#[derive(Debug, Clone, Default, Serialize)]
pub struct DisclosureReport {
    pub interactions_attached: usize,
    pub types: Vec<String>,
}

/// Attach a user's recent undisclosed interactions to a deliverable.
///
/// Called when a deliverable is submitted. Writes both sides: the
/// interaction rows are stamped, and a summary is merged into
/// `deliverables.verification_signal.ai_companion` so a reviewer sees the
/// disclosure on the artifact itself rather than having to know this table
/// exists.
///
/// Idempotent: already-disclosed interactions are excluded, so re-running
/// it after an edit does not double-count.
pub async fn disclose_on_deliverable(
    db: &PgPool,
    user_id: Uuid,
    deliverable_id: Uuid,
) -> Result<DisclosureReport, AppError> {
    let attached: Vec<(Uuid, String)> = sqlx::query_as(
        r#"
        UPDATE ai_interactions
           SET disclosed_on_deliverable_id = $2,
               disclosed_at = NOW()
         WHERE user_id = $1
           AND status = 'ok'
           AND disclosed_on_deliverable_id IS NULL
           AND created_at >= NOW() - MAKE_INTERVAL(days => $3::INT)
        RETURNING id, interaction_type
        "#,
    )
    .bind(user_id)
    .bind(deliverable_id)
    .bind(DISCLOSURE_WINDOW_DAYS as i32)
    .fetch_all(db)
    .await?;

    if attached.is_empty() {
        return Ok(DisclosureReport::default());
    }

    let mut types: Vec<String> = attached.iter().map(|(_, t)| t.clone()).collect();
    types.sort();
    types.dedup();

    let summary = serde_json::json!({
        "ai_companion": {
            "interactions": attached.len(),
            "types": types,
            "window_days": DISCLOSURE_WINDOW_DAYS,
            "disclosed_at": chrono::Utc::now().to_rfc3339(),
        }
    });

    // Merged rather than overwritten: verification_signal already carries
    // webhook payloads and plagiarism results.
    sqlx::query(
        "UPDATE deliverables
            SET verification_signal = COALESCE(verification_signal, '{}'::jsonb) || $2::jsonb
          WHERE id = $1",
    )
    .bind(deliverable_id)
    .bind(&summary)
    .execute(db)
    .await?;

    Ok(DisclosureReport {
        interactions_attached: attached.len(),
        types,
    })
}

/// Serialize a cached answer for Redis.
pub(crate) fn encode_cache(
    answer_markdown: &str,
    items: &[CompanionItem],
    disclosure_label: &str,
    model_version: Option<&str>,
) -> String {
    serde_json::to_string(&CachedAnswer {
        answer_markdown: answer_markdown.to_string(),
        items: items.to_vec(),
        disclosure_label: disclosure_label.to_string(),
        model_version: model_version.map(str::to_string),
    })
    .unwrap_or_default()
}

/// Parse a cached answer. Returns `None` on anything unreadable, so a
/// stale or corrupted entry degrades to a cache miss.
pub(crate) fn decode_cache(
    raw: &str,
) -> Option<(String, Vec<CompanionItem>, String, Option<String>)> {
    let c: CachedAnswer = serde_json::from_str(raw).ok()?;
    Some((
        c.answer_markdown,
        c.items,
        c.disclosure_label,
        c.model_version,
    ))
}

/// Redis key for a cached answer.
pub fn cache_key(hash: &str) -> String {
    format!("ai:companion:{hash}")
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn interaction_types_are_validated() {
        for t in INTERACTION_TYPES {
            assert!(validate_interaction_type(t).is_ok());
        }
        assert!(validate_interaction_type("write_it_for_me").is_err());
        assert!(validate_interaction_type("").is_err());
    }

    #[test]
    fn hash_ignores_prose_formatting_but_not_code() {
        let a = request_hash("explain", "What  is\na  borrow?", "", "", "", "fr");
        let b = request_hash("explain", "what is a borrow?", "", "", "", "fr");
        assert_eq!(a, b, "whitespace and casing must not split the cache");

        // Code layout is meaningful — two differently-indented snippets are
        // different questions.
        let c1 = request_hash("pre_review", "check", "fn a() {}", "rust", "", "fr");
        let c2 = request_hash("pre_review", "check", "fn a() { }", "rust", "", "fr");
        assert_ne!(c1, c2);
    }

    #[test]
    fn hash_separates_fields() {
        // Without length prefixing, moving text across a field boundary
        // would collide.
        let a = request_hash("explain", "ab", "c", "", "", "");
        let b = request_hash("explain", "a", "bc", "", "", "");
        assert_ne!(a, b);
    }

    #[test]
    fn hash_varies_with_every_input() {
        let base = request_hash("explain", "q", "code", "rust", "react", "fr");
        assert_ne!(
            base,
            request_hash("debug_help", "q", "code", "rust", "react", "fr")
        );
        assert_ne!(
            base,
            request_hash("explain", "q2", "code", "rust", "react", "fr")
        );
        assert_ne!(
            base,
            request_hash("explain", "q", "code", "python", "react", "fr")
        );
        assert_ne!(
            base,
            request_hash("explain", "q", "code", "rust", "vue", "fr")
        );
        assert_ne!(
            base,
            request_hash("explain", "q", "code", "rust", "react", "en")
        );
    }

    #[test]
    fn hash_is_not_user_scoped() {
        // Two learners asking the same question share one cache entry —
        // that is where the saving comes from.
        let a = request_hash("explain", "same question", "", "", "", "fr");
        let b = request_hash("explain", "SAME QUESTION", "", "", "", "fr");
        assert_eq!(a, b);
        assert_eq!(a.len(), 64, "sha-256 hex digest");
    }

    #[test]
    fn cache_roundtrips_and_rejects_garbage() {
        let items = vec![CompanionItem {
            title: "Exercise 1".into(),
            body_markdown: "Do the thing".into(),
            kind: "exercise".into(),
            priority: 1,
        }];
        let encoded = encode_cache("answer", &items, "label", Some("haiku-4.5"));
        let (answer, decoded_items, label, model) =
            decode_cache(&encoded).expect("roundtrip succeeds");
        assert_eq!(answer, "answer");
        assert_eq!(decoded_items.len(), 1);
        assert_eq!(decoded_items[0].title, "Exercise 1");
        assert_eq!(label, "label");
        assert_eq!(model.as_deref(), Some("haiku-4.5"));

        // A corrupted entry is a cache miss, not a 500.
        assert!(decode_cache("{not json").is_none());
        assert!(decode_cache("").is_none());
    }
}
