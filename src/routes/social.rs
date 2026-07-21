//! Social primitives endpoints (Phase 2 Sprint 1).
//!
//! Routes:
//!   POST   /api/social/comments
//!   GET    /api/social/comments/{target_type}/{target_id}
//!   PUT    /api/social/comments/{id}
//!   DELETE /api/social/comments/{id}
//!   POST   /api/social/reactions
//!   GET    /api/social/reactions/{target_type}/{target_id}/summary
//!   GET    /api/social/mentions/me
//!   GET    /api/tags
//!   GET    /api/social/tag-map/{target_type}/{target_id}
//!   POST   /api/social/tag-map
//!   DELETE /api/social/tag-map
//!   POST   /api/admin/tags

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::{AuthUser, AuthUserComplete, OptionalAuth};
use crate::routes::analytics_consent;
use crate::services::analytics::{events, props};
use crate::services::{NotificationService, social};

pub fn social_routes() -> Router<AppState> {
    Router::new()
        .route("/social/comments", post(create_comment))
        .route(
            "/social/comments/{target_type}/{target_id}",
            get(list_comments),
        )
        .route("/social/comments/{id}", put(edit_comment))
        .route("/social/comments/{id}", delete(delete_comment))
        .route("/social/reactions", post(toggle_reaction))
        .route(
            "/social/reactions/{target_type}/{target_id}/summary",
            get(reaction_summary),
        )
        .route("/social/mentions/me", get(my_mentions))
        .route("/tags", get(list_tags))
        .route(
            "/social/tag-map/{target_type}/{target_id}",
            get(list_target_tags),
        )
        .route("/social/tag-map", post(attach_tag))
        .route("/social/tag-map", delete(detach_tag))
        .route("/admin/tags", post(admin_create_tag))
}

fn build_response(data: Value) -> Value {
    json!({
        "data": data,
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })
}

// ─── Comments ─────────────────────────────────────────────────────

#[derive(Deserialize)]
struct CreateCommentBody {
    target_type: String,
    target_id: Uuid,
    body: String,
    parent_id: Option<Uuid>,
}

