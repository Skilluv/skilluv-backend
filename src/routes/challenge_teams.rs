use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::models::Challenge;

pub fn challenge_team_routes() -> Router<AppState> {
    Router::new()
        .route("/challenges/{id}/team/create", post(create_team))
        .route("/challenges/{id}/team/{team_id}/join", post(join_team))
        .route("/challenges/{id}/teams", get(list_teams))
        .route("/challenges/{id}/team/{team_id}/submit", post(submit_team))
        .route("/challenges/{id}/timer", get(get_timer))
        .route("/challenges/{id}/timer/extend", post(extend_timer))
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

#[derive(Debug, Deserialize)]
struct CreateTeamRequest {
    name: String,
    max_members: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct SubmitTeamRequest {
    /// Reçu du frontend pour compat, non persisté depuis P9.1 (le pipeline
    /// team submit ne fait pas encore de dual-write vers deliverables).
    #[allow(dead_code)]
    code: String,
    language: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ExtendTimerRequest {
    minutes: i32,
}

#[derive(Debug, serde::Serialize, sqlx::FromRow)]
struct Team {
    id: Uuid,
    challenge_id: Uuid,
    name: String,
    created_by: Uuid,
    max_members: i32,
    status: String,
    created_at: chrono::DateTime<chrono::Utc>,
}

// POST /api/challenges/:id/team/create
async fn create_team(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(challenge_id): Path<Uuid>,
    Json(body): Json<CreateTeamRequest>,
) -> Result<impl IntoResponse, AppError> {
    // Verify challenge exists and is team mode
    let challenge: Challenge =
        sqlx::query_as("SELECT * FROM challenges WHERE id = $1 AND status = 'published'")
            .bind(challenge_id)
            .fetch_optional(&state.db)
            .await?
            .ok_or(AppError::NotFound("Challenge not found".to_string()))?;

    if challenge.mode != "team" {
        return Err(AppError::Validation(
            "This challenge does not support teams".to_string(),
        ));
    }

    if body.name.trim().is_empty() || body.name.len() > 100 {
        return Err(AppError::Validation(
            "Team name must be between 1 and 100 characters".to_string(),
        ));
    }

    let max_members = body.max_members.unwrap_or(4).clamp(2, 10);

    let team: Team = sqlx::query_as(
        "INSERT INTO challenge_teams (challenge_id, name, created_by, max_members) VALUES ($1, $2, $3, $4) RETURNING *",
    )
    .bind(challenge_id)
    .bind(body.name.trim())
    .bind(auth.user_id)
    .bind(max_members)
    .fetch_one(&state.db)
    .await?;

    // Auto-join creator
    sqlx::query("INSERT INTO team_members (team_id, user_id) VALUES ($1, $2)")
        .bind(team.id)
        .bind(auth.user_id)
        .execute(&state.db)
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(build_response(json!({ "team": team }))),
    ))
}

// POST /api/challenges/:id/team/:team_id/join
async fn join_team(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((challenge_id, team_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, AppError> {
    let team: Team =
        sqlx::query_as("SELECT * FROM challenge_teams WHERE id = $1 AND challenge_id = $2")
            .bind(team_id)
            .bind(challenge_id)
            .fetch_optional(&state.db)
            .await?
            .ok_or(AppError::NotFound("Team not found".to_string()))?;

    if team.status != "open" {
        return Err(AppError::Validation(
            "Team is not open for joining".to_string(),
        ));
    }

    // Check not already in a team for this challenge
    let already: Option<(Uuid,)> = sqlx::query_as(
        "SELECT tm.team_id FROM team_members tm JOIN challenge_teams ct ON ct.id = tm.team_id WHERE ct.challenge_id = $1 AND tm.user_id = $2",
    )
    .bind(challenge_id)
    .bind(auth.user_id)
    .fetch_optional(&state.db)
    .await?;

    if already.is_some() {
        return Err(AppError::Validation(
            "You are already in a team for this challenge".to_string(),
        ));
    }

    // Check capacity
    let member_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM team_members WHERE team_id = $1")
            .bind(team_id)
            .fetch_one(&state.db)
            .await?;

    if member_count >= team.max_members as i64 {
        return Err(AppError::Validation("Team is full".to_string()));
    }

    sqlx::query("INSERT INTO team_members (team_id, user_id) VALUES ($1, $2)")
        .bind(team_id)
        .bind(auth.user_id)
        .execute(&state.db)
        .await?;

    // Update status to full if needed
    if member_count + 1 >= team.max_members as i64 {
        sqlx::query("UPDATE challenge_teams SET status = 'full' WHERE id = $1")
            .bind(team_id)
            .execute(&state.db)
            .await?;
    }

    Ok(Json(build_response(json!({
        "message": "Joined team successfully"
    }))))
}

// GET /api/challenges/:id/teams
async fn list_teams(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(challenge_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let teams: Vec<Team> =
        sqlx::query_as("SELECT * FROM challenge_teams WHERE challenge_id = $1 ORDER BY created_at")
            .bind(challenge_id)
            .fetch_all(&state.db)
            .await?;

    let mut result = Vec::new();
    for team in &teams {
        let members: Vec<(Uuid, String, String)> = sqlx::query_as(
            "SELECT u.id, u.username, u.display_name FROM team_members tm JOIN users u ON u.id = tm.user_id WHERE tm.team_id = $1",
        )
        .bind(team.id)
        .fetch_all(&state.db)
        .await?;

        result.push(json!({
            "team": team,
            "members": members.iter().map(|m| json!({
                "id": m.0,
                "username": m.1,
                "display_name": m.2,
            })).collect::<Vec<_>>(),
            "member_count": members.len(),
        }));
    }

    Ok(Json(build_response(json!({ "teams": result }))))
}

// POST /api/challenges/:id/team/:team_id/submit
async fn submit_team(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((challenge_id, team_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<SubmitTeamRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    // Verify membership
    let is_member: Option<(Uuid,)> =
        sqlx::query_as("SELECT user_id FROM team_members WHERE team_id = $1 AND user_id = $2")
            .bind(team_id)
            .bind(auth.user_id)
            .fetch_optional(&state.db)
            .await?;

    if is_member.is_none() {
        return Err(AppError::Forbidden);
    }

    // Verify team not already submitted
    let team: Team =
        sqlx::query_as("SELECT * FROM challenge_teams WHERE id = $1 AND challenge_id = $2")
            .bind(team_id)
            .bind(challenge_id)
            .fetch_optional(&state.db)
            .await?
            .ok_or(AppError::NotFound("Team not found".to_string()))?;

    if team.status == "submitted" {
        return Err(AppError::Validation(
            "Team has already submitted".to_string(),
        ));
    }

    let challenge: Challenge = sqlx::query_as("SELECT * FROM challenges WHERE id = $1")
        .bind(challenge_id)
        .fetch_one(&state.db)
        .await?;

    // P9.1 : `code` n'est plus persisté sur challenge_submissions ; le contenu
    // vit dans le deliverable lié (créé plus tard dans le pipeline de vérif).
    let submission: crate::models::ChallengeSubmission = sqlx::query_as(
        r#"
        INSERT INTO challenge_submissions (challenge_id, user_id, team_id, language, status, submitted_at, attempt_number)
        VALUES ($1, $2, $3, $4, 'submitted', NOW(), 1)
        RETURNING *
        "#,
    )
    .bind(challenge_id)
    .bind(auth.user_id)
    .bind(team_id)
    .bind(&body.language)
    .fetch_one(&state.db)
    .await?;

    // Mark team as submitted
    sqlx::query("UPDATE challenge_teams SET status = 'submitted' WHERE id = $1")
        .bind(team_id)
        .execute(&state.db)
        .await?;

    // Award fragments to all team members
    let members: Vec<(Uuid,)> =
        sqlx::query_as("SELECT user_id FROM team_members WHERE team_id = $1")
            .bind(team_id)
            .fetch_all(&state.db)
            .await?;

    let fragments_per_member = challenge.reward_fragments;
    for (member_id,) in &members {
        sqlx::query(
            "UPDATE users SET total_fragments = total_fragments + $1, updated_at = NOW() WHERE id = $2",
        )
        .bind(fragments_per_member)
        .bind(member_id)
        .execute(&state.db)
        .await?;
    }

    Ok(Json(build_response(json!({
        "submission": submission,
        "fragments_per_member": fragments_per_member,
        "team_members": members.len(),
        "message": "Team submission recorded"
    }))))
}

// GET /api/challenges/:id/timer — time remaining for current submission
async fn get_timer(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(challenge_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let submission: Option<crate::models::ChallengeSubmission> = sqlx::query_as(
        "SELECT * FROM challenge_submissions WHERE user_id = $1 AND challenge_id = $2 AND status = 'in_progress' ORDER BY started_at DESC LIMIT 1",
    )
    .bind(auth.user_id)
    .bind(challenge_id)
    .fetch_optional(&state.db)
    .await?;

    let submission =
        submission.ok_or(AppError::NotFound("No active submission found".to_string()))?;

    let remaining = submission.expires_at.map(|exp| {
        let now = chrono::Utc::now();
        let diff = exp - now;
        diff.num_seconds().max(0)
    });

    let expired = submission
        .expires_at
        .map(|exp| exp < chrono::Utc::now())
        .unwrap_or(false);

    Ok(Json(build_response(json!({
        "submission_id": submission.id,
        "started_at": submission.started_at.to_rfc3339(),
        "expires_at": submission.expires_at.map(|e| e.to_rfc3339()),
        "remaining_seconds": remaining,
        "expired": expired,
        "has_timer": submission.expires_at.is_some(),
    }))))
}

// POST /api/challenges/:id/timer/extend — admin only
async fn extend_timer(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(challenge_id): Path<Uuid>,
    Json(body): Json<ExtendTimerRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    // Require admin
    let role: String = sqlx::query_scalar("SELECT role FROM users WHERE id = $1")
        .bind(auth.user_id)
        .fetch_one(&state.db)
        .await?;
    if role != "admin" {
        return Err(AppError::Forbidden);
    }

    if body.minutes <= 0 || body.minutes > 120 {
        return Err(AppError::Validation(
            "Extension must be between 1 and 120 minutes".to_string(),
        ));
    }

    // Extend all active submissions for this challenge
    let updated = sqlx::query(
        &format!(
            "UPDATE challenge_submissions SET expires_at = expires_at + INTERVAL '{} minutes' WHERE challenge_id = $1 AND status = 'in_progress' AND expires_at IS NOT NULL",
            body.minutes
        ),
    )
    .bind(challenge_id)
    .execute(&state.db)
    .await?;

    Ok(Json(build_response(json!({
        "message": format!("Extended timer by {} minutes", body.minutes),
        "submissions_affected": updated.rows_affected(),
    }))))
}
