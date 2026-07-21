//! Social primitives (Phase 2 Sprint 1) — comments, reactions, tags, mentions.
//!
//! All entities are **polymorphic** : they attach to any other entity via
//! `(target_type, target_id)`. The list of valid target types is enforced in code,
//! not at the DB level, so adding a new target type is one constant edit.

use std::collections::HashSet;

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

/// Edit window: after this many seconds, a comment becomes immutable for the author.
/// Mods/admins keep the right to delete anytime via the moderation routes.
pub const COMMENT_EDIT_WINDOW_SECONDS: i64 = 3600; // 1 hour

/// Authoritative list of polymorphic target types. Adding a new one ?
/// 1. Add the literal here
/// 2. Pass it from your handler
pub const VALID_TARGET_TYPES: &[&str] = &[
    "challenge",
    "submission",
    "post",     // forum / Q&A post (Sprint 3)
    "question", // Q&A (Sprint 3)
    "answer",   // Q&A answer (Sprint 3)
    "project",  // OSS project (Sprint 5)
    "profile",  // user profile comment
    "guild",    // guild discussion (Sprint 4)
    "comment",  // nested reply via parent_id, but also reactions on comments
    "repo",     // GitHub repo card (Sprint 5)
];

pub const VALID_REACTION_KINDS: &[&str] = &["upvote", "downvote", "heart", "fire", "wow"];

pub fn validate_target_type(target_type: &str) -> Result<(), AppError> {
    if !VALID_TARGET_TYPES.contains(&target_type) {
        return Err(AppError::Validation(format!(
            "Unknown target_type '{target_type}'. Allowed: {}",
            VALID_TARGET_TYPES.join(", ")
        )));
    }
    Ok(())
}

pub fn validate_reaction_kind(kind: &str) -> Result<(), AppError> {
    if !VALID_REACTION_KINDS.contains(&kind) {
        return Err(AppError::Validation(format!(
            "Unknown reaction kind '{kind}'. Allowed: {}",
            VALID_REACTION_KINDS.join(", ")
        )));
    }
    Ok(())
}

// ─── Comments ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Comment {
    pub id: Uuid,
    pub target_type: String,
    pub target_id: Uuid,
    pub author_id: Uuid,
    pub body: String,
    pub parent_id: Option<Uuid>,
    pub edited: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

