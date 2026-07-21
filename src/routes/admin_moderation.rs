use axum::extract::{Path, Query, State};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::{AuthUser, extract_ip};
use crate::services::{AuthService, LeaderboardService, NotificationService, SessionService};

pub fn admin_moderation_routes() -> Router<AppState> {
    Router::new()
        .route("/admin/users", get(list_users))
        .route("/admin/users/{id}", get(get_user))
        .route("/admin/users/{id}/ban", post(ban_user))
        .route("/admin/users/{id}/unban", post(unban_user))
        .route("/admin/reports", get(list_reports))
        .route("/admin/reports/{id}", put(handle_report))
        .route("/admin/audit-log", get(audit_log))
        .route("/admin/dashboard/moderation", get(moderation_dashboard))
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

async fn write_audit_log(
    db: &PgPool,
    admin_id: Uuid,
    action: &str,
    target_type: Option<&str>,
    target_id: Option<Uuid>,
    details: Option<serde_json::Value>,
    ip: &str,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO admin_audit_log (admin_id, action, target_type, target_id, details, ip_address) VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(admin_id)
    .bind(action)
    .bind(target_type)
    .bind(target_id)
    .bind(&details)
    .bind(ip)
    .execute(db)
    .await?;
    Ok(())
}

// ─── Request types ──────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ListUsersQuery {
    role: Option<String>,
    banned: Option<bool>,
    q: Option<String>,
    page: Option<i64>,
    per_page: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct BanRequest {
    reason: String,
}

#[derive(Debug, Deserialize)]
struct HandleReportRequest {
    status: String,
    admin_note: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ReportsQuery {
    status: Option<String>,
    target_type: Option<String>,
    page: Option<i64>,
    per_page: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct AuditLogQuery {
    action: Option<String>,
    page: Option<i64>,
    per_page: Option<i64>,
}

// ─── Structs ────────────────────────────────────────────────────

#[derive(Debug, serde::Serialize, sqlx::FromRow)]
struct Report {
    id: Uuid,
    reporter_id: Uuid,
    target_type: String,
    target_id: Uuid,
    reason: String,
    details: Option<String>,
    status: String,
    admin_note: Option<String>,
    handled_by: Option<Uuid>,
    handled_at: Option<chrono::DateTime<chrono::Utc>>,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, serde::Serialize, sqlx::FromRow)]
struct AuditEntry {
    id: Uuid,
    admin_id: Uuid,
    action: String,
    target_type: Option<String>,
    target_id: Option<Uuid>,
    details: Option<serde_json::Value>,
    ip_address: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
}

// ─── Routes ─────────────────────────────────────────────────────

// GET /api/admin/users
async fn list_users(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<ListUsersQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin(&state, &auth).await?;

    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(20).clamp(1, 50);
    let offset = (page - 1) * per_page;

    let mut where_clauses = vec![];
    let mut param_idx = 0u32;

    if query.role.is_some() {
        param_idx += 1;
        where_clauses.push(format!("role = ${param_idx}"));
    }
    if query.banned.is_some() {
        param_idx += 1;
        where_clauses.push(format!("is_banned = ${param_idx}"));
    }
    if query.q.is_some() {
        param_idx += 1;
        where_clauses.push(format!(
            "search_vector @@ to_tsquery('simple', ${param_idx})"
        ));
    }

    let where_str = if where_clauses.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", where_clauses.join(" AND "))
    };

    let sql = format!(
        "SELECT id, username, display_name, email, role, title, total_fragments, is_banned, profile_active, created_at FROM users {where_str} ORDER BY created_at DESC LIMIT {per_page} OFFSET {offset}"
    );
    let count_sql = format!("SELECT COUNT(*) FROM users {where_str}");

    #[derive(Debug, serde::Serialize, sqlx::FromRow)]
    struct UserSummary {
        id: Uuid,
        username: String,
        display_name: String,
        email: String,
        role: String,
        title: String,
        total_fragments: i32,
        is_banned: bool,
        profile_active: bool,
        created_at: chrono::DateTime<chrono::Utc>,
    }

    let mut db_query = sqlx::query_as::<_, UserSummary>(&sql);
    let mut cnt_query = sqlx::query_scalar::<_, i64>(&count_sql);

    if let Some(ref role) = query.role {
        db_query = db_query.bind(role);
        cnt_query = cnt_query.bind(role);
    }
    if let Some(banned) = query.banned {
        db_query = db_query.bind(banned);
        cnt_query = cnt_query.bind(banned);
    }
    if let Some(ref q) = query.q {
        let tsq = q.split_whitespace().collect::<Vec<_>>().join(" & ");
        db_query = db_query.bind(tsq.clone());
        cnt_query = cnt_query.bind(tsq);
    }

    let users: Vec<UserSummary> = db_query.fetch_all(&state.db).await?;
    let total: i64 = cnt_query.fetch_one(&state.db).await?;

    Ok(Json(json!({
        "data": users,
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

// GET /api/admin/users/:id
async fn get_user(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin(&state, &auth).await?;

    let user: crate::models::User = sqlx::query_as("SELECT * FROM users WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound("User not found".to_string()))?;

    let reports_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM reports WHERE target_type = 'user' AND target_id = $1",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await?;

    let submissions_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM challenge_submissions WHERE user_id = $1")
            .bind(id)
            .fetch_one(&state.db)
            .await?;

    Ok(Json(build_response(json!({
        "user": {
            "id": user.id,
            "email": user.email,
            "username": user.username,
            "display_name": user.display_name,
            "skill_domain": user.skill_domain,
            "role": user.role,
            "title": user.title,
            "total_fragments": user.total_fragments,
            "streak_current": user.streak_current,
            "trust_score": user.trust_score,
            "country": user.country,
            "email_verified": user.email_verified,
            "profile_active": user.profile_active,
            "is_banned": user.is_banned,
            "created_at": user.created_at.to_rfc3339(),
        },
        "reports_against": reports_count,
        "total_submissions": submissions_count,
    }))))
}

// POST /api/admin/users/:id/ban
async fn ban_user(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    headers: axum::http::HeaderMap,
    Json(body): Json<BanRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin(&state, &auth).await?;

    if id == auth.user_id {
        return Err(AppError::Validation("Cannot ban yourself".to_string()));
    }

    let user: crate::models::User = sqlx::query_as("SELECT * FROM users WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound("User not found".to_string()))?;

    if user.is_banned {
        return Err(AppError::Validation("User is already banned".to_string()));
    }

    if user.role == "admin" {
        return Err(AppError::Validation("Cannot ban an admin".to_string()));
    }

    // 1. Set is_banned + persist audit metadata (Vague 3 columns).
    sqlx::query(
        "UPDATE users
         SET is_banned = TRUE,
             ban_reason = $1,
             banned_at = NOW(),
             banned_by = $2,
             updated_at = NOW()
         WHERE id = $3",
    )
    .bind(body.reason.trim())
    .bind(auth.user_id)
    .bind(id)
    .execute(&state.db)
    .await?;

    // 2. Kick every device out immediately (Vague 2 sessions).
    SessionService::revoke_all(&state.db, id).await?;
    // Also clear any legacy Redis refresh token if still present.
    AuthService::revoke_refresh_token(&mut state.redis.clone(), id).await?;

    // 3. Remove from leaderboards
    LeaderboardService::remove_user(&mut state.redis.clone(), id).await?;

    // 4. Close all conversations
    sqlx::query(
        "UPDATE conversations SET closed = TRUE WHERE (talent_id = $1 OR enterprise_id IN (SELECT enterprise_id FROM enterprise_members WHERE user_id = $1)) AND closed = FALSE",
    )
    .bind(id)
    .execute(&state.db)
    .await?;

    // 5. Audit log
    let ip = extract_ip(&headers);
    write_audit_log(
        &state.db,
        auth.user_id,
        "user.ban",
        Some("user"),
        Some(id),
        Some(json!({ "reason": body.reason, "username": user.username })),
        &ip,
    )
    .await?;

    // 6. Notify user
    NotificationService::send(
        &state.db,
        &mut state.redis.clone(),
        &state.ws,
        crate::services::notification::NotificationPayload {
            user_id: id,
            notification_type: "account_banned",
            title: "Votre compte a été suspendu",
            body: Some(&format!("Raison : {}", body.reason)),
            data: None,
        },
    )
    .await?;

    tracing::warn!(
        admin = %auth.user_id,
        user = %id,
        username = %user.username,
        reason = %body.reason,
        "User banned"
    );

    Ok(Json(build_response(json!({
        "message": format!("User {} banned", user.username),
        "reason": body.reason,
    }))))
}

// POST /api/admin/users/:id/unban
async fn unban_user(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin(&state, &auth).await?;

    let user: crate::models::User = sqlx::query_as("SELECT * FROM users WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound("User not found".to_string()))?;

    if !user.is_banned {
        return Err(AppError::Validation("User is not banned".to_string()));
    }

    sqlx::query(
        "UPDATE users
         SET is_banned = FALSE,
             ban_reason = NULL,
             banned_at = NULL,
             banned_by = NULL,
             updated_at = NOW()
         WHERE id = $1",
    )
    .bind(id)
    .execute(&state.db)
    .await?;

    let ip = extract_ip(&headers);
    write_audit_log(
        &state.db,
        auth.user_id,
        "user.unban",
        Some("user"),
        Some(id),
        Some(json!({ "username": user.username })),
        &ip,
    )
    .await?;

    NotificationService::send(
        &state.db,
        &mut state.redis.clone(),
        &state.ws,
        crate::services::notification::NotificationPayload {
            user_id: id,
            notification_type: "account_unbanned",
            title: "Votre compte a été réactivé",
            body: Some("Vous pouvez à nouveau utiliser la plateforme."),
            data: None,
        },
    )
    .await?;

    Ok(Json(build_response(json!({
        "message": format!("User {} unbanned", user.username),
    }))))
}

// GET /api/admin/reports
async fn list_reports(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<ReportsQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin(&state, &auth).await?;

    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(20).clamp(1, 50);
    let offset = (page - 1) * per_page;

    let mut where_clauses = vec![];
    let mut param_idx = 0u32;

    if query.status.is_some() {
        param_idx += 1;
        where_clauses.push(format!("r.status = ${param_idx}"));
    }
    if query.target_type.is_some() {
        param_idx += 1;
        where_clauses.push(format!("r.target_type = ${param_idx}"));
    }

    let where_str = if where_clauses.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", where_clauses.join(" AND "))
    };

    let sql = format!(
        "SELECT r.* FROM reports r {where_str} ORDER BY r.created_at DESC LIMIT {per_page} OFFSET {offset}"
    );
    let count_sql = format!("SELECT COUNT(*) FROM reports r {where_str}");

    let mut db_query = sqlx::query_as::<_, Report>(&sql);
    let mut cnt_query = sqlx::query_scalar::<_, i64>(&count_sql);

    if let Some(ref status) = query.status {
        db_query = db_query.bind(status);
        cnt_query = cnt_query.bind(status);
    }
    if let Some(ref target_type) = query.target_type {
        db_query = db_query.bind(target_type);
        cnt_query = cnt_query.bind(target_type);
    }

    let reports: Vec<Report> = db_query.fetch_all(&state.db).await?;
    let total: i64 = cnt_query.fetch_one(&state.db).await?;

    // Enrich with reporter info
    let reporter_ids: Vec<Uuid> = reports.iter().map(|r| r.reporter_id).collect();
    let reporters: Vec<(Uuid, String, String)> =
        sqlx::query_as("SELECT id, username, display_name FROM users WHERE id = ANY($1)")
            .bind(&reporter_ids)
            .fetch_all(&state.db)
            .await?;

    let reporter_map: std::collections::HashMap<Uuid, _> =
        reporters.into_iter().map(|r| (r.0, r)).collect();

    let enriched: Vec<serde_json::Value> = reports
        .iter()
        .map(|r| {
            let reporter = reporter_map.get(&r.reporter_id);
            json!({
                "id": r.id,
                "reporter": {
                    "id": r.reporter_id,
                    "username": reporter.map(|rp| &rp.1),
                    "display_name": reporter.map(|rp| &rp.2),
                },
                "target_type": r.target_type,
                "target_id": r.target_id,
                "reason": r.reason,
                "details": r.details,
                "status": r.status,
                "admin_note": r.admin_note,
                "handled_at": r.handled_at.map(|d| d.to_rfc3339()),
                "created_at": r.created_at.to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(json!({
        "data": enriched,
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

// PUT /api/admin/reports/:id
async fn handle_report(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    headers: axum::http::HeaderMap,
    Json(body): Json<HandleReportRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin(&state, &auth).await?;

    if body.status != "resolved" && body.status != "dismissed" {
        return Err(AppError::Validation(
            "status must be 'resolved' or 'dismissed'".to_string(),
        ));
    }

    let report: Report = sqlx::query_as(
        r#"
        UPDATE reports SET
            status = $1,
            admin_note = $2,
            handled_by = $3,
            handled_at = NOW()
        WHERE id = $4 AND status = 'pending'
        RETURNING *
        "#,
    )
    .bind(&body.status)
    .bind(&body.admin_note)
    .bind(auth.user_id)
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound(
        "Report not found or already handled".to_string(),
    ))?;

    let ip = extract_ip(&headers);
    let action = format!("report.{}", body.status);
    write_audit_log(
        &state.db,
        auth.user_id,
        &action,
        Some(&report.target_type),
        Some(report.target_id),
        Some(json!({ "report_id": id, "admin_note": body.admin_note })),
        &ip,
    )
    .await?;

    Ok(Json(build_response(json!({
        "report": report,
        "message": format!("Report {}", body.status),
    }))))
}

// GET /api/admin/audit-log
async fn audit_log(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<AuditLogQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin(&state, &auth).await?;

    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(20).clamp(1, 50);
    let offset = (page - 1) * per_page;

    let (entries, total) = if let Some(ref action) = query.action {
        let entries: Vec<AuditEntry> = sqlx::query_as(
            "SELECT * FROM admin_audit_log WHERE action = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3",
        )
        .bind(action)
        .bind(per_page)
        .bind(offset)
        .fetch_all(&state.db)
        .await?;

        let total: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM admin_audit_log WHERE action = $1")
                .bind(action)
                .fetch_one(&state.db)
                .await?;

        (entries, total)
    } else {
        let entries: Vec<AuditEntry> = sqlx::query_as(
            "SELECT * FROM admin_audit_log ORDER BY created_at DESC LIMIT $1 OFFSET $2",
        )
        .bind(per_page)
        .bind(offset)
        .fetch_all(&state.db)
        .await?;

        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM admin_audit_log")
            .fetch_one(&state.db)
            .await?;

        (entries, total)
    };

    Ok(Json(json!({
        "data": entries,
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

// GET /api/admin/dashboard/moderation
async fn moderation_dashboard(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin(&state, &auth).await?;

    let banned_users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE is_banned = TRUE")
        .fetch_one(&state.db)
        .await?;

    let pending_reports: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM reports WHERE status = 'pending'")
            .fetch_one(&state.db)
            .await?;

    let resolved_reports: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM reports WHERE status = 'resolved'")
            .fetch_one(&state.db)
            .await?;

    let dismissed_reports: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM reports WHERE status = 'dismissed'")
            .fetch_one(&state.db)
            .await?;

    let recent_bans: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM admin_audit_log WHERE action = 'user.ban' AND created_at > NOW() - INTERVAL '30 days'",
    )
    .fetch_one(&state.db)
    .await?;

    let admin_actions_today: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM admin_audit_log WHERE created_at > NOW() - INTERVAL '1 day'",
    )
    .fetch_one(&state.db)
    .await?;

    Ok(Json(build_response(json!({
        "banned_users": banned_users,
        "reports": {
            "pending": pending_reports,
            "resolved": resolved_reports,
            "dismissed": dismissed_reports,
            "total": pending_reports + resolved_reports + dismissed_reports,
        },
        "recent_bans_30d": recent_bans,
        "admin_actions_today": admin_actions_today,
    }))))
}
