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
    /// True when the answer came from the response cache — no worker call,
    /// no tokens. Migration 0444 explains why this is stored rather than
    /// inferred from `tokens_used`.
    pub cached: bool,
    /// `burst` | `daily_quota` when the request was refused by a guard
    /// rail, `None` otherwise.
    pub refusal_kind: Option<String>,
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

/// Everything one ledger row records.
///
/// A struct rather than a dozen positional arguments: the call sites all
/// differ by one or two fields, and a transposed `&str` pair among ten
/// would compile and quietly corrupt the disclosure ledger.
pub struct RecordParams<'a> {
    pub interaction_type: &'a str,
    pub prompt: &'a str,
    pub skill_slug: Option<&'a str>,
    pub status: &'a str,
    pub disclosure_label: &'a str,
    pub model_version: Option<&'a str>,
    pub tokens_used: i32,
    pub request_hash: Option<&'a str>,
    /// Served from the response cache.
    pub cached: bool,
    /// Set only alongside `status = "rate_limited"`.
    pub refusal_kind: Option<&'a str>,
}

impl<'a> RecordParams<'a> {
    /// A plain exchange: reached the worker, spent tokens.
    pub fn call(interaction_type: &'a str, prompt: &'a str) -> Self {
        Self {
            interaction_type,
            prompt,
            skill_slug: None,
            status: "ok",
            disclosure_label: "",
            model_version: None,
            tokens_used: 0,
            request_hash: None,
            cached: false,
            refusal_kind: None,
        }
    }
}

