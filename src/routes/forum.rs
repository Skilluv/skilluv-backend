//! Forum + Q&A routes — Phase 2 Sprint 3.

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::{AuthUser, AuthUserComplete, RateLimiter};
use crate::routes::analytics_consent;
use crate::services::analytics::{events, props};
use crate::services::{NotificationService, forum};

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

async fn list_categories(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let cats = forum::list_categories(&state.db).await?;
    Ok(Json(build_response(json!({ "categories": cats }))))
}

#[derive(Deserialize)]
struct ListPostsQuery {
    category: Option<String>,
    kind: Option<String>,
    sort: Option<String>,
    page: Option<i64>,
    per_page: Option<i64>,
}

async fn list_posts(
    State(state): State<AppState>,
    Query(q): Query<ListPostsQuery>,
) -> Result<Json<Value>, AppError> {
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

#[derive(Deserialize)]
struct CreatePostBody {
    category_slug: String,
    kind: String,
    title: String,
    body: String,
    bounty_fragments: Option<i32>,
}

async fn create_post(
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

async fn get_post(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let post = forum::get_post(&state.db, id).await?;
    // Best-effort view count bump (non-blocking semantics)
    forum::increment_view_count(&state.db, id).await;
    Ok(Json(build_response(json!({ "post": post }))))
}

#[derive(Deserialize)]
struct EditPostBody {
    title: String,
    body: String,
}

async fn edit_post(
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

async fn delete_post(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    forum::delete_post(&state.db, id, auth.user_id, &auth.role).await?;
    Ok(Json(build_response(json!({ "deleted": true }))))
}

#[derive(Deserialize)]
struct AcceptAnswerBody {
    answer_comment_id: Uuid,
}

async fn accept_answer(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<AcceptAnswerBody>,
) -> Result<Json<Value>, AppError> {
    let res = forum::accept_answer(&state.db, auth.user_id, id, body.answer_comment_id).await?;

    // Notify answer author
    let bounty_msg = if res.bounty_transferred > 0 {
        Some(format!("+{} fragments de bounty", res.bounty_transferred))
    } else {
        None
    };
    let _ = NotificationService::send(
        &state.db,
        &mut state.redis.clone(),
        &state.ws,
        res.answer_author_id,
        "answer.accepted",
        "Ta réponse a été acceptée",
        bounty_msg.as_deref(),
        Some(json!({
            "post_id": id,
            "comment_id": res.answer_id,
            "bounty_fragments": res.bounty_transferred,
        })),
    )
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

#[derive(Deserialize)]
struct TogglePinBody {
    pinned: bool,
}

async fn toggle_pin(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<TogglePinBody>,
) -> Result<Json<Value>, AppError> {
    forum::set_pinned(&state.db, id, &auth.role, body.pinned).await?;
    Ok(Json(build_response(json!({ "pinned": body.pinned }))))
}

#[derive(Deserialize)]
struct ToggleLockBody {
    locked: bool,
}

async fn toggle_lock(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ToggleLockBody>,
) -> Result<Json<Value>, AppError> {
    forum::set_locked(&state.db, id, &auth.role, body.locked).await?;
    Ok(Json(build_response(json!({ "locked": body.locked }))))
}

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
    limit: Option<i64>,
}

async fn search(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Value>, AppError> {
    let hits = forum::search_posts(&state.db, &query.q, query.limit.unwrap_or(20)).await?;
    Ok(Json(build_response(json!({ "hits": hits }))))
}
