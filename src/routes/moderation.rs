//! FE-M9 — Routes modération inline accessibles aux modérateurs non-admin.
//!
//! Le panneau admin utilise `/admin/*` (dans admin_gate = admin origin + 2FA).
//! Les modérateurs communautaires (forum_moderator, community_curator,
//! plagiarism_reviewer) travaillent depuis le front principal donc ces routes
//! vivent hors admin_gate, avec `require_any_capability` comme gate.
//!
//! Endpoints (6) :
//!   - GET  /api/community/challenges/review              (community_curator | admin)
//!   - POST /api/community/challenges/{id}/approve|reject (community_curator | admin)
//!   - GET  /api/fraud/deliverables/flagged               (plagiarism_reviewer | admin)
//!   - POST /api/fraud/deliverables/{id}/mark-valid|revoke (plagiarism_reviewer | admin)
//!   - POST /api/forum/posts/{id}/moderate                (forum_moderator | admin)
//!   - POST /api/forum/users/{id}/mute                    (forum_moderator | admin)

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;

pub fn moderation_routes() -> Router<AppState> {
    Router::new()
        // Community curation.
        .route("/community/challenges/review", get(community_review_queue))
        .route(
            "/community/challenges/{id}/approve",
            post(community_challenge_approve),
        )
        .route(
            "/community/challenges/{id}/reject",
            post(community_challenge_reject),
        )
        // Fraud review (P14.5 + P25).
        .route("/fraud/deliverables/flagged", get(fraud_flagged_list))
        .route(
            "/fraud/deliverables/{id}/mark-valid",
            post(fraud_mark_valid),
        )
        .route("/fraud/deliverables/{id}/revoke", post(fraud_revoke))
        // Forum moderation.
        .route("/forum/posts/{id}/moderate", post(forum_moderate_post))
        .route("/forum/users/{id}/mute", post(forum_mute_user))
}

fn wrap(data: Value) -> Value {
    json!({
        "data": data,
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })
}

#[derive(Debug, Deserialize)]
struct PaginationQuery {
    #[serde(default)]
    page: Option<i64>,
    #[serde(default)]
    per_page: Option<i64>,
}

// ═══════════════════════════════════════════════════════════════════
// Community curation
// ═══════════════════════════════════════════════════════════════════

