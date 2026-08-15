//! SKI-286 — @username mentions across the four writing surfaces.
//!
//! Extraction itself lives in [`crate::services::social::parse_mentions`],
//! which already handles the tricky part (not matching email addresses).
//! This module owns what turns those matches into an inbox:
//!
//!   * [`record`] — idempotent insertion, one row per (target, source,
//!     author), so re-saving edited content adds only the genuinely new
//!     mentions.
//!   * [`list_for_user`] — the read path, which resolves each mention to
//!     an excerpt and a front-end URL, and **enforces confidentiality**.
//!
//! ## Where confidentiality is enforced, and why there
//!
//! A mention inside private content must only reach a target who can
//! already read that content: naming someone in a DM they are not part of
//! must not leak the DM's text to them through their mention inbox.
//!
//! That check happens on **read**, not on write. Writing the row
//! unconditionally and filtering when it is read means visibility always
//! reflects the content's *current* state: a diary entry flipped from
//! private to public starts surfacing its mentions, and one flipped back
//! stops. Deciding at write time would freeze the answer at the moment the
//! text was saved and quietly get it wrong afterwards.
//!
//! The same join drops mentions whose source has since been deleted or
//! hidden by moderation, which the ticket also requires.

use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

pub const SOURCE_FORUM_POST: &str = "forum_post";
pub const SOURCE_COMMENT: &str = "comment";
pub const SOURCE_SLICE_DIARY: &str = "slice_diary";
pub const SOURCE_MESSAGE: &str = "message";

pub const SOURCE_TYPES: &[&str] = &[
    SOURCE_FORUM_POST,
    SOURCE_COMMENT,
    SOURCE_SLICE_DIARY,
    SOURCE_MESSAGE,
];

/// Characters of surrounding context returned with each mention.
pub const EXCERPT_CHARS: usize = 160;

/// One mention, resolved for display.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct Mention {
    pub id: Uuid,
    pub source_type: String,
    pub source_id: Uuid,
    /// Front-end path, built here so the client does not have to maintain
    /// its own type-to-route table that would drift on the next URL change.
    pub source_url: String,
    /// Plain text around the mention — no markdown, no HTML.
    pub excerpt: String,
    pub author: MentionAuthor,
    pub read_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct MentionAuthor {
    pub user_id: Uuid,
    pub username: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
}

/// Row as read from the visibility-filtered query.
#[derive(Debug, sqlx::FromRow)]
struct MentionRow {
    id: Uuid,
    source_type: String,
    source_id: Uuid,
    body: String,
    author_id: Uuid,
    author_username: String,
    author_display_name: String,
    author_avatar_url: Option<String>,
    read_at: Option<chrono::DateTime<chrono::Utc>>,
    created_at: chrono::DateTime<chrono::Utc>,
}

pub fn validate_source_type(source_type: &str) -> Result<(), AppError> {
    if SOURCE_TYPES.contains(&source_type) {
        return Ok(());
    }
    Err(AppError::Validation(format!(
        "source_type must be one of: {}",
        SOURCE_TYPES.join(", ")
    )))
}

/// Front-end path for a mention's source.
pub fn source_url(source_type: &str, source_id: Uuid) -> String {
    match source_type {
        SOURCE_FORUM_POST => format!("/forum/{source_id}"),
        SOURCE_COMMENT => format!("/comments/{source_id}"),
        SOURCE_SLICE_DIARY => format!("/slices/diary/{source_id}"),
        SOURCE_MESSAGE => format!("/messages/{source_id}"),
        // Unknown type from a future migration: link nowhere rather than
        // fabricate a route.
        _ => String::new(),
    }
}

