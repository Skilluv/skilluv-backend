use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::{AuthUser, RateLimiter};
use crate::models::{Conversation, Enterprise, InterestRequest, Message};
use crate::services::NotificationService;

pub fn contact_routes() -> Router<AppState> {
    Router::new()
        // Interest requests
        .route("/contact/interest", post(send_interest))
        .route("/contact/interest/sent", get(sent_requests))
        .route("/contact/interest/received", get(received_requests))
        .route("/contact/interest/{id}/accept", post(accept_interest))
        .route("/contact/interest/{id}/decline", post(decline_interest))
        // Conversations
        .route("/contact/conversations", get(list_conversations))
        .route("/contact/conversations/{id}", get(get_conversation))
        .route("/contact/conversations/{id}/messages", post(send_message))
        // Blocking
        .route("/contact/block/{enterprise_id}", post(block_enterprise))
        .route("/contact/block/{enterprise_id}", delete(unblock_enterprise))
}

fn build_response(data: serde_json::Value) -> serde_json::Value {
    json!({
        "data": data,
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })
}

async fn get_enterprise_for_user(state: &AppState, user_id: Uuid) -> Result<Enterprise, AppError> {
    sqlx::query_as(
        "SELECT e.* FROM enterprises e JOIN enterprise_members em ON em.enterprise_id = e.id WHERE em.user_id = $1 AND em.status = 'active'",
    )
    .bind(user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound("Enterprise not found".to_string()))
}

#[derive(Debug, Deserialize)]
struct InterestRequestBody {
    talent_id: Uuid,
    message: String,
}

#[derive(Debug, Deserialize)]
struct PaginationQuery {
    page: Option<i64>,
    per_page: Option<i64>,
}

// ─── Interest Requests ──────────────────────────────────────────

// POST /api/contact/interest
async fn send_interest(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<InterestRequestBody>,
) -> Result<impl IntoResponse, AppError> {
    let enterprise = get_enterprise_for_user(&state, auth.user_id).await?;

    // Rate limit: 5 interest requests per hour per enterprise
    RateLimiter::check(
        &mut state.redis.clone(),
        "contact",
        &enterprise.id.to_string(),
        5,
        3600,
    )
    .await?;

    if body.message.trim().is_empty() || body.message.len() > 2000 {
        return Err(AppError::Validation(
            "Message must be between 1 and 2000 characters".to_string(),
        ));
    }

    // Verify talent exists and is active
    let _talent: crate::models::User = sqlx::query_as(
        "SELECT * FROM users WHERE id = $1 AND profile_active = TRUE AND is_banned = FALSE",
    )
    .bind(body.talent_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound("Talent not found".to_string()))?;

    // Check if blocked
    let blocked: Option<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT talent_id, enterprise_id FROM enterprise_blocks WHERE talent_id = $1 AND enterprise_id = $2",
    )
    .bind(body.talent_id)
    .bind(enterprise.id)
    .fetch_optional(&state.db)
    .await?;

    if blocked.is_some() {
        return Err(AppError::Blocked);
    }

    // Check cooldown (declined within 30 days)
    let cooldown: Option<InterestRequest> = sqlx::query_as(
        "SELECT * FROM interest_requests WHERE enterprise_id = $1 AND talent_id = $2 AND status = 'declined' AND cooldown_until > NOW()",
    )
    .bind(enterprise.id)
    .bind(body.talent_id)
    .fetch_optional(&state.db)
    .await?;

    if let Some(req) = cooldown {
        let until = req
            .cooldown_until
            .map(|d| d.to_rfc3339())
            .unwrap_or_default();
        return Err(AppError::CooldownActive(until));
    }

    // Check no pending request
    let pending: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM interest_requests WHERE enterprise_id = $1 AND talent_id = $2 AND status = 'pending'",
    )
    .bind(enterprise.id)
    .bind(body.talent_id)
    .fetch_optional(&state.db)
    .await?;

    if pending.is_some() {
        return Err(AppError::AlreadyRequested);
    }

    // Phase 3.9 — atomic credit gating. 1 credit per interest request.
    // We insert first, then spend ; if spend fails, we roll the insert back.
    let mut tx = state.db.begin().await?;
    let request: InterestRequest = sqlx::query_as(
        r#"
        INSERT INTO interest_requests (enterprise_id, sender_id, talent_id, initial_message)
        VALUES ($1, $2, $3, $4)
        RETURNING *
        "#,
    )
    .bind(enterprise.id)
    .bind(auth.user_id)
    .bind(body.talent_id)
    .bind(body.message.trim())
    .fetch_one(&mut *tx)
    .await?;
    // Spend 1 credit guarded by atomic balance check. On failure, rolling back the
    // tx undoes the insert. Done before commit so we never leak free contacts.
    let spend_amount = crate::services::credits::dec(crate::services::credits::SPEND_INTEREST_REQUEST_AMOUNT);
    let spend_result = sqlx::query(
        r#"
        UPDATE enterprise_credits
        SET balance = balance - $1, total_used = total_used + $1, updated_at = NOW()
        WHERE enterprise_id = $2 AND balance >= $1
        RETURNING balance
        "#,
    )
    .bind(&spend_amount)
    .bind(enterprise.id)
    .fetch_optional(&mut *tx)
    .await?;
    if spend_result.is_none() {
        return Err(AppError::Validation(
            "Insufficient credits — recharge the enterprise account before contacting talents.".into(),
        ));
    }
    sqlx::query(
        r#"
        INSERT INTO credit_transactions
            (enterprise_id, delta, balance_after, reason, related_interest_request_id, actor_user_id)
        SELECT $1, -$2, balance, 'spend_interest_request', $3, $4
        FROM enterprise_credits WHERE enterprise_id = $1
        "#,
    )
    .bind(enterprise.id)
    .bind(&spend_amount)
    .bind(request.id)
    .bind(auth.user_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    metrics::counter!("skilluv_credits_spent_total").increment(1);

    // Send notification to talent
    NotificationService::send(
        &state.db,
        &mut state.redis.clone(),
        &state.ws,
        body.talent_id,
        "interest_request_received",
        &format!("{} souhaite te contacter", enterprise.company_name),
        Some(&body.message[..body.message.len().min(100)]),
        Some(json!({
            "request_id": request.id,
            "enterprise_id": enterprise.id,
            "enterprise_name": enterprise.company_name,
        })),
    )
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(build_response(json!({
            "interest_request": request,
            "message": "Interest request sent"
        }))),
    ))
}