pub async fn create_comment(
    db: &PgPool,
    author_id: Uuid,
    target_type: &str,
    target_id: Uuid,
    body: &str,
    parent_id: Option<Uuid>,
) -> Result<Comment, AppError> {
    validate_target_type(target_type)?;
    validate_comment_body(body)?;

    // If parent_id is set, ensure it's on the same target (replies stay in their thread).
    if let Some(parent_id) = parent_id {
        let parent: Option<(String, Uuid)> = sqlx::query_as(
            "SELECT target_type, target_id FROM comments WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(parent_id)
        .fetch_optional(db)
        .await?;
        let Some((ptype, ptarget)) = parent else {
            return Err(AppError::Validation(
                "parent_id refers to a missing or deleted comment".into(),
            ));
        };
        if ptype != target_type || ptarget != target_id {
            return Err(AppError::Validation(
                "reply target must match its parent's target".into(),
            ));
        }
    }

    let comment: Comment = sqlx::query_as(
        r#"
        INSERT INTO comments (target_type, target_id, author_id, body, parent_id)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING *
        "#,
    )
    .bind(target_type)
    .bind(target_id)
    .bind(author_id)
    .bind(body.trim())
    .bind(parent_id)
    .fetch_one(db)
    .await?;

    Ok(comment)
}

pub async fn list_comments(
    db: &PgPool,
    target_type: &str,
    target_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<Vec<Comment>, AppError> {
    validate_target_type(target_type)?;
    let rows: Vec<Comment> = sqlx::query_as(
        r#"
        SELECT * FROM comments
        WHERE target_type = $1 AND target_id = $2 AND deleted_at IS NULL
        ORDER BY created_at ASC
        LIMIT $3 OFFSET $4
        "#,
    )
    .bind(target_type)
    .bind(target_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

pub async fn edit_comment(
    db: &PgPool,
    comment_id: Uuid,
    requester_id: Uuid,
    requester_role: &str,
    new_body: &str,
) -> Result<Comment, AppError> {
    validate_comment_body(new_body)?;
    let existing: Option<Comment> =
        sqlx::query_as("SELECT * FROM comments WHERE id = $1 AND deleted_at IS NULL")
            .bind(comment_id)
            .fetch_optional(db)
            .await?;
    let existing = existing.ok_or(AppError::NotFound("comment not found".into()))?;

    let is_mod = requester_role == "admin" || requester_role == "mentor";
    if !is_mod {
        if existing.author_id != requester_id {
            return Err(AppError::Forbidden);
        }
        if Utc::now() - existing.created_at > ChronoDuration::seconds(COMMENT_EDIT_WINDOW_SECONDS) {
            return Err(AppError::Validation(
                "Edit window expired (1 hour after creation)".into(),
            ));
        }
    }

    let updated: Comment = sqlx::query_as(
        r#"
        UPDATE comments
        SET body = $1, edited = TRUE, updated_at = NOW()
        WHERE id = $2
        RETURNING *
        "#,
    )
    .bind(new_body.trim())
    .bind(comment_id)
    .fetch_one(db)
    .await?;
    Ok(updated)
}

pub async fn delete_comment(
    db: &PgPool,
    comment_id: Uuid,
    requester_id: Uuid,
    requester_role: &str,
) -> Result<(), AppError> {
    let existing: Option<(Uuid,)> =
        sqlx::query_as("SELECT author_id FROM comments WHERE id = $1 AND deleted_at IS NULL")
            .bind(comment_id)
            .fetch_optional(db)
            .await?;
    let Some((author_id,)) = existing else {
        return Err(AppError::NotFound("comment not found".into()));
    };
    let is_mod = requester_role == "admin" || requester_role == "mentor";
    if !is_mod && author_id != requester_id {
        return Err(AppError::Forbidden);
    }
    sqlx::query("UPDATE comments SET deleted_at = NOW() WHERE id = $1")
        .bind(comment_id)
        .execute(db)
        .await?;
    Ok(())
}

fn validate_comment_body(body: &str) -> Result<(), AppError> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return Err(AppError::Validation("Comment body is empty".into()));
    }
    if trimmed.len() > 4000 {
        return Err(AppError::Validation(
            "Comment body must be at most 4000 characters".into(),
        ));
    }
    Ok(())
}

// ─── Reactions ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ReactionSummaryRow {
    pub kind: String,
    pub count: i64,
}

/// Toggle a reaction: if the user already reacted with this kind, remove it ; else insert.
/// Returns true if the reaction is now active, false if it was just removed.
pub async fn toggle_reaction(
    db: &PgPool,
    user_id: Uuid,
    target_type: &str,
    target_id: Uuid,
    kind: &str,
) -> Result<bool, AppError> {
    validate_target_type(target_type)?;
    validate_reaction_kind(kind)?;

    let deleted = sqlx::query(
        "DELETE FROM reactions WHERE target_type = $1 AND target_id = $2 AND user_id = $3 AND kind = $4",
    )
    .bind(target_type)
    .bind(target_id)
    .bind(user_id)
    .bind(kind)
    .execute(db)
    .await?
    .rows_affected();

    if deleted > 0 {
        return Ok(false);
    }
    sqlx::query(
        "INSERT INTO reactions (target_type, target_id, user_id, kind) VALUES ($1, $2, $3, $4)",
    )
    .bind(target_type)
    .bind(target_id)
    .bind(user_id)
    .bind(kind)
    .execute(db)
    .await?;
    Ok(true)
}

pub async fn reactions_summary(
    db: &PgPool,
    target_type: &str,
    target_id: Uuid,
) -> Result<Vec<ReactionSummaryRow>, AppError> {
    validate_target_type(target_type)?;
    let rows = sqlx::query_as(
        r#"
        SELECT kind, COUNT(*)::BIGINT as count
        FROM reactions
        WHERE target_type = $1 AND target_id = $2
        GROUP BY kind
        ORDER BY count DESC
        "#,
    )
    .bind(target_type)
    .bind(target_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

pub async fn user_reactions_for_target(
    db: &PgPool,
    user_id: Uuid,
    target_type: &str,
    target_id: Uuid,
) -> Result<Vec<String>, AppError> {
    validate_target_type(target_type)?;
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT kind FROM reactions WHERE target_type = $1 AND target_id = $2 AND user_id = $3",
    )
    .bind(target_type)
    .bind(target_id)
    .bind(user_id)
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(|(k,)| k).collect())
}

// ─── Mentions ─────────────────────────────────────────────────────

/// Extract @username mentions from a body. Returns deduplicated usernames (lowercased).
/// Pattern: `@` followed by 3-30 alphanumeric / `_-`.
pub fn parse_mentions(body: &str) -> Vec<String> {
    let mut found = HashSet::new();
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'@' {
            // Must not be preceded by alphanumeric (avoid emails like a@b.c)
            let preceded_by_word_char = i > 0
                && (bytes[i - 1].is_ascii_alphanumeric()
                    || bytes[i - 1] == b'_'
                    || bytes[i - 1] == b'-');
            if preceded_by_word_char {
                i += 1;
                continue;
            }
            let mut j = i + 1;
            while j < bytes.len()
                && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_' || bytes[j] == b'-')
            {
                j += 1;
            }
            let username = &body[i + 1..j];
            if username.len() >= 3 && username.len() <= 30 {
                found.insert(username.to_ascii_lowercase());
            }
            i = j.max(i + 1);
        } else {
            i += 1;
        }
    }
    let mut result: Vec<String> = found.into_iter().collect();
    result.sort();
    result
}

