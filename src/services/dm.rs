//! Direct messaging talent↔talent + user-to-user blocks (Phase 2 Sprint 2).
//!
//! Convention: every `dm_conversations` row stores `(user_a_id, user_b_id)` with
//! `user_a_id < user_b_id`. Always pass the pair through [`canonical_pair`] before
//! a lookup or insert.

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct DmConversation {
    pub id: Uuid,
    pub user_a_id: Uuid,
    pub user_b_id: Uuid,
    pub last_message_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct DmMessage {
    pub id: Uuid,
    pub conversation_id: Uuid,
    pub sender_id: Uuid,
    pub body: String,
    pub read_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConversationSummary {
    pub conversation: DmConversation,
    pub peer_id: Uuid,
    pub unread_count: i64,
    pub last_message_body: Option<String>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct UserBlock {
    pub blocker_id: Uuid,
    pub blocked_id: Uuid,
    pub reason: Option<String>,
    pub created_at: DateTime<Utc>,
}

pub fn canonical_pair(a: Uuid, b: Uuid) -> (Uuid, Uuid) {
    if a < b { (a, b) } else { (b, a) }
}

pub fn peer_of(conv: &DmConversation, me: Uuid) -> Uuid {
    if conv.user_a_id == me { conv.user_b_id } else { conv.user_a_id }
}

pub async fn is_blocked_either_way(
    db: &PgPool,
    a: Uuid,
    b: Uuid,
) -> Result<bool, AppError> {
    let exists: Option<(i32,)> = sqlx::query_as(
        r#"
        SELECT 1 FROM user_blocks
        WHERE (blocker_id = $1 AND blocked_id = $2)
           OR (blocker_id = $2 AND blocked_id = $1)
        LIMIT 1
        "#,
    )
    .bind(a)
    .bind(b)
    .fetch_optional(db)
    .await?;
    Ok(exists.is_some())
}

pub async fn open_or_get_conversation(
    db: &PgPool,
    me: Uuid,
    peer: Uuid,
) -> Result<DmConversation, AppError> {
    if me == peer {
        return Err(AppError::Validation(
            "Cannot start a conversation with yourself".into(),
        ));
    }
    if is_blocked_either_way(db, me, peer).await? {
        return Err(AppError::Forbidden);
    }
    // Validate peer exists and is not banned
    let peer_ok: Option<(bool,)> =
        sqlx::query_as("SELECT is_banned FROM users WHERE id = $1")
            .bind(peer)
            .fetch_optional(db)
            .await?;
    match peer_ok {
        Some((true,)) => return Err(AppError::Forbidden),
        None => return Err(AppError::NotFound("peer user not found".into())),
        _ => {}
    }

    let (a, b) = canonical_pair(me, peer);
    let conv: DmConversation = sqlx::query_as(
        r#"
        INSERT INTO dm_conversations (user_a_id, user_b_id)
        VALUES ($1, $2)
        ON CONFLICT (user_a_id, user_b_id) DO UPDATE SET last_message_at = dm_conversations.last_message_at
        RETURNING *
        "#,
    )
    .bind(a)
    .bind(b)
    .fetch_one(db)
    .await?;
    Ok(conv)
}

pub async fn send_message(
    db: &PgPool,
    sender_id: Uuid,
    conversation_id: Uuid,
    body: &str,
) -> Result<(DmMessage, Uuid), AppError> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return Err(AppError::Validation("Message body is empty".into()));
    }
    if trimmed.len() > 4000 {
        return Err(AppError::Validation(
            "Message body must be at most 4000 characters".into(),
        ));
    }

    let conv: Option<DmConversation> =
        sqlx::query_as("SELECT * FROM dm_conversations WHERE id = $1")
            .bind(conversation_id)
            .fetch_optional(db)
            .await?;
    let conv = conv.ok_or(AppError::NotFound("conversation not found".into()))?;

    if conv.user_a_id != sender_id && conv.user_b_id != sender_id {
        return Err(AppError::Forbidden);
    }
    let peer = peer_of(&conv, sender_id);
    if is_blocked_either_way(db, sender_id, peer).await? {
        return Err(AppError::Forbidden);
    }

    let message: DmMessage = sqlx::query_as(
        r#"
        INSERT INTO dm_messages (conversation_id, sender_id, body)
        VALUES ($1, $2, $3)
        RETURNING *
        "#,
    )
    .bind(conversation_id)
    .bind(sender_id)
    .bind(trimmed)
    .fetch_one(db)
    .await?;

    sqlx::query("UPDATE dm_conversations SET last_message_at = NOW() WHERE id = $1")
        .bind(conversation_id)
        .execute(db)
        .await?;

    Ok((message, peer))
}

pub async fn list_messages(
    db: &PgPool,
    me: Uuid,
    conversation_id: Uuid,
    limit: i64,
    before: Option<DateTime<Utc>>,
) -> Result<Vec<DmMessage>, AppError> {
    ensure_participant(db, me, conversation_id).await?;
    let limit = limit.clamp(1, 200);
    let rows = if let Some(before) = before {
        sqlx::query_as(
            r#"
            SELECT * FROM dm_messages
            WHERE conversation_id = $1 AND created_at < $2
            ORDER BY created_at DESC
            LIMIT $3
            "#,
        )
        .bind(conversation_id)
        .bind(before)
        .bind(limit)
        .fetch_all(db)
        .await?
    } else {
        sqlx::query_as(
            r#"
            SELECT * FROM dm_messages
            WHERE conversation_id = $1
            ORDER BY created_at DESC
            LIMIT $2
            "#,
        )
        .bind(conversation_id)
        .bind(limit)
        .fetch_all(db)
        .await?
    };
    Ok(rows)
}

pub async fn mark_conversation_read(
    db: &PgPool,
    me: Uuid,
    conversation_id: Uuid,
) -> Result<u64, AppError> {
    ensure_participant(db, me, conversation_id).await?;
    let res = sqlx::query(
        r#"
        UPDATE dm_messages SET read_at = NOW()
        WHERE conversation_id = $1 AND sender_id <> $2 AND read_at IS NULL
        "#,
    )
    .bind(conversation_id)
    .bind(me)
    .execute(db)
    .await?;
    Ok(res.rows_affected())
}

pub async fn list_conversations(
    db: &PgPool,
    me: Uuid,
    limit: i64,
    offset: i64,
) -> Result<Vec<ConversationSummary>, AppError> {
    let limit = limit.clamp(1, 100);
    let rows: Vec<(Uuid, Uuid, Uuid, DateTime<Utc>, DateTime<Utc>, Option<String>, i64)> =
        sqlx::query_as(
            r#"
            SELECT c.id, c.user_a_id, c.user_b_id, c.last_message_at, c.created_at,
                   (SELECT body FROM dm_messages WHERE conversation_id = c.id ORDER BY created_at DESC LIMIT 1) AS last_body,
                   COALESCE((SELECT COUNT(*) FROM dm_messages WHERE conversation_id = c.id AND sender_id <> $1 AND read_at IS NULL), 0)::BIGINT AS unread
            FROM dm_conversations c
            WHERE c.user_a_id = $1 OR c.user_b_id = $1
            ORDER BY c.last_message_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(me)
        .bind(limit)
        .bind(offset)
        .fetch_all(db)
        .await?;

    let out = rows
        .into_iter()
        .map(|(id, a, b, last_message_at, created_at, last_body, unread)| {
            let conv = DmConversation {
                id,
                user_a_id: a,
                user_b_id: b,
                last_message_at,
                created_at,
            };
            let peer_id = peer_of(&conv, me);
            ConversationSummary {
                conversation: conv,
                peer_id,
                unread_count: unread,
                last_message_body: last_body,
            }
        })
        .collect();
    Ok(out)
}

pub async fn block_user(
    db: &PgPool,
    blocker_id: Uuid,
    blocked_id: Uuid,
    reason: Option<&str>,
) -> Result<(), AppError> {
    if blocker_id == blocked_id {
        return Err(AppError::Validation("Cannot block yourself".into()));
    }
    sqlx::query(
        r#"
        INSERT INTO user_blocks (blocker_id, blocked_id, reason)
        VALUES ($1, $2, $3)
        ON CONFLICT (blocker_id, blocked_id) DO UPDATE SET reason = EXCLUDED.reason, created_at = NOW()
        "#,
    )
    .bind(blocker_id)
    .bind(blocked_id)
    .bind(reason)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn unblock_user(
    db: &PgPool,
    blocker_id: Uuid,
    blocked_id: Uuid,
) -> Result<(), AppError> {
    sqlx::query("DELETE FROM user_blocks WHERE blocker_id = $1 AND blocked_id = $2")
        .bind(blocker_id)
        .bind(blocked_id)
        .execute(db)
        .await?;
    Ok(())
}

pub async fn list_blocks(db: &PgPool, blocker_id: Uuid) -> Result<Vec<UserBlock>, AppError> {
    let rows = sqlx::query_as(
        "SELECT * FROM user_blocks WHERE blocker_id = $1 ORDER BY created_at DESC",
    )
    .bind(blocker_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

async fn ensure_participant(
    db: &PgPool,
    me: Uuid,
    conversation_id: Uuid,
) -> Result<(), AppError> {
    let row: Option<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT user_a_id, user_b_id FROM dm_conversations WHERE id = $1",
    )
    .bind(conversation_id)
    .fetch_optional(db)
    .await?;
    match row {
        Some((a, b)) if a == me || b == me => Ok(()),
        Some(_) => Err(AppError::Forbidden),
        None => Err(AppError::NotFound("conversation not found".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_pair_sorts() {
        let a = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
        let b = Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap();
        assert_eq!(canonical_pair(a, b), (a, b));
        assert_eq!(canonical_pair(b, a), (a, b));
    }

    #[test]
    fn peer_of_works_either_side() {
        let a = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
        let b = Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap();
        let conv = DmConversation {
            id: Uuid::new_v4(),
            user_a_id: a,
            user_b_id: b,
            last_message_at: Utc::now(),
            created_at: Utc::now(),
        };
        assert_eq!(peer_of(&conv, a), b);
        assert_eq!(peer_of(&conv, b), a);
    }
}
