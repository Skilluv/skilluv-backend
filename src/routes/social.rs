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
        // SKI-293 — `/social/mentions/me` removed. It served the same data as
        // `GET /api/users/me/mentions` with a different shape and an empty
        // OpenAPI schema, so the documented route was not the one anyone used.
        // No client called it: checked across the front and admin repos.
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

#[derive(Debug, Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
pub struct CreateCommentBody {
    #[schema(max_length = 10000)]
    pub target_type: String,
    pub target_id: Uuid,
    #[schema(max_length = 10000)]
    pub body: String,
    pub parent_id: Option<Uuid>,
}

/// Create a comment on a target (or reply to another comment via parent_id).
#[utoipa::path(
    post, path = "/api/social/comments", tag = "social",
    request_body = CreateCommentBody,
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn create_comment(
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
            crate::services::notification::NotificationPayload {
                user_id: *uid,
                notification_type: "mention.received",
                title: "Tu as été mentionné·e",
                body: Some(body.body.chars().take(140).collect::<String>().as_str()),
                data: Some(json!({
                    "comment_id": comment.id,
                    "target_type": comment.target_type,
                    "target_id": comment.target_id,
                    "author_id": auth.user_id,
                })),
            },
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
    if comment.parent_id.is_none()
        && comment.target_type == "post"
        && let Ok(Some((post_author, post_kind, post_title))) =
            sqlx::query_as::<_, (Uuid, String, String)>(
                "SELECT author_id, kind, title FROM posts WHERE id = $1 AND deleted_at IS NULL",
            )
            .bind(comment.target_id)
            .fetch_optional(&state.db)
            .await
        && post_author != auth.user_id
        && !mentioned_ids.contains(&post_author)
    {
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
            crate::services::notification::NotificationPayload {
                user_id: post_author,
                notification_type: event_kind,
                title,
                body: Some(post_title.chars().take(140).collect::<String>().as_str()),
                data: Some(json!({
                    "post_id": comment.target_id,
                    "comment_id": comment.id,
                    "author_id": auth.user_id,
                })),
            },
        )
        .await;
    }

    // If this comment is a reply, notify the parent comment's author (unless it's the same user).
    if let Some(parent_id) = comment.parent_id
        && let Ok(Some((parent_author,))) =
            sqlx::query_as::<_, (Uuid,)>("SELECT author_id FROM comments WHERE id = $1")
                .bind(parent_id)
                .fetch_optional(&state.db)
                .await
        && parent_author != auth.user_id
        && !mentioned_ids.contains(&parent_author)
    {
        let _ = NotificationService::send(
            &state.db,
            &mut state.redis.clone(),
            &state.ws,
            crate::services::notification::NotificationPayload {
                user_id: parent_author,
                notification_type: "reply.received",
                title: "Réponse à ton commentaire",
                body: Some(body.body.chars().take(140).collect::<String>().as_str()),
                data: Some(json!({
                    "comment_id": comment.id,
                    "parent_id": parent_id,
                    "target_type": comment.target_type,
                    "target_id": comment.target_id,
                    "author_id": auth.user_id,
                })),
            },
        )
        .await;
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

#[derive(Debug, Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct ListCommentsQuery {
    #[param(minimum = 1, maximum = 100000)]
    pub page: Option<i64>,
    #[param(minimum = 1, maximum = 200)]
    pub per_page: Option<i64>,
}

/// List comments on a target (paginated).
#[utoipa::path(
    get, path = "/api/social/comments/{target_type}/{target_id}", tag = "social",
    params(
        ("target_type" = String, Path, pattern = r"^(challenge|submission|post|question|answer|project|profile|guild|comment|repo)$"),
        ("target_id" = Uuid, Path),
        ListCommentsQuery,
    ),
    responses((status = 200, body = serde_json::Value)),
)]
pub async fn list_comments(
    State(state): State<AppState>,
    Path((target_type, target_id)): Path<(String, Uuid)>,
    Query(q): Query<ListCommentsQuery>,
) -> Result<Json<Value>, AppError> {
    crate::validators::check_range_opt(q.page, "page", 1, 100_000)?;
    crate::validators::check_range_opt(q.per_page, "per_page", 1, 200)?;
    let per_page = q.per_page.unwrap_or(50).clamp(1, 200);
    let offset = (q.page.unwrap_or(1).max(1) - 1) * per_page;
    let rows = social::list_comments(&state.db, &target_type, target_id, per_page, offset).await?;
    Ok(Json(build_response(json!({ "comments": rows }))))
}

#[derive(Debug, Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
pub struct EditCommentBody {
    #[schema(max_length = 10000)]
    pub body: String,
}