/// Strip the light markdown our bodies may contain, so an excerpt is
/// readable text rather than syntax.
///
/// Not a markdown parser: it removes the handful of markers that would
/// otherwise show up mid-sentence, and collapses whitespace. Anything more
/// would be a rendering decision, which belongs to the client.
fn to_plain_text(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut chars = body.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            // Inline code / emphasis markers.
            '`' | '*' | '_' | '~' => continue,
            // Image and link syntax: keep the label, drop the target.
            '!' if chars.peek() == Some(&'[') => continue,
            '[' => continue,
            ']' => {
                // Skip a following (...) target if present.
                if chars.peek() == Some(&'(') {
                    for c2 in chars.by_ref() {
                        if c2 == ')' {
                            break;
                        }
                    }
                }
                continue;
            }
            // Heading and quote markers only matter at line start; dropping
            // them everywhere is close enough for a 160-char excerpt.
            '#' | '>' => continue,
            _ => out.push(c),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Build an excerpt of roughly [`EXCERPT_CHARS`] centred on the first
/// mention of `username`, falling back to the start of the text.
///
/// Operates on chars, not bytes: slicing a UTF-8 body by byte offset would
/// panic mid-codepoint on any accented text, which is most of ours.
pub fn build_excerpt(body: &str, username: &str) -> String {
    let text = to_plain_text(body);
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= EXCERPT_CHARS {
        return text;
    }

    let needle = format!("@{}", username.to_lowercase());
    let lowered: String = text.to_lowercase();
    let hit_char_index = lowered.find(&needle).map(|byte_idx| {
        // Convert the byte offset from `find` into a char offset.
        lowered[..byte_idx].chars().count()
    });

    let half = EXCERPT_CHARS / 2;
    let start = match hit_char_index {
        Some(i) => i.saturating_sub(half),
        None => 0,
    };
    let end = (start + EXCERPT_CHARS).min(chars.len());
    let start = end.saturating_sub(EXCERPT_CHARS);

    let mut excerpt: String = chars[start..end].iter().collect();
    if start > 0 {
        excerpt.insert(0, '…');
    }
    if end < chars.len() {
        excerpt.push('…');
    }
    excerpt
}

/// Record mentions found in `body`, idempotently.
///
/// Returns the users newly mentioned by this call — an edit that adds one
/// `@username` returns just that user, so the caller can notify exactly
/// the people who have not been told yet.
///
/// Self-mentions are dropped, as are banned accounts and unknown handles.
pub async fn record(
    db: &PgPool,
    author_id: Uuid,
    source_type: &str,
    source_id: Uuid,
    body: &str,
) -> Result<Vec<Uuid>, AppError> {
    validate_source_type(source_type)?;

    let usernames = crate::services::social::parse_mentions(body);
    if usernames.is_empty() {
        return Ok(Vec::new());
    }

    let candidates: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM users
          WHERE LOWER(username) = ANY($1) AND is_banned = FALSE AND id <> $2",
    )
    .bind(&usernames)
    .bind(author_id)
    .fetch_all(db)
    .await?;

    let mut newly = Vec::new();
    for uid in candidates {
        // RETURNING yields nothing when the row already existed, which is
        // exactly the "already mentioned, do not notify again" signal.
        let inserted: Option<Uuid> = sqlx::query_scalar(
            r#"
            INSERT INTO mentions (mentioned_user_id, author_id, source_type, source_id)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (mentioned_user_id, source_type, source_id, author_id) DO NOTHING
            RETURNING id
            "#,
        )
        .bind(uid)
        .bind(author_id)
        .bind(source_type)
        .bind(source_id)
        .fetch_optional(db)
        .await?;
        if inserted.is_some() {
            newly.push(uid);
        }
    }
    Ok(newly)
}

/// Record mentions and notify the people newly named.
///
/// The writing surfaces (`forum`, `social`, `dm`, slice diary) are
/// service-layer functions holding only a `PgPool`, so this writes the
/// notification row directly rather than going through a full
/// [`crate::services::notify`] context, which additionally carries Redis and
/// the WebSocket manager. The durable row is what
/// `GET /api/notifications` reads, so nothing is lost — only the real-time
/// push, which a mention does not need to be useful.
///
/// Best-effort by design: a failure to notify must never fail the post,
/// comment or message that triggered it. Errors are logged and swallowed.
pub async fn record_and_notify(
    db: &PgPool,
    author_id: Uuid,
    source_type: &str,
    source_id: Uuid,
    body: &str,
) {
    let newly = match record(db, author_id, source_type, source_id, body).await {
        Ok(ids) => ids,
        Err(e) => {
            tracing::warn!(
                %source_type, %source_id, error = %e,
                "SKI-286: recording mentions failed"
            );
            return;
        }
    };
    if newly.is_empty() {
        return;
    }

    let author_username: String = sqlx::query_scalar("SELECT username FROM users WHERE id = $1")
        .bind(author_id)
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
        .unwrap_or_default();

    let url = source_url(source_type, source_id);
    for uid in newly {
        let payload = serde_json::json!({
            "source_type": source_type,
            "source_id": source_id,
            "source_url": url,
            "author_username": author_username,
        });
        let res = sqlx::query(
            r#"
            INSERT INTO notifications (user_id, notification_type, title, body, data)
            VALUES ($1, 'mention_received', $2, $3, $4)
            "#,
        )
        .bind(uid)
        .bind(format!("@{author_username} t'a mentionné"))
        .bind("Quelqu'un t'a cité dans une discussion.")
        .bind(&payload)
        .execute(db)
        .await;
        if let Err(e) = res {
            tracing::warn!(user_id = %uid, error = %e, "SKI-286: mention notification failed");
        }
    }
}

