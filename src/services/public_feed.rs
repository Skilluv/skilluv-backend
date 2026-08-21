//! The public artefact feed (migration 0203).
//!
//! ## What it is for
//!
//! Showing, on the most public surface the product has, what actually comes
//! out of it. Every line points at something a stranger can open and check:
//! a merged pull request, a verification page, a published package. That is
//! the whole argument — a feed of points proves nothing to anybody, and a
//! feed of invented people proves less than nothing.
//!
//! ## Two rules that are not negotiable
//!
//! **Visibility is decided at write time.** [`emit`] asks the database once,
//! stores the answer and the reason for it. Reading the feed never joins to
//! `users` or to a preference table, so a missing predicate cannot leak
//! somebody who opted out.
//!
//! **Only artefact-backed events.** The `kind` column has a CHECK, and every
//! emitter here passes a URL. An event with nowhere to go is a claim, and a
//! public feed is a surface people optimise against.

use bigdecimal::BigDecimal;
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

/// Everything the feed admits. Mirrors the CHECK in migration 0203 — a kind
/// added here without the migration is refused by the database, which is the
/// right way round.
pub const KINDS: &[&str] = &[
    "pr_merged_upstream",
    "deliverable_verified",
    "attestation_issued",
    "bounty_paid",
    "mission_delivered",
    "library_published",
];

/// Kinds that may carry a figure. The others are refused one: a mission
/// delivered says the work happened, not what it paid.
pub const MONETARY_KINDS: &[&str] = &["bounty_paid"];

/// A page of the feed. Deliberately small — this is a landing page, and
/// nobody scrolls a ticker.
pub const DEFAULT_PAGE: i64 = 20;
pub const MAX_PAGE: i64 = 100;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct FeedItem {
    pub id: Uuid,
    pub kind: String,
    pub subject_type: String,
    pub subject_label: String,
    pub headline: String,
    /// Where the artefact is. Always present: this is the point.
    pub artifact_url: String,
    pub repository: Option<String>,
    pub amount: Option<BigDecimal>,
    pub currency: Option<String>,
    pub occurred_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
pub struct Emission<'a> {
    pub kind: &'a str,
    pub subject_type: &'a str,
    pub subject_id: Uuid,
    pub subject_label: &'a str,
    pub headline: String,
    pub artifact_url: String,
    pub repository: Option<String>,
    pub amount: Option<BigDecimal>,
    pub currency: Option<&'a str>,
    /// What this is a projection of, so a revocation can find its line.
    pub source_type: &'a str,
    pub source_id: Uuid,
}

/// Write one line, with its visibility decided now.
///
/// Idempotent on `(source_type, source_id, kind)`: a deliverable verified by
/// two code paths is one thing that happened. The second call updates the
/// wording rather than adding a line, because the wording is the only thing
/// that can legitimately have improved.
pub async fn emit(db: &PgPool, event: Emission<'_>) -> Result<Option<Uuid>, AppError> {
    if !KINDS.contains(&event.kind) {
        return Err(AppError::Internal(format!(
            "'{}' is not a public feed kind",
            event.kind
        )));
    }
    if !event.artifact_url.starts_with("http") {
        return Err(AppError::Internal(format!(
            "a {} event with no artefact to point at is a claim, not a proof",
            event.kind
        )));
    }
    if event.amount.is_some() && !MONETARY_KINDS.contains(&event.kind) {
        return Err(AppError::Internal(format!(
            "a {} event must not carry a figure",
            event.kind
        )));
    }

    // A guild has no preference of its own: the decision belongs to the
    // people in it, and until there is a guild-level setting the safe answer
    // is that team work is as public as the artefact behind it.
    let (visible, reason): (bool, String) = if event.subject_type == "user" {
        sqlx::query_as("SELECT visible, reason FROM public_feed_visibility($1, $2)")
            .bind(event.subject_id)
            .bind(event.kind)
            .fetch_one(db)
            .await?
    } else {
        let default_visible: Option<bool> = sqlx::query_scalar(
            "SELECT default_visible FROM public_feed_event_kinds WHERE kind = $1",
        )
        .bind(event.kind)
        .fetch_optional(db)
        .await?;
        match default_visible {
            Some(true) => (true, "default_public".to_string()),
            _ => (false, "kind_private".to_string()),
        }
    };

    let id: Option<Uuid> = sqlx::query_scalar(
        r#"
        INSERT INTO public_artifact_events
            (kind, subject_type, subject_id, subject_label, headline,
             artifact_url, repository, amount, currency,
             public, visibility_reason, source_type, source_id)
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
        ON CONFLICT (source_type, source_id, kind) DO UPDATE
            SET headline = EXCLUDED.headline,
                artifact_url = EXCLUDED.artifact_url,
                repository = EXCLUDED.repository
        RETURNING id
        "#,
    )
    .bind(event.kind)
    .bind(event.subject_type)
    .bind(event.subject_id)
    .bind(event.subject_label)
    .bind(&event.headline)
    .bind(&event.artifact_url)
    .bind(event.repository.as_deref())
    .bind(event.amount.as_ref())
    .bind(event.currency)
    .bind(visible)
    .bind(&reason)
    .bind(event.source_type)
    .bind(event.source_id)
    .fetch_optional(db)
    .await?;

    if visible {
        metrics::counter!(
            "skilluv_public_feed_events_total",
            "kind" => event.kind.to_string()
        )
        .increment(1);
    }

    Ok(id)
}