async fn create_comment(
    State(state): State<AppState>,
    auth: AuthUserComplete,
    headers: HeaderMap,
    Json(body): Json<CreateCommentBody>,
) -> Result<Json<Value>, AppError> {
    let comment = social::create_comment(
        &state.db,
        auth.user_id,
        &body.target_type,
        body.target_id,
        &body.body,
        body.parent_id,
    )
    .await?;

    // Extract @mentions and notify mentioned users
    let usernames = social::parse_mentions(&body.body);
    let mentioned_ids =
        social::record_mentions(&state.db, auth.user_id, "comment", comment.id, &usernames).await?;
    for uid in &mentioned_ids {
        // Persistent notif (DB) + ws push + redis counter, via the centralised service.
        let _ = NotificationService::send(
            &state.db,
            &mut state.redis.clone(),
            &state.ws,
            *uid,
            "mention.received",
            "Tu as été mentionné·e",
            Some(body.body.chars().take(140).collect::<String>().as_str()),
            Some(json!({
                "comment_id": comment.id,
                "target_type": comment.target_type,
                "target_id": comment.target_id,
                "author_id": auth.user_id,
            })),
        )
        .await;
        if analytics_consent(&headers) {
            state.analytics.track(
                *uid,
                events::MENTION_RECEIVED,
                props(&[
                    ("source_type", json!("comment")),
                    ("source_id", json!(comment.id)),
                ]),
            );
        }
    }

    // If commenting on a forum post (top-level, i.e. not a reply), notify the post author.
    if comment.parent_id.is_none() && comment.target_type == "post" {
        if let Ok(Some((post_author, post_kind, post_title))) =
            sqlx::query_as::<_, (Uuid, String, String)>(
                "SELECT author_id, kind, title FROM posts WHERE id = $1 AND deleted_at IS NULL",
            )
            .bind(comment.target_id)
            .fetch_optional(&state.db)
            .await
        {
            if post_author != auth.user_id && !mentioned_ids.contains(&post_author) {
                let event_kind = if post_kind == "question" {
                    "question.answered"
                } else {
                    "post.replied"
                };
                let title = if post_kind == "question" {
                    "Nouvelle réponse à ta question"
                } else {
                    "Nouvelle réponse à ton post"
                };
                let _ = NotificationService::send(
                    &state.db,
                    &mut state.redis.clone(),
                    &state.ws,
                    post_author,
                    event_kind,
                    title,
                    Some(post_title.chars().take(140).collect::<String>().as_str()),
                    Some(json!({
                        "post_id": comment.target_id,
                        "comment_id": comment.id,
                        "author_id": auth.user_id,
                    })),
                )
                .await;
            }
        }
    }

    // If this comment is a reply, notify the parent comment's author (unless it's the same user).
    if let Some(parent_id) = comment.parent_id {
        if let Ok(Some((parent_author,))) =
            sqlx::query_as::<_, (Uuid,)>("SELECT author_id FROM comments WHERE id = $1")
                .bind(parent_id)
                .fetch_optional(&state.db)
                .await
        {
            if parent_author != auth.user_id && !mentioned_ids.contains(&parent_author) {
                let _ = NotificationService::send(
                    &state.db,
                    &mut state.redis.clone(),
                    &state.ws,
                    parent_author,
                    "reply.received",
                    "Réponse à ton commentaire",
                    Some(body.body.chars().take(140).collect::<String>().as_str()),
                    Some(json!({
                        "comment_id": comment.id,
                        "parent_id": parent_id,
                        "target_type": comment.target_type,
                        "target_id": comment.target_id,
                        "author_id": auth.user_id,
                    })),
                )
                .await;
            }
        }
    }

    if analytics_consent(&headers) {
        state.analytics.track(
            auth.user_id,
            events::COMMENT_POSTED,
            props(&[
                ("target_type", json!(comment.target_type)),
                ("mentions_count", json!(mentioned_ids.len())),
            ]),
        );
    }
    metrics::counter!("skilluv_comments_posted_total", "target_type" => comment.target_type.clone())
        .increment(1);

    Ok(Json(build_response(json!({
        "comment": comment,
        "mentioned_user_ids": mentioned_ids,
    }))))
}

#[derive(Deserialize)]
struct ListCommentsQuery {
    page: Option<i64>,
    per_page: Option<i64>,
}

async fn list_comments(
    State(state): State<AppState>,
    Path((target_type, target_id)): Path<(String, Uuid)>,
    Query(q): Query<ListCommentsQuery>,
) -> Result<Json<Value>, AppError> {
    let per_page = q.per_page.unwrap_or(50).clamp(1, 200);
    let offset = (q.page.unwrap_or(1).max(1) - 1) * per_page;
    let rows = social::list_comments(&state.db, &target_type, target_id, per_page, offset).await?;
    Ok(Json(build_response(json!({ "comments": rows }))))
}

#[derive(Deserialize)]
struct EditCommentBody {
    body: String,
}

async fn edit_comment(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<EditCommentBody>,
) -> Result<Json<Value>, AppError> {
    let updated = social::edit_comment(&state.db, id, auth.user_id, &auth.role, &body.body).await?;
    Ok(Json(build_response(json!({ "comment": updated }))))
}

async fn delete_comment(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    social::delete_comment(&state.db, id, auth.user_id, &auth.role).await?;
    Ok(Json(build_response(json!({ "deleted": true }))))
}

// ─── Reactions ────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ToggleReactionBody {
    target_type: String,
    target_id: Uuid,
    kind: String,
}

