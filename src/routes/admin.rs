use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::models::Challenge;
use crate::services::LeaderboardService;

pub fn admin_routes() -> Router<AppState> {
    Router::new()
        .route("/admin/challenges", post(create_challenge))
        .route("/admin/challenges", get(list_all_challenges))
        .route("/admin/challenges/{id}", put(update_challenge))
        .route("/admin/challenges/{id}/publish", post(publish_challenge))
        .route("/admin/challenges/{id}/archive", post(archive_challenge))
        .route("/admin/stats", get(admin_stats))
        .route("/admin/leaderboards/rebuild", post(rebuild_leaderboards))
        // /admin/audit-log/generic = Phase 1.18 generic audit_log table.
        // /admin/audit-log (in admin_moderation.rs) reads the legacy admin_audit_log table.
        .route("/admin/audit-log/generic", get(list_audit_log))
        // Enterprise B2B SSO — visibility on active IdP-authenticated sessions.
        .route("/admin/sso/sessions", get(list_sso_sessions))
        .route("/admin/sso/sessions/{id}/revoke", post(revoke_sso_session))
}

#[derive(Debug, Deserialize)]
struct AuditLogQuery {
    actor_type: Option<String>,
    actor_id: Option<Uuid>,
    action: Option<String>,
    target_type: Option<String>,
    target_id: Option<Uuid>,
    page: Option<i64>,
    per_page: Option<i64>,
}

async fn list_audit_log(
    State(state): State<AppState>,
    auth: AuthUser,
    axum::extract::Query(q): axum::extract::Query<AuditLogQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    if auth.role != "admin" {
        return Err(AppError::Forbidden);
    }
    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(50).clamp(1, 200);
    let offset = (page - 1) * per_page;

    let rows: Vec<sqlx::postgres::PgRow> = sqlx::query(
        r#"
        SELECT id, actor_type, actor_id, action, target_type, target_id, metadata, ip, user_agent, created_at
        FROM audit_log
        WHERE ($1::text IS NULL OR actor_type = $1)
          AND ($2::uuid IS NULL OR actor_id = $2)
          AND ($3::text IS NULL OR action = $3)
          AND ($4::text IS NULL OR target_type = $4)
          AND ($5::uuid IS NULL OR target_id = $5)
        ORDER BY created_at DESC
        LIMIT $6 OFFSET $7
        "#,
    )
    .bind(&q.actor_type)
    .bind(q.actor_id)
    .bind(&q.action)
    .bind(&q.target_type)
    .bind(q.target_id)
    .bind(per_page)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    use sqlx::Row;
    let entries: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.get::<Uuid, _>("id"),
                "actor_type": r.get::<String, _>("actor_type"),
                "actor_id": r.get::<Option<Uuid>, _>("actor_id"),
                "action": r.get::<String, _>("action"),
                "target_type": r.get::<Option<String>, _>("target_type"),
                "target_id": r.get::<Option<Uuid>, _>("target_id"),
                "metadata": r.get::<Option<serde_json::Value>, _>("metadata"),
                "ip": r.get::<Option<String>, _>("ip"),
                "user_agent": r.get::<Option<String>, _>("user_agent"),
                "created_at": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(json!({
        "data": entries,
        "pagination": {
            "page": page,
            "per_page": per_page,
            "returned": entries.len(),
        },
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })))
}

