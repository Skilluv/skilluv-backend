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
        // P10.1 — teams persistentes (indépendantes d'un challenge)
        .route("/teams", post(create_persistent_team))
        .route("/teams/{team_id}", get(get_team).post(join_persistent_team))
        .route("/teams/{team_id}/disband", post(disband_team))
        .route("/users/me/teams", get(my_teams))
        // P10.2 — role slots multidisciplinaires
        .route("/teams/{team_id}/slots", get(list_team_slots).post(create_team_slot))
        .route("/teams/{team_id}/slots/{slot_id}/fill", post(fill_team_slot))
        .route("/teams/{team_id}/slots/{slot_id}/leave", post(leave_team_slot))
        .route("/teams/{team_id}/slots/{slot_id}", axum::routing::delete(delete_team_slot))
        .route("/team-slots/open", get(list_open_slots_by_role))
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

/// P10.3 — Format d'une entrée dans challenge_templates.team_composition JSONB.
/// L'admin définit N slots par rôle : `count` détermine combien créer.
#[derive(Debug, Deserialize)]
struct CompositionSlot {
    role_slug: String,
    #[serde(default)]
    role_display_name: Option<String>,
    #[serde(default)]
    required_skill_slug: Option<String>,
    #[serde(default)]
    min_proficiency_level: Option<i16>,
    #[serde(default = "default_count")]
    count: i32,
}

fn default_count() -> i32 {
    1
}

#[derive(Debug, serde::Serialize, sqlx::FromRow)]
struct Team {
    id: Uuid,
    /// P10.1 : nullable pour supporter les teams persistentes hors challenge.
    challenge_id: Option<Uuid>,
    name: String,
    created_by: Uuid,
    max_members: i32,
    status: String,
    /// P10.1 : true pour les teams réutilisables sur plusieurs challenges/slices.
    is_persistent: bool,
    description: Option<String>,
    disbanded_at: Option<chrono::DateTime<chrono::Utc>>,
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
        sqlx::query_as("SELECT * FROM challenge_templates WHERE id = $1 AND status = 'published'")
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

    // P10.3 : si le challenge prescrit une composition, la somme des `count`
    // définit max_members (override le default 4). Sinon fallback body.max_members.
    let template_slots: Vec<CompositionSlot> = challenge
        .team_composition
        .as_ref()
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    let template_total: i32 = template_slots.iter().map(|s| s.count.max(1)).sum();
    let max_members = if template_total > 0 {
        template_total.clamp(2, 20)
    } else {
        body.max_members.unwrap_or(4).clamp(2, 10)
    };

    let team: Team = sqlx::query_as(
        "INSERT INTO challenge_teams (challenge_id, name, created_by, max_members) VALUES ($1, $2, $3, $4) RETURNING *",
    )
    .bind(challenge_id)
    .bind(body.name.trim())
    .bind(auth.user_id)
    .bind(max_members)
    .fetch_one(&state.db)
    .await?;

    // P10.3 : auto-création des slots depuis le template (best-effort ;
    // si un slug de skill n'existe pas, on skip juste ce slot).
    for tmpl in &template_slots {
        for _ in 0..tmpl.count.max(1) {
            let _ = crate::services::TeamRolesService::create_slot(
                &state.db,
                crate::services::CreateSlotParams {
                    team_id: team.id,
                    role_slug: &tmpl.role_slug,
                    role_display_name: tmpl.role_display_name.as_deref(),
                    required_skill_slug: tmpl.required_skill_slug.as_deref(),
                    min_proficiency_level: tmpl.min_proficiency_level.unwrap_or(1),
                },
            )
            .await;
        }
    }

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

