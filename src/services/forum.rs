//! Forum + Q&A service (Phase 2 Sprint 3).

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

pub const VALID_POST_KINDS: &[&str] = &["discussion", "question", "announcement"];

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ForumCategory {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub position: i32,
    pub locked: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Post {
    pub id: Uuid,
    pub category_id: Uuid,
    pub author_id: Uuid,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub bounty_fragments: i32,
    pub accepted_answer_id: Option<Uuid>,
    pub pinned: bool,
    pub locked: bool,
    pub view_count: i64,
    pub edited: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct PostListItem {
    pub id: Uuid,
    pub category_slug: String,
    pub author_id: Uuid,
    pub author_username: String,
    pub kind: String,
    pub title: String,
    pub bounty_fragments: i32,
    pub has_accepted_answer: bool,
    pub pinned: bool,
    pub locked: bool,
    pub view_count: i64,
    pub reply_count: i64,
    pub upvotes: i64,
    pub created_at: DateTime<Utc>,
}

// ─── Categories ───────────────────────────────────────────────────

pub async fn list_categories(db: &PgPool) -> Result<Vec<ForumCategory>, AppError> {
    let rows = sqlx::query_as("SELECT * FROM forum_categories ORDER BY position, name")
        .fetch_all(db)
        .await?;
    Ok(rows)
}

pub async fn get_category_by_slug(
    db: &PgPool,
    slug: &str,
) -> Result<ForumCategory, AppError> {
    let row: Option<ForumCategory> =
        sqlx::query_as("SELECT * FROM forum_categories WHERE slug = $1")
            .bind(slug)
            .fetch_optional(db)
            .await?;
    row.ok_or(AppError::NotFound("category not found".into()))
}

// ─── Posts ────────────────────────────────────────────────────────

pub struct CreatePostInput {
    pub category_id: Uuid,
    pub author_id: Uuid,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub bounty_fragments: i32,
}

pub async fn create_post(
    db: &PgPool,
    input: CreatePostInput,
    author_role: &str,
) -> Result<Post, AppError> {
    if !VALID_POST_KINDS.contains(&input.kind.as_str()) {
        return Err(AppError::Validation(format!(
            "kind must be one of {}",
            VALID_POST_KINDS.join(", ")
        )));
    }
    let title = input.title.trim();
    let body = input.body.trim();
    if title.len() < 3 || title.len() > 200 {
        return Err(AppError::Validation("Title must be 3-200 characters".into()));
    }
    if body.is_empty() || body.len() > 20_000 {
        return Err(AppError::Validation("Body must be 1-20000 characters".into()));
    }
    if input.kind == "announcement" && author_role != "admin" {
        return Err(AppError::Forbidden);
    }
    if input.bounty_fragments < 0 {
        return Err(AppError::Validation("bounty must be >= 0".into()));
    }
    if input.bounty_fragments > 0 && input.kind != "question" {
        return Err(AppError::Validation("bounty only on questions".into()));
    }
    // Reject posting in a locked category unless mod/admin
    let category: ForumCategory = sqlx::query_as(
        "SELECT * FROM forum_categories WHERE id = $1",
    )
    .bind(input.category_id)
    .fetch_optional(db)
    .await?
    .ok_or(AppError::NotFound("category not found".into()))?;
    let is_mod = author_role == "admin" || author_role == "mentor";
    if category.locked && !is_mod {
        return Err(AppError::Forbidden);
    }

    // If bounty > 0, atomically deduct from user's fragments.
    if input.bounty_fragments > 0 {
        let updated = sqlx::query(
            "UPDATE users SET total_fragments = total_fragments - $1 WHERE id = $2 AND total_fragments >= $1",
        )
        .bind(input.bounty_fragments)
        .bind(input.author_id)
        .execute(db)
        .await?;
        if updated.rows_affected() == 0 {
            return Err(AppError::Validation(
                "Insufficient fragments to set this bounty".into(),
            ));
        }
    }

    let post: Post = sqlx::query_as(
        r#"
        INSERT INTO posts (category_id, author_id, kind, title, body, bounty_fragments)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING *
        "#,
    )
    .bind(input.category_id)
    .bind(input.author_id)
    .bind(&input.kind)
    .bind(title)
    .bind(body)
    .bind(input.bounty_fragments)
    .fetch_one(db)
    .await?;

    Ok(post)
}

pub async fn get_post(db: &PgPool, id: Uuid) -> Result<Post, AppError> {
    let row: Option<Post> =
        sqlx::query_as("SELECT * FROM posts WHERE id = $1 AND deleted_at IS NULL")
            .bind(id)
            .fetch_optional(db)
            .await?;
    row.ok_or(AppError::NotFound("post not found".into()))
}

pub async fn increment_view_count(db: &PgPool, id: Uuid) {
    let _ = sqlx::query("UPDATE posts SET view_count = view_count + 1 WHERE id = $1")
        .bind(id)
        .execute(db)
        .await;
}

#[derive(Debug, Default, Clone)]
pub struct ListPostsFilters<'a> {
    pub category_slug: Option<&'a str>,
    pub kind: Option<&'a str>,
    pub sort: PostSort,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Clone, Copy, Default)]
