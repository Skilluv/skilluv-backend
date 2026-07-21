//! Routes HTTP pour les attestations (Phase P5 ⭐ LAUNCH).
//!
//! Endpoints :
//!   GET  /api/users/{user_id}/attestations       — portfolio public (public)
//!   GET  /api/attestations/verify/{code}         — vérification publique (public)
//!   POST /api/attestations/compagnonnage         — émission steward (auth)
//!   POST /api/attestations/{id}/revoke           — révocation admin (auth admin)
//!
//! Voir docs/challenges-target-model-and-roadmap.md sections B.12, G.3, 6.3-6.5.

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::{AttestationsService, CompagnonnageParams};

pub fn attestation_routes() -> Router<AppState> {
    Router::new()
        .route("/users/{user_id}/attestations", get(list_user_attestations))
        .route("/attestations/verify/{code}", get(verify_attestation))
        .route("/attestations/compagnonnage", post(issue_compagnonnage))
        .route("/attestations/{id}/revoke", post(revoke_attestation))
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
// GET /api/users/{user_id}/attestations
// ═══════════════════════════════════════════════════════════════════

async fn list_user_attestations(
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let attestations = AttestationsService::list_public_by_user(&state.db, user_id).await?;
    Ok(Json(build_response(
        json!({ "attestations": attestations }),
    )))
}

// ═══════════════════════════════════════════════════════════════════
// GET /api/attestations/verify/{code}
// ═══════════════════════════════════════════════════════════════════

async fn verify_attestation(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> Result<Json<Value>, AppError> {
    let attestation = AttestationsService::verify_by_code(&state.db, &code).await?;

    match attestation {
        Some(a) if a.revoked_at.is_none() => Ok(Json(build_response(json!({
            "valid": true,
            "attestation": a,
            "verification_url": format!("/attestations/verify/{}", a.verification_code),
        })))),
        Some(a) => Ok(Json(build_response(json!({
            "valid": false,
            "reason": "revoked",
            "attestation": a,
        })))),
        None => Ok(Json(build_response(json!({
            "valid": false,
            "reason": "not_found",
        })))),
    }
}

// ═══════════════════════════════════════════════════════════════════
// POST /api/attestations/compagnonnage
// ═══════════════════════════════════════════════════════════════════

#[derive(Deserialize)]
struct CompagnonnageBody {
    user_id: Uuid,
    project_id: Uuid,
    title: String,
    description: String,
    linked_deliverable_ids: Vec<Uuid>,
    linked_skill_node_ids: Vec<Uuid>,
}

async fn issue_compagnonnage(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CompagnonnageBody>,
) -> Result<Json<Value>, AppError> {
    // Vérifier que le user courant est bien steward du projet
    let is_steward: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM project_stewards
            WHERE project_id = $1 AND user_id = $2 AND ended_at IS NULL
        )
        OR EXISTS (
            SELECT 1 FROM users WHERE id = $2 AND role = 'admin'
        )",
    )
    .bind(body.project_id)
    .bind(auth.user_id)
    .fetch_one(&state.db)
    .await?;

    if !is_steward {
        return Err(AppError::Forbidden);
    }

    let params = CompagnonnageParams {
        user_id: body.user_id,
        project_id: body.project_id,
        title: body.title,
        description: body.description,
        linked_deliverable_ids: body.linked_deliverable_ids,
        linked_skill_node_ids: body.linked_skill_node_ids,
    };

    let recipient_id = params.user_id;
    let id = AttestationsService::issue_compagnonnage(&state.db, auth.user_id, params).await?;

    // P20.1 — Best-effort recompute proof engines pour le récipiendaire.
    // Attestation reçue peut débloquer capability mentor (5 attestations) et
    // les rangs artisan/maitre/doyen (seuils attestations reçues).
    let db_clone = state.db.clone();
    tokio::spawn(async move {
        let _ = crate::services::proof_hooks::recompute_all_for_user(&db_clone, recipient_id).await;
    });

    Ok(Json(build_response(json!({
        "attestation_id": id,
        "message": "Compagnonnage attestation issued."
    }))))
}

// ═══════════════════════════════════════════════════════════════════
// POST /api/attestations/{id}/revoke
// ═══════════════════════════════════════════════════════════════════

#[derive(Deserialize)]
struct RevokeBody {
    reason: String,
}

async fn revoke_attestation(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<RevokeBody>,
) -> Result<Json<Value>, AppError> {
    // Réservé aux admins
    let role: String = sqlx::query_scalar("SELECT role FROM users WHERE id = $1")
        .bind(auth.user_id)
        .fetch_one(&state.db)
        .await?;
    if role != "admin" {
        return Err(AppError::Forbidden);
    }

    AttestationsService::revoke(&state.db, id, Some(auth.user_id), body.reason).await?;
    Ok(Json(build_response(json!({
        "attestation_id": id,
        "revoked": true,
    }))))
}
