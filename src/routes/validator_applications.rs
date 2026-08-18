//! P26 v2 SKI-81 / SKI-82 — validator candidacy + admin invitation routes.
//!
//! Two route groups:
//!   `validator_application_routes()`       — user-facing (apply, accept
//!                                             invitation, withdraw)
//!   `admin_validator_application_routes()` — admin-only (invite, approve
//!                                             candidacy, reject)
//! The admin group is mounted through `admin_gate` in `lib.rs`.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::json;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::validator_applications::{self, ApplyInput, InviteInput};

pub fn validator_application_routes() -> Router<AppState> {
    Router::new()
        .route("/me/apply-as-validator", post(apply))
        .route("/me/validator-applications", get(my_applications))
        .route(
            "/validator-applications/{id}/accept",
            post(accept_invitation),
        )
        .route("/validator-applications/{id}/withdraw", post(withdraw))
}

pub fn admin_validator_application_routes() -> Router<AppState> {
    Router::new()
        .route("/admin/validators/invite", post(admin_invite))
        .route(
            "/admin/validator-applications/{id}/approve",
            post(admin_approve),
        )
        .route(
            "/admin/validator-applications/{id}/reject",
            post(admin_reject),
        )
        // SKI-107 — listing endpoint for the admin dashboard.
        .route(
            "/admin/validator-applications",
            axum::routing::get(list_applications_admin),
        )
}

fn wrap(data: serde_json::Value) -> serde_json::Value {
    json!({
        "data": data,
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })
}

// ─── User routes ─────────────────────────────────────────────────

/// SKI-81 — user self-nominates for a validator domain.
pub async fn apply(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<ApplyInput>,
) -> Result<impl IntoResponse, AppError> {
    let app = validator_applications::apply(&state.db, auth.user_id, input).await?;
    Ok((
        StatusCode::CREATED,
        Json(wrap(json!({ "application": app }))),
    ))
}