pub enum PostSort {
    #[default]
    Recent,
    Hot,
    TopBounty,
}

pub async fn list_posts(
    db: &PgPool,
    filters: ListPostsFilters<'_>,
) -> Result<Vec<PostListItem>, AppError> {
    let order = match filters.sort {
        PostSort::Recent => "p.pinned DESC, p.created_at DESC",
        PostSort::Hot => "p.pinned DESC, (upvotes - downvotes) DESC, p.created_at DESC",
        PostSort::TopBounty => "p.bounty_fragments DESC, p.created_at DESC",
    };

    let sql = format!(
        r#"
        SELECT
            p.id,
            c.slug AS category_slug,
            p.author_id,
            u.username AS author_username,
            p.kind,
            p.title,
            p.bounty_fragments,
            (p.accepted_answer_id IS NOT NULL) AS has_accepted_answer,
            p.pinned,
            p.locked,
            p.view_count,
            COALESCE((SELECT COUNT(*) FROM comments WHERE target_type = 'post' AND target_id = p.id AND deleted_at IS NULL), 0)::BIGINT AS reply_count,
            COALESCE((SELECT COUNT(*) FROM reactions WHERE target_type = 'post' AND target_id = p.id AND kind = 'upvote'), 0)::BIGINT AS upvotes,
            COALESCE((SELECT COUNT(*) FROM reactions WHERE target_type = 'post' AND target_id = p.id AND kind = 'downvote'), 0)::BIGINT AS downvotes,
            p.created_at
        FROM posts p
        JOIN forum_categories c ON c.id = p.category_id
        JOIN users u ON u.id = p.author_id
        WHERE p.deleted_at IS NULL
          AND ($1::text IS NULL OR c.slug = $1)
          AND ($2::text IS NULL OR p.kind = $2)
        ORDER BY {order}
        LIMIT $3 OFFSET $4
        "#
    );

    let rows: Vec<(Uuid, String, Uuid, String, String, String, i32, bool, bool, bool, i64, i64, i64, i64, DateTime<Utc>)> =
        sqlx::query_as(&sql)
            .bind(filters.category_slug)
            .bind(filters.kind)
            .bind(filters.limit.max(1).min(100))
            .bind(filters.offset.max(0))
            .fetch_all(db)
            .await?;

    Ok(rows
        .into_iter()
        .map(|(id, slug, author_id, author_username, kind, title, bounty, has_ans, pinned, locked, view_count, reply_count, upvotes, _downvotes, created_at)| {
            PostListItem {
                id,
                category_slug: slug,
                author_id,
                author_username,
                kind,
                title,
                bounty_fragments: bounty,
                has_accepted_answer: has_ans,
                pinned,
                locked,
                view_count,
                reply_count,
                upvotes,
                created_at,
            }
        })
        .collect())
}