/// Where a page stopped, so the next one can carry on.
///
/// Keyset rather than an offset: the feed is ordered by time and new rows
/// arrive at the front, so an offset skips or repeats items exactly when the
/// feed is busy — which is the only time anybody paginates it.
#[derive(Debug, Clone, Copy)]
pub struct Cursor {
    pub occurred_at: chrono::DateTime<chrono::Utc>,
    pub id: Uuid,
}

impl Cursor {
    /// `<rfc3339>|<uuid>`, base64url without padding.
    ///
    /// The encoding is not decoration. An RFC3339 timestamp carries a `+`
    /// before its offset, and a `+` in a query string decodes as a space — so
    /// a caller who takes `next_cursor` and puts it in a URL, which is the
    /// only thing anybody does with it, gets a cursor the server cannot read
    /// and a 400 on the second page. base64url has no character a URL treats
    /// as anything.
    pub fn encode(&self) -> String {
        use base64::Engine;
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(format!(
            "{}|{}",
            self.occurred_at.to_rfc3339(),
            self.id
        ))
    }

    pub fn decode(raw: &str) -> Option<Self> {
        use base64::Engine;
        let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(raw.as_bytes())
            .ok()?;
        let decoded = String::from_utf8(decoded).ok()?;
        let (time, id) = decoded.split_once('|')?;
        Some(Cursor {
            occurred_at: chrono::DateTime::parse_from_rfc3339(time)
                .ok()?
                .with_timezone(&chrono::Utc),
            id: id.parse().ok()?,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FeedPage {
    pub items: Vec<FeedItem>,
    /// Absent when there is nothing after this page.
    pub next_cursor: Option<String>,
}

/// Read the feed.
///
/// One table, one predicate. Nothing here joins to anything private, which is
/// the reason the projection exists.
pub async fn page(
    db: &PgPool,
    after: Option<Cursor>,
    limit: i64,
    kind: Option<&str>,
) -> Result<FeedPage, AppError> {
    let limit = limit.clamp(1, MAX_PAGE);

    let items = sqlx::query_as::<_, FeedItem>(
        r#"
        SELECT id, kind, subject_type, subject_label, headline,
               artifact_url, repository, amount, currency, occurred_at
          FROM public_artifact_events
         WHERE public = TRUE
           AND retracted_at IS NULL
           AND ($3::TEXT IS NULL OR kind = $3)
           AND ($1::TIMESTAMPTZ IS NULL
                OR (occurred_at, id) < ($1::TIMESTAMPTZ, $2::UUID))
         ORDER BY occurred_at DESC, id DESC
         LIMIT $4
        "#,
    )
    .bind(after.map(|c| c.occurred_at))
    .bind(after.map(|c| c.id))
    .bind(kind)
    .bind(limit)
    .fetch_all(db)
    .await?;

    // A cursor only when the page was full. Handing one back on a short page
    // means a caller polls forever for a page that will always be empty.
    let next_cursor = (items.len() as i64 == limit)
        .then(|| items.last())
        .flatten()
        .map(|last| {
            Cursor {
                occurred_at: last.occurred_at,
                id: last.id,
            }
            .encode()
        });

    Ok(FeedPage { items, next_cursor })
}

/// How much has actually come out of the forge lately.
///
/// Read by the landing page before it decides to show a live ticker at all: a
/// feed whose first line says "two days ago" is worse than no feed, because
/// it proves the place is empty. Below the threshold the honest presentation
/// is a "latest work" list with no pulsing dot.
pub async fn density_last_days(db: &PgPool, days: i32) -> Result<f64, AppError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM public_artifact_events
          WHERE public = TRUE AND retracted_at IS NULL
            AND occurred_at > NOW() - ($1 || ' days')::INTERVAL",
    )
    .bind(days.max(1).to_string())
    .fetch_one(db)
    .await?;
    Ok(count as f64 / days.max(1) as f64)
}

/// Somebody's own choices, including the ones they have not made.
///
/// Returns every kind with the default filled in, rather than only the rows
/// that exist: a settings screen showing three of six options because the
/// other three were never chosen is a settings screen nobody trusts.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct PreferenceRow {
    pub kind: String,
    pub label: String,
    pub description: String,
    pub visible: bool,
    /// True when this is the default rather than something the person chose.
    pub is_default: bool,
    pub already_public_elsewhere: bool,
}

pub async fn preferences_for(db: &PgPool, user_id: Uuid) -> Result<Vec<PreferenceRow>, AppError> {
    let rows = sqlx::query_as::<_, PreferenceRow>(
        r#"
        SELECT k.kind, k.label, k.description,
               COALESCE(p.visible, k.default_visible) AS visible,
               (p.visible IS NULL) AS is_default,
               k.already_public_elsewhere
          FROM public_feed_event_kinds k
          LEFT JOIN public_feed_preferences p
                 ON p.kind = k.kind AND p.user_id = $1
         ORDER BY k.already_public_elsewhere DESC, k.kind
        "#,
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// Record a choice, and apply it to what is already on the feed.
///
/// Retroactive on purpose. Somebody who turns this off is asking to be taken
/// off the page, and answering "only from now on" would leave their name up
/// there — which is the thing they just objected to.
pub async fn set_preference(
    db: &PgPool,
    user_id: Uuid,
    kind: &str,
    visible: bool,
) -> Result<u64, AppError> {
    let known: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM public_feed_event_kinds WHERE kind = $1)")
            .bind(kind)
            .fetch_one(db)
            .await?;
    if !known {
        return Err(AppError::Validation(format!(
            "'{kind}' is not something the public feed shows"
        )));
    }

    let mut tx = db.begin().await?;
    sqlx::query(
        "INSERT INTO public_feed_preferences (user_id, kind, visible)
         VALUES ($1, $2, $3)
         ON CONFLICT (user_id, kind) DO UPDATE
             SET visible = EXCLUDED.visible, updated_at = NOW()",
    )
    .bind(user_id)
    .bind(kind)
    .bind(visible)
    .execute(&mut *tx)
    .await?;

    let changed = sqlx::query(
        "UPDATE public_artifact_events
            SET public = $3,
                visibility_reason = CASE WHEN $3 THEN 'consented' ELSE 'opted_out' END
          WHERE subject_type = 'user'
            AND subject_id = $1
            AND kind = $2
            AND retracted_at IS NULL
            AND public <> $3",
    )
    .bind(user_id)
    .bind(kind)
    .bind(visible)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(changed.rows_affected())
}

/// Take everything of somebody's off the feed, now.
///
/// The one button somebody wants when they find their name on a page they did
/// not expect. Sets every preference to hidden and clears what is already up.
pub async fn withdraw_entirely(db: &PgPool, user_id: Uuid) -> Result<u64, AppError> {
    let mut tx = db.begin().await?;
    sqlx::query(
        "INSERT INTO public_feed_preferences (user_id, kind, visible)
         SELECT $1, kind, FALSE FROM public_feed_event_kinds
         ON CONFLICT (user_id, kind) DO UPDATE
             SET visible = FALSE, updated_at = NOW()",
    )
    .bind(user_id)
    .execute(&mut *tx)
    .await?;

    let cleared = sqlx::query(
        "UPDATE public_artifact_events
            SET public = FALSE, visibility_reason = 'opted_out'
          WHERE subject_type = 'user' AND subject_id = $1 AND public = TRUE",
    )
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(cleared.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cursor_survives_the_round_trip() {
        let cursor = Cursor {
            occurred_at: chrono::DateTime::parse_from_rfc3339("2026-08-17T10:30:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
            id: Uuid::parse_str("11111111-2222-3333-4444-555555555555").unwrap(),
        };
        let decoded = Cursor::decode(&cursor.encode()).expect("round trip");
        assert_eq!(decoded.occurred_at, cursor.occurred_at);
        assert_eq!(decoded.id, cursor.id);
    }

    #[test]
    fn a_cursor_goes_into_a_url_unchanged() {
        // The only thing anybody does with `next_cursor` is put it in a query
        // string. The first version of this encoding was `<rfc3339>|<uuid>`,
        // and an RFC3339 offset carries a `+`, which a query string decodes as
        // a space — so the second page answered 400 to a cursor the server had
        // just handed out.
        let cursor = Cursor {
            occurred_at: chrono::DateTime::parse_from_rfc3339("2026-08-17T10:30:00+02:00")
                .unwrap()
                .with_timezone(&chrono::Utc),
            id: Uuid::parse_str("11111111-2222-3333-4444-555555555555").unwrap(),
        };
        let encoded = cursor.encode();
        assert!(
            encoded
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "a URL must not have to escape it: {encoded}"
        );
    }

    #[test]
    fn a_malformed_cursor_is_refused_rather_than_guessed() {
        // Answering "from the beginning" to a broken cursor makes a client
        // silently re-read the whole feed.
        assert!(Cursor::decode("").is_none());
        assert!(Cursor::decode("not-a-date|11111111-2222-3333-4444-555555555555").is_none());
        assert!(Cursor::decode("2026-08-17T10:30:00Z|not-a-uuid").is_none());
        assert!(Cursor::decode("2026-08-17T10:30:00Z").is_none());
    }

    #[test]
    fn only_money_events_carry_money() {
        assert!(MONETARY_KINDS.iter().all(|k| KINDS.contains(k)));
        // A delivered mission says the work happened, not what it paid.
        assert!(!MONETARY_KINDS.contains(&"mission_delivered"));
        assert!(!MONETARY_KINDS.contains(&"deliverable_verified"));
    }

    // Both sides are constants, so this is decided at compile time rather
    // than by running a test — which is the right place for it: a default
    // above the maximum should never build.
    const _: () = assert!(DEFAULT_PAGE <= MAX_PAGE);
    const _: () = assert!(DEFAULT_PAGE <= 50, "nobody scrolls a ticker");
}