/// Record an interaction in the disclosure ledger. Best-effort on the
/// caller's side is not acceptable here — an undisclosed AI interaction is
/// exactly what this table exists to prevent — so errors propagate.
pub async fn record(
    db: &PgPool,
    user_id: Uuid,
    params: RecordParams<'_>,
) -> Result<AiInteraction, AppError> {
    let truncated: String = params.prompt.chars().take(MAX_PROMPT_CHARS).collect();
    let interaction: AiInteraction = sqlx::query_as(
        r#"
        INSERT INTO ai_interactions
            (user_id, interaction_type, prompt, skill_slug, status,
             disclosure_label, model_version, tokens_used, request_hash,
             cached, refusal_kind)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
        RETURNING *
        "#,
    )
    .bind(user_id)
    .bind(params.interaction_type)
    .bind(&truncated)
    .bind(params.skill_slug)
    .bind(params.status)
    .bind(params.disclosure_label)
    .bind(params.model_version)
    .bind(params.tokens_used.max(0))
    .bind(params.request_hash)
    .bind(params.cached)
    .bind(params.refusal_kind)
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

/// SKI-298 (T3-01b) — operational projection of the companion.
///
/// The ticket's premise is that a feature whose cost can explode has no
/// business shipping without a way to watch the cost. Everything here is
/// an aggregate: no prompt text crosses this boundary, because the point
/// is spending and quota pressure, and reading learners' questions is not
/// needed to answer either.
#[derive(Debug, Clone, Serialize)]
pub struct AdminStats {
    pub window_days: i64,
    pub total_requests: i64,
    /// Requests that actually reached the worker and were billed.
    pub billed_calls: i64,
    pub cache_hits: i64,
    /// Hits over (hits + billed calls). `None` when neither happened —
    /// a rate of 0.0 on an empty window would read as "the cache is
    /// broken" rather than "nothing was asked".
    pub cache_hit_rate: Option<f64>,
    pub tokens_total: i64,
    pub refused_burst: i64,
    pub refused_daily_quota: i64,
    /// Worker unreachable or erroring — the gRPC side of the story.
    pub worker_failures: i64,
    pub distinct_users: i64,
    pub by_interaction_type: std::collections::BTreeMap<String, i64>,
    pub by_status: std::collections::BTreeMap<String, i64>,
    pub top_consumers: Vec<TopConsumer>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct TopConsumer {
    pub user_id: Uuid,
    pub username: String,
    pub display_name: String,
    pub requests: i64,
    pub tokens_used: i64,
}

/// Longest window the stats endpoint will aggregate over.
pub const MAX_STATS_WINDOW_DAYS: i64 = 365;

pub async fn admin_stats(
    db: &PgPool,
    window_days: i64,
    top_n: i64,
) -> Result<AdminStats, AppError> {
    let window_days = window_days.clamp(1, MAX_STATS_WINDOW_DAYS);

    // One pass for the scalars. Splitting these into separate queries would
    // let them disagree about the window boundary between round trips.
    let totals: (i64, i64, i64, i64, i64, i64, i64, i64) = sqlx::query_as(
        r#"
        SELECT COUNT(*),
               COUNT(*) FILTER (WHERE status = 'ok' AND NOT cached),
               COUNT(*) FILTER (WHERE cached),
               COALESCE(SUM(tokens_used), 0)::BIGINT,
               COUNT(*) FILTER (WHERE refusal_kind = 'burst'),
               COUNT(*) FILTER (WHERE refusal_kind = 'daily_quota'),
               COUNT(*) FILTER (WHERE status IN ('unavailable', 'error')),
               COUNT(DISTINCT user_id)
          FROM ai_interactions
         WHERE created_at >= NOW() - MAKE_INTERVAL(days => $1::INT)
        "#,
    )
    .bind(window_days as i32)
    .fetch_one(db)
    .await?;

    let (total, billed, hits, tokens, burst, quota, failures, users) = totals;

    let by_interaction_type = count_by_type(db, window_days).await?;
    let by_status = count_by_status(db, window_days).await?;

    let top_consumers: Vec<TopConsumer> = sqlx::query_as(
        r#"
        SELECT a.user_id,
               u.username,
               COALESCE(NULLIF(u.display_name, ''), u.username) AS display_name,
               COUNT(*)                          AS requests,
               COALESCE(SUM(a.tokens_used), 0)::BIGINT AS tokens_used
          FROM ai_interactions a
          JOIN users u ON u.id = a.user_id
         WHERE a.created_at >= NOW() - MAKE_INTERVAL(days => $1::INT)
         GROUP BY a.user_id, u.username, u.display_name
         ORDER BY requests DESC, tokens_used DESC
         LIMIT $2
        "#,
    )
    .bind(window_days as i32)
    .bind(top_n.clamp(1, 100))
    .fetch_all(db)
    .await?;

    let served = hits + billed;
    Ok(AdminStats {
        window_days,
        total_requests: total,
        billed_calls: billed,
        cache_hits: hits,
        cache_hit_rate: (served > 0).then(|| hits as f64 / served as f64),
        tokens_total: tokens,
        refused_burst: burst,
        refused_daily_quota: quota,
        worker_failures: failures,
        distinct_users: users,
        by_interaction_type,
        by_status,
        top_consumers,
    })
}

/// Counts by interaction type over the window.
async fn count_by_type(
    db: &PgPool,
    window_days: i64,
) -> Result<std::collections::BTreeMap<String, i64>, AppError> {
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT interaction_type, COUNT(*)
           FROM ai_interactions
          WHERE created_at >= NOW() - MAKE_INTERVAL(days => $1::INT)
          GROUP BY 1",
    )
    .bind(window_days as i32)
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().collect())
}

/// Counts by outcome over the window.
///
/// A sibling of [`count_by_type`] rather than one function taking the
/// column name: interpolating a column into the SQL would trade a
/// compile-time guarantee for a runtime `debug_assert`, and the two value
/// sets are both fixed by a CHECK constraint anyway.
async fn count_by_status(
    db: &PgPool,
    window_days: i64,
) -> Result<std::collections::BTreeMap<String, i64>, AppError> {
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT status, COUNT(*)
           FROM ai_interactions
          WHERE created_at >= NOW() - MAKE_INTERVAL(days => $1::INT)
          GROUP BY 1",
    )
    .bind(window_days as i32)
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().collect())
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
