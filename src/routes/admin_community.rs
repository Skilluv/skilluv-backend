use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::models::ChallengeTemplate;
use crate::services::NotificationService;

pub fn admin_community_routes() -> Router<AppState> {
    Router::new()
        .route("/admin/community/review", get(pending_review))
        .route("/admin/community/{id}/approve", post(approve_challenge))
        .route("/admin/community/{id}/reject", post(reject_challenge))
}

// P21.1 : délègue à user_capabilities (source de vérité canonique).
async fn require_admin(state: &AppState, auth: &AuthUser) -> Result<(), AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RejectRequest {
    /// Feedback shown to the creator and stored in `review_feedback`.
    pub feedback: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CreatorSummary {
    pub username: String,
    pub display_name: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EnrichedChallenge {
    pub challenge: ChallengeTemplate,
    /// Present when the challenge has a `created_by` row.
    pub creator: Option<CreatorSummary>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PendingReviewResponse {
    pub challenges: Vec<EnrichedChallenge>,
    pub total: usize,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminChallengeDecisionResponse {
    pub challenge: ChallengeTemplate,
    pub message: String,
}

/// Community challenges awaiting admin review. Creator info is joined
/// so the admin panel doesn't need N+1 lookups.
#[utoipa::path(
    get,
    path = "/api/admin/community/review",
    tag = "admin",
    responses(
        (status = 200, description = "Pending review", body = ApiResponse<PendingReviewResponse>),
        (status = 403, description = "Not an admin", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn pending_review(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<PendingReviewResponse>>, AppError> {
    require_admin(&state, &auth).await?;

    let challenges: Vec<ChallengeTemplate> = sqlx::query_as(
        "SELECT * FROM challenge_templates WHERE is_community = TRUE AND community_status = 'review' ORDER BY created_at ASC",
    )
    .fetch_all(&state.db)
    .await?;

    // Get creator info
    let creator_ids: Vec<Option<Uuid>> = challenges.iter().map(|c| c.created_by).collect();
    let valid_ids: Vec<Uuid> = creator_ids.iter().filter_map(|id| *id).collect();

    let creators: Vec<(Uuid, String, String)> =
        sqlx::query_as("SELECT id, username, display_name FROM users WHERE id = ANY($1)")
            .bind(&valid_ids)
            .fetch_all(&state.db)
            .await?;

    let creator_map: std::collections::HashMap<Uuid, _> =
        creators.into_iter().map(|c| (c.0, c)).collect();

    let enriched: Vec<EnrichedChallenge> = challenges
        .into_iter()
        .map(|c| {
            let creator =
                c.created_by
                    .and_then(|id| creator_map.get(&id))
                    .map(|cr| CreatorSummary {
                        username: cr.1.clone(),
                        display_name: cr.2.clone(),
                    });
            EnrichedChallenge {
                challenge: c,
                creator,
            }
        })
        .collect();

    let total = enriched.len();
    Ok(Json(ApiResponse::new(PendingReviewResponse {
        challenges: enriched,
        total,
    })))
}

/// Approve a community challenge: bumps status to `published`, notifies
/// the creator, audit-logs the decision.
#[utoipa::path(
    post,
    path = "/api/admin/community/{id}/approve",
    tag = "admin",
    params(("id" = Uuid, Path, description = "Challenge UUID")),
    responses(
        (status = 200, description = "Approved and published", body = ApiResponse<AdminChallengeDecisionResponse>),
        (status = 403, description = "Not an admin", body = crate::api_response::ErrorResponse),
        (status = 404, description = "Challenge not in review", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn approve_challenge(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<AdminChallengeDecisionResponse>>, AppError> {
    require_admin(&state, &auth).await?;

    // Trello hVImXbUS — pré-check business avant UPDATE.
    // Règle dure P3 (migration 0061 + trigger PG) : aucun challenge ne peut
    // passer status='published' sans project_id, sauf si is_training=TRUE.
    // Si on tente l'UPDATE sans respecter la règle, le trigger renvoie une
    // erreur DB qui remonte en 500. On préfère renvoyer 400 avec un message
    // clair pour que le front admin puisse guider l'action de l'admin.
    let precheck: Option<(bool, Option<Uuid>, String)> = sqlx::query_as(
        "SELECT is_training, project_id, community_status \
         FROM challenge_templates \
         WHERE id = $1 AND is_community = TRUE",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?;
    let Some((is_training, project_id, community_status)) = precheck else {
        return Err(AppError::NotFound(
            "Challenge not found or not a community challenge".into(),
        ));
    };
    if community_status != "review" {
        return Err(AppError::Validation(format!(
            "Challenge is in '{community_status}' state, only 'review' can be approved"
        )));
    }
    if !is_training && project_id.is_none() {
        return Err(AppError::Validation(
            "Community challenge must be linked to a project (project_id) \
             or flagged as training (is_training=true) before approval — \
             ask the creator to attach one, or set is_training via \
             PATCH /admin/challenges/{id}"
                .into(),
        ));
    }

    let challenge: ChallengeTemplate = sqlx::query_as(
        r#"
        UPDATE challenge_templates SET
            community_status = 'approved',
            status = 'published',
            updated_at = NOW()
        WHERE id = $1 AND is_community = TRUE AND community_status = 'review'
        RETURNING *
        "#,
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound(
        "Challenge not found or not in review".to_string(),
    ))?;

    // Notify creator
    if let Some(creator_id) = challenge.created_by {
        NotificationService::send(
            &state.db,
            &mut state.redis.clone(),
            &state.ws,
            crate::services::notification::NotificationPayload {
                user_id: creator_id,
                notification_type: "challenge_approved",
                title: &format!("Ton challenge '{}' a été approuvé !", challenge.title),
                body: Some("Il est maintenant visible par tous les utilisateurs."),
                data: Some(json!({ "challenge_id": id })),
            },
        )
        .await?;
    }

    // BE-F — audit log unifié.
    crate::services::audit::record(
        &state.db,
        crate::services::audit::AuditEntry {
            actor_type: crate::services::audit::ActorType::Admin,
            actor_id: Some(auth.user_id),
            action: "community_challenge_approve",
            target_type: Some("challenge_template"),
            target_id: Some(id),
            metadata: Some(json!({ "title": challenge.title })),
            headers: None,
        },
    )
    .await;

    Ok(Json(ApiResponse::new(AdminChallengeDecisionResponse {
        challenge,
        message: "Challenge approved and published".to_string(),
    })))
}

/// Reject a community challenge with feedback. Notifies creator +
/// audit-logs the decision.
#[utoipa::path(
    post,
    path = "/api/admin/community/{id}/reject",
    tag = "admin",
    params(("id" = Uuid, Path, description = "Challenge UUID")),
    request_body = RejectRequest,
    responses(
        (status = 200, description = "Rejected", body = ApiResponse<AdminChallengeDecisionResponse>),
        (status = 403, description = "Not an admin", body = crate::api_response::ErrorResponse),
        (status = 404, description = "Challenge not in review", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn reject_challenge(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<RejectRequest>,
) -> Result<Json<ApiResponse<AdminChallengeDecisionResponse>>, AppError> {
    require_admin(&state, &auth).await?;

    let challenge: ChallengeTemplate = sqlx::query_as(
        r#"
        UPDATE challenge_templates SET
            community_status = 'rejected',
            review_feedback = $1,
            updated_at = NOW()
        WHERE id = $2 AND is_community = TRUE AND community_status = 'review'
        RETURNING *
        "#,
    )
    .bind(&body.feedback)
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound(
        "Challenge not found or not in review".to_string(),
    ))?;

    // Notify creator
    if let Some(creator_id) = challenge.created_by {
        NotificationService::send(
            &state.db,
            &mut state.redis.clone(),
            &state.ws,
            crate::services::notification::NotificationPayload {
                user_id: creator_id,
                notification_type: "challenge_rejected",
                title: &format!("Ton challenge '{}' n'a pas été retenu", challenge.title),
                body: Some(&body.feedback),
                data: Some(json!({ "challenge_id": id })),
            },
        )
        .await?;
    }

    // BE-F — audit log unifié.
    crate::services::audit::record(
        &state.db,
        crate::services::audit::AuditEntry {
            actor_type: crate::services::audit::ActorType::Admin,
            actor_id: Some(auth.user_id),
            action: "community_challenge_reject",
            target_type: Some("challenge_template"),
            target_id: Some(id),
            metadata: Some(json!({
                "title": challenge.title,
                "feedback": body.feedback,
            })),
            headers: None,
        },
    )
    .await;

    Ok(Json(ApiResponse::new(AdminChallengeDecisionResponse {
        challenge,
        message: "Challenge rejected".to_string(),
    })))
}
