use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::AppState;
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

fn build_response(data: serde_json::Value) -> serde_json::Value {
    json!({
        "data": data,
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })
}

// P21.1 : délègue à user_capabilities (source de vérité canonique).
async fn require_admin(state: &AppState, auth: &AuthUser) -> Result<(), AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await
}

#[derive(Debug, Deserialize)]
struct RejectRequest {
    feedback: String,
}

// GET /api/admin/community/review — challenges awaiting review
async fn pending_review(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, AppError> {
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

    let enriched: Vec<serde_json::Value> = challenges
        .iter()
        .map(|c| {
            let creator = c.created_by.and_then(|id| creator_map.get(&id));
            json!({
                "challenge": c,
                "creator": creator.map(|cr| json!({
                    "username": cr.1,
                    "display_name": cr.2,
                })),
            })
        })
        .collect();

    Ok(Json(build_response(json!({
        "challenges": enriched,
        "total": enriched.len(),
    }))))
}

// POST /api/admin/community/:id/approve
async fn approve_challenge(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin(&state, &auth).await?;

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
            creator_id,
            "challenge_approved",
            &format!("Ton challenge '{}' a été approuvé !", challenge.title),
            Some("Il est maintenant visible par tous les utilisateurs."),
            Some(json!({ "challenge_id": id })),
        )
        .await?;
    }

    Ok(Json(build_response(json!({
        "challenge": challenge,
        "message": "Challenge approved and published"
    }))))
}

// POST /api/admin/community/:id/reject
async fn reject_challenge(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<RejectRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
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
            creator_id,
            "challenge_rejected",
            &format!("Ton challenge '{}' n'a pas été retenu", challenge.title),
            Some(&body.feedback),
            Some(json!({ "challenge_id": id })),
        )
        .await?;
    }

    Ok(Json(build_response(json!({
        "challenge": challenge,
        "message": "Challenge rejected"
    }))))
}
