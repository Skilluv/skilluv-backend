//! Sponsored challenges workflow — Phase 3.12.

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
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
        .route(
            "/admin/sponsored-challenges",
            get(admin_list_requests),
        )
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

fn build_response(data: Value) -> Value {
    json!({
        "data": data,
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })
}

async fn current_enterprise_for(
    db: &sqlx::PgPool,
    user_id: Uuid,
) -> Result<Uuid, AppError> {
    let row: Option<(Uuid,)> = sqlx::query_as(
        "SELECT enterprise_id FROM enterprise_members WHERE user_id = $1 AND status = 'active' LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(db)
    .await?;
    row.map(|(id,)| id).ok_or(AppError::Forbidden)
}

#[derive(Deserialize)]
struct RequestBody {
    proposed_title: String,
    brief: String,
    skill_domain: String,
    difficulty: i16,
    duration_days: i32,
    budget_eur_cents: i64,
}

async fn request_sponsorship(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<RequestBody>,
) -> Result<Json<Value>, AppError> {
    let enterprise_id = current_enterprise_for(&state.db, auth.user_id).await?;
    if !matches!(body.skill_domain.as_str(), "code" | "design" | "game" | "security") {
        return Err(AppError::Validation("invalid skill_domain".into()));
    }
    if body.brief.trim().len() < 30 {
        return Err(AppError::Validation("brief must be at least 30 characters".into()));
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
    Ok(Json(build_response(json!({ "request_id": row.0, "status": "pending" }))))
}

async fn list_my_requests(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let enterprise_id = current_enterprise_for(&state.db, auth.user_id).await?;
    let rows: Vec<sqlx::postgres::PgRow> = sqlx::query(
        "SELECT id, proposed_title, status, skill_domain, difficulty, duration_days, budget_eur_cents, challenge_id, decided_at, created_at FROM sponsored_challenge_requests WHERE enterprise_id = $1 ORDER BY created_at DESC",
    )
    .bind(enterprise_id)
    .fetch_all(&state.db)
    .await?;
    use sqlx::Row;
    let items: Vec<Value> = rows.iter().map(|r| json!({
        "id": r.get::<Uuid, _>("id"),
        "proposed_title": r.get::<String, _>("proposed_title"),
        "status": r.get::<String, _>("status"),
        "skill_domain": r.get::<String, _>("skill_domain"),
        "difficulty": r.get::<i16, _>("difficulty"),
        "duration_days": r.get::<i32, _>("duration_days"),
        "budget_eur_cents": r.get::<i64, _>("budget_eur_cents"),
        "challenge_id": r.get::<Option<Uuid>, _>("challenge_id"),
        "decided_at": r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("decided_at"),
        "created_at": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
    })).collect();
    Ok(Json(build_response(json!({ "requests": items }))))
}

async fn admin_list_requests(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    if auth.role != "admin" {
        return Err(AppError::Forbidden);
    }
    let rows: Vec<sqlx::postgres::PgRow> = sqlx::query(
        "SELECT id, enterprise_id, proposed_title, status, brief, skill_domain, difficulty, duration_days, budget_eur_cents, challenge_id, created_at FROM sponsored_challenge_requests ORDER BY created_at DESC LIMIT 200",
    )
    .fetch_all(&state.db)
    .await?;
    use sqlx::Row;
    let items: Vec<Value> = rows.iter().map(|r| json!({
        "id": r.get::<Uuid, _>("id"),
        "enterprise_id": r.get::<Uuid, _>("enterprise_id"),
        "proposed_title": r.get::<String, _>("proposed_title"),
        "status": r.get::<String, _>("status"),
        "brief": r.get::<String, _>("brief"),
        "skill_domain": r.get::<String, _>("skill_domain"),
        "difficulty": r.get::<i16, _>("difficulty"),
        "duration_days": r.get::<i32, _>("duration_days"),
        "budget_eur_cents": r.get::<i64, _>("budget_eur_cents"),
        "challenge_id": r.get::<Option<Uuid>, _>("challenge_id"),
        "created_at": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
    })).collect();
    Ok(Json(build_response(json!({ "requests": items }))))
}

#[derive(Deserialize)]
struct DecideBody {
    action: String,            // "approve" | "reject" | "negotiate"
    admin_notes: Option<String>,
}

async fn admin_decide_request(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<DecideBody>,
) -> Result<Json<Value>, AppError> {
    if auth.role != "admin" {
        return Err(AppError::Forbidden);
    }
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
    Ok(Json(build_response(json!({ "id": id, "status": new_status }))))
}

#[derive(Deserialize)]
struct LinkChallengeBody {
    challenge_id: Uuid,
    sponsor_logo_url: Option<String>,
    sponsor_blurb: Option<String>,
    sponsor_visible_until: chrono::DateTime<chrono::Utc>,
    free_contact_until: chrono::DateTime<chrono::Utc>,
}

async fn admin_link_challenge(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(request_id): Path<Uuid>,
    Json(body): Json<LinkChallengeBody>,
) -> Result<Json<Value>, AppError> {
    if auth.role != "admin" {
        return Err(AppError::Forbidden);
    }
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
    Ok(Json(build_response(json!({ "linked": true, "challenge_id": body.challenge_id }))))
}

async fn public_active(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
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
    use sqlx::Row;
    let items: Vec<Value> = rows.iter().map(|r| json!({
        "id": r.get::<Uuid, _>("id"),
        "title": r.get::<String, _>("title"),
        "skill_domain": r.get::<String, _>("skill_domain"),
        "difficulty": r.get::<i16, _>("difficulty"),
        "sponsor_logo_url": r.get::<Option<String>, _>("sponsor_logo_url"),
        "sponsor_blurb": r.get::<Option<String>, _>("sponsor_blurb"),
        "sponsor_name": r.get::<String, _>("sponsor_name"),
        "sponsor_visible_until": r.get::<chrono::DateTime<chrono::Utc>, _>("sponsor_visible_until"),
    })).collect();
    Ok(Json(build_response(json!({ "active": items }))))
}

async fn sponsor_view_submissions(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(challenge_id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
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
    use sqlx::Row;
    let items: Vec<Value> = rows.iter().map(|r| json!({
        "submission_id": r.get::<Uuid, _>("submission_id"),
        "user_id": r.get::<Uuid, _>("user_id"),
        "username": r.get::<String, _>("username"),
        "display_name": r.get::<String, _>("display_name"),
        "skill_domain": r.get::<String, _>("skill_domain"),
        "total_fragments": r.get::<i32, _>("total_fragments"),
        "title": r.get::<String, _>("title"),
        "fragments_earned": r.get::<i32, _>("fragments_earned"),
        "evaluated_at": r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("evaluated_at"),
    })).collect();
    Ok(Json(build_response(json!({
        "submissions": items,
        "free_contact_active": free_contact_active,
        "free_contact_until": until,
    }))))
}