/// Edit a comment (author or moderator+).
#[utoipa::path(
    put, path = "/api/social/comments/{id}", tag = "social",
    params(("id" = Uuid, Path)),
    request_body = EditCommentBody,
    responses((status = 200, body = serde_json::Value), (status = 403, body = crate::api_response::ErrorResponse)),
    security(("cookie_auth" = [])),
)]
pub async fn edit_comment(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<EditCommentBody>,
) -> Result<Json<Value>, AppError> {
    let updated = social::edit_comment(&state.db, id, auth.user_id, &auth.role, &body.body).await?;
    Ok(Json(build_response(json!({ "comment": updated }))))
}

/// Delete a comment (author or moderator+).
#[utoipa::path(
    delete, path = "/api/social/comments/{id}", tag = "social",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = serde_json::Value), (status = 403, body = crate::api_response::ErrorResponse)),
    security(("cookie_auth" = [])),
)]
pub async fn delete_comment(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    social::delete_comment(&state.db, id, auth.user_id, &auth.role).await?;
    Ok(Json(build_response(json!({ "deleted": true }))))
}

// ─── Reactions ────────────────────────────────────────────────────

#[derive(Debug, Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
pub struct ToggleReactionBody {
    #[schema(max_length = 10000)]
    pub target_type: String,
    pub target_id: Uuid,
    #[schema(max_length = 10000)]
    pub kind: String,
}

/// Toggle a reaction on a target. Returns `{ active }`.
#[utoipa::path(
    post, path = "/api/social/reactions", tag = "social",
    request_body = ToggleReactionBody,
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn toggle_reaction(
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

/// Reaction summary for a target (counts per kind).
#[utoipa::path(
    get, path = "/api/social/reactions/{target_type}/{target_id}/summary", tag = "social",
    params(
        ("target_type" = String, Path, pattern = r"^(challenge|submission|post|question|answer|project|profile|guild|comment|repo)$"),
        ("target_id" = Uuid, Path),
    ),
    responses((status = 200, body = serde_json::Value)),
)]
pub async fn reaction_summary(
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

#[derive(Debug, Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
pub struct PaginationQuery {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

/// List mentions received by the caller (paginated).
#[utoipa::path(
    get, path = "/api/social/mentions/me", tag = "social",
    params(PaginationQuery),
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn my_mentions(
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

#[derive(Debug, Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
pub struct ListTagsQuery {
    #[schema(max_length = 10000)]
    pub category: Option<String>,
}

/// Public tag list (optionally filter by category).
#[utoipa::path(
    get, path = "/api/tags", tag = "social",
    params(ListTagsQuery),
    responses((status = 200, body = serde_json::Value)),
)]
pub async fn list_tags(
    State(state): State<AppState>,
    Query(q): Query<ListTagsQuery>,
) -> Result<Json<Value>, AppError> {
    let rows = social::list_tags(&state.db, q.category.as_deref()).await?;
    Ok(Json(build_response(json!({ "tags": rows }))))
}

/// Tags attached to a specific target.
#[utoipa::path(
    get, path = "/api/social/tag-map/{target_type}/{target_id}", tag = "social",
    params(
        ("target_type" = String, Path, pattern = r"^(challenge|submission|post|question|answer|project|profile|guild|comment|repo)$"),
        ("target_id" = Uuid, Path),
    ),
    responses((status = 200, body = serde_json::Value)),
)]
pub async fn list_target_tags(
    State(state): State<AppState>,
    Path((target_type, target_id)): Path<(String, Uuid)>,
) -> Result<Json<Value>, AppError> {
    let rows = social::tags_for_target(&state.db, &target_type, target_id).await?;
    Ok(Json(build_response(json!({ "tags": rows }))))
}

#[derive(Debug, Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
pub struct TagMapBody {
    pub tag_id: Uuid,
    #[schema(max_length = 10000)]
    pub target_type: String,
    pub target_id: Uuid,
}

/// Attach a tag to a target (rate-limited).
#[utoipa::path(
    post, path = "/api/social/tag-map", tag = "social",
    request_body = TagMapBody,
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn attach_tag(
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

/// Detach a tag from a target.
#[utoipa::path(
    delete, path = "/api/social/tag-map", tag = "social",
    request_body = TagMapBody,
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn detach_tag(
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

/// Admin: create a new tag.
#[utoipa::path(
    post, path = "/api/admin/tags", tag = "admin",
    request_body(content = serde_json::Value, description = "CreateTagInput"),
    responses((status = 200, body = serde_json::Value), (status = 403, body = crate::api_response::ErrorResponse)),
    security(("cookie_auth" = [])),
)]
pub async fn admin_create_tag(
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
