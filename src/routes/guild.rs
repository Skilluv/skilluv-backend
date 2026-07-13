//! Guild routes — Phase 2 Sprint 4.

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::routes::analytics_consent;
use crate::services::analytics::{events, props};
use crate::services::{NotificationService, guild};

pub fn guild_routes() -> Router<AppState> {
    Router::new()
        .route("/guilds", post(create_guild).get(list_for_leaderboard))
        .route("/guilds/{slug}", get(get_by_slug))
        .route("/guilds/{id}/members", get(list_members))
        .route("/guilds/{id}/members/{user_id}/role", post(promote_member))
        .route("/guilds/{id}/members/{user_id}", delete(kick_member))
        .route("/guilds/me/leave", post(leave_guild))
        // Invitations
        .route("/guilds/{id}/invitations", post(invite_direct))
        .route("/guilds/{id}/invitations/link", post(create_token_link))
        .route("/guild-invitations/{id}/accept", post(accept_invite))
        .route("/guilds/join-by-token", post(join_by_token))
        // Applications
        .route("/guilds/{id}/applications", post(apply))
        .route(
            "/guild-applications/{id}/decide",
            post(decide_application),
        )
        // Wars
        .route("/guild-wars", post(propose_war).get(list_wars))
        .route("/guild-wars/{id}/respond", post(respond_war))
        .route("/guild-wars/{id}/conclude", post(conclude_war))
        // Moderation
        .route("/admin/guilds/{id}/dissolve", post(admin_dissolve))
        // P10.6 — skill matrix (agrégat par domaine)
        .route("/guilds/{slug}/composition", get(guild_composition))
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

// ─── Create ──────────────────────────────────────────────────────

#[derive(Deserialize)]
struct CreateGuildBody {
    slug: String,
    tag: String,
    name: String,
    description: Option<String>,
    motto: Option<String>,
    membership_mode: Option<String>,
    cofounder_ids: Vec<Uuid>,
}

async fn create_guild(
    State(state): State<AppState>,
    auth: AuthUser,
    headers: HeaderMap,
    Json(body): Json<CreateGuildBody>,
) -> Result<Json<Value>, AppError> {
    let created = guild::create_guild(
        &state.db,
        guild::CreateGuildInput {
            founder_id: auth.user_id,
            slug: body.slug,
            tag: body.tag,
            name: body.name,
            description: body.description,
            motto: body.motto,
            membership_mode: body
                .membership_mode
                .unwrap_or_else(|| "application".into()),
            cofounder_ids: body.cofounder_ids,
        },
    )
    .await?;

    // Notify co-founders
    for uid in &created.cofounders_added {
        let _ = NotificationService::send(
            &state.db,
            &mut state.redis.clone(),
            &state.ws,
            *uid,
            "guild.cofounder_added",
            "Tu as co-fondé une guilde",
            Some(&format!("[{}] {}", created.guild.tag, created.guild.name)),
            Some(json!({ "guild_id": created.guild.id, "guild_slug": created.guild.slug })),
        )
        .await;
    }

    if analytics_consent(&headers) {
        state.analytics.track(
            auth.user_id,
            events::GUILD_CREATED,
            props(&[
                ("guild_id", json!(created.guild.id)),
                ("cofounders", json!(created.cofounders_added.len())),
            ]),
        );
    }
    metrics::counter!("skilluv_guilds_created_total").increment(1);

    Ok(Json(build_response(json!({
        "guild": created.guild,
        "forum_category_id": created.forum_category_id,
        "cofounders_added": created.cofounders_added,
    }))))
}

#[derive(Deserialize)]
struct LeaderboardQuery {
    season: Option<bool>,
    division: Option<String>,
    limit: Option<i64>,
}

async fn list_for_leaderboard(
    State(state): State<AppState>,
    Query(q): Query<LeaderboardQuery>,
) -> Result<Json<Value>, AppError> {
    let rows = guild::leaderboard(
        &state.db,
        q.season.unwrap_or(false),
        q.division.as_deref(),
        q.limit.unwrap_or(50),
    )
    .await?;
    Ok(Json(build_response(json!({ "guilds": rows }))))
}

async fn get_by_slug(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<Value>, AppError> {
    let g = guild::by_slug(&state.db, &slug).await?;
    Ok(Json(build_response(json!({ "guild": g }))))
}

async fn list_members(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let members = guild::list_members(&state.db, id).await?;
    Ok(Json(build_response(json!({ "members": members }))))
}

#[derive(Deserialize)]
struct PromoteBody {
    role: String,
}

async fn promote_member(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((guild_id, target_id)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    Json(body): Json<PromoteBody>,
) -> Result<Json<Value>, AppError> {
    guild::promote(&state.db, guild_id, auth.user_id, target_id, &body.role).await?;
    let _ = NotificationService::send(
        &state.db,
        &mut state.redis.clone(),
        &state.ws,
        target_id,
        "guild.role_changed",
        "Ton rôle dans la guilde a changé",
        Some(&format!("nouveau rôle : {}", body.role)),
        Some(json!({ "guild_id": guild_id, "new_role": body.role })),
    )
    .await;
    if analytics_consent(&headers) {
        state.analytics.track(
            auth.user_id,
            events::GUILD_MEMBER_PROMOTED,
            props(&[
                ("guild_id", json!(guild_id)),
                ("target_user_id", json!(target_id)),
                ("new_role", json!(body.role)),
            ]),
        );
    }
    Ok(Json(build_response(json!({ "updated": true }))))
}

async fn kick_member(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((guild_id, target_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>, AppError> {
    guild::kick_member(&state.db, guild_id, auth.user_id, target_id).await?;
    let _ = NotificationService::send(
        &state.db,
        &mut state.redis.clone(),
        &state.ws,
        target_id,
        "guild.kicked",
        "Tu as été retiré·e de la guilde",
        None,
        Some(json!({ "guild_id": guild_id })),
    )
    .await;
    Ok(Json(build_response(json!({ "kicked": true }))))
}

async fn leave_guild(
    State(state): State<AppState>,
    auth: AuthUser,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let guild_id = guild::leave_guild(&state.db, auth.user_id).await?;
    if analytics_consent(&headers) {
        state.analytics.track(
            auth.user_id,
            events::GUILD_LEFT,
            props(&[("guild_id", json!(guild_id))]),
        );
    }
    metrics::counter!("skilluv_guild_leaves_total").increment(1);
    Ok(Json(build_response(json!({ "left_guild_id": guild_id }))))
}

// ─── Invitations ─────────────────────────────────────────────────

#[derive(Deserialize)]
struct InviteDirectBody {
    invited_user_id: Uuid,
}

async fn invite_direct(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(guild_id): Path<Uuid>,
    headers: HeaderMap,
    Json(body): Json<InviteDirectBody>,
) -> Result<Json<Value>, AppError> {
    let invite =
        guild::invite_direct(&state.db, auth.user_id, guild_id, body.invited_user_id).await?;
    let g = guild::by_id(&state.db, guild_id).await?;
    let _ = NotificationService::send(
        &state.db,
        &mut state.redis.clone(),
        &state.ws,
        body.invited_user_id,
        "guild.invitation",
        "Tu as reçu une invitation de guilde",
        Some(&format!("[{}] {}", g.tag, g.name)),
        Some(json!({ "guild_id": guild_id, "invitation_id": invite.id })),
    )
    .await;
    if analytics_consent(&headers) {
        state.analytics.track(
            auth.user_id,
            events::GUILD_INVITE_SENT,
            props(&[
                ("guild_id", json!(guild_id)),
                ("kind", json!("direct")),
            ]),
        );
    }
    Ok(Json(build_response(json!({ "invitation": invite }))))
}

async fn create_token_link(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(guild_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let invite = guild::create_shareable_token(&state.db, auth.user_id, guild_id).await?;
    if analytics_consent(&headers) {
        state.analytics.track(
            auth.user_id,
            events::GUILD_INVITE_SENT,
            props(&[
                ("guild_id", json!(guild_id)),
                ("kind", json!("token")),
            ]),
        );
    }
    Ok(Json(build_response(json!({ "invitation": invite }))))
}

async fn accept_invite(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(invitation_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let guild_id = guild::accept_direct_invitation(&state.db, invitation_id, auth.user_id).await?;
    if analytics_consent(&headers) {
        state.analytics.track(
            auth.user_id,
            events::GUILD_JOINED,
            props(&[
                ("guild_id", json!(guild_id)),
                ("via", json!("direct_invite")),
            ]),
        );
    }
    metrics::counter!("skilluv_guild_joins_total", "via" => "direct_invite").increment(1);
    Ok(Json(build_response(json!({ "joined_guild_id": guild_id }))))
}

#[derive(Deserialize)]
struct JoinByTokenBody {
    token: String,
}

async fn join_by_token(
    State(state): State<AppState>,
    auth: AuthUser,
    headers: HeaderMap,
    Json(body): Json<JoinByTokenBody>,
) -> Result<Json<Value>, AppError> {
    let guild_id = guild::join_by_token(&state.db, &body.token, auth.user_id).await?;
    if analytics_consent(&headers) {
        state.analytics.track(
            auth.user_id,
            events::GUILD_JOINED,
            props(&[
                ("guild_id", json!(guild_id)),
                ("via", json!("token")),
            ]),
        );
    }
    metrics::counter!("skilluv_guild_joins_total", "via" => "token").increment(1);
    Ok(Json(build_response(json!({ "joined_guild_id": guild_id }))))
}

// ─── Applications ────────────────────────────────────────────────

#[derive(Deserialize)]
struct ApplyBody {
    message: String,
}

async fn apply(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(guild_id): Path<Uuid>,
    headers: HeaderMap,
    Json(body): Json<ApplyBody>,
) -> Result<Json<Value>, AppError> {
    let app = guild::apply_to_guild(&state.db, guild_id, auth.user_id, &body.message).await?;
    if analytics_consent(&headers) {
        state.analytics.track(
            auth.user_id,
            events::GUILD_APPLICATION_SUBMITTED,
            props(&[("guild_id", json!(guild_id))]),
        );
    }
    Ok(Json(build_response(json!({ "application": app }))))
}

#[derive(Deserialize)]
struct DecideBody {
    accept: bool,
}

async fn decide_application(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(application_id): Path<Uuid>,
    headers: HeaderMap,
    Json(body): Json<DecideBody>,
) -> Result<Json<Value>, AppError> {
    let app = guild::decide_application(&state.db, application_id, auth.user_id, body.accept).await?;
    let _ = NotificationService::send(
        &state.db,
        &mut state.redis.clone(),
        &state.ws,
        app.applicant_id,
        "guild.application_decision",
        if body.accept {
            "Ta candidature a été acceptée"
        } else {
            "Ta candidature a été refusée"
        },
        None,
        Some(json!({ "guild_id": app.guild_id, "accepted": body.accept })),
    )
    .await;
    if analytics_consent(&headers) {
        state.analytics.track(
            auth.user_id,
            events::GUILD_APPLICATION_DECIDED,
            props(&[
                ("guild_id", json!(app.guild_id)),
                ("accepted", json!(body.accept)),
            ]),
        );
    }
    Ok(Json(build_response(json!({ "application": app }))))
}

// ─── Wars ────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ProposeWarBody {
    challenger_guild_id: Uuid,
    defender_guild_id: Uuid,
    stake_gp: i64,
}

async fn propose_war(
    State(state): State<AppState>,
    auth: AuthUser,
    headers: HeaderMap,
    Json(body): Json<ProposeWarBody>,
) -> Result<Json<Value>, AppError> {
    let war = guild::propose_war(
        &state.db,
        auth.user_id,
        body.challenger_guild_id,
        body.defender_guild_id,
        body.stake_gp,
    )
    .await?;
    // Notify all officers of the defender guild
    let officers: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT user_id FROM guild_members WHERE guild_id = $1 AND role IN ('founder', 'officer')",
    )
    .bind(body.defender_guild_id)
    .fetch_all(&state.db)
    .await?;
    for (uid,) in &officers {
        let _ = NotificationService::send(
            &state.db,
            &mut state.redis.clone(),
            &state.ws,
            *uid,
            "guild_war.proposed",
            "Une guilde te défie",
            None,
            Some(json!({
                "war_id": war.id,
                "challenger_guild_id": body.challenger_guild_id,
                "stake_gp": body.stake_gp,
            })),
        )
        .await;
    }
    if analytics_consent(&headers) {
        state.analytics.track(
            auth.user_id,
            events::GUILD_WAR_PROPOSED,
            props(&[
                ("war_id", json!(war.id)),
                ("stake_gp", json!(body.stake_gp)),
            ]),
        );
    }
    metrics::counter!("skilluv_guild_wars_proposed_total").increment(1);
    Ok(Json(build_response(json!({ "war": war }))))
}

async fn list_wars(
    State(state): State<AppState>,
    Query(q): Query<ListWarsQuery>,
) -> Result<Json<Value>, AppError> {
    let rows: Vec<guild::GuildWar> = sqlx::query_as(
        r#"
        SELECT * FROM guild_wars
        WHERE ($1::text IS NULL OR status = $1)
        ORDER BY proposed_at DESC
        LIMIT $2
        "#,
    )
    .bind(q.status.as_deref())
    .bind(q.limit.unwrap_or(50).clamp(1, 200))
    .fetch_all(&state.db)
    .await?;
    Ok(Json(build_response(json!({ "wars": rows }))))
}

#[derive(Deserialize)]
struct ListWarsQuery {
    status: Option<String>,
    limit: Option<i64>,
}

#[derive(Deserialize)]
struct WarResponseBody {
    accept: bool,
}

async fn respond_war(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(war_id): Path<Uuid>,
    headers: HeaderMap,
    Json(body): Json<WarResponseBody>,
) -> Result<Json<Value>, AppError> {
    let war = guild::respond_to_war(&state.db, war_id, auth.user_id, body.accept).await?;
    if analytics_consent(&headers) {
        state.analytics.track(
            auth.user_id,
            events::GUILD_WAR_ACCEPTED,
            props(&[
                ("war_id", json!(war.id)),
                ("accepted", json!(body.accept)),
            ]),
        );
    }
    Ok(Json(build_response(json!({ "war": war }))))
}

#[derive(Deserialize)]
struct ConcludeBody {
    winner_guild_id: Uuid,
}

async fn conclude_war(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(war_id): Path<Uuid>,
    headers: HeaderMap,
    Json(body): Json<ConcludeBody>,
) -> Result<Json<Value>, AppError> {
    // For Sprint 4 V1, only admins can conclude (Sprint 6 will add automatic scoring).
    if auth.role != "admin" {
        return Err(AppError::Forbidden);
    }
    let war = guild::conclude_war(&state.db, war_id, body.winner_guild_id).await?;
    if analytics_consent(&headers) {
        state.analytics.track(
            auth.user_id,
            events::GUILD_WAR_CONCLUDED,
            props(&[
                ("war_id", json!(war.id)),
                ("winner_guild_id", json!(body.winner_guild_id)),
            ]),
        );
    }
    metrics::counter!("skilluv_guild_wars_concluded_total").increment(1);
    Ok(Json(build_response(json!({ "war": war }))))
}

// ─── Admin moderation ────────────────────────────────────────────

async fn admin_dissolve(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    if auth.role != "admin" {
        return Err(AppError::Forbidden);
    }
    guild::admin_dissolve(&state.db, id).await?;
    Ok(Json(build_response(json!({ "dissolved": true }))))
}

// ═══════════════════════════════════════════════════════════════════
// P10.6 — Skill matrix (dashboard officer + matching guilde ↔ project)
// ═══════════════════════════════════════════════════════════════════

/// GET /api/guilds/{slug}/composition
///
/// Retourne l'agrégat par domaine des skills des membres :
/// { domain, member_count, avg_level, top_skills }.
async fn guild_composition(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<Value>, AppError> {
    let guild_id: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM guilds WHERE slug = $1 AND disbanded_at IS NULL")
            .bind(&slug)
            .fetch_optional(&state.db)
            .await?;
    let guild_id = guild_id.ok_or_else(|| AppError::NotFound("Guild not found".into()))?;
    let matrix = guild::guild_skill_matrix(&state.db, guild_id).await?;
    Ok(Json(build_response(json!({
        "guild_id": guild_id,
        "composition": matrix,
    }))))
}