#[derive(Debug, Deserialize)]
struct CreateChallengeRequest {
    title: String,
    description: String,
    instructions: String,
    skill_domain: String,
    difficulty: i16,
    mode: Option<String>,
    duration_minutes: Option<i32>,
    /// **Deprecated en P8** au profit de `ai_policy`. Toujours accepté pour
    /// backward compat : si `ai_policy` n'est pas fourni, on dérive de `ai_allowed`.
    ai_allowed: Option<bool>,
    /// Politique IA typée. Valeurs : unrestricted | disclosure_required |
    /// human_verified | no_ai_declared | ai_native. Défaut : disclosure_required.
    ai_policy: Option<String>,
    tone: Option<String>,
    language: Option<String>,
    prerequisite_fragments: Option<i32>,
    reward_fragments: Option<i32>,
    is_onboarding: Option<bool>,
    /// Introduit en P8.1 pour aligner avec la règle dure #1.
    is_training: Option<bool>,
    /// Lien projet réel (règle dure #1).
    project_id: Option<Uuid>,
    expected_output: Option<String>,
    test_cases: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct UpdateChallengeRequest {
    title: Option<String>,
    description: Option<String>,
    instructions: Option<String>,
    skill_domain: Option<String>,
    difficulty: Option<i16>,
    mode: Option<String>,
    duration_minutes: Option<i32>,
    ai_allowed: Option<bool>,
    ai_policy: Option<String>,
    tone: Option<String>,
    language: Option<String>,
    prerequisite_fragments: Option<i32>,
    reward_fragments: Option<i32>,
    is_training: Option<bool>,
    project_id: Option<Uuid>,
    expected_output: Option<String>,
    test_cases: Option<serde_json::Value>,
}

const VALID_AI_POLICIES: &[&str] = &[
    "unrestricted",
    "disclosure_required",
    "human_verified",
    "no_ai_declared",
    "ai_native",
];

/// Résout `ai_policy` selon les données du body :
/// 1. Si `ai_policy` explicite fourni → valide et utilise
/// 2. Sinon, si `ai_allowed` fourni → dérive : true → 'unrestricted', false → 'no_ai_declared'
/// 3. Sinon → défaut 'disclosure_required'
fn resolve_ai_policy(
    ai_policy: Option<&str>,
    ai_allowed: Option<bool>,
) -> Result<String, AppError> {
    if let Some(p) = ai_policy {
        if !VALID_AI_POLICIES.contains(&p) {
            return Err(AppError::Validation(format!(
                "Invalid ai_policy '{p}'; valid: {VALID_AI_POLICIES:?}"
            )));
        }
        return Ok(p.to_string());
    }
    match ai_allowed {
        Some(true) => Ok("unrestricted".to_string()),
        Some(false) => Ok("no_ai_declared".to_string()),
        None => Ok("disclosure_required".to_string()),
    }
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

async fn require_admin(state: &AppState, auth: &AuthUser) -> Result<(), AppError> {
    let role: String = sqlx::query_scalar("SELECT role FROM users WHERE id = $1")
        .bind(auth.user_id)
        .fetch_one(&state.db)
        .await?;

    if role != "admin" {
        return Err(AppError::Forbidden);
    }
    Ok(())
}

// POST /api/admin/challenges
async fn create_challenge(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateChallengeRequest>,
) -> Result<impl IntoResponse, AppError> {
    require_admin(&state, &auth).await?;

    if body.title.trim().is_empty() || body.title.len() > 200 {
        return Err(AppError::Validation(
            "title must be between 1 and 200 characters".to_string(),
        ));
    }
    if body.description.trim().is_empty() {
        return Err(AppError::Validation("description is required".to_string()));
    }
    if body.instructions.trim().is_empty() {
        return Err(AppError::Validation(
            "instructions are required".to_string(),
        ));
    }
    if !(1..=5).contains(&body.difficulty) {
        return Err(AppError::Validation(
            "difficulty must be between 1 and 5".to_string(),
        ));
    }

    // Résolution ai_policy (P8.1) : privilégie ai_policy explicite, fallback dérivé
    // depuis ai_allowed pour backward compat.
    let ai_policy = resolve_ai_policy(body.ai_policy.as_deref(), body.ai_allowed)?;
    let ai_allowed_bool = matches!(ai_policy.as_str(), "unrestricted" | "ai_native");

    // is_training auto-marqué si is_onboarding=true (règle dure #1 aligned)
    let is_onboarding = body.is_onboarding.unwrap_or(false);
    let is_training = body.is_training.unwrap_or(is_onboarding);

    let challenge: Challenge = sqlx::query_as(
        r#"
        INSERT INTO challenges (
            title, description, instructions, skill_domain, difficulty,
            mode, duration_minutes, ai_allowed, ai_policy, tone, language,
            prerequisite_fragments, reward_fragments, is_onboarding, is_training,
            project_id, expected_output, test_cases, created_by, status
        ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,'draft')
        RETURNING *
        "#,
    )
    .bind(body.title.trim())
    .bind(body.description.trim())
    .bind(body.instructions.trim())
    .bind(&body.skill_domain)
    .bind(body.difficulty)
    .bind(body.mode.as_deref().unwrap_or("solo"))
    .bind(body.duration_minutes)
    .bind(ai_allowed_bool)
    .bind(&ai_policy)
    .bind(body.tone.as_deref().unwrap_or("serious"))
    .bind(&body.language)
    .bind(body.prerequisite_fragments.unwrap_or(0))
    .bind(body.reward_fragments.unwrap_or(10))
    .bind(is_onboarding)
    .bind(is_training)
    .bind(body.project_id)
    .bind(&body.expected_output)
    .bind(&body.test_cases)
    .bind(auth.user_id)
    .fetch_one(&state.db)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(build_response(json!({ "challenge": challenge }))),
    ))
}