pub async fn edit_post(
    db: &PgPool,
    post_id: Uuid,
    requester_id: Uuid,
    requester_role: &str,
    new_title: &str,
    new_body: &str,
) -> Result<Post, AppError> {
    let post = get_post(db, post_id).await?;
    let is_mod = requester_role == "admin" || requester_role == "mentor";
    if !is_mod && post.author_id != requester_id {
        return Err(AppError::Forbidden);
    }
    let title = new_title.trim();
    let body = new_body.trim();
    if title.len() < 3 || title.len() > 200 {
        return Err(AppError::Validation("Title must be 3-200 characters".into()));
    }
    if body.is_empty() || body.len() > 20_000 {
        return Err(AppError::Validation("Body must be 1-20000 characters".into()));
    }
    let updated: Post = sqlx::query_as(
        r#"
        UPDATE posts SET title = $1, body = $2, edited = TRUE, updated_at = NOW()
        WHERE id = $3 RETURNING *
        "#,
    )
    .bind(title)
    .bind(body)
    .bind(post_id)
    .fetch_one(db)
    .await?;
    Ok(updated)
}

pub async fn delete_post(
    db: &PgPool,
    post_id: Uuid,
    requester_id: Uuid,
    requester_role: &str,
) -> Result<(), AppError> {
    let post = get_post(db, post_id).await?;
    let is_mod = requester_role == "admin" || requester_role == "mentor";
    if !is_mod && post.author_id != requester_id {
        return Err(AppError::Forbidden);
    }
    sqlx::query("UPDATE posts SET deleted_at = NOW() WHERE id = $1")
        .bind(post_id)
        .execute(db)
        .await?;
    Ok(())
}

pub async fn set_pinned(
    db: &PgPool,
    post_id: Uuid,
    requester_role: &str,
    pinned: bool,
) -> Result<(), AppError> {
    if requester_role != "admin" && requester_role != "mentor" {
        return Err(AppError::Forbidden);
    }
    sqlx::query("UPDATE posts SET pinned = $1, updated_at = NOW() WHERE id = $2")
        .bind(pinned)
        .bind(post_id)
        .execute(db)
        .await?;
    Ok(())
}

pub async fn set_locked(
    db: &PgPool,
    post_id: Uuid,
    requester_role: &str,
    locked: bool,
) -> Result<(), AppError> {
    if requester_role != "admin" && requester_role != "mentor" {
        return Err(AppError::Forbidden);
    }
    sqlx::query("UPDATE posts SET locked = $1, updated_at = NOW() WHERE id = $2")
        .bind(locked)
        .bind(post_id)
        .execute(db)
        .await?;
    Ok(())
}

// ─── Q&A acceptance + bounty transfer ─────────────────────────────

pub struct AcceptAnswerResult {
    pub answer_id: Uuid,
    pub answer_author_id: Uuid,
    pub bounty_transferred: i32,
}

