//! Sponsored challenges workflow — Phase 3.12.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::middleware::AuthUser;

pub fn sponsored_routes() -> Router<AppState> {
    Router::new()
        // Enterprise side
        .route(
            "/enterprise/sponsored-challenges",
            get(list_my_requests).post(request_sponsorship),
        )
        // Admin side
        .route("/admin/sponsored-challenges", get(admin_list_requests))
        .route(
            "/admin/sponsored-challenges/{id}/decide",
            post(admin_decide_request),
        )
        .route(
            "/admin/sponsored-challenges/{id}/link",
            post(admin_link_challenge),
        )
        // Public sponsor visibility — leaderboard of currently sponsored challenges
        .route("/sponsored-challenges/active", get(public_active))
        // Sponsor-side : list submissions for the challenge they sponsored
        .route(
            "/enterprise/sponsored-challenges/{id}/submissions",
            get(sponsor_view_submissions),
        )
}

async fn current_enterprise_for(db: &sqlx::PgPool, user_id: Uuid) -> Result<Uuid, AppError> {
    let row: Option<(Uuid,)> = sqlx::query_as(
        "SELECT enterprise_id FROM enterprise_members WHERE user_id = $1 AND status = 'active' LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(db)
    .await?;
    row.map(|(id,)| id).ok_or(AppError::Forbidden)
}

// ─── Response types ──────────────────────────────────────────────

#[derive(Debug, Deserialize, ToSchema)]
pub struct RequestBody {
    #[schema(max_length = 10000)]
    pub proposed_title: String,
    /// At least 30 chars — enforced server-side.
    #[schema(max_length = 10000)]
    pub brief: String,
    #[schema(schema_with = crate::validators::skill_domain_schema)]
    pub skill_domain: String,
    pub difficulty: i16,
    pub duration_days: i32,
    pub budget_eur_cents: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RequestCreatedResponse {
    pub request_id: Uuid,
    /// Always `"pending"` on creation.
    pub status: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SponsorshipRequestRow {
    pub id: Uuid,
    pub proposed_title: String,
    /// `pending`, `approved`, `rejected`, `negotiating`, `live`.
    pub status: String,
    pub skill_domain: String,
    pub difficulty: i16,
    pub duration_days: i32,
    pub budget_eur_cents: i64,
    pub challenge_id: Option<Uuid>,
    pub decided_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MyRequestsResponse {
    pub requests: Vec<SponsorshipRequestRow>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminSponsorshipRow {
    pub id: Uuid,
    pub enterprise_id: Uuid,
    pub proposed_title: String,
    pub status: String,
    pub brief: String,
    pub skill_domain: String,
    pub difficulty: i16,
    pub duration_days: i32,
    pub budget_eur_cents: i64,
    pub challenge_id: Option<Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct AdminRequestsQuery {
    #[param(minimum = 1, maximum = 100000)]
    pub page: Option<i64>,
    #[param(minimum = 1, maximum = 200)]
    pub per_page: Option<i64>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = SponsoredChallengesDecideBody)]
pub struct DecideBody {
    /// `approve`, `reject`, `negotiate`.
    #[schema(max_length = 10000)]
    pub action: String,
    #[schema(max_length = 10000)]
    pub admin_notes: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DecideResponse {
    pub id: Uuid,
    pub status: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct LinkChallengeBody {
    pub challenge_id: Uuid,
    #[schema(max_length = 10000)]
    pub sponsor_logo_url: Option<String>,
    #[schema(max_length = 10000)]
    pub sponsor_blurb: Option<String>,
    pub sponsor_visible_until: chrono::DateTime<chrono::Utc>,
    pub free_contact_until: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LinkChallengeResponse {
    pub linked: bool,
    pub challenge_id: Uuid,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ActiveSponsoredRow {
    pub id: Uuid,
    pub title: String,
    pub skill_domain: String,
    pub difficulty: i16,
    pub sponsor_logo_url: Option<String>,
    pub sponsor_blurb: Option<String>,
    pub sponsor_name: String,
    pub sponsor_visible_until: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ActiveSponsoredResponse {
    pub active: Vec<ActiveSponsoredRow>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SponsorSubmissionRow {
    pub submission_id: Uuid,
    pub user_id: Uuid,
    pub username: String,
    pub display_name: String,
    pub skill_domain: Option<String>,
    pub total_fragments: i32,
    pub title: String,
    pub fragments_earned: i32,
    pub evaluated_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SponsorSubmissionsResponse {
    pub submissions: Vec<SponsorSubmissionRow>,
    /// True as long as `free_contact_until > now()`. After the window
    /// the sponsor pays per contact like every other enterprise.
    pub free_contact_active: bool,
    pub free_contact_until: chrono::DateTime<chrono::Utc>,
}

/// Submit a sponsorship request. The admin then reviews and either
/// approves + links to a challenge, rejects, or negotiates.
#[utoipa::path(
    post,
    path = "/api/enterprise/sponsored-challenges",
    tag = "enterprise",
    request_body = RequestBody,
    responses(
        (status = 200, description = "Request created", body = ApiResponse<RequestCreatedResponse>),
        (status = 400, description = "Invalid skill_domain or brief too short", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Caller has no enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn request_sponsorship(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<RequestBody>,
) -> Result<Json<ApiResponse<RequestCreatedResponse>>, AppError> {
    let enterprise_id = current_enterprise_for(&state.db, auth.user_id).await?;
    // The same stale four as `routes/challenges.rs` had. A company could not
    // sponsor an AI, ops or audio challenge, and the refusal said only
    // "invalid skill_domain" — so the answer to "why can I not sponsor this"
    // was nowhere in it.
    crate::validators::check_skill_domain(&body.skill_domain, "skill_domain")?;
    if body.brief.trim().len() < 30 {
        return Err(AppError::Validation(
            "brief must be at least 30 characters".into(),
        ));
    }
    let row: (Uuid,) = sqlx::query_as(
        r#"
        INSERT INTO sponsored_challenge_requests
            (enterprise_id, requested_by_user_id, proposed_title, brief, skill_domain, difficulty, duration_days, budget_eur_cents)
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
        RETURNING id
        "#,
    )
    .bind(enterprise_id)
    .bind(auth.user_id)
    .bind(body.proposed_title.trim())
    .bind(body.brief.trim())
    .bind(&body.skill_domain)
    .bind(body.difficulty)
    .bind(body.duration_days)
    .bind(body.budget_eur_cents)
    .fetch_one(&state.db)
    .await?;
    metrics::counter!("skilluv_sponsorship_requests_total").increment(1);
    Ok(Json(ApiResponse::new(RequestCreatedResponse {
        request_id: row.0,
        status: "pending".to_string(),
    })))
}

/// List the caller enterprise's sponsorship requests.
#[utoipa::path(
    get,
    path = "/api/enterprise/sponsored-challenges",
    tag = "enterprise",
    responses(
        (status = 200, description = "Requests list", body = ApiResponse<MyRequestsResponse>),
        (status = 403, description = "Caller has no enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn list_my_requests(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<MyRequestsResponse>>, AppError> {
    let enterprise_id = current_enterprise_for(&state.db, auth.user_id).await?;
    let rows: Vec<sqlx::postgres::PgRow> = sqlx::query(
        "SELECT id, proposed_title, status, skill_domain, difficulty, duration_days, budget_eur_cents, challenge_id, decided_at, created_at FROM sponsored_challenge_requests WHERE enterprise_id = $1 ORDER BY created_at DESC",
    )
    .bind(enterprise_id)
    .fetch_all(&state.db)
    .await?;
    let items: Vec<SponsorshipRequestRow> = rows
        .iter()
        .map(|r| SponsorshipRequestRow {
            id: r.get("id"),
            proposed_title: r.get("proposed_title"),
            status: r.get("status"),
            skill_domain: r.get("skill_domain"),
            difficulty: r.get("difficulty"),
            duration_days: r.get("duration_days"),
            budget_eur_cents: r.get("budget_eur_cents"),
            challenge_id: r.get("challenge_id"),
            decided_at: r.get("decided_at"),
            created_at: r.get("created_at"),
        })
        .collect();
    Ok(Json(ApiResponse::new(MyRequestsResponse {
        requests: items,
    })))
}

/// Admin only: paginated list of sponsorship requests, all statuses.
///
/// **Payload shape**: standard admin listing convention
/// `{data: [AdminSponsorshipRow], pagination: {...}, meta: {...}}`.
#[utoipa::path(
    get,
    path = "/api/admin/sponsored-challenges",
    tag = "admin",
    params(AdminRequestsQuery),
    responses(
        (status = 200, description = "Requests list (paginated)", body = serde_json::Value),
        (status = 403, description = "Not an admin", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn admin_list_requests(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<AdminRequestsQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await?;
    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(50).clamp(1, 200);
    let offset = (page - 1) * per_page;

    let rows: Vec<sqlx::postgres::PgRow> = sqlx::query(
        "SELECT id, enterprise_id, proposed_title, status, brief, skill_domain, difficulty, duration_days, budget_eur_cents, challenge_id, created_at FROM sponsored_challenge_requests ORDER BY created_at DESC LIMIT $1 OFFSET $2",
    )
    .bind(per_page)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sponsored_challenge_requests")
        .fetch_one(&state.db)
        .await?;

    let items: Vec<AdminSponsorshipRow> = rows
        .iter()
        .map(|r| AdminSponsorshipRow {
            id: r.get("id"),
            enterprise_id: r.get("enterprise_id"),
            proposed_title: r.get("proposed_title"),
            status: r.get("status"),
            brief: r.get("brief"),
            skill_domain: r.get("skill_domain"),
            difficulty: r.get("difficulty"),
            duration_days: r.get("duration_days"),
            budget_eur_cents: r.get("budget_eur_cents"),
            challenge_id: r.get("challenge_id"),
            created_at: r.get("created_at"),
        })
        .collect();
    Ok(Json(serde_json::json!({
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

/// Admin only: decide on a sponsorship request. `approve` unlocks the
/// admin_link_challenge step; `reject` closes; `negotiate` keeps the
/// thread open with the sponsor.
#[utoipa::path(
    post,
    path = "/api/admin/sponsored-challenges/{id}/decide",
    tag = "admin",
    params(("id" = Uuid, Path, description = "Sponsorship request UUID")),
    request_body = DecideBody,
    responses(
        (status = 200, description = "Decision recorded", body = ApiResponse<DecideResponse>),
        (status = 400, description = "Invalid action", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not an admin", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn admin_decide_request(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<DecideBody>,
) -> Result<Json<ApiResponse<DecideResponse>>, AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await?;
    let new_status = match body.action.as_str() {
        "approve" => "approved",
        "reject" => "rejected",
        "negotiate" => "negotiating",
        _ => return Err(AppError::Validation("invalid action".into())),
    };
    sqlx::query(
        "UPDATE sponsored_challenge_requests SET status = $1, admin_notes = $2, decided_by_user_id = $3, decided_at = NOW(), updated_at = NOW() WHERE id = $4",
    )
    .bind(new_status)
    .bind(body.admin_notes.as_deref())
    .bind(auth.user_id)
    .bind(id)
    .execute(&state.db)
    .await?;
    Ok(Json(ApiResponse::new(DecideResponse {
        id,
        status: new_status.to_string(),
    })))
}

/// Admin only: link an approved request to an actual challenge. Sets
/// the sponsor visibility window on the challenge and the free-contact
/// window in `sponsor_challenge_access`. Bumps the request to `live`.
#[utoipa::path(
    post,
    path = "/api/admin/sponsored-challenges/{id}/link",
    tag = "admin",
    params(("id" = Uuid, Path, description = "Sponsorship request UUID")),
    request_body = LinkChallengeBody,
    responses(
        (status = 200, description = "Sponsorship live", body = ApiResponse<LinkChallengeResponse>),
        (status = 400, description = "Request must be approved / negotiating first", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not an admin", body = crate::api_response::ErrorResponse),
        (status = 404, description = "Request not found", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn admin_link_challenge(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(request_id): Path<Uuid>,
    Json(body): Json<LinkChallengeBody>,
) -> Result<Json<ApiResponse<LinkChallengeResponse>>, AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await?;
    let req: (Uuid, String) = sqlx::query_as(
        "SELECT enterprise_id, status FROM sponsored_challenge_requests WHERE id = $1",
    )
    .bind(request_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound("request not found".into()))?;
    if !matches!(req.1.as_str(), "approved" | "negotiating") {
        return Err(AppError::Validation(
            "request must be approved before linking a challenge".into(),
        ));
    }
    let enterprise_id = req.0;

    let mut tx = state.db.begin().await?;
    sqlx::query(
        r#"
        UPDATE challenge_templates SET
            sponsor_enterprise_id = $1,
            sponsor_logo_url = $2,
            sponsor_blurb = $3,
            sponsor_visible_from = NOW(),
            sponsor_visible_until = $4
        WHERE id = $5
        "#,
    )
    .bind(enterprise_id)
    .bind(body.sponsor_logo_url.as_deref())
    .bind(body.sponsor_blurb.as_deref())
    .bind(body.sponsor_visible_until)
    .bind(body.challenge_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        r#"
        INSERT INTO sponsor_challenge_access (challenge_id, enterprise_id, free_contact_until)
        VALUES ($1, $2, $3)
        ON CONFLICT (challenge_id, enterprise_id) DO UPDATE SET free_contact_until = EXCLUDED.free_contact_until
        "#,
    )
    .bind(body.challenge_id)
    .bind(enterprise_id)
    .bind(body.free_contact_until)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE sponsored_challenge_requests SET status = 'live', challenge_id = $1, updated_at = NOW() WHERE id = $2",
    )
    .bind(body.challenge_id)
    .bind(request_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    metrics::counter!("skilluv_sponsored_challenges_live_total").increment(1);
    Ok(Json(ApiResponse::new(LinkChallengeResponse {
        linked: true,
        challenge_id: body.challenge_id,
    })))
}

/// Public: list currently-sponsored challenges (top 50 by expiring
/// window). Used for the sponsor-page carousel.
#[utoipa::path(
    get,
    path = "/api/sponsored-challenges/active",
    tag = "challenges",
    responses(
        (status = 200, description = "Active sponsored challenges", body = ApiResponse<ActiveSponsoredResponse>),
    ),
)]
pub async fn public_active(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<ActiveSponsoredResponse>>, AppError> {
    let rows: Vec<sqlx::postgres::PgRow> = sqlx::query(
        r#"
        SELECT c.id, c.title, c.skill_domain, c.difficulty, c.sponsor_logo_url, c.sponsor_blurb,
               c.sponsor_visible_until, e.company_name AS sponsor_name
        FROM challenge_templates c
        JOIN enterprises e ON e.id = c.sponsor_enterprise_id
        WHERE c.sponsor_enterprise_id IS NOT NULL
          AND c.sponsor_visible_until > NOW()
          AND c.status = 'published'
        ORDER BY c.sponsor_visible_until ASC
        LIMIT 50
        "#,
    )
    .fetch_all(&state.db)
    .await?;
    let items: Vec<ActiveSponsoredRow> = rows
        .iter()
        .map(|r| ActiveSponsoredRow {
            id: r.get("id"),
            title: r.get("title"),
            skill_domain: r.get("skill_domain"),
            difficulty: r.get("difficulty"),
            sponsor_logo_url: r.get("sponsor_logo_url"),
            sponsor_blurb: r.get("sponsor_blurb"),
            sponsor_name: r.get("sponsor_name"),
            sponsor_visible_until: r.get("sponsor_visible_until"),
        })
        .collect();
    Ok(Json(ApiResponse::new(ActiveSponsoredResponse {
        active: items,
    })))
}

/// Sponsor-side: list submissions for a challenge the caller
/// sponsored. Restricted by `sponsor_challenge_access`; also returns
/// `free_contact_active` so the front knows whether contacting a
/// talent is on the house.
#[utoipa::path(
    get,
    path = "/api/enterprise/sponsored-challenges/{id}/submissions",
    tag = "enterprise",
    params(("id" = Uuid, Path, description = "Challenge UUID")),
    responses(
        (status = 200, description = "Successful submissions", body = ApiResponse<SponsorSubmissionsResponse>),
        (status = 403, description = "Caller doesn't sponsor this challenge", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn sponsor_view_submissions(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(challenge_id): Path<Uuid>,
) -> Result<Json<ApiResponse<SponsorSubmissionsResponse>>, AppError> {
    let enterprise_id = current_enterprise_for(&state.db, auth.user_id).await?;
    // Confirm this enterprise has access to this challenge
    let allowed: Option<(chrono::DateTime<chrono::Utc>,)> = sqlx::query_as(
        "SELECT free_contact_until FROM sponsor_challenge_access WHERE challenge_id = $1 AND enterprise_id = $2",
    )
    .bind(challenge_id)
    .bind(enterprise_id)
    .fetch_optional(&state.db)
    .await?;
    let until = allowed.ok_or(AppError::Forbidden)?.0;
    let free_contact_active = until > chrono::Utc::now();

    let rows: Vec<sqlx::postgres::PgRow> = sqlx::query(
        r#"
        SELECT cs.id AS submission_id, cs.user_id, u.username, u.display_name, u.skill_domain,
               u.total_fragments, u.title, cs.fragments_earned, cs.evaluated_at
        FROM challenge_submissions cs
        JOIN users u ON u.id = cs.user_id
        WHERE cs.challenge_id = $1 AND cs.status = 'success' AND u.profile_active = TRUE AND u.is_banned = FALSE
        ORDER BY cs.evaluated_at DESC
        LIMIT 200
        "#,
    )
    .bind(challenge_id)
    .fetch_all(&state.db)
    .await?;
    let items: Vec<SponsorSubmissionRow> = rows
        .iter()
        .map(|r| SponsorSubmissionRow {
            submission_id: r.get("submission_id"),
            user_id: r.get("user_id"),
            username: r.get("username"),
            display_name: r.get("display_name"),
            skill_domain: r.get("skill_domain"),
            total_fragments: r.get("total_fragments"),
            title: r.get("title"),
            fragments_earned: r.get("fragments_earned"),
            evaluated_at: r.get("evaluated_at"),
        })
        .collect();
    Ok(Json(ApiResponse::new(SponsorSubmissionsResponse {
        submissions: items,
        free_contact_active,
        free_contact_until: until,
    })))
}
