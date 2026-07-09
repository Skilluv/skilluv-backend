use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde::Serialize;
use tokio::sync::{RwLock, mpsc};
use uuid::Uuid;

/// A connected client
#[derive(Debug)]
struct Client {
    user_id: Uuid,
    sender: mpsc::UnboundedSender<String>,
    rooms: HashSet<String>,
}

/// WebSocket room manager
/// Manages connections, room subscriptions, and message broadcasting
#[derive(Debug, Clone)]
pub struct WsManager {
    inner: Arc<RwLock<WsManagerInner>>,
}

#[derive(Debug, Default)]
struct WsManagerInner {
    /// connection_id → Client
    clients: HashMap<Uuid, Client>,
    /// room_name → set of connection_ids
    rooms: HashMap<String, HashSet<Uuid>>,
    /// user_id → set of connection_ids (a user can have multiple tabs)
    user_connections: HashMap<Uuid, HashSet<Uuid>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WsMessage {
    pub event: String,
    pub room: Option<String>,
    pub payload: serde_json::Value,
}

impl WsManager {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(WsManagerInner::default())),
        }
    }

    /// Register a new client connection, returns (connection_id, receiver)
    pub async fn connect(&self, user_id: Uuid) -> (Uuid, mpsc::UnboundedReceiver<String>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let conn_id = Uuid::new_v4();

        let client = Client {
            user_id,
            sender: tx,
            rooms: HashSet::new(),
        };

        let mut inner = self.inner.write().await;
        inner.clients.insert(conn_id, client);
        inner
            .user_connections
            .entry(user_id)
            .or_default()
            .insert(conn_id);

        // Auto-join personal notification room
        let user_room = format!("user:{user_id}");
        Self::join_room_inner(&mut inner, conn_id, &user_room);

        tracing::debug!(conn_id = %conn_id, user_id = %user_id, "WebSocket client connected");

        (conn_id, rx)
    }

    /// Remove a client connection
    pub async fn disconnect(&self, conn_id: Uuid) {
        let mut inner = self.inner.write().await;

        if let Some(client) = inner.clients.remove(&conn_id) {
            // Remove from all rooms
            for room in &client.rooms {
                if let Some(members) = inner.rooms.get_mut(room) {
                    members.remove(&conn_id);
                    if members.is_empty() {
                        inner.rooms.remove(room);
                    }
                }
            }

            // Remove from user_connections
            if let Some(conns) = inner.user_connections.get_mut(&client.user_id) {
                conns.remove(&conn_id);
                if conns.is_empty() {
                    inner.user_connections.remove(&client.user_id);
                }
            }

            tracing::debug!(conn_id = %conn_id, user_id = %client.user_id, "WebSocket client disconnected");
        }
    }

    /// Subscribe a connection to a room
    pub async fn join(&self, conn_id: Uuid, room: &str) {
        let mut inner = self.inner.write().await;
        Self::join_room_inner(&mut inner, conn_id, room);
    }

    /// Unsubscribe a connection from a room
    pub async fn leave(&self, conn_id: Uuid, room: &str) {
        let mut inner = self.inner.write().await;

        if let Some(client) = inner.clients.get_mut(&conn_id) {
            client.rooms.remove(room);
        }
        if let Some(members) = inner.rooms.get_mut(room) {
            members.remove(&conn_id);
            if members.is_empty() {
                inner.rooms.remove(room);
            }
        }
    }

    /// Broadcast a message to all connections in a room
    pub async fn broadcast_to_room(&self, room: &str, message: WsMessage) {
        let msg = serde_json::to_string(&message).unwrap_or_default();
        let inner = self.inner.read().await;

        if let Some(members) = inner.rooms.get(room) {
            for conn_id in members {
                if let Some(client) = inner.clients.get(conn_id) {
                    let _ = client.sender.send(msg.clone());
                }
            }
        }
    }

    /// Send a message to a specific user (all their connections)
    pub async fn send_to_user(&self, user_id: Uuid, message: WsMessage) {
        let msg = serde_json::to_string(&message).unwrap_or_default();
        let inner = self.inner.read().await;

        if let Some(conns) = inner.user_connections.get(&user_id) {
            for conn_id in conns {
                if let Some(client) = inner.clients.get(conn_id) {
                    let _ = client.sender.send(msg.clone());
                }
            }
        }
    }

    /// Broadcast to all connected clients
    pub async fn broadcast_all(&self, message: WsMessage) {
        let msg = serde_json::to_string(&message).unwrap_or_default();
        let inner = self.inner.read().await;

        for client in inner.clients.values() {
            let _ = client.sender.send(msg.clone());
        }
    }

    /// Get stats
    pub async fn stats(&self) -> (usize, usize, usize) {
        let inner = self.inner.read().await;
        (
            inner.clients.len(),
            inner.rooms.len(),
            inner.user_connections.len(),
        )
    }

    fn join_room_inner(inner: &mut WsManagerInner, conn_id: Uuid, room: &str) {
        if let Some(client) = inner.clients.get_mut(&conn_id) {
            client.rooms.insert(room.to_string());
        }
        inner
            .rooms
            .entry(room.to_string())
            .or_default()
            .insert(conn_id);
    }
}