// GET /api/contact/interest/sent
async fn sent_requests(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let enterprise = get_enterprise_for_user(&state, auth.user_id).await?;

    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(20).clamp(1, 50);
    let offset = (page - 1) * per_page;

    let requests: Vec<InterestRequest> = sqlx::query_as(
        "SELECT * FROM interest_requests WHERE enterprise_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3",
    )
    .bind(enterprise.id)
    .bind(per_page)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let total: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM interest_requests WHERE enterprise_id = $1")
            .bind(enterprise.id)
            .fetch_one(&state.db)
            .await?;

    // Fetch talent info
    let talent_ids: Vec<Uuid> = requests.iter().map(|r| r.talent_id).collect();
    let talents: Vec<(Uuid, String, String, String)> =
        sqlx::query_as("SELECT id, username, display_name, title FROM users WHERE id = ANY($1)")
            .bind(&talent_ids)
            .fetch_all(&state.db)
            .await?;

    let talent_map: std::collections::HashMap<Uuid, _> =
        talents.into_iter().map(|t| (t.0, t)).collect();

    let results: Vec<serde_json::Value> = requests
        .iter()
        .map(|r| {
            let talent = talent_map.get(&r.talent_id);
            json!({
                "id": r.id,
                "talent_id": r.talent_id,
                "talent_username": talent.map(|t| &t.1),
                "talent_display_name": talent.map(|t| &t.2),
                "status": r.status,
                "initial_message": r.initial_message,
                "created_at": r.created_at.to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(json!({
        "data": results,
        "pagination": {
            "page": page,
            "per_page": per_page,
            "total": total,
            "total_pages": (total as f64 / per_page as f64).ceil() as i64,
        },
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })))
}

// GET /api/contact/interest/received
async fn received_requests(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(20).clamp(1, 50);
    let offset = (page - 1) * per_page;

    let requests: Vec<InterestRequest> = sqlx::query_as(
        "SELECT * FROM interest_requests WHERE talent_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3",
    )
    .bind(auth.user_id)
    .bind(per_page)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let total: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM interest_requests WHERE talent_id = $1")
            .bind(auth.user_id)
            .fetch_one(&state.db)
            .await?;

    // Fetch enterprise info
    let enterprise_ids: Vec<Uuid> = requests.iter().map(|r| r.enterprise_id).collect();
    let enterprises: Vec<(Uuid, String, Option<String>)> =
        sqlx::query_as("SELECT id, company_name, logo_url FROM enterprises WHERE id = ANY($1)")
            .bind(&enterprise_ids)
            .fetch_all(&state.db)
            .await?;

    let ent_map: std::collections::HashMap<Uuid, _> =
        enterprises.into_iter().map(|e| (e.0, e)).collect();

    let results: Vec<serde_json::Value> = requests
        .iter()
        .map(|r| {
            let ent = ent_map.get(&r.enterprise_id);
            json!({
                "id": r.id,
                "enterprise_id": r.enterprise_id,
                "enterprise_name": ent.map(|e| &e.1),
                "enterprise_logo": ent.and_then(|e| e.2.as_ref()),
                "status": r.status,
                "initial_message": r.initial_message,
                "created_at": r.created_at.to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(json!({
        "data": results,
        "pagination": {
            "page": page,
            "per_page": per_page,
            "total": total,
            "total_pages": (total as f64 / per_page as f64).ceil() as i64,
        },
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })))
}

// POST /api/contact/interest/:id/accept
async fn accept_interest(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let request: InterestRequest = sqlx::query_as(
        "SELECT * FROM interest_requests WHERE id = $1 AND talent_id = $2 AND status = 'pending'",
    )
    .bind(id)
    .bind(auth.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound(
        "Interest request not found or already handled".to_string(),
    ))?;

    // Accept the request
    sqlx::query(
        "UPDATE interest_requests SET status = 'accepted', accepted_at = NOW() WHERE id = $1",
    )
    .bind(id)
    .execute(&state.db)
    .await?;

    // Create conversation
    let conversation: Conversation = sqlx::query_as(
        "INSERT INTO conversations (interest_request_id, enterprise_id, talent_id) VALUES ($1, $2, $3) RETURNING *",
    )
    .bind(id)
    .bind(request.enterprise_id)
    .bind(auth.user_id)
    .fetch_one(&state.db)
    .await?;

    // Insert initial message into conversation
    sqlx::query("INSERT INTO messages (conversation_id, sender_id, content) VALUES ($1, $2, $3)")
        .bind(conversation.id)
        .bind(request.sender_id)
        .bind(&request.initial_message)
        .execute(&state.db)
        .await?;

    // Notify enterprise
    NotificationService::send(
        &state.db,
        &mut state.redis.clone(),
        &state.ws,
        request.sender_id,
        "interest_accepted",
        "Demande d'intérêt acceptée",
        Some("Le talent a accepté votre demande. La conversation est ouverte."),
        Some(json!({
            "conversation_id": conversation.id,
            "request_id": id,
        })),
    )
    .await?;

    Ok(Json(build_response(json!({
        "conversation": conversation,
        "message": "Interest request accepted. Conversation opened."
    }))))
}

// POST /api/contact/interest/:id/decline
async fn decline_interest(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let request: InterestRequest = sqlx::query_as(
        "SELECT * FROM interest_requests WHERE id = $1 AND talent_id = $2 AND status = 'pending'",
    )
    .bind(id)
    .bind(auth.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound(
        "Interest request not found or already handled".to_string(),
    ))?;

    sqlx::query(
        "UPDATE interest_requests SET status = 'declined', declined_at = NOW(), cooldown_until = NOW() + INTERVAL '30 days' WHERE id = $1",
    )
    .bind(id)
    .execute(&state.db)
    .await?;

    // Phase 3.9 — refund 50% of the credit to the enterprise on decline
    if let Some(spend_txn) = sqlx::query_as::<_, (Uuid,)>(
        "SELECT id FROM credit_transactions WHERE related_interest_request_id = $1 AND reason = 'spend_interest_request' LIMIT 1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    {
        let _ = crate::services::credits::refund_spend(
            &state.db,
            spend_txn.0,
            crate::services::credits::REFUND_RATIO_PARTIAL,
            "refund_refused",
        )
        .await;
        metrics::counter!("skilluv_credits_refunded_total", "reason" => "refused").increment(1);
    }

    // Notify enterprise
    NotificationService::send(
        &state.db,
        &mut state.redis.clone(),
        &state.ws,
        request.sender_id,
        "interest_declined",
        "Demande d'intérêt déclinée",
        Some("50% du crédit a été remboursé sur ton solde."),
        Some(json!({ "request_id": id })),
    )
    .await?;

    Ok(Json(build_response(json!({
        "message": "Interest request declined"
    }))))
}

// ─── Conversations ──────────────────────────────────────────────

// GET /api/contact/conversations
async fn list_conversations(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, AppError> {
    // User can be either talent or enterprise member
    let conversations: Vec<Conversation> = sqlx::query_as(
        r#"
        SELECT c.* FROM conversations c
        WHERE c.talent_id = $1
           OR c.enterprise_id IN (
               SELECT enterprise_id FROM enterprise_members WHERE user_id = $1 AND status = 'active'
           )
        ORDER BY c.created_at DESC
        "#,
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await?;

    // Get last message + unread count for each conversation
    let mut results = Vec::new();
    for conv in &conversations {
        let last_msg: Option<Message> = sqlx::query_as(
            "SELECT * FROM messages WHERE conversation_id = $1 ORDER BY created_at DESC LIMIT 1",
        )
        .bind(conv.id)
        .fetch_optional(&state.db)
        .await?;

        let unread: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM messages WHERE conversation_id = $1 AND sender_id != $2 AND read_at IS NULL",
        )
        .bind(conv.id)
        .bind(auth.user_id)
        .fetch_one(&state.db)
        .await?;

        // Get other party info
        let other_info = if conv.talent_id == auth.user_id {
            // I'm the talent, get enterprise info
            let ent: Option<(String, Option<String>)> =
                sqlx::query_as("SELECT company_name, logo_url FROM enterprises WHERE id = $1")
                    .bind(conv.enterprise_id)
                    .fetch_optional(&state.db)
                    .await?;
            json!({
                "type": "enterprise",
                "name": ent.as_ref().map(|e| &e.0),
                "logo_url": ent.as_ref().and_then(|e| e.1.as_ref()),
            })
        } else {
            // I'm enterprise, get talent info
            let talent: Option<(String, String)> =
                sqlx::query_as("SELECT username, display_name FROM users WHERE id = $1")
                    .bind(conv.talent_id)
                    .fetch_optional(&state.db)
                    .await?;
            json!({
                "type": "talent",
                "username": talent.as_ref().map(|t| &t.0),
                "display_name": talent.as_ref().map(|t| &t.1),
            })
        };

        results.push(json!({
            "id": conv.id,
            "closed": conv.closed,
            "other_party": other_info,
            "last_message": last_msg.as_ref().map(|m| json!({
                "content": &m.content[..m.content.len().min(100)],
                "sender_id": m.sender_id,
                "created_at": m.created_at.to_rfc3339(),
            })),
            "unread_count": unread,
            "created_at": conv.created_at.to_rfc3339(),
        }));
    }

    Ok(Json(build_response(json!({
        "conversations": results,
    }))))
}

// GET /api/contact/conversations/:id
async fn get_conversation(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let conversation: Conversation = sqlx::query_as(
        r#"
        SELECT c.* FROM conversations c
        WHERE c.id = $1 AND (
            c.talent_id = $2
            OR c.enterprise_id IN (
                SELECT enterprise_id FROM enterprise_members WHERE user_id = $2 AND status = 'active'
            )
        )
        "#,
    )
    .bind(id)
    .bind(auth.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound("Conversation not found".to_string()))?;

    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(50).clamp(1, 100);
    let offset = (page - 1) * per_page;

    let messages: Vec<Message> = sqlx::query_as(
        "SELECT * FROM messages WHERE conversation_id = $1 ORDER BY created_at ASC LIMIT $2 OFFSET $3",
    )
    .bind(id)
    .bind(per_page)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE conversation_id = $1")
        .bind(id)
        .fetch_one(&state.db)
        .await?;

    // Mark messages from the other party as read
    sqlx::query(
        "UPDATE messages SET read_at = NOW() WHERE conversation_id = $1 AND sender_id != $2 AND read_at IS NULL",
    )
    .bind(id)
    .bind(auth.user_id)
    .execute(&state.db)
    .await?;

    Ok(Json(json!({
        "data": {
            "conversation": conversation,
            "messages": messages,
        },
        "pagination": {
            "page": page,
            "per_page": per_page,
            "total": total,
            "total_pages": (total as f64 / per_page as f64).ceil() as i64,
        },
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })))
}

#[derive(Debug, Deserialize)]
struct SendMessageBody {
    content: String,
}

// POST /api/contact/conversations/:id/messages
async fn send_message(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<SendMessageBody>,
) -> Result<impl IntoResponse, AppError> {
    if body.content.trim().is_empty() || body.content.len() > 5000 {
        return Err(AppError::Validation(
            "Message must be between 1 and 5000 characters".to_string(),
        ));
    }

    let conversation: Conversation = sqlx::query_as(
        r#"
        SELECT c.* FROM conversations c
        WHERE c.id = $1 AND (
            c.talent_id = $2
            OR c.enterprise_id IN (
                SELECT enterprise_id FROM enterprise_members WHERE user_id = $2 AND status = 'active'
            )
        )
        "#,
    )
    .bind(id)
    .bind(auth.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound("Conversation not found".to_string()))?;

    if conversation.closed {
        return Err(AppError::ConversationClosed);
    }

    let message: Message = sqlx::query_as(
        "INSERT INTO messages (conversation_id, sender_id, content) VALUES ($1, $2, $3) RETURNING *",
    )
    .bind(id)
    .bind(auth.user_id)
    .bind(body.content.trim())
    .fetch_one(&state.db)
    .await?;

    // Determine recipient
    let recipient_id = if conversation.talent_id == auth.user_id {
        // Talent sent message → notify enterprise owner/sender
        // Find the sender from the interest request
        let sender_id: Option<Uuid> =
            sqlx::query_scalar("SELECT sender_id FROM interest_requests WHERE id = $1")
                .bind(conversation.interest_request_id)
                .fetch_optional(&state.db)
                .await?;
        sender_id.unwrap_or(conversation.enterprise_id) // fallback
    } else {
        conversation.talent_id
    };

    // Get sender display name
    let sender_name: String = sqlx::query_scalar("SELECT display_name FROM users WHERE id = $1")
        .bind(auth.user_id)
        .fetch_one(&state.db)
        .await?;

    NotificationService::send(
        &state.db,
        &mut state.redis.clone(),
        &state.ws,
        recipient_id,
        "new_message",
        &format!("Nouveau message de {sender_name}"),
        Some(&body.content[..body.content.len().min(100)]),
        Some(json!({
            "conversation_id": id,
            "message_id": message.id,
        })),
    )
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(build_response(json!({ "message": message }))),
    ))
}

// ─── Blocking ───────────────────────────────────────────────────

// POST /api/contact/block/:enterprise_id
async fn block_enterprise(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(enterprise_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    // Insert block
    sqlx::query(
        "INSERT INTO enterprise_blocks (talent_id, enterprise_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(auth.user_id)
    .bind(enterprise_id)
    .execute(&state.db)
    .await?;

    // Close any open conversations
    sqlx::query(
        "UPDATE conversations SET closed = TRUE WHERE talent_id = $1 AND enterprise_id = $2 AND closed = FALSE",
    )
    .bind(auth.user_id)
    .bind(enterprise_id)
    .execute(&state.db)
    .await?;

    Ok(Json(build_response(json!({
        "message": "Enterprise blocked"
    }))))
}

// DELETE /api/contact/block/:enterprise_id
async fn unblock_enterprise(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(enterprise_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    sqlx::query("DELETE FROM enterprise_blocks WHERE talent_id = $1 AND enterprise_id = $2")
        .bind(auth.user_id)
        .bind(enterprise_id)
        .execute(&state.db)
        .await?;

    Ok(Json(build_response(json!({
        "message": "Enterprise unblocked"
    }))))
}