/// Resolve mentioned usernames to user IDs and insert mentions rows.
/// Returns the list of UUIDs that were actually mentioned (and exist as users).
pub async fn record_mentions(
    db: &PgPool,
    author_id: Uuid,
    source_type: &str,
    source_id: Uuid,
    usernames: &[String],
) -> Result<Vec<Uuid>, AppError> {
    if usernames.is_empty() {
        return Ok(Vec::new());
    }
    let rows: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM users WHERE LOWER(username) = ANY($1) AND is_banned = FALSE",
    )
    .bind(usernames)
    .fetch_all(db)
    .await?;
    let user_ids: Vec<Uuid> = rows
        .into_iter()
        .map(|(id,)| id)
        .filter(|id| id != &author_id)
        .collect();
    if user_ids.is_empty() {
        return Ok(Vec::new());
    }
    for uid in &user_ids {
        sqlx::query(
            "INSERT INTO mentions (mentioned_user_id, author_id, source_type, source_id) VALUES ($1, $2, $3, $4)",
        )
        .bind(uid)
        .bind(author_id)
        .bind(source_type)
        .bind(source_id)
        .execute(db)
        .await?;
    }
    Ok(user_ids)
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct MentionRow {
    pub id: Uuid,
    pub mentioned_user_id: Uuid,
    pub author_id: Uuid,
    pub source_type: String,
    pub source_id: Uuid,
    pub created_at: DateTime<Utc>,
}

