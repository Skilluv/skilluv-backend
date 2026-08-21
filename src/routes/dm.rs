//! DM (direct messaging) routes — Phase 2 Sprint 2.

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::middleware::{AuthUser, AuthUserComplete};
use crate::routes::analytics_consent;
use crate::services::analytics::{events, props};
use crate::services::dm;
use crate::services::dm::{ConversationSummary, DmConversation, DmMessage, UserBlock};
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

#[derive(Debug, Deserialize, ToSchema)]
pub struct OpenConversationBody {
    pub peer_user_id: Uuid,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OpenConversationResponse {
    pub conversation: DmConversation,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct PaginationQuery {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ConversationsResponse {
    pub conversations: Vec<ConversationSummary>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct ListMessagesQuery {
    /// Max rows. Defaults to 50.
    pub limit: Option<i64>,
    /// Filter to messages created strictly before this timestamp
    /// (used for oldest-page pagination).
    pub before: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MessagesResponse {
    pub messages: Vec<DmMessage>,
}

/// `text` accepted as alias — the audit doc used to name the field `text` ;
/// keeping both means a stale client (or an SDK that trusted the doc) won't
/// 422 while the front migrates.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SendMessageBody {
    #[serde(alias = "text")]
    #[schema(max_length = 10000)]
    pub body: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SendMessageResponse {
    pub message: DmMessage,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MarkReadResponse {
    /// Number of messages transitioned from unread to read.
    pub marked_read: u64,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct BlockBody {
    pub user_id: Uuid,
    #[schema(max_length = 10000)]
    pub reason: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BlockedResponse {
    pub blocked: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UnblockedResponse {
    pub unblocked: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BlocksResponse {
    pub blocks: Vec<UserBlock>,
}

/// Open (or fetch) a DM conversation with a peer user.
#[utoipa::path(
    post,
    path = "/api/dm/conversations",
    tag = "dm",
    request_body = OpenConversationBody,
    responses(
        (status = 200, description = "Conversation", body = ApiResponse<OpenConversationResponse>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn open_conversation(
    State(state): State<AppState>,
    auth: AuthUser,
    headers: HeaderMap,
    Json(body): Json<OpenConversationBody>,
) -> Result<Json<ApiResponse<OpenConversationResponse>>, AppError> {
    let conv = dm::open_or_get_conversation(&state.db, auth.user_id, body.peer_user_id).await?;
    if analytics_consent(&headers) {
        state.analytics.track(
            auth.user_id,
            events::DM_CONVERSATION_OPENED,
            props(&[("peer_user_id", json!(body.peer_user_id))]),
        );
    }
    Ok(Json(ApiResponse::new(OpenConversationResponse {
        conversation: conv,
    })))
}

/// Paginated list of the caller's DM conversations (with unread count
/// + last message preview).
#[utoipa::path(
    get,
    path = "/api/dm/conversations",
    tag = "dm",
    params(PaginationQuery),
    responses(
        (status = 200, description = "Conversations", body = ApiResponse<ConversationsResponse>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
    operation_id = "dmListConversations",
)]
pub async fn list_conversations(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<PaginationQuery>,
) -> Result<Json<ApiResponse<ConversationsResponse>>, AppError> {
    let per_page = q.per_page.unwrap_or(30).clamp(1, 100);
    let offset = (q.page.unwrap_or(1).max(1) - 1) * per_page;
    let summaries = dm::list_conversations(&state.db, auth.user_id, per_page, offset).await?;
    Ok(Json(ApiResponse::new(ConversationsResponse {
        conversations: summaries,
    })))
}

/// Fetch messages in a conversation. Reverse-chronological with
/// optional `before` cursor for oldest-page pagination.
#[utoipa::path(
    get,
    path = "/api/dm/conversations/{id}/messages",
    tag = "dm",
    params(
        ("id" = Uuid, Path, description = "Conversation UUID"),
        ListMessagesQuery,
    ),
    responses(
        (status = 200, description = "Messages", body = ApiResponse<MessagesResponse>),
        (status = 403, description = "Not a participant in this conversation", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn list_messages(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Query(q): Query<ListMessagesQuery>,
) -> Result<Json<ApiResponse<MessagesResponse>>, AppError> {
    let messages =
        dm::list_messages(&state.db, auth.user_id, id, q.limit.unwrap_or(50), q.before).await?;
    Ok(Json(ApiResponse::new(MessagesResponse { messages })))
}

/// Send a DM. Also emits a `dm.received` notification + WS event to
/// the peer.
#[utoipa::path(
    post,
    path = "/api/dm/conversations/{id}/messages",
    tag = "dm",
    params(("id" = Uuid, Path, description = "Conversation UUID")),
    request_body = SendMessageBody,
    responses(
        (status = 200, description = "Message sent", body = ApiResponse<SendMessageResponse>),
        (status = 403, description = "Blocked or not a participant", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
    operation_id = "dmSendMessage",
)]
pub async fn send_message(
    State(state): State<AppState>,
    auth: AuthUserComplete,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(body): Json<SendMessageBody>,
) -> Result<Json<ApiResponse<SendMessageResponse>>, AppError> {
    let (message, peer_id) = dm::send_message(&state.db, auth.user_id, id, &body.body).await?;

    // Persistent notification to peer + live push (notify handles both)
    let preview: String = body.body.chars().take(140).collect();
    let sender_name: String = sqlx::query_scalar("SELECT display_name FROM users WHERE id = $1")
        .bind(auth.user_id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "Quelqu'un".to_string());

    let _ = crate::services::notify::send(
        &state,
        crate::services::notify::Recipient::User(peer_id),
        "dm.received",
    )
    .arg("author", sender_name)
    .arg("excerpt", preview.clone())
    .payload(json!({
        "conversation_id": id,
        "message_id": message.id,
        "from_user_id": auth.user_id,
    }))
    .execute()
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

    Ok(Json(ApiResponse::new(SendMessageResponse { message })))
}

/// Mark every unread message in a conversation as read (by the caller).
#[utoipa::path(
    post,
    path = "/api/dm/conversations/{id}/read",
    tag = "dm",
    params(("id" = Uuid, Path, description = "Conversation UUID")),
    responses(
        (status = 200, description = "Marked count", body = ApiResponse<MarkReadResponse>),
    ),
    security(("cookie_auth" = [])),
    operation_id = "dmMarkRead",
)]
pub async fn mark_read(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<MarkReadResponse>>, AppError> {
    let marked = dm::mark_conversation_read(&state.db, auth.user_id, id).await?;
    Ok(Json(ApiResponse::new(MarkReadResponse {
        marked_read: marked,
    })))
}

/// Block a user (DMs from them are silently refused).
#[utoipa::path(
    post,
    path = "/api/dm/blocks",
    tag = "dm",
    request_body = BlockBody,
    responses(
        (status = 200, description = "Blocked", body = ApiResponse<BlockedResponse>),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn block_user(
    State(state): State<AppState>,
    auth: AuthUser,
    headers: HeaderMap,
    Json(body): Json<BlockBody>,
) -> Result<Json<ApiResponse<BlockedResponse>>, AppError> {
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
    Ok(Json(ApiResponse::new(BlockedResponse { blocked: true })))
}

/// Remove a user from the caller's block list.
#[utoipa::path(
    delete,
    path = "/api/dm/blocks/{user_id}",
    tag = "dm",
    params(("user_id" = Uuid, Path, description = "Blocked user UUID")),
    responses(
        (status = 200, description = "Unblocked", body = ApiResponse<UnblockedResponse>),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn unblock_user(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(user_id): Path<Uuid>,
) -> Result<Json<ApiResponse<UnblockedResponse>>, AppError> {
    dm::unblock_user(&state.db, auth.user_id, user_id).await?;
    Ok(Json(ApiResponse::new(UnblockedResponse {
        unblocked: true,
    })))
}

/// List every user the caller currently blocks.
#[utoipa::path(
    get,
    path = "/api/dm/blocks",
    tag = "dm",
    responses(
        (status = 200, description = "Blocks", body = ApiResponse<BlocksResponse>),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn list_blocks(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<BlocksResponse>>, AppError> {
    let blocks = dm::list_blocks(&state.db, auth.user_id).await?;
    Ok(Json(ApiResponse::new(BlocksResponse { blocks })))
}
