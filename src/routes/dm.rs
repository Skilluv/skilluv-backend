//! DM (direct messaging) routes — Phase 2 Sprint 2.

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::{AuthUser, AuthUserComplete};
use crate::routes::analytics_consent;
use crate::services::analytics::{events, props};
use crate::services::{NotificationService, dm};
use crate::websocket::WsMessage;

pub fn dm_routes() -> Router<AppState> {
    Router::new()
        .route("/dm/conversations", post(open_conversation))
        .route("/dm/conversations", get(list_conversations))
        .route(
            "/dm/conversations/{id}/messages",
            get(list_messages).post(send_message),
        )
        .route("/dm/conversations/{id}/read", post(mark_read))
        .route("/dm/blocks", post(block_user).get(list_blocks))
        .route("/dm/blocks/{user_id}", delete(unblock_user))
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

#[derive(Deserialize)]
struct OpenConversationBody {
    peer_user_id: Uuid,
}

async fn open_conversation(
    State(state): State<AppState>,
    auth: AuthUser,
    headers: HeaderMap,
    Json(body): Json<OpenConversationBody>,
) -> Result<Json<Value>, AppError> {
    let conv = dm::open_or_get_conversation(&state.db, auth.user_id, body.peer_user_id).await?;
    if analytics_consent(&headers) {
        state.analytics.track(
            auth.user_id,
            events::DM_CONVERSATION_OPENED,
            props(&[("peer_user_id", json!(body.peer_user_id))]),
        );
    }
    Ok(Json(build_response(json!({ "conversation": conv }))))
}

#[derive(Deserialize)]
struct PaginationQuery {
    page: Option<i64>,
    per_page: Option<i64>,
}

async fn list_conversations(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<PaginationQuery>,
) -> Result<Json<Value>, AppError> {
    let per_page = q.per_page.unwrap_or(30).clamp(1, 100);
    let offset = (q.page.unwrap_or(1).max(1) - 1) * per_page;
    let summaries = dm::list_conversations(&state.db, auth.user_id, per_page, offset).await?;
    Ok(Json(build_response(json!({ "conversations": summaries }))))
}

#[derive(Deserialize)]
struct ListMessagesQuery {
    limit: Option<i64>,
    before: Option<chrono::DateTime<chrono::Utc>>,
}

async fn list_messages(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Query(q): Query<ListMessagesQuery>,
) -> Result<Json<Value>, AppError> {
    let messages =
        dm::list_messages(&state.db, auth.user_id, id, q.limit.unwrap_or(50), q.before).await?;
    Ok(Json(build_response(json!({ "messages": messages }))))
}

#[derive(Deserialize)]
struct SendMessageBody {
    body: String,
}

async fn send_message(
    State(state): State<AppState>,
    auth: AuthUserComplete,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(body): Json<SendMessageBody>,
) -> Result<Json<Value>, AppError> {
    let (message, peer_id) = dm::send_message(&state.db, auth.user_id, id, &body.body).await?;

    // Persistent notification to peer + WS push (NotificationService handles both)
    let preview: String = body.body.chars().take(140).collect();
    let _ = NotificationService::send(
        &state.db,
        &mut state.redis.clone(),
        &state.ws,
        peer_id,
        "dm.received",
        "Nouveau message",
        Some(&preview),
        Some(json!({
            "conversation_id": id,
            "message_id": message.id,
            "from_user_id": auth.user_id,
        })),
    )
    .await;

    // Additional realtime event for clients that subscribe to dm streams specifically
    state
        .ws
        .send_to_user(
            peer_id,
            WsMessage {
                event: "dm.received".to_string(),
                room: None,
                payload: json!({
                    "conversation_id": id,
                    "message": message,
                }),
            },
        )
        .await;

    if analytics_consent(&headers) {
        state.analytics.track(
            auth.user_id,
            events::DM_SENT,
            props(&[("conversation_id", json!(id))]),
        );
    }
    metrics::counter!("skilluv_dm_messages_total").increment(1);

    Ok(Json(build_response(json!({ "message": message }))))
}

async fn mark_read(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let marked = dm::mark_conversation_read(&state.db, auth.user_id, id).await?;
    Ok(Json(build_response(json!({ "marked_read": marked }))))
}

#[derive(Deserialize)]
struct BlockBody {
    user_id: Uuid,
    reason: Option<String>,
}

async fn block_user(
    State(state): State<AppState>,
    auth: AuthUser,
    headers: HeaderMap,
    Json(body): Json<BlockBody>,
) -> Result<Json<Value>, AppError> {
    dm::block_user(
        &state.db,
        auth.user_id,
        body.user_id,
        body.reason.as_deref(),
    )
    .await?;
    if analytics_consent(&headers) {
        state.analytics.track(
            auth.user_id,
            events::USER_BLOCKED,
            props(&[("blocked_user_id", json!(body.user_id))]),
        );
    }
    Ok(Json(build_response(json!({ "blocked": true }))))
}

async fn unblock_user(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(user_id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    dm::unblock_user(&state.db, auth.user_id, user_id).await?;
    Ok(Json(build_response(json!({ "unblocked": true }))))
}

async fn list_blocks(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let blocks = dm::list_blocks(&state.db, auth.user_id).await?;
    Ok(Json(build_response(json!({ "blocks": blocks }))))
}