pub async fn list_mentions_for_user(
    db: &PgPool,
    user_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<Vec<MentionRow>, AppError> {
    let rows = sqlx::query_as(
        r#"
        SELECT id, mentioned_user_id, author_id, source_type, source_id, created_at
        FROM mentions
        WHERE mentioned_user_id = $1
        ORDER BY created_at DESC
        LIMIT $2 OFFSET $3
        "#,
    )
    .bind(user_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

// ─── Tags ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Tag {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub category: String,
    pub color: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateTagInput {
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub category: String,
    pub color: Option<String>,
}

pub const VALID_TAG_CATEGORIES: &[&str] =
    &["language", "topic", "level", "framework", "tool", "other"];

pub async fn list_tags(db: &PgPool, category_filter: Option<&str>) -> Result<Vec<Tag>, AppError> {
    let rows = if let Some(category) = category_filter {
        sqlx::query_as("SELECT * FROM tags WHERE category = $1 ORDER BY name")
            .bind(category)
            .fetch_all(db)
            .await?
    } else {
        sqlx::query_as("SELECT * FROM tags ORDER BY category, name")
            .fetch_all(db)
            .await?
    };
    Ok(rows)
}

pub async fn create_tag(db: &PgPool, input: CreateTagInput) -> Result<Tag, AppError> {
    if !VALID_TAG_CATEGORIES.contains(&input.category.as_str()) {
        return Err(AppError::Validation(format!(
            "category must be one of: {}",
            VALID_TAG_CATEGORIES.join(", ")
        )));
    }
    let slug = input.slug.trim().to_lowercase();
    if !slug
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(AppError::Validation(
            "slug must be alphanumeric / - / _".into(),
        ));
    }
    let tag: Tag = sqlx::query_as(
        r#"
        INSERT INTO tags (slug, name, description, category, color)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING *
        "#,
    )
    .bind(&slug)
    .bind(input.name.trim())
    .bind(input.description.as_deref().map(str::trim))
    .bind(&input.category)
    .bind(input.color.as_deref())
    .fetch_one(db)
    .await?;
    Ok(tag)
}

pub async fn attach_tag(
    db: &PgPool,
    tag_id: Uuid,
    target_type: &str,
    target_id: Uuid,
    attached_by: Uuid,
) -> Result<(), AppError> {
    validate_target_type(target_type)?;
    sqlx::query(
        r#"
        INSERT INTO tag_map (tag_id, target_type, target_id, attached_by)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (tag_id, target_type, target_id) DO NOTHING
        "#,
    )
    .bind(tag_id)
    .bind(target_type)
    .bind(target_id)
    .bind(attached_by)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn detach_tag(
    db: &PgPool,
    tag_id: Uuid,
    target_type: &str,
    target_id: Uuid,
) -> Result<(), AppError> {
    validate_target_type(target_type)?;
    sqlx::query("DELETE FROM tag_map WHERE tag_id = $1 AND target_type = $2 AND target_id = $3")
        .bind(tag_id)
        .bind(target_type)
        .bind(target_id)
        .execute(db)
        .await?;
    Ok(())
}

pub async fn tags_for_target(
    db: &PgPool,
    target_type: &str,
    target_id: Uuid,
) -> Result<Vec<Tag>, AppError> {
    validate_target_type(target_type)?;
    let rows = sqlx::query_as(
        r#"
        SELECT t.* FROM tags t
        JOIN tag_map m ON m.tag_id = t.id
        WHERE m.target_type = $1 AND m.target_id = $2
        ORDER BY t.category, t.name
        "#,
    )
    .bind(target_type)
    .bind(target_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

// ─── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_mentions_basic() {
        let body = "Hey @alice and @bob what do you think? Also @alice again.";
        let m = parse_mentions(body);
        assert_eq!(m, vec!["alice", "bob"]);
    }

    #[test]
    fn parse_mentions_ignores_emails() {
        let body = "Contact me at user@example.com or ping @charlie";
        let m = parse_mentions(body);
        assert_eq!(m, vec!["charlie"]);
    }

    #[test]
    fn parse_mentions_respects_length() {
        let body = "@ab too short, @abc just right, @aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa (too long, >30 chars)";
        let m = parse_mentions(body);
        assert_eq!(m, vec!["abc"]);
    }

    #[test]
    fn parse_mentions_handles_dashes_and_underscores() {
        let m = parse_mentions("ping @user-name and @user_name and @camelCase42");
        assert_eq!(m, vec!["camelcase42", "user-name", "user_name"]);
    }

    #[test]
    fn parse_mentions_empty() {
        assert!(parse_mentions("no mention here").is_empty());
        assert!(parse_mentions("").is_empty());
    }

    #[test]
    fn validate_target_type_accepts_known() {
        assert!(validate_target_type("challenge").is_ok());
        assert!(validate_target_type("profile").is_ok());
    }

    #[test]
    fn validate_target_type_rejects_unknown() {
        assert!(validate_target_type("random_thing").is_err());
    }

    #[test]
    fn validate_reaction_kind_accepts_known() {
        assert!(validate_reaction_kind("upvote").is_ok());
        assert!(validate_reaction_kind("fire").is_ok());
    }

    #[test]
    fn validate_reaction_kind_rejects_unknown() {
        assert!(validate_reaction_kind("explosion").is_err());
    }

    #[test]
    fn validate_comment_body_rules() {
        assert!(validate_comment_body("   ").is_err());
        assert!(validate_comment_body("ok").is_ok());
        let long = "x".repeat(4001);
        assert!(validate_comment_body(&long).is_err());
    }
}