/// The visibility-filtered mention query.
///
/// One statement per source type, UNION-ed, because each carries its own
/// access rule and its own body column:
///
/// * `forum_post` / `comment` — public once not soft-deleted.
/// * `slice_diary` — public entries only, unless the reader is the author.
/// * `message` — direct messages: readable only by the two participants,
///   so a mention of a third party never surfaces.
async fn fetch_rows(
    db: &PgPool,
    user_id: Uuid,
    unread_only: bool,
    limit: i64,
    offset: i64,
) -> Result<Vec<MentionRow>, AppError> {
    let rows: Vec<MentionRow> = sqlx::query_as(
        r#"
        WITH visible AS (
            SELECT m.id, m.source_type, m.source_id, p.body, m.author_id,
                   m.read_at, m.created_at
              FROM mentions m
              JOIN posts p ON p.id = m.source_id
             WHERE m.mentioned_user_id = $1
               AND m.source_type = 'forum_post'
               AND p.deleted_at IS NULL

            UNION ALL

            SELECT m.id, m.source_type, m.source_id, c.body, m.author_id,
                   m.read_at, m.created_at
              FROM mentions m
              JOIN comments c ON c.id = m.source_id
             WHERE m.mentioned_user_id = $1
               AND m.source_type = 'comment'
               AND c.deleted_at IS NULL

            UNION ALL

            SELECT m.id, m.source_type, m.source_id, d.body_markdown, m.author_id,
                   m.read_at, m.created_at
              FROM mentions m
              JOIN slice_diary_entries d ON d.id = m.source_id
             WHERE m.mentioned_user_id = $1
               AND m.source_type = 'slice_diary'
               AND (d.is_public OR d.author_user_id = $1)

            UNION ALL

            SELECT m.id, m.source_type, m.source_id, dm.body, m.author_id,
                   m.read_at, m.created_at
              FROM mentions m
              JOIN dm_messages dm ON dm.id = m.source_id
              JOIN dm_conversations conv ON conv.id = dm.conversation_id
             WHERE m.mentioned_user_id = $1
               AND m.source_type = 'message'
               -- Only the two participants may see a direct message.
               AND (conv.user_a_id = $1 OR conv.user_b_id = $1)
        )
        SELECT v.id,
               v.source_type,
               v.source_id,
               v.body,
               v.author_id,
               u.username                                        AS author_username,
               COALESCE(NULLIF(u.display_name, ''), u.username)  AS author_display_name,
               u.avatar_url                                      AS author_avatar_url,
               v.read_at,
               v.created_at
          FROM visible v
          JOIN users u ON u.id = v.author_id
         WHERE (NOT $2::BOOLEAN OR v.read_at IS NULL)
           AND u.is_banned = FALSE
         ORDER BY v.created_at DESC, v.id DESC
         LIMIT $3 OFFSET $4
        "#,
    )
    .bind(user_id)
    .bind(unread_only)
    .bind(limit)
    .bind(offset)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// Count of visible mentions, for pagination.
///
/// Shares the visibility rules with [`fetch_rows`] by reusing it with a
/// wide window: the inbox is bounded by how often a person is named, which
/// stays small, and a second copy of the UNION would be one more place for
/// the confidentiality rules to drift out of sync.
pub async fn count_for_user(
    db: &PgPool,
    user_id: Uuid,
    unread_only: bool,
) -> Result<i64, AppError> {
    const COUNT_CEILING: i64 = 10_000;
    let rows = fetch_rows(db, user_id, unread_only, COUNT_CEILING, 0).await?;
    Ok(rows.len() as i64)
}

/// One page of the caller's mention inbox.
pub async fn list_for_user(
    db: &PgPool,
    user_id: Uuid,
    unread_only: bool,
    limit: i64,
    offset: i64,
) -> Result<Vec<Mention>, AppError> {
    let rows = fetch_rows(db, user_id, unread_only, limit, offset).await?;

    // The excerpt is centred on the reader's own handle, so resolve it once.
    let username: String = sqlx::query_scalar("SELECT username FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(db)
        .await?
        .unwrap_or_default();

    Ok(rows
        .into_iter()
        .map(|r| Mention {
            id: r.id,
            source_url: source_url(&r.source_type, r.source_id),
            excerpt: build_excerpt(&r.body, &username),
            source_type: r.source_type,
            source_id: r.source_id,
            author: MentionAuthor {
                user_id: r.author_id,
                username: r.author_username,
                display_name: r.author_display_name,
                avatar_url: r.author_avatar_url,
            },
            read_at: r.read_at,
            created_at: r.created_at,
        })
        .collect())
}

/// Mark one mention read. Idempotent: re-reading keeps the first
/// timestamp, so "when did I see this" stays meaningful.
pub async fn mark_read(
    db: &PgPool,
    user_id: Uuid,
    mention_id: Uuid,
) -> Result<chrono::DateTime<chrono::Utc>, AppError> {
    let read_at: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "UPDATE mentions
            SET read_at = COALESCE(read_at, NOW())
          WHERE id = $1 AND mentioned_user_id = $2
          RETURNING read_at",
    )
    .bind(mention_id)
    .bind(user_id)
    .fetch_optional(db)
    .await?;

    read_at.ok_or_else(|| AppError::NotFound(format!("mention {mention_id} not found")))
}

/// Mark every unread mention read. Returns how many were changed.
pub async fn mark_all_read(db: &PgPool, user_id: Uuid) -> Result<i64, AppError> {
    let marked = sqlx::query(
        "UPDATE mentions SET read_at = NOW()
          WHERE mentioned_user_id = $1 AND read_at IS NULL",
    )
    .bind(user_id)
    .execute(db)
    .await?
    .rows_affected();
    Ok(marked as i64)
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn source_types_are_validated() {
        for t in SOURCE_TYPES {
            assert!(validate_source_type(t).is_ok());
        }
        assert!(validate_source_type("tweet").is_err());
    }

    #[test]
    fn source_urls_are_built_per_type() {
        let id = Uuid::new_v4();
        assert_eq!(source_url(SOURCE_FORUM_POST, id), format!("/forum/{id}"));
        assert_eq!(source_url(SOURCE_COMMENT, id), format!("/comments/{id}"));
        assert_eq!(
            source_url(SOURCE_SLICE_DIARY, id),
            format!("/slices/diary/{id}")
        );
        assert_eq!(source_url(SOURCE_MESSAGE, id), format!("/messages/{id}"));
        assert_eq!(source_url("unknown", id), "");
    }

    #[test]
    fn plain_text_strips_markdown_noise() {
        assert_eq!(to_plain_text("**bold** and `code`"), "bold and code");
        assert_eq!(to_plain_text("# Title"), "Title");
        assert_eq!(to_plain_text("[label](http://x.test)"), "label");
        assert_eq!(to_plain_text("a\n\n  b   c"), "a b c");
    }

    #[test]
    fn short_bodies_are_returned_whole() {
        let body = "on devrait demander a @kofi de relire";
        assert_eq!(build_excerpt(body, "kofi"), body);
    }

    #[test]
    fn excerpt_centres_on_the_mention() {
        let filler = "x".repeat(400);
        let body = format!("{filler} on devrait demander a @kofi de relire {filler}");
        let excerpt = build_excerpt(&body, "kofi");
        assert!(
            excerpt.contains("@kofi"),
            "the excerpt must contain the mention it is centred on"
        );
        // Bounded, plus the two ellipsis characters.
        assert!(excerpt.chars().count() <= EXCERPT_CHARS + 2);
        assert!(excerpt.starts_with('…') && excerpt.ends_with('…'));
    }

    #[test]
    fn excerpt_falls_back_to_the_start_when_the_handle_is_absent() {
        let body = "y".repeat(400);
        let excerpt = build_excerpt(&body, "nobody");
        assert!(excerpt.chars().count() <= EXCERPT_CHARS + 2);
        assert!(
            !excerpt.starts_with('…'),
            "fallback starts at the beginning"
        );
    }

    #[test]
    fn excerpt_does_not_split_multibyte_characters() {
        // Slicing by byte offset would panic here; this is the common case
        // for French copy, not an edge case.
        let body = "é".repeat(400);
        let excerpt = build_excerpt(&body, "kofi");
        assert!(excerpt.chars().count() <= EXCERPT_CHARS + 2);
        assert!(excerpt.contains('é'));
    }

    #[test]
    fn excerpt_handles_a_mention_at_the_very_end() {
        let filler = "z".repeat(400);
        let body = format!("{filler} @kofi");
        let excerpt = build_excerpt(&body, "kofi");
        assert!(excerpt.contains("@kofi"));
        assert!(excerpt.chars().count() <= EXCERPT_CHARS + 2);
    }
}