    let challenge: Challenge = sqlx::query_as("SELECT * FROM challenge_templates WHERE id = $1")
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

// ═══════════════════════════════════════════════════════════════════
// P10.1 — Teams persistentes (indépendantes d'un challenge)
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct CreatePersistentTeamRequest {
    name: String,
    description: Option<String>,
    max_members: Option<i32>,
}

/// POST /api/teams — crée une team persistente réutilisable.
///
/// Le créateur est automatiquement ajouté comme premier membre.
async fn create_persistent_team(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreatePersistentTeamRequest>,
) -> Result<impl IntoResponse, AppError> {
    if body.name.trim().is_empty() || body.name.len() > 100 {
        return Err(AppError::Validation(
            "Team name must be between 1 and 100 characters".to_string(),
        ));
    }
    let max_members = body.max_members.unwrap_or(4).clamp(2, 10);

    let mut tx = state.db.begin().await?;
    let team: Team = sqlx::query_as(
        r#"
        INSERT INTO challenge_teams
            (challenge_id, name, description, created_by, max_members, is_persistent, status)
        VALUES (NULL, $1, $2, $3, $4, TRUE, 'open')
        RETURNING *
        "#,
    )
    .bind(body.name.trim())
    .bind(body.description.as_deref())
    .bind(auth.user_id)
    .bind(max_members)
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query("INSERT INTO team_members (team_id, user_id) VALUES ($1, $2)")
        .bind(team.id)
        .bind(auth.user_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    metrics::counter!("skilluv_persistent_teams_created_total").increment(1);

    Ok((
        StatusCode::CREATED,
        Json(build_response(json!({ "team": team }))),
    ))
}

/// GET /api/teams/{team_id} — détail d'une team + membres.
async fn get_team(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(team_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let team: Team = sqlx::query_as("SELECT * FROM challenge_teams WHERE id = $1")
        .bind(team_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Team not found".to_string()))?;

    let members: Vec<(Uuid, String, String)> = sqlx::query_as(
        "SELECT u.id, u.username, u.display_name
         FROM team_members tm JOIN users u ON u.id = tm.user_id
         WHERE tm.team_id = $1
         ORDER BY tm.joined_at",
    )
    .bind(team_id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(build_response(json!({
        "team": team,
        "members": members.iter().map(|(id, u, dn)| json!({
            "id": id,
            "username": u,
            "display_name": dn,
        })).collect::<Vec<_>>(),
        "member_count": members.len(),
    }))))
}

/// POST /api/teams/{team_id} — join une team persistente ouverte.
async fn join_persistent_team(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(team_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let team: Team = sqlx::query_as(
        "SELECT * FROM challenge_teams WHERE id = $1 AND is_persistent = TRUE AND disbanded_at IS NULL",
    )
    .bind(team_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Persistent team not found or disbanded".to_string()))?;

    if team.status != "open" {
        return Err(AppError::Validation(
            "Team is not open for joining".to_string(),
        ));
    }

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM team_members WHERE team_id = $1")
            .bind(team_id)
            .fetch_one(&state.db)
            .await?;
    if count >= team.max_members as i64 {
        return Err(AppError::Validation("Team is full".to_string()));
    }

    sqlx::query(
        "INSERT INTO team_members (team_id, user_id) VALUES ($1, $2)
         ON CONFLICT DO NOTHING",
    )
    .bind(team_id)
    .bind(auth.user_id)
    .execute(&state.db)
    .await?;

    if count + 1 >= team.max_members as i64 {
        sqlx::query("UPDATE challenge_teams SET status = 'full' WHERE id = $1")
            .bind(team_id)
            .execute(&state.db)
            .await?;
    }

    Ok(Json(build_response(json!({ "joined": true }))))
}

/// POST /api/teams/{team_id}/disband — le créateur dissout la team.
async fn disband_team(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(team_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let res = sqlx::query(
        "UPDATE challenge_teams
         SET disbanded_at = NOW()
         WHERE id = $1 AND created_by = $2 AND is_persistent = TRUE AND disbanded_at IS NULL",
    )
    .bind(team_id)
    .bind(auth.user_id)
    .execute(&state.db)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::Forbidden);
    }
    Ok(Json(build_response(json!({ "disbanded": true }))))
}

/// GET /api/users/me/teams — teams (challenge ou persistentes) où je suis membre.
async fn my_teams(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, AppError> {
    let teams: Vec<Team> = sqlx::query_as(
        r#"
        SELECT ct.*
        FROM challenge_teams ct
        JOIN team_members tm ON tm.team_id = ct.id
        WHERE tm.user_id = $1
          AND (ct.disbanded_at IS NULL OR ct.is_persistent = FALSE)
        ORDER BY ct.is_persistent DESC, ct.created_at DESC
        "#,
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(build_response(json!({ "teams": teams }))))
}

// ═══════════════════════════════════════════════════════════════════
// P10.2 — Role slots multidisciplinaires
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct CreateSlotBody {
    role_slug: String,
    role_display_name: Option<String>,
    required_skill_slug: Option<String>,
    min_proficiency_level: Option<i16>,
}

#[derive(Debug, Deserialize)]
struct OpenSlotsQuery {
    role: String,
    limit: Option<i64>,
}

/// Vérifie que le user est le créateur ou un membre de la team.
async fn require_team_creator(
    db: &sqlx::PgPool,
    team_id: Uuid,
    user_id: Uuid,
) -> Result<(), AppError> {
    let creator: Option<(Uuid,)> =
        sqlx::query_as("SELECT created_by FROM challenge_teams WHERE id = $1")
            .bind(team_id)
            .fetch_optional(db)
            .await?;
    match creator {
        Some((c,)) if c == user_id => Ok(()),
        Some(_) => Err(AppError::Forbidden),
        None => Err(AppError::NotFound("Team not found".into())),
    }
}

/// GET /api/teams/{team_id}/slots
async fn list_team_slots(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(team_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let slots = crate::services::TeamRolesService::list_slots(&state.db, team_id).await?;
    Ok(Json(build_response(json!({ "slots": slots }))))
}

/// POST /api/teams/{team_id}/slots — le créateur de la team définit un slot.
async fn create_team_slot(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(team_id): Path<Uuid>,
    Json(body): Json<CreateSlotBody>,
) -> Result<impl IntoResponse, AppError> {
    require_team_creator(&state.db, team_id, auth.user_id).await?;
    let slot = crate::services::TeamRolesService::create_slot(
        &state.db,
        crate::services::CreateSlotParams {
            team_id,
            role_slug: body.role_slug.trim(),
            role_display_name: body.role_display_name.as_deref(),
            required_skill_slug: body.required_skill_slug.as_deref(),
            min_proficiency_level: body.min_proficiency_level.unwrap_or(1),
        },
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(build_response(json!({ "slot": slot }))),
    ))
}

/// POST /api/teams/{team_id}/slots/{slot_id}/fill — user prend le slot.
async fn fill_team_slot(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((_team_id, slot_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, AppError> {
    let slot =
        crate::services::TeamRolesService::fill_slot(&state.db, slot_id, auth.user_id).await?;
    Ok(Json(build_response(json!({ "slot": slot }))))
}

/// POST /api/teams/{team_id}/slots/{slot_id}/leave — user libère son slot.
async fn leave_team_slot(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((_team_id, slot_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, AppError> {
    let slot =
        crate::services::TeamRolesService::leave_slot(&state.db, slot_id, auth.user_id).await?;
    Ok(Json(build_response(json!({ "slot": slot }))))
}

/// DELETE /api/teams/{team_id}/slots/{slot_id} — créateur supprime un slot vide.
async fn delete_team_slot(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((team_id, slot_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_team_creator(&state.db, team_id, auth.user_id).await?;
    crate::services::TeamRolesService::delete_slot(&state.db, slot_id).await?;
    Ok(Json(build_response(json!({ "deleted": true }))))
}

/// GET /api/team-slots/open?role=musician&limit=20
/// Marketplace : trouver les teams qui cherchent mon rôle.
async fn list_open_slots_by_role(
    State(state): State<AppState>,
    _auth: AuthUser,
    axum::extract::Query(q): axum::extract::Query<OpenSlotsQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let slots = crate::services::TeamRolesService::find_open_slots_by_role(
        &state.db,
        q.role.trim(),
        q.limit.unwrap_or(20),
    )
    .await?;
    Ok(Json(build_response(json!({ "slots": slots }))))
}