pub async fn accept_answer(
    db: &PgPool,
    requester_id: Uuid,
    post_id: Uuid,
    answer_comment_id: Uuid,
) -> Result<AcceptAnswerResult, AppError> {
    let post = get_post(db, post_id).await?;
    if post.kind != "question" {
        return Err(AppError::Validation(
            "Only questions can have an accepted answer".into(),
        ));
    }
    if post.author_id != requester_id {
        return Err(AppError::Forbidden);
    }
    if post.accepted_answer_id.is_some() {
        return Err(AppError::Validation(
            "An answer is already accepted for this question".into(),
        ));
    }

    // Verify the comment belongs to this post and is not deleted
    let comment_row: Option<(Uuid, String, Uuid)> = sqlx::query_as(
        "SELECT author_id, target_type, target_id FROM comments WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(answer_comment_id)
    .fetch_optional(db)
    .await?;
    let (answer_author_id, target_type, target_id) =
        comment_row.ok_or(AppError::NotFound("answer comment not found".into()))?;
    if target_type != "post" || target_id != post_id {
        return Err(AppError::Validation(
            "Comment does not belong to this post".into(),
        ));
    }
    if answer_author_id == requester_id {
        return Err(AppError::Validation(
            "Cannot accept your own answer".into(),
        ));
    }

    let mut tx = db.begin().await?;
    sqlx::query("UPDATE posts SET accepted_answer_id = $1, updated_at = NOW() WHERE id = $2")
        .bind(answer_comment_id)
        .bind(post_id)
        .execute(&mut *tx)
        .await?;

    let bounty = post.bounty_fragments;
    if bounty > 0 {
        sqlx::query("UPDATE users SET total_fragments = total_fragments + $1 WHERE id = $2")
            .bind(bounty)
            .bind(answer_author_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE posts SET bounty_fragments = 0 WHERE id = $1")
            .bind(post_id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;

    Ok(AcceptAnswerResult {
        answer_id: answer_comment_id,
        answer_author_id,
        bounty_transferred: bounty,
    })
}

// ─── FTS search ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct SearchHit {
    pub id: Uuid,
    pub kind: String,
    pub title: String,
    pub snippet: String,
    pub category_slug: String,
    pub rank: f32,
    pub created_at: DateTime<Utc>,
}

pub async fn search_posts(
    db: &PgPool,
    query: &str,
    limit: i64,
) -> Result<Vec<SearchHit>, AppError> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    // Build a tsquery from whitespace-separated terms, OR'd then AND'd
    let tsquery_str = trimmed
        .split_whitespace()
        .filter(|w| !w.is_empty())
        .map(|w| w.replace('\'', " "))
        .collect::<Vec<_>>()
        .join(" & ");
    if tsquery_str.is_empty() {
        return Ok(Vec::new());
    }

    let rows: Vec<(Uuid, String, String, String, String, f32, DateTime<Utc>)> = sqlx::query_as(
        r#"
        WITH q AS (SELECT to_tsquery('simple', $1) AS tq)
        SELECT
            p.id,
            p.kind,
            p.title,
            ts_headline('simple', p.body, q.tq, 'MaxFragments=2, MaxWords=20, MinWords=5') AS snippet,
            c.slug AS category_slug,
            ts_rank(p.search_vector, q.tq) AS rank,
            p.created_at
        FROM posts p
        JOIN forum_categories c ON c.id = p.category_id
        CROSS JOIN q
        WHERE p.search_vector @@ q.tq AND p.deleted_at IS NULL
        ORDER BY rank DESC, p.created_at DESC
        LIMIT $2
        "#,
    )
    .bind(&tsquery_str)
    .bind(limit.clamp(1, 50))
    .fetch_all(db)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(id, kind, title, snippet, category_slug, rank, created_at)| SearchHit {
            id,
            kind,
            title,
            snippet,
            category_slug,
            rank,
            created_at,
        })
        .collect())
}

// ─── Rate-limit policy for Q&A questions ──────────────────────────

/// Tier-based daily limit for posting questions.
/// Returns (limit_per_day, window_secs). 0 limit = unlimited.
pub fn question_rate_limit_for_title(title: &str) -> (u64, u64) {
    match title {
        "apprenti" => (3, 86_400),
        "artisan" => (10, 86_400),
        _ => (0, 86_400), // maitre+ : unlimited (skip the check)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limit_tiers() {
        assert_eq!(question_rate_limit_for_title("apprenti").0, 3);
        assert_eq!(question_rate_limit_for_title("artisan").0, 10);
        assert_eq!(question_rate_limit_for_title("maitre").0, 0);
        assert_eq!(question_rate_limit_for_title("legende").0, 0);
    }

    #[test]
    fn post_kinds_validated() {
        assert!(VALID_POST_KINDS.contains(&"discussion"));
        assert!(VALID_POST_KINDS.contains(&"question"));
        assert!(VALID_POST_KINDS.contains(&"announcement"));
        assert_eq!(VALID_POST_KINDS.len(), 3);
    }
}
