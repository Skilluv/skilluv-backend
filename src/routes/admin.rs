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
use crate::models::ChallengeTemplate;
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
        // BE-B — reset 2FA d'un user (TOTP + WebAuthn credentials wiped).
        // Réservé à admin, log audit obligatoire.
        .route("/admin/users/{id}/reset-2fa", post(admin_reset_2fa))
        // IA-C.1 — Générer une variante d'un challenge (harder/easier au MVP).
        .route(
            "/admin/challenges/{id}/variant",
            post(admin_generate_variant),
        )
        // ADM-M3.1 — CRUD orientations + orientation_skill_map.
        .merge(crate::routes::admin_orientation_routes())
        // ADM-M3.2 — CRUD badge_rules (proof engine editor).
        .merge(crate::routes::admin_badge_rule_routes())
        // ADM-M4 — Enterprise type manager.
        .merge(crate::routes::admin_enterprise_routes())
        // ADM-M5 — Recompute proofs + rank override + orientations peek.
        .merge(crate::routes::admin_user_routes())
        // ADM-M5+ — proof-hooks sweep + admin-triggered GDPR export.
        .merge(crate::routes::admin_ops_routes())
        // MVP.md #14 — Skill nodes catalog CRUD.
        .merge(crate::routes::admin_skill_routes())
}

// ═══════════════════════════════════════════════════════════════════
// BE-B — POST /admin/users/{id}/reset-2fa
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct Reset2faBody {
    /// Raison obligatoire (audit trail).
    reason: String,
}

async fn admin_reset_2fa(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(target_user_id): Path<Uuid>,
    Json(body): Json<Reset2faBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await?;

    // BE-D — rate-limit destructif (10/min, 100/heure).
    crate::middleware::admin_destructive::enforce_admin_destructive(&state, auth.user_id).await?;

    if body.reason.trim().len() < 8 {
        return Err(AppError::Validation(
            "reason must be at least 8 chars for audit trail".into(),
        ));
    }

    // BE-D — dry-run : log l'intention et return sans écrire.
    if crate::middleware::admin_destructive::is_admin_dry_run() {
        tracing::info!(
            admin_id = %auth.user_id, target_id = %target_user_id,
            action = "reset_2fa", dry_run = true,
            "BE-D dry-run: reset_2fa skipped"
        );
        return Ok(Json(serde_json::json!({
            "dry_run": true,
            "would_have_done": {
                "action": "reset_2fa",
                "target_user_id": target_user_id,
                "wipes": ["totp_secret", "totp_backup_codes", "webauthn_credentials"],
                "revokes_sessions": true,
            }
        })));
    }

    let mut tx = state.db.begin().await?;

    // 1. Vérifier que le user cible existe.
    let target_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE id = $1)")
            .bind(target_user_id)
            .fetch_one(&mut *tx)
            .await?;
    if !target_exists {
        return Err(AppError::NotFound(format!(
            "user {target_user_id} not found"
        )));
    }

    // 2. Reset TOTP secret + backup codes.
    sqlx::query(
        "UPDATE users
         SET totp_enabled = FALSE,
             totp_secret = NULL,
             email_2fa_enabled = FALSE
         WHERE id = $1",
    )
    .bind(target_user_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM totp_backup_codes WHERE user_id = $1")
        .bind(target_user_id)
        .execute(&mut *tx)
        .await?;

    // 3. Supprimer TOUS les webauthn credentials du user cible.
    sqlx::query("DELETE FROM webauthn_credentials WHERE user_id = $1")
        .bind(target_user_id)
        .execute(&mut *tx)
        .await?;

    // 4. Révoquer toutes les sessions actives du user cible (force re-login).
    sqlx::query(
        "UPDATE user_sessions
         SET revoked_at = NOW()
         WHERE user_id = $1 AND revoked_at IS NULL",
    )
    .bind(target_user_id)
    .execute(&mut *tx)
    .await?;

    // 5. Audit log (schéma legacy admin_audit_log — la migration append-only
    //    et le rôle audit_admin arriveront en BE-E).
    sqlx::query(
        "INSERT INTO admin_audit_log
            (admin_id, action, target_type, target_id, details)
         VALUES ($1, 'reset_2fa', 'user', $2, $3::jsonb)",
    )
    .bind(auth.user_id)
    .bind(target_user_id)
    .bind(serde_json::json!({
        "reason": body.reason,
        "wiped": ["totp_secret", "totp_backup_codes", "webauthn_credentials"],
        "sessions_revoked": true,
    }))
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    metrics::counter!("skilluv_admin_2fa_resets_total").increment(1);

    Ok(Json(serde_json::json!({
        "reset": true,
        "user_id": target_user_id,
        "message": "TOTP, backup codes, and WebAuthn credentials wiped. All sessions revoked. User must re-login and re-configure 2FA."
    })))
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
    /// Politique IA typée. Valeurs : unrestricted | disclosure_required |
    /// human_verified | no_ai_declared | ai_native. Défaut : disclosure_required.
    /// L'ancien `ai_allowed` bool est droppé en P8.3.
    ai_policy: Option<String>,
    tone: Option<String>,
    language: Option<String>,
    reward_fragments: Option<i32>,
    is_onboarding: Option<bool>,
    /// Introduit en P8.1 pour aligner avec la règle dure #1.
    is_training: Option<bool>,
    /// Lien projet réel (règle dure #1).
    project_id: Option<Uuid>,
    expected_output: Option<String>,
    test_cases: Option<serde_json::Value>,
    /// P10.3 : composition team attendue si mode='team'.
    /// Format: JSON array de { role_slug, role_display_name?, required_skill_slug?, min_proficiency_level?, count }.
    team_composition: Option<serde_json::Value>,
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
    ai_policy: Option<String>,
    tone: Option<String>,
    language: Option<String>,
    reward_fragments: Option<i32>,
    is_training: Option<bool>,
    project_id: Option<Uuid>,
    expected_output: Option<String>,
    test_cases: Option<serde_json::Value>,
    /// P10.3 : update de la composition team (JSONB) ou clear si null explicite.
    team_composition: Option<serde_json::Value>,
}