// GET /api/admin/challenges
async fn list_all_challenges(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin(&state, &auth).await?;

    let challenges: Vec<Challenge> =
        sqlx::query_as("SELECT * FROM challenges ORDER BY created_at DESC")
            .fetch_all(&state.db)
            .await?;

    let total = challenges.len();

    Ok(Json(build_response(json!({
        "challenges": challenges,
        "total": total,
    }))))
}

// PUT /api/admin/challenges/:id
async fn update_challenge(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateChallengeRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin(&state, &auth).await?;

    let existing: Challenge = sqlx::query_as("SELECT * FROM challenges WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound("Challenge not found".to_string()))?;

    // P8.1 : accepte ai_policy en update. Si non fourni, on garde l'existant.
    // Si ai_policy fourni → valide + met à jour ai_allowed en cohérence
    // (le temps que la colonne ai_allowed soit droppée en P8.3).
    let ai_policy = match (body.ai_policy.as_deref(), body.ai_allowed) {
        (Some(_), _) | (None, Some(_)) => {
            resolve_ai_policy(body.ai_policy.as_deref(), body.ai_allowed)?
        }
        (None, None) => existing.ai_policy.clone(),
    };
    let ai_allowed_bool = matches!(ai_policy.as_str(), "unrestricted" | "ai_native");

    let challenge: Challenge = sqlx::query_as(
        r#"
        UPDATE challenges SET
            title = $1, description = $2, instructions = $3, skill_domain = $4,
            difficulty = $5, mode = $6, duration_minutes = $7, ai_allowed = $8,
            ai_policy = $9, tone = $10, language = $11, prerequisite_fragments = $12,
            reward_fragments = $13, is_training = $14, project_id = $15,
            expected_output = $16, test_cases = $17,
            updated_at = NOW()
        WHERE id = $18
        RETURNING *
        "#,
    )
    .bind(body.title.as_deref().unwrap_or(&existing.title))
    .bind(body.description.as_deref().unwrap_or(&existing.description))
    .bind(
        body.instructions
            .as_deref()
            .unwrap_or(&existing.instructions),
    )
    .bind(
        body.skill_domain
            .as_deref()
            .unwrap_or(&existing.skill_domain),
    )
    .bind(body.difficulty.unwrap_or(existing.difficulty))
    .bind(body.mode.as_deref().unwrap_or(&existing.mode))
    .bind(body.duration_minutes.or(existing.duration_minutes))
    .bind(ai_allowed_bool)
    .bind(&ai_policy)
    .bind(body.tone.as_deref().unwrap_or(&existing.tone))
    .bind(body.language.as_ref().or(existing.language.as_ref()))
    .bind(
        body.prerequisite_fragments
            .unwrap_or(existing.prerequisite_fragments),
    )
    .bind(body.reward_fragments.unwrap_or(existing.reward_fragments))
    .bind(body.is_training.unwrap_or(existing.is_training))
    .bind(body.project_id.or(existing.project_id))
    .bind(
        body.expected_output
            .as_ref()
            .or(existing.expected_output.as_ref()),
    )
    .bind(body.test_cases.as_ref().or(existing.test_cases.as_ref()))
    .bind(id)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(build_response(json!({ "challenge": challenge }))))
}

// POST /api/admin/challenges/:id/publish
async fn publish_challenge(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin(&state, &auth).await?;

    let challenge: Challenge = sqlx::query_as(
        "UPDATE challenges SET status = 'published', updated_at = NOW() WHERE id = $1 RETURNING *",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound("Challenge not found".to_string()))?;

    Ok(Json(build_response(json!({ "challenge": challenge }))))
}

// POST /api/admin/challenges/:id/archive
async fn archive_challenge(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin(&state, &auth).await?;

    let challenge: Challenge = sqlx::query_as(
        "UPDATE challenges SET status = 'archived', updated_at = NOW() WHERE id = $1 RETURNING *",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound("Challenge not found".to_string()))?;

    Ok(Json(build_response(json!({ "challenge": challenge }))))
}

