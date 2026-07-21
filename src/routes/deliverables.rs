//! Routes HTTP pour les `deliverables` (Phase P2.1).
//!
//! Endpoints publics :
//!   GET   /api/deliverables/{id}                  — détail d'un deliverable
//!   GET   /api/users/{user_id}/deliverables       — portfolio public d'un user
//!
//! Endpoint webhook :
//!   POST  /api/webhooks/github/slices/{project_id}
//!         — reçoit les événements pull_request.closed merged=true et crée le
//!           deliverable auto-vérifié (workflow G.1)
//!
//! Voir docs/challenges-target-model-and-roadmap.md partie G.1 pour le workflow détaillé.

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use hmac::{Hmac, KeyInit, Mac};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::Sha256;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::services::{DeliverablesService, PrMergedOutcome, PrMergedParams};

type HmacSha256 = Hmac<Sha256>;

pub fn deliverable_routes() -> Router<AppState> {
    Router::new()
        .route("/deliverables/{id}", get(get_deliverable))
        .route("/users/{user_id}/deliverables", get(list_user_deliverables))
        .route(
            "/webhooks/github/slices/{project_id}",
            post(github_slices_webhook),
        )
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

// ═══════════════════════════════════════════════════════════════════
// GET /api/deliverables/{id}
// ═══════════════════════════════════════════════════════════════════

async fn get_deliverable(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let deliverable = DeliverablesService::get(&state.db, id).await?;

    // On ne retourne pas les deliverables non-public sans auth
    // (Phase P2.1 : simple check, une future itération ajoutera un check owner)
    if !deliverable.public || deliverable.revoked_at.is_some() {
        return Err(AppError::NotFound("Deliverable not found".to_string()));
    }

    Ok(Json(build_response(json!({ "deliverable": deliverable }))))
}

// ═══════════════════════════════════════════════════════════════════
// GET /api/users/{user_id}/deliverables
// ═══════════════════════════════════════════════════════════════════

#[derive(Deserialize)]
struct UserDeliverablesQuery {
    limit: Option<i64>,
}

async fn list_user_deliverables(
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
    Query(q): Query<UserDeliverablesQuery>,
) -> Result<Json<Value>, AppError> {
    let limit = q.limit.unwrap_or(20);
    let deliverables = DeliverablesService::list_public_by_user(&state.db, user_id, limit).await?;
    Ok(Json(build_response(
        json!({ "deliverables": deliverables }),
    )))
}

// ═══════════════════════════════════════════════════════════════════
// POST /api/webhooks/github/slices/{project_id}
// ═══════════════════════════════════════════════════════════════════

async fn github_slices_webhook(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<impl IntoResponse, AppError> {
    // 1. HMAC signature verification (partagée avec le webhook bounties existant)
    let secret = std::env::var("GITHUB_WEBHOOK_SECRET")
        .ok()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::Internal("GITHUB_WEBHOOK_SECRET not set".to_string()))?;

    let signature = headers
        .get("x-hub-signature-256")
        .and_then(|v| v.to_str().ok())
        .ok_or(AppError::Unauthorized)?;

    let expected = format!("sha256={}", {
        let mut mac = <HmacSha256 as KeyInit>::new_from_slice(secret.as_bytes())
            .map_err(|_| AppError::Internal("hmac init".to_string()))?;
        mac.update(&body);
        hex::encode(mac.finalize().into_bytes())
    });

    if !constant_time_eq(signature.as_bytes(), expected.as_bytes()) {
        return Err(AppError::Unauthorized);
    }

    // 2. Extract event metadata
    let delivery_id = headers
        .get("x-github-delivery")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let event_type = headers
        .get("x-github-event")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    // 3. Idempotence via github_webhook_events (partagée avec le webhook bounties)
    //    Note : on log le prefix `slices:` sur delivery_id pour distinguer les
    //    deux consommateurs, sinon un même delivery pourrait bloquer les deux flows.
    let scoped_delivery_id = format!("slices:{delivery_id}");
    let already: Option<(String,)> =
        sqlx::query_as("SELECT delivery_id FROM github_webhook_events WHERE delivery_id = $1")
            .bind(&scoped_delivery_id)
            .fetch_optional(&state.db)
            .await?;

    if already.is_some() {
        return Ok(Json(build_response(
            json!({ "duplicate": true, "outcome": "no_op" }),
        )));
    }

    let payload: Value = serde_json::from_slice(&body)
        .map_err(|e| AppError::Validation(format!("github payload decode: {e}")))?;

    // 4. Enregistre l'événement (idempotence future)
    sqlx::query(
        "INSERT INTO github_webhook_events (delivery_id, event_type, payload)
         VALUES ($1, $2, $3)",
    )
    .bind(&scoped_delivery_id)
    .bind(&event_type)
    .bind(&payload)
    .execute(&state.db)
    .await?;

    // 5. Filtre l'événement : seuls les PR merged nous intéressent en P2.1
    if event_type != "pull_request" {
        return Ok(Json(build_response(
            json!({ "outcome": "ignored", "reason": "event type not pull_request" }),
        )));
    }
    let action = payload.get("action").and_then(|v| v.as_str()).unwrap_or("");
    if action != "closed" {
        return Ok(Json(build_response(
            json!({ "outcome": "ignored", "reason": "action not closed" }),
        )));
    }
    let pr = payload.get("pull_request").cloned().unwrap_or(Value::Null);
    let merged = pr.get("merged").and_then(|v| v.as_bool()).unwrap_or(false);
    if !merged {
        return Ok(Json(build_response(
            json!({ "outcome": "ignored", "reason": "PR closed without merge" }),
        )));
    }

    // 6. Extract PR params
    let params = extract_pr_merged_params(project_id, &payload, &pr)?;

    // 7. Delegate to service (workflow G.1)
    let outcome = DeliverablesService::create_from_pr_merged(&state.db, params).await?;

    // 8. Broadcast WebSocket events on verified
    if let PrMergedOutcome::Verified { deliverable_id } = &outcome {
        broadcast_verified(&state, *deliverable_id).await;
    }

    Ok(Json(build_response(json!({ "outcome": outcome }))))
}

fn extract_pr_merged_params(
    project_id: Uuid,
    payload: &Value,
    pr: &Value,
) -> Result<PrMergedParams, AppError> {
    let repo = payload
        .get("repository")
        .ok_or_else(|| AppError::Validation("missing repository".to_string()))?;

    let repo_owner = repo
        .get("owner")
        .and_then(|o| o.get("login"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("missing repository.owner.login".to_string()))?
        .to_string();

    let repo_name = repo
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("missing repository.name".to_string()))?
        .to_string();

    let pr_number = pr
        .get("number")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| AppError::Validation("missing pull_request.number".to_string()))?
        as i32;

    let pr_url = pr
        .get("html_url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let pr_body = pr
        .get("body")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let merge_commit_sha = pr
        .get("merge_commit_sha")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("missing pull_request.merge_commit_sha".to_string()))?
        .to_string();

    let github_login = pr
        .get("user")
        .and_then(|u| u.get("login"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let commits_count = pr.get("commits").and_then(|v| v.as_i64()).map(|n| n as i32);
    let additions = pr
        .get("additions")
        .and_then(|v| v.as_i64())
        .map(|n| n as i32);
    let deletions = pr
        .get("deletions")
        .and_then(|v| v.as_i64())
        .map(|n| n as i32);
    let files_changed = pr
        .get("changed_files")
        .and_then(|v| v.as_i64())
        .map(|n| n as i32);

    let base_branch = pr
        .get("base")
        .and_then(|b| b.get("ref"))
        .and_then(|v| v.as_str())
        .map(str::to_string);

    Ok(PrMergedParams {
        project_id,
        repo_owner,
        repo_name,
        pr_number,
        pr_url,
        pr_body,
        merge_commit_sha,
        github_login,
        commits_count,
        additions,
        deletions,
        files_changed,
        base_branch,
    })
}

async fn broadcast_verified(state: &AppState, deliverable_id: Uuid) {
    // Fetch minimal info for the broadcast (best-effort, ne bloque pas le webhook)
    let row: Result<Option<(Uuid, i32)>, sqlx::Error> =
        sqlx::query_as("SELECT user_id, fragments_awarded FROM deliverables WHERE id = $1")
            .bind(deliverable_id)
            .fetch_optional(&state.db)
            .await;

    let Ok(Some((user_id, fragments))) = row else {
        return;
    };

    state
        .ws
        .send_to_user(
            user_id,
            crate::websocket::WsMessage {
                event: "deliverable.verified".to_string(),
                room: None,
                payload: json!({
                    "deliverable_id": deliverable_id,
                    "fragments_awarded": fragments,
                }),
            },
        )
        .await;

    if fragments > 0 {
        state
            .ws
            .send_to_user(
                user_id,
                crate::websocket::WsMessage {
                    event: "fragment.earned".to_string(),
                    room: None,
                    payload: json!({
                        "fragments_earned": fragments,
                        "source": "deliverable",
                    }),
                },
            )
            .await;
    }
}

/// Constant-time byte comparison for HMAC verification.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut result = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        result |= x ^ y;
    }
    result == 0
}
