//! Forum + Q&A routes — Phase 2 Sprint 3.

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::{AuthUser, AuthUserComplete, RateLimiter};
use crate::routes::analytics_consent;
use crate::services::analytics::{events, props};
use crate::services::forum;

pub fn forum_routes() -> Router<AppState> {
    Router::new()
        .route("/forum/categories", get(list_categories))
        .route("/forum/posts", get(list_posts).post(create_post))
        .route(
            "/forum/posts/{id}",
            get(get_post).put(edit_post).delete(delete_post),
        )
        .route("/forum/posts/{id}/accept-answer", post(accept_answer))
        .route("/forum/posts/{id}/pin", post(toggle_pin))
        .route("/forum/posts/{id}/lock", post(toggle_lock))
        .route("/forum/search", get(search))
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

/// Public forum categories.
#[utoipa::path(
    get,
    path = "/api/forum/categories",
    tag = "forum",
    responses(
        (status = 200, description = "Categories", body = serde_json::Value),
    ),
    operation_id = "forumListCategories",
)]
pub async fn list_categories(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let cats = forum::list_categories(&state.db).await?;
    Ok(Json(build_response(json!({ "categories": cats }))))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct ListPostsQuery {
    #[param(max_length = 100)]
    pub category: Option<String>,
    #[param(max_length = 50)]
    pub kind: Option<String>,
    /// `recent` (default), `hot`, `top-bounty`.
    #[param(pattern = r"^(recent|hot|top-bounty)$")]
    pub sort: Option<String>,
    #[param(minimum = 1, maximum = 100000)]
    pub page: Option<i64>,
    #[param(minimum = 1, maximum = 100)]
    pub per_page: Option<i64>,
}

/// Paginated forum posts. Optional filters on category and kind.
#[utoipa::path(
    get,
    path = "/api/forum/posts",
    tag = "forum",
    params(ListPostsQuery),
    responses(
        (status = 200, description = "Posts", body = serde_json::Value),
    ),
)]
pub async fn list_posts(
    State(state): State<AppState>,
    Query(q): Query<ListPostsQuery>,
) -> Result<Json<Value>, AppError> {
    crate::validators::check_max_len_opt(&q.category, "category", 100)?;
    crate::validators::check_max_len_opt(&q.kind, "kind", 50)?;
    if let Some(s) = &q.sort
        && !matches!(s.as_str(), "recent" | "hot" | "top-bounty")
    {
        return Err(AppError::Validation(
            "sort must be one of: recent, hot, top-bounty".into(),
        ));
    }
    crate::validators::check_range_opt(q.page, "page", 1, 100_000)?;
    crate::validators::check_range_opt(q.per_page, "per_page", 1, 100)?;

    let per_page = q.per_page.unwrap_or(30).clamp(1, 100);
    let offset = (q.page.unwrap_or(1).max(1) - 1) * per_page;
    let sort = match q.sort.as_deref() {
        Some("hot") => forum::PostSort::Hot,
        Some("top-bounty") => forum::PostSort::TopBounty,
        _ => forum::PostSort::Recent,
    };
    let posts = forum::list_posts(
        &state.db,
        forum::ListPostsFilters {
            category_slug: q.category.as_deref(),
            kind: q.kind.as_deref(),
            sort,
            limit: per_page,
            offset,
        },
    )
    .await?;
    Ok(Json(build_response(json!({ "posts": posts }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreatePostBody {
    #[schema(max_length = 10000)]
    pub category_slug: String,
    /// `question`, `discussion`, `announcement`, …
    #[schema(max_length = 10000)]
    pub kind: String,
    #[schema(max_length = 10000)]
    pub title: String,
    #[schema(max_length = 10000)]
    pub body: String,
    /// Bounty fragments for a question (0 = no bounty).
    pub bounty_fragments: Option<i32>,
}

/// Create a forum post. Question kind is rate-limited by user tier.
#[utoipa::path(
    post,
    path = "/api/forum/posts",
    tag = "forum",
    request_body = CreatePostBody,
    responses(
        (status = 200, description = "Post created", body = serde_json::Value),
        (status = 429, description = "Question rate limit hit for tier", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn create_post(
    State(state): State<AppState>,
    auth: AuthUserComplete,
    headers: HeaderMap,
    Json(body): Json<CreatePostBody>,
) -> Result<Json<Value>, AppError> {
    let category = forum::get_category_by_slug(&state.db, &body.category_slug).await?;

    // Tier-based rate limit for questions only (anti-spam)
    if body.kind == "question" {
        let title: Option<(String,)> = sqlx::query_as("SELECT title FROM users WHERE id = $1")
            .bind(auth.user_id)
            .fetch_optional(&state.db)
            .await?;
        let user_title = title.map(|(t,)| t).unwrap_or_else(|| "apprenti".into());
        let (limit, window) = forum::question_rate_limit_for_title(&user_title);
        if limit > 0 {
            RateLimiter::check(
                &mut state.redis.clone(),
                "forum_question",
                &auth.user_id.to_string(),
                limit,
                window,
            )
            .await?;
        }
    }

    let post = forum::create_post(
        &state.db,
        forum::CreatePostInput {
            category_id: category.id,
            author_id: auth.user_id,
            kind: body.kind.clone(),
            title: body.title,
            body: body.body,
            bounty_fragments: body.bounty_fragments.unwrap_or(0),
        },
        &auth.role,
    )
    .await?;

    if analytics_consent(&headers) {
        state.analytics.track(
            auth.user_id,
            events::COMMENT_POSTED, // reuse for "content created"; we have a separate event below too
            props(&[("target_type", json!("post"))]),
        );
    }
    metrics::counter!(
        "skilluv_forum_posts_total",
        "kind" => post.kind.clone(),
        "category" => body.category_slug.clone()
    )
    .increment(1);

    Ok(Json(build_response(json!({ "post": post }))))
}

/// Get a post by id. Bumps the view counter.
#[utoipa::path(
    get,
    path = "/api/forum/posts/{id}",
    tag = "forum",
    params(("id" = Uuid, Path, description = "Post UUID")),
    responses(
        (status = 200, description = "Post", body = serde_json::Value),
        (status = 404, description = "Post not found", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn get_post(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let post = forum::get_post(&state.db, id).await?;
    // Best-effort view count bump (non-blocking semantics)
    forum::increment_view_count(&state.db, id).await;
    Ok(Json(build_response(json!({ "post": post }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct EditPostBody {
    #[schema(max_length = 10000)]
    pub title: String,
    #[schema(max_length = 10000)]
    pub body: String,
}

/// Edit a post — restricted to author or moderator+ role.
#[utoipa::path(
    put,
    path = "/api/forum/posts/{id}",
    tag = "forum",
    params(("id" = Uuid, Path, description = "Post UUID")),
    request_body = EditPostBody,
    responses(
        (status = 200, description = "Post updated", body = serde_json::Value),
        (status = 403, description = "Not the author nor moderator", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn edit_post(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<EditPostBody>,
) -> Result<Json<Value>, AppError> {
    let post = forum::edit_post(
        &state.db,
        id,
        auth.user_id,
        &auth.role,
        &body.title,
        &body.body,
    )
    .await?;
    Ok(Json(build_response(json!({ "post": post }))))
}

/// Delete a post — restricted to author or moderator+ role.
#[utoipa::path(
    delete,
    path = "/api/forum/posts/{id}",
    tag = "forum",
    params(("id" = Uuid, Path, description = "Post UUID")),
    responses(
        (status = 200, description = "Deleted", body = serde_json::Value),
        (status = 403, description = "Not the author nor moderator", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn delete_post(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    forum::delete_post(&state.db, id, auth.user_id, &auth.role).await?;
    Ok(Json(build_response(json!({ "deleted": true }))))
}

/// Field renamed from `answer_comment_id` to the more concise `answer_id`
/// (the accepted answer is more than "just" a comment — it's the
/// canonical resolution of the question). Both legacy names are accepted
/// as aliases so we can roll out the front migration (FE-P0-BE08) without
/// coordinating a big-bang deploy.
#[derive(Debug, Deserialize, ToSchema)]
pub struct AcceptAnswerBody {
    #[serde(alias = "comment_id", alias = "answer_comment_id")]
    pub answer_id: Uuid,
}

/// Accept an answer on a question. Transfers any bounty to the answer
/// author and notifies them.
#[utoipa::path(
    post,
    path = "/api/forum/posts/{id}/accept-answer",
    tag = "forum",
    params(("id" = Uuid, Path, description = "Question post UUID")),
    request_body = AcceptAnswerBody,
    responses(
        (status = 200, description = "Answer accepted, bounty transferred", body = serde_json::Value),
        (status = 403, description = "Not the question author", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn accept_answer(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<AcceptAnswerBody>,
) -> Result<Json<Value>, AppError> {
    let res = forum::accept_answer(&state.db, auth.user_id, id, body.answer_id).await?;

    // The person who asked, since it is their acceptance being announced.
    let asker_name: String = sqlx::query_scalar("SELECT display_name FROM users WHERE id = $1")
        .bind(auth.user_id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .unwrap_or_default();
    let post_title: String = sqlx::query_scalar("SELECT title FROM posts WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .unwrap_or_default();

    let _ = crate::services::notify::send(
        &state,
        crate::services::notify::Recipient::User(res.answer_author_id),
        "forum.answer_accepted",
    )
    .arg("author", asker_name.clone())
    .arg("title", post_title)
    .payload(json!({
        "post_id": id,
        "comment_id": res.answer_id,
        "bounty_fragments": res.bounty_transferred,
    }))
    .execute()
    .await;
    metrics::counter!("skilluv_answers_accepted_total").increment(1);
    if res.bounty_transferred > 0 {
        metrics::counter!("skilluv_bounty_fragments_paid_total")
            .increment(res.bounty_transferred as u64);
    }

    Ok(Json(build_response(json!({
        "accepted_answer_id": res.answer_id,
        "bounty_transferred": res.bounty_transferred,
    }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct TogglePinBody {
    pub pinned: bool,
}

/// Pin / unpin a post — moderator+ only.
#[utoipa::path(
    post,
    path = "/api/forum/posts/{id}/pin",
    tag = "forum",
    params(("id" = Uuid, Path, description = "Post UUID")),
    request_body = TogglePinBody,
    responses(
        (status = 200, description = "Toggled", body = serde_json::Value),
        (status = 403, description = "Not a moderator", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn toggle_pin(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<TogglePinBody>,
) -> Result<Json<Value>, AppError> {
    forum::set_pinned(&state.db, id, &auth.role, body.pinned).await?;
    Ok(Json(build_response(json!({ "pinned": body.pinned }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ToggleLockBody {
    pub locked: bool,
}

/// Lock / unlock a post — moderator+ only.
#[utoipa::path(
    post,
    path = "/api/forum/posts/{id}/lock",
    tag = "forum",
    params(("id" = Uuid, Path, description = "Post UUID")),
    request_body = ToggleLockBody,
    responses(
        (status = 200, description = "Toggled", body = serde_json::Value),
        (status = 403, description = "Not a moderator", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn toggle_lock(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ToggleLockBody>,
) -> Result<Json<Value>, AppError> {
    forum::set_locked(&state.db, id, &auth.role, body.locked).await?;
    Ok(Json(build_response(json!({ "locked": body.locked }))))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct SearchQuery {
    #[param(min_length = 1, max_length = 200)]
    pub q: String,
    /// Max hits. Defaults to 20.
    #[param(minimum = 1, maximum = 100)]
    pub limit: Option<i64>,
}

/// Full-text search across forum posts.
#[utoipa::path(
    get,
    path = "/api/forum/search",
    tag = "forum",
    params(SearchQuery),
    responses(
        (status = 200, description = "Search hits", body = serde_json::Value),
    ),
    operation_id = "forumSearch",
)]
pub async fn search(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Value>, AppError> {
    if query.q.is_empty() || query.q.len() > 200 {
        return Err(AppError::Validation(
            "q must be between 1 and 200 characters".into(),
        ));
    }
    crate::validators::check_range_opt(query.limit, "limit", 1, 100)?;
    let hits = forum::search_posts(&state.db, &query.q, query.limit.unwrap_or(20)).await?;
    Ok(Json(build_response(json!({ "hits": hits }))))
}
