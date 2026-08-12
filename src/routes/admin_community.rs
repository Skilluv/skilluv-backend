use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;
use utoipa::{IntoParams, ToSchema};
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
    #[schema(max_length = 10000)]
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
pub struct AdminChallengeDecisionResponse {
    pub challenge: ChallengeTemplate,
    pub message: String,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct PendingReviewQuery {
    #[param(minimum = 1, maximum = 100000)]
    pub page: Option<i64>,
    #[param(minimum = 1, maximum = 100)]
    pub per_page: Option<i64>,
}

/// Response of `GET /admin/community/review`.
///
/// SKI-111 — `EnrichedChallenge` already existed and derives `ToSchema`;
/// only the envelope was missing.
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct PendingReviewResponse {
    pub data: Vec<EnrichedChallenge>,
    pub pagination: crate::api_response::Pagination,
    pub meta: crate::api_response::MetaInfo,
}

/// Community challenges awaiting admin review. Creator info is joined
/// so the admin panel doesn't need N+1 lookups.
#[utoipa::path(
    get,
    path = "/api/admin/community/review",
    tag = "admin",
    params(PendingReviewQuery),
    responses(
        (status = 200, description = "Pending review (paginated)", body = PendingReviewResponse),
        (status = 403, description = "Not an admin", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn pending_review(
    _gate: crate::middleware::admin_gate::AdminGate,
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<PendingReviewQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin(&state, &auth).await?;
    crate::validators::check_range_opt(q.page, "page", 1, 100_000)?;
    crate::validators::check_range_opt(q.per_page, "per_page", 1, 100)?;

    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(20).clamp(1, 100);
    let offset = (page - 1) * per_page;

    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::BIGINT FROM challenge_templates WHERE is_community = TRUE AND community_status = 'review'",
    )
    .fetch_one(&state.db)
    .await?;

    let challenges: Vec<ChallengeTemplate> = sqlx::query_as(
        "SELECT * FROM challenge_templates WHERE is_community = TRUE AND community_status = 'review' ORDER BY created_at ASC LIMIT $1 OFFSET $2",
    )
    .bind(per_page)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let valid_ids: Vec<Uuid> = challenges.iter().filter_map(|c| c.created_by).collect();

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

    let total_pages = if per_page > 0 {
        (total as f64 / per_page as f64).ceil() as i64
    } else {
        0
    };

    Ok(Json(json!({
        "data": enriched,
        "pagination": {
            "page": page,
            "per_page": per_page,
            "total": total,
            "total_pages": total_pages,
        },
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
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
    _gate: crate::middleware::admin_gate::AdminGate,
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
    _gate: crate::middleware::admin_gate::AdminGate,
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