// GET /api/admin/stats
async fn admin_stats(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin(&state, &auth).await?;

    let total_users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&state.db)
        .await?;
    let active_users: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE profile_active = TRUE")
            .fetch_one(&state.db)
            .await?;
    let total_challenges: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM challenges")
        .fetch_one(&state.db)
        .await?;
    let published_challenges: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM challenges WHERE status = 'published'")
            .fetch_one(&state.db)
            .await?;
    let total_submissions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM challenge_submissions")
        .fetch_one(&state.db)
        .await?;

    let (ws_connections, ws_rooms, ws_users) = state.ws.stats().await;

    Ok(Json(build_response(json!({
        "users": { "total": total_users, "active": active_users },
        "challenges": { "total": total_challenges, "published": published_challenges },
        "submissions": { "total": total_submissions },
        "websocket": { "connections": ws_connections, "rooms": ws_rooms, "users": ws_users },
    }))))
}

// POST /api/admin/leaderboards/rebuild
async fn rebuild_leaderboards(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin(&state, &auth).await?;

    LeaderboardService::seed_from_db(&mut state.redis.clone(), &state.db).await?;

    Ok(Json(build_response(json!({
        "message": "Leaderboards rebuilt successfully"
    }))))
}

// ─── Enterprise SSO admin visibility ─────────────────────────────

#[derive(Debug, Deserialize)]
struct SsoSessionsQuery {
    enterprise_id: Option<Uuid>,
    page: Option<i64>,
    per_page: Option<i64>,
}

/// GET /api/admin/sso/sessions — list active SSO-authenticated sessions.
///
/// Joins `user_sessions` × `users` × `enterprise_members` × `enterprises` so
/// operators can see who is currently logged in via an external IdP, from
/// which enterprise, and revoke sessions on demand.
async fn list_sso_sessions(
    State(state): State<AppState>,
    auth: AuthUser,
    axum::extract::Query(q): axum::extract::Query<SsoSessionsQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin(&state, &auth).await?;

    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(50).clamp(1, 200);
    let offset = (page - 1) * per_page;

    let rows: Vec<(
        Uuid,
        Uuid,
        Option<String>,
        Option<String>,
        chrono::DateTime<chrono::Utc>,
        chrono::DateTime<chrono::Utc>,
        String,
        String,
        Option<Uuid>,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(
        r#"
        SELECT
            us.id, us.user_id, us.ip, us.user_agent,
            us.created_at, us.last_used_at,
            u.email, u.username,
            e.id AS enterprise_id, e.slug AS enterprise_slug, e.company_name
        FROM user_sessions us
        JOIN users u ON u.id = us.user_id
        LEFT JOIN enterprise_members em
            ON em.user_id = us.user_id AND em.status = 'active'
        LEFT JOIN enterprises e ON e.id = em.enterprise_id
        WHERE us.login_method = 'sso'
          AND us.revoked_at IS NULL
          AND ($1::UUID IS NULL OR e.id = $1)
        ORDER BY us.last_used_at DESC
        LIMIT $2 OFFSET $3
        "#,
    )
    .bind(q.enterprise_id)
    .bind(per_page)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let total: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM user_sessions us
        LEFT JOIN enterprise_members em
            ON em.user_id = us.user_id AND em.status = 'active'
        WHERE us.login_method = 'sso'
          AND us.revoked_at IS NULL
          AND ($1::UUID IS NULL OR em.enterprise_id = $1)
        "#,
    )
    .bind(q.enterprise_id)
    .fetch_one(&state.db)
    .await?;

    let sessions: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|(id, user_id, ip, ua, created_at, last_used, email, username, ent_id, ent_slug, company)| {
            json!({
                "session_id": id,
                "user_id": user_id,
                "user_email": email,
                "user_username": username,
                "enterprise_id": ent_id,
                "enterprise_slug": ent_slug,
                "company_name": company,
                "ip": ip,
                "user_agent": ua,
                "created_at": created_at.to_rfc3339(),
                "last_used_at": last_used.to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(json!({
        "data": { "sessions": sessions },
        "pagination": {
            "page": page,
            "per_page": per_page,
            "total": total,
        },
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })))
}

/// POST /api/admin/sso/sessions/{id}/revoke — kill a specific SSO session.
async fn revoke_sso_session(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(session_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin(&state, &auth).await?;

    let affected = sqlx::query(
        "UPDATE user_sessions SET revoked_at = NOW()
         WHERE id = $1 AND login_method = 'sso' AND revoked_at IS NULL",
    )
    .bind(session_id)
    .execute(&state.db)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(AppError::NotFound(
            "SSO session not found or already revoked".into(),
        ));
    }
    Ok(Json(build_response(json!({ "revoked": true }))))
}
