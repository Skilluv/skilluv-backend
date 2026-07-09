use axum::Router;
use axum::extract::ws::{Message, WebSocket};
use axum::extract::{State, WebSocketUpgrade};
use axum::response::IntoResponse;
use axum::routing::get;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use uuid::Uuid;

use crate::AppState;
use crate::services::AuthService;

pub fn ws_routes() -> Router<AppState> {
    Router::new().route("/ws", get(ws_upgrade))
}

/// WebSocket upgrade handler — authenticates via cookie
async fn ws_upgrade(
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    // Extract JWT from cookie header
    let user_id = extract_user_from_cookie(&headers, &state.config.jwt_secret);

    ws.on_upgrade(move |socket| handle_socket(socket, state, user_id))
}

fn extract_user_from_cookie(headers: &axum::http::HeaderMap, jwt_secret: &str) -> Option<Uuid> {
    let cookie_header = headers.get("cookie")?.to_str().ok()?;

    let token = cookie_header
        .split(';')
        .map(|s| s.trim())
        .find(|s| s.starts_with("access_token="))?
        .strip_prefix("access_token=")?;

    let claims = AuthService::verify_access_token(token, jwt_secret).ok()?;
    claims.sub.parse().ok()
}

#[derive(Debug, Deserialize)]
struct WsClientMessage {
    action: String,
    room: Option<String>,
}

async fn handle_socket(socket: WebSocket, state: AppState, user_id: Option<Uuid>) {
    let user_id = match user_id {
        Some(id) => id,
        None => {
            // Not authenticated — close connection
            let (mut sink, _) = socket.split();
            let _ = sink
                .send(Message::Text(
                    serde_json::json!({
                        "event": "error",
                        "payload": { "code": "AUTH_UNAUTHORIZED", "message": "Authentication required" }
                    })
                    .to_string()
                    .into(),
                ))
                .await;
            let _ = sink.close().await;
            return;
        }
    };

    let (conn_id, mut rx) = state.ws.connect(user_id).await;
    let (mut sink, mut stream) = socket.split();

    // Send welcome message
    let welcome = serde_json::json!({
        "event": "connected",
        "payload": {
            "connection_id": conn_id.to_string(),
            "user_id": user_id.to_string(),
        }
    });
    let _ = sink.send(Message::Text(welcome.to_string().into())).await;

    // Task: forward messages from WsManager → client
    let send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if sink.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });

    // Task: receive messages from client → process
    let ws_manager = state.ws.clone();
    let recv_conn_id = conn_id;
    let recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = stream.next().await {
            match msg {
                Message::Text(text) => {
                    if let Ok(client_msg) = serde_json::from_str::<WsClientMessage>(&text) {
                        match client_msg.action.as_str() {
                            "join" => {
                                if let Some(room) = &client_msg.room {
                                    ws_manager.join(recv_conn_id, room).await;
                                    tracing::debug!(conn_id = %recv_conn_id, room = %room, "Joined room");
                                }
                            }
                            "leave" => {
                                if let Some(room) = &client_msg.room {
                                    ws_manager.leave(recv_conn_id, room).await;
                                    tracing::debug!(conn_id = %recv_conn_id, room = %room, "Left room");
                                }
                            }
                            "ping" => {
                                // Keep-alive handled by protocol
                            }
                            _ => {}
                        }
                    }
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
    });

    // Wait for either task to complete
    tokio::select! {
        _ = send_task => {},
        _ = recv_task => {},
    }

    // Cleanup
    state.ws.disconnect(conn_id).await;
}
