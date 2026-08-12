//! Routes HTTP pour les attestations (Phase P5 * LAUNCH).
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
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::attestations::Attestation;
use crate::services::{AttestationsService, CompagnonnageParams};

pub fn attestation_routes() -> Router<AppState> {
    Router::new()
        .route("/users/{user_id}/attestations", get(list_user_attestations))
        .route("/attestations/verify/{code}", get(verify_attestation))
        .route("/attestations/compagnonnage", post(issue_compagnonnage))
        .route("/attestations/{id}/revoke", post(revoke_attestation))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserAttestationsResponse {
    pub attestations: Vec<Attestation>,
}

/// Verification result — one of the three shapes an attestation
/// lookup can produce: valid, revoked, or not-found. Discriminated on
/// `valid` + `reason`.
#[derive(Debug, Serialize, ToSchema)]
pub struct AttestationVerifyResponse {
    pub valid: bool,
    /// Present only when `valid == false`. `"revoked"` or `"not_found"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attestation: Option<Attestation>,
    /// Present only when `valid == true`. Client-facing URL echoed for
    /// convenience.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_url: Option<String>,
}

// ═══════════════════════════════════════════════════════════════════
// GET /api/users/{user_id}/attestations
// ═══════════════════════════════════════════════════════════════════

/// Public portfolio: every non-revoked attestation the user has been
/// awarded. Used by the profile page and the recruiter portfolio view.
#[utoipa::path(
    get,
    path = "/api/users/{user_id}/attestations",
    tag = "profile",
    params(("user_id" = Uuid, Path, description = "User UUID")),
    responses(
        (status = 200, description = "User's public attestations", body = ApiResponse<UserAttestationsResponse>),
    ),
)]
pub async fn list_user_attestations(
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
) -> Result<Json<ApiResponse<UserAttestationsResponse>>, AppError> {
    let attestations = AttestationsService::list_public_by_user(&state.db, user_id).await?;
    Ok(Json(ApiResponse::new(UserAttestationsResponse {
        attestations,
    })))
}

// ═══════════════════════════════════════════════════════════════════
// GET /api/attestations/verify/{code}
// ═══════════════════════════════════════════════════════════════════

/// Public verification endpoint. Returns 200 with a discriminated
/// `AttestationVerifyResponse` for all three outcomes (valid, revoked,
/// not-found) so third-party verifiers can render each state without
/// re-implementing error decoding.
#[utoipa::path(
    get,
    path = "/api/attestations/verify/{code}",
    tag = "profile",
    params(("code" = String, Path, description = "Verification code")),
    responses(
        (status = 200, description = "Verification result", body = ApiResponse<AttestationVerifyResponse>),
    ),
)]
pub async fn verify_attestation(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> Result<Json<ApiResponse<AttestationVerifyResponse>>, AppError> {
    let attestation = AttestationsService::verify_by_code(&state.db, &code).await?;

    let resp = match attestation {
        Some(a) if a.revoked_at.is_none() => AttestationVerifyResponse {
            valid: true,
            reason: None,
            verification_url: Some(format!("/attestations/verify/{}", a.verification_code)),
            attestation: Some(a),
        },
        Some(a) => AttestationVerifyResponse {
            valid: false,
            reason: Some("revoked".to_string()),
            attestation: Some(a),
            verification_url: None,
        },
        None => AttestationVerifyResponse {
            valid: false,
            reason: Some("not_found".to_string()),
            attestation: None,
            verification_url: None,
        },
    };
    Ok(Json(ApiResponse::new(resp)))
}

// ═══════════════════════════════════════════════════════════════════
// POST /api/attestations/compagnonnage
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, ToSchema)]
pub struct CompagnonnageBody {
    pub user_id: Uuid,
    pub project_id: Uuid,
    #[schema(max_length = 10000)]
    pub title: String,
    #[schema(max_length = 10000)]
    pub description: String,
    pub linked_deliverable_ids: Vec<Uuid>,
    pub linked_skill_node_ids: Vec<Uuid>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct IssueAttestationResponse {
    pub attestation_id: Uuid,
    pub message: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RevokeBody {
    #[schema(max_length = 10000)]
    pub reason: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RevokeAttestationResponse {
    pub attestation_id: Uuid,
    pub revoked: bool,
}

/// Steward-only: issue a compagnonnage attestation to a user for
/// work on a project. Caller must be an active steward on the
/// project OR an admin.
#[utoipa::path(
    post,
    path = "/api/attestations/compagnonnage",
    tag = "profile",
    request_body = CompagnonnageBody,
    responses(
        (status = 200, description = "Attestation issued", body = ApiResponse<IssueAttestationResponse>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Caller is not a steward of the project", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn issue_compagnonnage(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CompagnonnageBody>,
) -> Result<Json<ApiResponse<IssueAttestationResponse>>, AppError> {
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
    // SKI-43 — live variant: this path has AppState, so the recipient gets
    // the WebSocket / mobile push as well as the persisted notification.
    let db_clone = state.db.clone();
    let mut redis_clone = state.redis.clone();
    let ws_clone = state.ws.clone();
    tokio::spawn(async move {
        let _ = crate::services::proof_hooks::recompute_all_for_user_live(
            &db_clone,
            &mut redis_clone,
            &ws_clone,
            recipient_id,
        )
        .await;
    });

    Ok(Json(ApiResponse::new(IssueAttestationResponse {
        attestation_id: id,
        message: "Compagnonnage attestation issued.".to_string(),
    })))
}

// ═══════════════════════════════════════════════════════════════════
// POST /api/attestations/{id}/revoke
// ═══════════════════════════════════════════════════════════════════

/// Admin only: revoke an attestation with an audit reason.
#[utoipa::path(
    post,
    path = "/api/attestations/{id}/revoke",
    tag = "admin",
    params(("id" = Uuid, Path, description = "Attestation UUID")),
    request_body = RevokeBody,
    responses(
        (status = 200, description = "Attestation revoked", body = ApiResponse<RevokeAttestationResponse>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not an admin", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn revoke_attestation(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<RevokeBody>,
) -> Result<Json<ApiResponse<RevokeAttestationResponse>>, AppError> {
    // Réservé aux admins
    let role: String = sqlx::query_scalar("SELECT role FROM users WHERE id = $1")
        .bind(auth.user_id)
        .fetch_one(&state.db)
        .await?;
    if role != "admin" {
        return Err(AppError::Forbidden);
    }

    AttestationsService::revoke(&state.db, id, Some(auth.user_id), body.reason).await?;
    Ok(Json(ApiResponse::new(RevokeAttestationResponse {
        attestation_id: id,
        revoked: true,
    })))
}