async fn community_review_queue(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<PaginationQuery>,
) -> Result<Json<Value>, AppError> {
    crate::middleware::capabilities::require_any_capability(
        &state.db,
        auth.user_id,
        &["community_curator", "admin"],
    )
    .await?;

    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(20).clamp(1, 100);
    let offset = (page - 1) * per_page;

    let rows: Vec<(
        Uuid,
        String,
        String,
        Option<String>,
        Option<Uuid>,
        chrono::DateTime<chrono::Utc>,
    )> = sqlx::query_as(
        r#"SELECT id, title, description, review_feedback, created_by, created_at
               FROM challenge_templates
               WHERE is_community = TRUE AND community_status = 'review'
               ORDER BY created_at ASC
               LIMIT $1 OFFSET $2"#,
    )
    .bind(per_page)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;
    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM challenge_templates
         WHERE is_community = TRUE AND community_status = 'review'",
    )
    .fetch_one(&state.db)
    .await?;

    let items: Vec<Value> = rows
        .into_iter()
        .map(|(id, title, desc, fb, creator, created)| {
            json!({
                "id": id, "title": title, "description": desc,
                "review_feedback": fb, "created_by": creator,
                "created_at": created.to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(json!({
        "data": items,
        "pagination": {
            "page": page, "per_page": per_page, "total": total,
            "total_pages": if per_page > 0 { (total + per_page - 1) / per_page } else { 0 },
        },
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })))
}

async fn community_challenge_approve(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    crate::middleware::capabilities::require_any_capability(
        &state.db,
        auth.user_id,
        &["community_curator", "admin"],
    )
    .await?;

    let (title,): (String,) = sqlx::query_as(
        r#"UPDATE challenge_templates
           SET community_status = 'approved', status = 'published', updated_at = NOW()
           WHERE id = $1 AND is_community = TRUE AND community_status = 'review'
           RETURNING title"#,
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("challenge not in review".into()))?;

    crate::services::audit::record(
        &state.db,
        crate::services::audit::AuditEntry {
            actor_type: crate::services::audit::ActorType::User,
            actor_id: Some(auth.user_id),
            action: "community_challenge_approve",
            target_type: Some("challenge_template"),
            target_id: Some(id),
            metadata: Some(json!({ "title": title })),
            headers: None,
        },
    )
    .await;

    Ok(Json(wrap(
        json!({ "approved": true, "id": id, "title": title }),
    )))
}

#[derive(Debug, Deserialize)]
struct RejectBody {
    feedback: String,
}

async fn community_challenge_reject(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<RejectBody>,
) -> Result<Json<Value>, AppError> {
    crate::middleware::capabilities::require_any_capability(
        &state.db,
        auth.user_id,
        &["community_curator", "admin"],
    )
    .await?;

    if body.feedback.trim().len() < 8 {
        return Err(AppError::Validation(
            "feedback must be at least 8 chars".into(),
        ));
    }

    let (title,): (String,) = sqlx::query_as(
        r#"UPDATE challenge_templates
           SET community_status = 'rejected', review_feedback = $1, updated_at = NOW()
           WHERE id = $2 AND is_community = TRUE AND community_status = 'review'
           RETURNING title"#,
    )
    .bind(&body.feedback)
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("challenge not in review".into()))?;

    crate::services::audit::record(
        &state.db,
        crate::services::audit::AuditEntry {
            actor_type: crate::services::audit::ActorType::User,
            actor_id: Some(auth.user_id),
            action: "community_challenge_reject",
            target_type: Some("challenge_template"),
            target_id: Some(id),
            metadata: Some(json!({ "title": title, "feedback": body.feedback })),
            headers: None,
        },
    )
    .await;

    Ok(Json(wrap(
        json!({ "rejected": true, "id": id, "title": title }),
    )))
}

// ═══════════════════════════════════════════════════════════════════
// Fraud review
// ═══════════════════════════════════════════════════════════════════

async fn fraud_flagged_list(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<PaginationQuery>,
) -> Result<Json<Value>, AppError> {
    crate::middleware::capabilities::require_any_capability(
        &state.db,
        auth.user_id,
        &["plagiarism_reviewer", "admin"],
    )
    .await?;

    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(20).clamp(1, 100);
    let offset = (page - 1) * per_page;

    let rows: Vec<(
        Uuid,
        Uuid,
        String,
        Option<serde_json::Value>,
        chrono::DateTime<chrono::Utc>,
    )> = sqlx::query_as(
        r#"SELECT id, user_id, verification_status, verification_signal, submitted_at
               FROM deliverables
               WHERE (verification_status = 'flagged'
                      OR verification_signal ? 'plagiarism_flag')
               ORDER BY submitted_at DESC
               LIMIT $1 OFFSET $2"#,
    )
    .bind(per_page)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let items: Vec<Value> = rows
        .into_iter()
        .map(|(id, uid, status, signal, ts)| {
            json!({
                "id": id, "user_id": uid, "verification_status": status,
                "verification_signal": signal, "submitted_at": ts.to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(json!({
        "data": items,
        "pagination": { "page": page, "per_page": per_page },
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })))
}

#[derive(Debug, Deserialize)]
struct ReasonBody {
    #[serde(default)]
    reason: Option<String>,
}

async fn fraud_mark_valid(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ReasonBody>,
) -> Result<Json<Value>, AppError> {
    crate::middleware::capabilities::require_any_capability(
        &state.db,
        auth.user_id,
        &["plagiarism_reviewer", "admin"],
    )
    .await?;

    let affected = sqlx::query(
        r#"UPDATE deliverables
           SET verification_status = 'verified',
               verified_at = NOW(),
               verification_signal = COALESCE(verification_signal, '{}'::jsonb)
                    || jsonb_build_object('plagiarism_flag', FALSE,
                                           'moderator_review', jsonb_build_object(
                                               'action', 'mark_valid',
                                               'reason', $2::text))
           WHERE id = $1"#,
    )
    .bind(id)
    .bind(&body.reason)
    .execute(&state.db)
    .await?
    .rows_affected();
    if affected == 0 {
        return Err(AppError::NotFound(format!("deliverable {id} not found")));
    }

    crate::services::audit::record(
        &state.db,
        crate::services::audit::AuditEntry {
            actor_type: crate::services::audit::ActorType::User,
            actor_id: Some(auth.user_id),
            action: "fraud_mark_valid",
            target_type: Some("deliverable"),
            target_id: Some(id),
            metadata: Some(json!({ "reason": body.reason })),
            headers: None,
        },
    )
    .await;

    Ok(Json(wrap(json!({ "marked_valid": true, "id": id }))))
}

async fn fraud_revoke(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ReasonBody>,
) -> Result<Json<Value>, AppError> {
    crate::middleware::capabilities::require_any_capability(
        &state.db,
        auth.user_id,
        &["plagiarism_reviewer", "admin"],
    )
    .await?;

    let reason = body.reason.clone().unwrap_or_default();
    if reason.trim().len() < 8 {
        return Err(AppError::Validation(
            "reason must be at least 8 chars".into(),
        ));
    }

    let affected = sqlx::query(
        r#"UPDATE deliverables
           SET verification_status = 'revoked',
               verification_signal = COALESCE(verification_signal, '{}'::jsonb)
                    || jsonb_build_object('moderator_review', jsonb_build_object(
                            'action', 'revoke', 'reason', $2::text))
           WHERE id = $1"#,
    )
    .bind(id)
    .bind(&reason)
    .execute(&state.db)
    .await?
    .rows_affected();
    if affected == 0 {
        return Err(AppError::NotFound(format!("deliverable {id} not found")));
    }

    crate::services::audit::record(
        &state.db,
        crate::services::audit::AuditEntry {
            actor_type: crate::services::audit::ActorType::User,
            actor_id: Some(auth.user_id),
            action: "fraud_revoke_deliverable",
            target_type: Some("deliverable"),
            target_id: Some(id),
            metadata: Some(json!({ "reason": reason })),
            headers: None,
        },
    )
    .await;

    Ok(Json(wrap(json!({ "revoked": true, "id": id }))))
}

// ═══════════════════════════════════════════════════════════════════
// Forum moderation
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct ModeratePostBody {
    /// `hide` (soft-delete) | `lock` | `unlock` | `unhide`.
    action: String,
    reason: String,
}

async fn forum_moderate_post(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ModeratePostBody>,
) -> Result<Json<Value>, AppError> {
    crate::middleware::capabilities::require_any_capability(
        &state.db,
        auth.user_id,
        &["forum_moderator", "admin"],
    )
    .await?;

    if body.reason.trim().len() < 8 {
        return Err(AppError::Validation(
            "reason must be at least 8 chars".into(),
        ));
    }

    let sql = match body.action.as_str() {
        "hide" => "UPDATE posts SET deleted_at = NOW() WHERE id = $1",
        "unhide" => "UPDATE posts SET deleted_at = NULL WHERE id = $1",
        "lock" => "UPDATE posts SET locked = TRUE WHERE id = $1",
        "unlock" => "UPDATE posts SET locked = FALSE WHERE id = $1",
        _ => {
            return Err(AppError::Validation(
                "action must be one of: hide|unhide|lock|unlock".into(),
            ));
        }
    };
    let affected = sqlx::query(sql)
        .bind(id)
        .execute(&state.db)
        .await?
        .rows_affected();
    if affected == 0 {
        return Err(AppError::NotFound(format!("post {id} not found")));
    }

    crate::services::audit::record(
        &state.db,
        crate::services::audit::AuditEntry {
            actor_type: crate::services::audit::ActorType::User,
            actor_id: Some(auth.user_id),
            action: "forum_moderate_post",
            target_type: Some("post"),
            target_id: Some(id),
            metadata: Some(json!({ "action": body.action, "reason": body.reason })),
            headers: None,
        },
    )
    .await;

    Ok(Json(wrap(
        json!({ "moderated": true, "id": id, "action": body.action }),
    )))
}

#[derive(Debug, Deserialize)]
struct MuteUserBody {
    /// Durée en heures. Défaut 24. Max 168 (7j) pour un moderator ; illimité
    /// pour admin (utiliser is_banned via un autre endpoint).
    #[serde(default)]
    duration_hours: Option<i32>,
    reason: String,
    #[serde(default)]
    scope: Option<String>, // forum | community | all
}

async fn forum_mute_user(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(target_id): Path<Uuid>,
    Json(body): Json<MuteUserBody>,
) -> Result<Json<Value>, AppError> {
    crate::middleware::capabilities::require_any_capability(
        &state.db,
        auth.user_id,
        &["forum_moderator", "admin"],
    )
    .await?;

    if body.reason.trim().len() < 8 {
        return Err(AppError::Validation(
            "reason must be at least 8 chars".into(),
        ));
    }
    let hours = body.duration_hours.unwrap_or(24).clamp(1, 168);
    let scope = body.scope.clone().unwrap_or_else(|| "forum".into());
    if !["forum", "community", "all"].contains(&scope.as_str()) {
        return Err(AppError::Validation(
            "scope must be forum|community|all".into(),
        ));
    }

    let (mute_id, expires_at): (Uuid, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
        r#"INSERT INTO user_mutes (user_id, muted_by, reason, scope, expires_at)
           VALUES ($1, $2, $3, $4, NOW() + MAKE_INTERVAL(hours => $5))
           RETURNING id, expires_at"#,
    )
    .bind(target_id)
    .bind(auth.user_id)
    .bind(&body.reason)
    .bind(&scope)
    .bind(hours)
    .fetch_one(&state.db)
    .await?;

    crate::services::audit::record(
        &state.db,
        crate::services::audit::AuditEntry {
            actor_type: crate::services::audit::ActorType::User,
            actor_id: Some(auth.user_id),
            action: "forum_mute_user",
            target_type: Some("user"),
            target_id: Some(target_id),
            metadata: Some(json!({
                "mute_id": mute_id, "reason": body.reason,
                "scope": scope, "duration_hours": hours,
            })),
            headers: None,
        },
    )
    .await;

    Ok(Json(wrap(json!({
        "muted": true,
        "user_id": target_id,
        "mute_id": mute_id,
        "expires_at": expires_at.to_rfc3339(),
        "scope": scope,
    }))))
}