async fn toggle_reaction(
    State(state): State<AppState>,
    auth: AuthUser,
    headers: HeaderMap,
    Json(body): Json<ToggleReactionBody>,
) -> Result<Json<Value>, AppError> {
    let active = social::toggle_reaction(
        &state.db,
        auth.user_id,
        &body.target_type,
        body.target_id,
        &body.kind,
    )
    .await?;

    if active && analytics_consent(&headers) {
        state.analytics.track(
            auth.user_id,
            events::REACTION_ADDED,
            props(&[
                ("kind", json!(body.kind)),
                ("target_type", json!(body.target_type)),
            ]),
        );
    }
    metrics::counter!(
        "skilluv_reactions_total",
        "kind" => body.kind.clone(),
        "target_type" => body.target_type.clone()
    )
    .increment(1);

    Ok(Json(build_response(json!({ "active": active }))))
}

async fn reaction_summary(
    State(state): State<AppState>,
    Path((target_type, target_id)): Path<(String, Uuid)>,
    OptionalAuth(auth): OptionalAuth,
) -> Result<Json<Value>, AppError> {
    let summary = social::reactions_summary(&state.db, &target_type, target_id).await?;
    let my_reactions = if let Some(auth) = auth {
        social::user_reactions_for_target(&state.db, auth.user_id, &target_type, target_id).await?
    } else {
        Vec::new()
    };
    Ok(Json(build_response(json!({
        "summary": summary,
        "my_reactions": my_reactions,
    }))))
}

// ─── Mentions ─────────────────────────────────────────────────────

#[derive(Deserialize)]
struct PaginationQuery {
    page: Option<i64>,
    per_page: Option<i64>,
}

async fn my_mentions(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<PaginationQuery>,
) -> Result<Json<Value>, AppError> {
    let per_page = q.per_page.unwrap_or(50).clamp(1, 200);
    let offset = (q.page.unwrap_or(1).max(1) - 1) * per_page;
    let rows = social::list_mentions_for_user(&state.db, auth.user_id, per_page, offset).await?;
    Ok(Json(build_response(json!({ "mentions": rows }))))
}

// ─── Tags ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ListTagsQuery {
    category: Option<String>,
}

async fn list_tags(
    State(state): State<AppState>,
    Query(q): Query<ListTagsQuery>,
) -> Result<Json<Value>, AppError> {
    let rows = social::list_tags(&state.db, q.category.as_deref()).await?;
    Ok(Json(build_response(json!({ "tags": rows }))))
}

async fn list_target_tags(
    State(state): State<AppState>,
    Path((target_type, target_id)): Path<(String, Uuid)>,
) -> Result<Json<Value>, AppError> {
    let rows = social::tags_for_target(&state.db, &target_type, target_id).await?;
    Ok(Json(build_response(json!({ "tags": rows }))))
}

#[derive(Deserialize)]
struct TagMapBody {
    tag_id: Uuid,
    target_type: String,
    target_id: Uuid,
}

async fn attach_tag(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<TagMapBody>,
) -> Result<Json<Value>, AppError> {
    social::attach_tag(
        &state.db,
        body.tag_id,
        &body.target_type,
        body.target_id,
        auth.user_id,
    )
    .await?;
    Ok(Json(build_response(json!({ "attached": true }))))
}

async fn detach_tag(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<TagMapBody>,
) -> Result<Json<Value>, AppError> {
    // Authorization: any authenticated user can detach for now. Sprint 3 will refine
    // (only original attacher, target owner, or moderator).
    let _ = auth.user_id;
    social::detach_tag(&state.db, body.tag_id, &body.target_type, body.target_id).await?;
    Ok(Json(build_response(json!({ "detached": true }))))
}

async fn admin_create_tag(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<social::CreateTagInput>,
) -> Result<Json<Value>, AppError> {
    if auth.role != "admin" {
        return Err(AppError::Forbidden);
    }
    let tag = social::create_tag(&state.db, input).await?;
    Ok(Json(build_response(json!({ "tag": tag }))))
}