pub async fn my_applications(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<impl IntoResponse, AppError> {
    let rows: Vec<validator_applications::ValidatorApplication> = sqlx::query_as(
        "SELECT * FROM validator_applications WHERE user_id = $1 ORDER BY created_at DESC",
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(wrap(json!({ "applications": rows }))))
}

/// SKI-82 — invitee accepts the pending invitation.
pub async fn accept_invitation(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let app = validator_applications::accept(&state.db, id, auth.user_id, None).await?;
    Ok(Json(wrap(json!({ "application": app }))))
}

pub async fn withdraw(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let app =
        validator_applications::resolve(&state.db, id, auth.user_id, "withdrawn", None, false)
            .await?;
    Ok(Json(wrap(json!({ "application": app }))))
}

// ─── Admin routes ────────────────────────────────────────────────

/// SKI-82 — admin invites a user (bypasses stats).
pub async fn admin_invite(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<InviteInput>,
) -> Result<impl IntoResponse, AppError> {
    let app = validator_applications::invite(&state.db, auth.user_id, input).await?;
    Ok((
        StatusCode::CREATED,
        Json(wrap(json!({ "application": app }))),
    ))
}

pub async fn admin_approve(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let app =
        validator_applications::accept(&state.db, id, auth.user_id, Some(auth.user_id)).await?;
    Ok(Json(wrap(json!({ "application": app }))))
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RejectBody {
    pub reason: String,
}

pub async fn admin_reject(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<RejectBody>,
) -> Result<impl IntoResponse, AppError> {
    let app = validator_applications::resolve(
        &state.db,
        id,
        auth.user_id,
        "rejected",
        Some(body.reason),
        true,
    )
    .await?;
    Ok(Json(wrap(json!({ "application": app }))))
}

// ─── SKI-107 — admin listing with live stats ─────────────────────

#[derive(Debug, serde::Deserialize)]
pub struct ListApplicationsQuery {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    domain: Option<String>,
    #[serde(default)]
    origin: Option<String>,
    #[serde(default)]
    page: Option<i64>,
    #[serde(default)]
    per_page: Option<i64>,
}

/// Row shape: application + stats live pour la review admin.
/// Stats (rank, PRs sur domaine, repos, ancienneté) sont computed en
/// SQL pour éviter que le front fasse N appels distincts (N candidates
/// = N round-trips → cauchemar UX).
type AppListRow = (
    // application fields
    Uuid,                                  // id
    Uuid,                                  // user_id
    String,                                // domain
    String,                                // origin
    String,                                // status
    Option<String>,                        // motivation
    Option<Uuid>,                          // admin_actor_id
    Option<chrono::DateTime<chrono::Utc>>, // reviewed_at
    chrono::DateTime<chrono::Utc>,         // created_at
    // user snapshot
    Option<String>, // username
    Option<String>, // display_name
    Option<String>, // avatar_url
    // live stats
    Option<String>, // current rank
    i64,            // validated_prs_on_domain
    i64,            // distinct_repos_covered
    i32,            // tenure_days
);

pub async fn list_applications_admin(
    _gate: crate::middleware::admin_gate::AdminGate,
    State(state): State<AppState>,
    auth: AuthUser,
    axum::extract::Query(q): axum::extract::Query<ListApplicationsQuery>,
) -> Result<axum::Json<serde_json::Value>, AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await?;

    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(50).clamp(1, 200);
    let offset = (page - 1) * per_page;

    // Validate optional filters.
    if let Some(s) = &q.status
        && !matches!(
            s.as_str(),
            "pending" | "accepted" | "rejected" | "withdrawn"
        )
    {
        return Err(AppError::Validation("status invalid".into()));
    }
    if let Some(o) = &q.origin
        && !matches!(o.as_str(), "candidacy" | "invitation")
    {
        return Err(AppError::Validation("origin invalid".into()));
    }
    if let Some(d) = &q.domain
        && !crate::validators::SKILL_DOMAINS.contains(&d.as_str())
    {
        return Err(AppError::Validation("domain invalid".into()));
    }

    let rows: Vec<AppListRow> = sqlx::query_as(
        r#"
        SELECT a.id, a.user_id, a.domain, a.origin, a.status, a.motivation,
               a.admin_actor_id, a.reviewed_at, a.created_at,
               u.username, u.display_name, u.avatar_url,
               r.rank,
               (SELECT COUNT(*)::bigint FROM project_slices s
                 WHERE s.claimed_by_user_id = a.user_id
                   AND s.primary_domain = a.domain
                   AND s.status IN ('validated','merged')),
               (SELECT COUNT(DISTINCT s.project_id)::bigint FROM project_slices s
                 WHERE s.claimed_by_user_id = a.user_id
                   AND s.primary_domain = a.domain
                   AND s.status IN ('validated','merged')),
               EXTRACT(DAY FROM (NOW() - u.created_at))::int
          FROM validator_applications a
          LEFT JOIN users u ON u.id = a.user_id
          LEFT JOIN user_ranks r ON r.user_id = a.user_id
         WHERE ($1::text IS NULL OR a.status = $1)
           AND ($2::text IS NULL OR a.domain = $2)
           AND ($3::text IS NULL OR a.origin = $3)
         ORDER BY a.created_at DESC
         LIMIT $4 OFFSET $5
        "#,
    )
    .bind(&q.status)
    .bind(&q.domain)
    .bind(&q.origin)
    .bind(per_page)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let total: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM validator_applications
         WHERE ($1::text IS NULL OR status = $1)
           AND ($2::text IS NULL OR domain = $2)
           AND ($3::text IS NULL OR origin = $3)
        "#,
    )
    .bind(&q.status)
    .bind(&q.domain)
    .bind(&q.origin)
    .fetch_one(&state.db)
    .await?;

    let items: Vec<serde_json::Value> = rows
        .into_iter()
        .map(
            |(
                id,
                user_id,
                domain,
                origin,
                status,
                motivation,
                admin_actor_id,
                reviewed_at,
                created_at,
                username,
                display_name,
                avatar_url,
                rank,
                prs,
                repos,
                tenure_days,
            )| {
                json!({
                    "id": id,
                    "user_id": user_id,
                    "domain": domain,
                    "origin": origin,
                    "status": status,
                    "motivation": motivation,
                    "admin_actor_id": admin_actor_id,
                    "reviewed_at": reviewed_at.map(|d| d.to_rfc3339()),
                    "created_at": created_at.to_rfc3339(),
                    "user": {
                        "username": username,
                        "display_name": display_name,
                        "avatar_url": avatar_url,
                    },
                    "live_stats": {
                        "rank": rank.unwrap_or_else(|| "apprenti".into()),
                        "validated_prs_on_domain": prs,
                        "distinct_repos_covered": repos,
                        "tenure_days": tenure_days,
                        // Reference thresholds (SKI-81) surface so the admin
                        // sees at a glance whether the candidate would pass
                        // the auto-gate.
                        "thresholds": {
                            "min_rank": validator_applications::MIN_RANK,
                            "min_merged_prs": validator_applications::MIN_MERGED_PRS,
                            "min_repos_covered": validator_applications::MIN_REPOS_COVERED,
                            "min_tenure_days": validator_applications::MIN_TENURE_DAYS,
                        }
                    }
                })
            },
        )
        .collect();

    let total_pages = if per_page > 0 {
        (total + per_page - 1) / per_page
    } else {
        0
    };

    Ok(Json(json!({
        "data": items,
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