const VALID_AI_POLICIES: &[&str] = &[
    "unrestricted",
    "disclosure_required",
    "human_verified",
    "no_ai_declared",
    "ai_native",
];

/// Résout `ai_policy` : valide si fourni, sinon défaut `disclosure_required`.
///
/// L'ancien fallback via `ai_allowed` est supprimé en P8.3.
fn resolve_ai_policy(ai_policy: Option<&str>) -> Result<String, AppError> {
    match ai_policy {
        Some(p) => {
            if !VALID_AI_POLICIES.contains(&p) {
                return Err(AppError::Validation(format!(
                    "Invalid ai_policy '{p}'; valid: {VALID_AI_POLICIES:?}"
                )));
            }
            Ok(p.to_string())
        }
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

// P21.1 : délègue à la source de vérité canonique (user_capabilities).
// Backfill 0094 garantit que tout users.role='admin' historique a la
// capability. Fait fallback nul — plus de query users.role directe.
async fn require_admin(state: &AppState, auth: &AuthUser) -> Result<(), AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await
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

    let ai_policy = resolve_ai_policy(body.ai_policy.as_deref())?;

    // is_training auto-marqué si is_onboarding=true (règle dure #1 aligned)
    let is_onboarding = body.is_onboarding.unwrap_or(false);
    let is_training = body.is_training.unwrap_or(is_onboarding);

    let challenge: ChallengeTemplate = sqlx::query_as(
        r#"
        INSERT INTO challenge_templates (
            title, description, instructions, skill_domain, difficulty,
            mode, duration_minutes, ai_policy, tone, language,
            reward_fragments, is_onboarding, is_training,
            project_id, expected_output, test_cases, created_by, status,
            team_composition
        ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,'draft',$18)
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
    .bind(&ai_policy)
    .bind(body.tone.as_deref().unwrap_or("serious"))
    .bind(&body.language)
    .bind(body.reward_fragments.unwrap_or(10))
    .bind(is_onboarding)
    .bind(is_training)
    .bind(body.project_id)
    .bind(&body.expected_output)
    .bind(&body.test_cases)
    .bind(auth.user_id)
    .bind(&body.team_composition)
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

    let challenges: Vec<ChallengeTemplate> =
        sqlx::query_as("SELECT * FROM challenge_templates ORDER BY created_at DESC")
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

    let existing: ChallengeTemplate =
        sqlx::query_as("SELECT * FROM challenge_templates WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await?
            .ok_or(AppError::NotFound("Challenge not found".to_string()))?;

    let ai_policy = match body.ai_policy.as_deref() {
        Some(_) => resolve_ai_policy(body.ai_policy.as_deref())?,
        None => existing.ai_policy.clone(),
    };

    let challenge: ChallengeTemplate = sqlx::query_as(
        r#"
        UPDATE challenge_templates SET
            title = $1, description = $2, instructions = $3, skill_domain = $4,
            difficulty = $5, mode = $6, duration_minutes = $7,
            ai_policy = $8, tone = $9, language = $10,
            reward_fragments = $11, is_training = $12, project_id = $13,
            expected_output = $14, test_cases = $15,
            team_composition = $16,
            updated_at = NOW()
        WHERE id = $17
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
    .bind(&ai_policy)
    .bind(body.tone.as_deref().unwrap_or(&existing.tone))
    .bind(body.language.as_ref().or(existing.language.as_ref()))
    .bind(body.reward_fragments.unwrap_or(existing.reward_fragments))
    .bind(body.is_training.unwrap_or(existing.is_training))
    .bind(body.project_id.or(existing.project_id))
    .bind(
        body.expected_output
            .as_ref()
            .or(existing.expected_output.as_ref()),
    )
    .bind(body.test_cases.as_ref().or(existing.test_cases.as_ref()))
    .bind(
        body.team_composition
            .as_ref()
            .or(existing.team_composition.as_ref()),
    )
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

    // Pre-check règle dure #1 : un challenge publishé doit être is_training=TRUE
    // ou avoir un project_id. La contrainte challenges_project_or_training (mig
    // 0061) le refuserait autrement avec un DB error opaque ; on renvoie une
    // erreur explicite côté API.
    let (is_training, project_id): (bool, Option<Uuid>) =
        sqlx::query_as("SELECT is_training, project_id FROM challenge_templates WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await?
            .ok_or(AppError::NotFound("Challenge not found".to_string()))?;

    if !is_training && project_id.is_none() {
        return Err(AppError::Validation(
            "Cannot publish : challenge doit être is_training=TRUE ou avoir un project_id (règle dure #1)".to_string(),
        ));
    }

    let challenge: ChallengeTemplate = sqlx::query_as(
        "UPDATE challenge_templates SET status = 'published', updated_at = NOW() WHERE id = $1 RETURNING *",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(build_response(json!({ "challenge": challenge }))))
}

// POST /api/admin/challenges/:id/archive
async fn archive_challenge(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin(&state, &auth).await?;

    let challenge: ChallengeTemplate = sqlx::query_as(
        "UPDATE challenge_templates SET status = 'archived', updated_at = NOW() WHERE id = $1 RETURNING *",
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
    let total_challenges: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM challenge_templates")
        .fetch_one(&state.db)
        .await?;
    let published_challenges: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM challenge_templates WHERE status = 'published'")
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
        .map(
            |(
                id,
                user_id,
                ip,
                ua,
                created_at,
                last_used,
                email,
                username,
                ent_id,
                ent_slug,
                company,
            )| {
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
            },
        )
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

    // BE-F — audit log unifié.
    crate::services::audit::record(
        &state.db,
        crate::services::audit::AuditEntry {
            actor_type: crate::services::audit::ActorType::Admin,
            actor_id: Some(auth.user_id),
            action: "sso_session_revoke",
            target_type: Some("user_session"),
            target_id: Some(session_id),
            metadata: None,
            headers: None,
        },
    )
    .await;

    Ok(Json(build_response(json!({ "revoked": true }))))
}

// ═══════════════════════════════════════════════════════════════════
// IA-C.1 — POST /admin/challenges/{id}/variant
// ═══════════════════════════════════════════════════════════════════
//
// Génère une variante d'un challenge existant via IA (skilluv-ai
// GenerateVariant). Le nouveau challenge est créé en `status='draft'` —
// l'admin devra le review + publier via l'endpoint existant.
//
// MVP scope : `variant_type ∈ {harder, easier}` uniquement (voir doc IA-C.1).
// Post-MVP : ajouter `different_lang`, `shorter`, `longer`.

#[derive(Debug, Deserialize)]
struct GenerateVariantBody {
    /// `harder` | `easier` au MVP.
    variant_type: String,
    /// Ex: '3' pour difficulty, '30' pour minutes. Vide accepté.
    #[serde(default)]
    target_param: String,
}

async fn admin_generate_variant(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(original_id): Path<Uuid>,
    Json(body): Json<GenerateVariantBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await?;
    crate::middleware::admin_destructive::enforce_admin_destructive(&state, auth.user_id).await?;

    if !matches!(body.variant_type.as_str(), "harder" | "easier") {
        return Err(AppError::Validation(
            "variant_type MVP: 'harder' | 'easier' uniquement".into(),
        ));
    }

    let ai = state
        .ai
        .as_deref()
        .ok_or_else(|| AppError::Internal("AI client not connected (grpc_ai_url absent)".into()))?;

    // 1. Fetch le challenge original + convert en GeneratedChallenge proto.
    let orig: crate::models::ChallengeTemplate =
        sqlx::query_as("SELECT * FROM challenge_templates WHERE id = $1")
            .bind(original_id)
            .fetch_optional(&state.db)
            .await?
            .ok_or(AppError::NotFound(format!(
                "challenge {original_id} not found"
            )))?;

    let original_proto = crate::grpc::proto::GeneratedChallenge {
        title: orig.title.clone(),
        description: orig.description.clone(),
        instructions: orig.instructions.clone(),
        difficulty: orig.difficulty as i32,
        duration_minutes: orig.duration_minutes.unwrap_or(60),
        skill_domain: orig.skill_domain.clone(),
        tone: orig.tone.clone(),
        tags: Vec::new(),
        starter_code: String::new(),
        test_cases: Vec::new(),
        evaluation_criteria: String::new(),
        fragment_reward: orig.reward_fragments,
        ai_allowed: orig.ai_policy != "no_ai_declared",
        language: orig.language.clone().unwrap_or_default(),
        orientation_slug: String::new(),
    };

    // 2. Appelle l'IA.
    let request = crate::grpc::proto::GenerateVariantRequest {
        original_challenge_id: original_id.to_string(),
        variant_type: body.variant_type.clone(),
        target_param: body.target_param.clone(),
        original: Some(original_proto),
    };
    let started = std::time::Instant::now();
    let result = ai.generate_variant(request).await;
    crate::services::ai_log::record(
        &state.db,
        "GenerateVariant",
        None,
        Some(auth.user_id),
        started.elapsed(),
        &result,
        None,
    )
    .await;
    let resp =
        result.map_err(|s| AppError::Internal(format!("gRPC generate_variant failed: {s}")))?;

    if !resp.success {
        return Err(AppError::Internal(format!(
            "IA generate_variant failed: {}",
            resp.error_message
        )));
    }
    let generated = resp
        .challenge
        .ok_or_else(|| AppError::Internal("IA returned success but empty challenge".into()))?;

    // 3. Persist comme NOUVEAU challenge en draft. Aucun update sur l'original.
    let new_challenge: crate::models::ChallengeTemplate = sqlx::query_as(
        r#"
        INSERT INTO challenge_templates
            (title, description, instructions, skill_domain, difficulty,
             mode, duration_minutes, ai_policy, tone, language,
             reward_fragments, is_training, created_by, status)
        VALUES ($1, $2, $3, $4, $5, 'solo', $6, $7, $8, $9, $10, $11, $12, 'draft')
        RETURNING *
        "#,
    )
    .bind(&generated.title)
    .bind(&generated.description)
    .bind(&generated.instructions)
    .bind(&generated.skill_domain)
    .bind(generated.difficulty as i16)
    .bind(generated.duration_minutes)
    .bind(if generated.ai_allowed {
        "unrestricted"
    } else {
        "no_ai_declared"
    })
    .bind(&generated.tone)
    .bind(if generated.language.is_empty() {
        None
    } else {
        Some(&generated.language)
    })
    .bind(generated.fragment_reward)
    .bind(orig.is_training)
    .bind(auth.user_id)
    .fetch_one(&state.db)
    .await?;

    metrics::counter!(
        "skilluv_challenge_variants_generated_total",
        "variant_type" => body.variant_type.clone(),
    )
    .increment(1);

    // Audit log.
    crate::services::audit::record(
        &state.db,
        crate::services::audit::AuditEntry {
            actor_type: crate::services::audit::ActorType::Admin,
            actor_id: Some(auth.user_id),
            action: "challenge_variant_generated",
            target_type: Some("challenge_template"),
            target_id: Some(new_challenge.id),
            metadata: Some(json!({
                "original_challenge_id": original_id,
                "variant_type": body.variant_type,
                "target_param": body.target_param,
            })),
            headers: None,
        },
    )
    .await;

    Ok(Json(build_response(json!({
        "new_challenge_id": new_challenge.id,
        "original_challenge_id": original_id,
        "variant_type": body.variant_type,
        "status": "draft",
        "message": "Variant generated in draft — review and publish separately."
    }))))
}
