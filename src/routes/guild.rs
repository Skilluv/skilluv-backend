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
use crate::services::guild;

// Type aliases pour clippy::type_complexity (rows sqlx::query_as des handlers
// BE-P0-39/40 qui joignent applications/invitations avec users).
type ApplicationRow = (
    Uuid,
    Uuid,
    String,
    String,
    chrono::DateTime<chrono::Utc>,
    String,
    Option<String>,
);
type InvitationRow = (
    Uuid,
    Option<Uuid>,
    Option<String>,
    Option<String>,
    Option<String>,
    chrono::DateTime<chrono::Utc>,
    chrono::DateTime<chrono::Utc>,
);

pub fn guild_routes() -> Router<AppState> {
    Router::new()
        .route("/guilds", post(create_guild).get(list_for_leaderboard))
        .route("/guilds/{slug}", get(get_by_slug))
        .route("/guilds/{id}/members", get(list_members))
        .route("/guilds/{id}/members/{user_id}/role", post(promote_member))
        .route("/guilds/{id}/members/{user_id}", delete(kick_member))
        .route("/guilds/me/leave", post(leave_guild))
        // Invitations
        .route(
            "/guilds/{id}/invitations",
            post(invite_direct).get(list_invitations), // BE-P0-40
        )
        .route("/guilds/{id}/invitations/link", post(create_token_link))
        .route("/guild-invitations/{id}/accept", post(accept_invite))
        // Trello BE-P0-41 — owner/officer révoque une invitation avant expires_at.
        .route("/guild-invitations/{id}", delete(revoke_invitation))
        // SKI-289 — guild-scoped spelling, which is what the front calls.
        .route(
            "/guilds/{id}/invitations/{invitation_id}",
            delete(revoke_guild_invitation),
        )
        .route("/guilds/join-by-token", post(join_by_token))
        // Applications
        .route(
            "/guilds/{id}/applications",
            post(apply).get(list_applications), // BE-P0-39
        )
        .route("/guild-applications/{id}/decide", post(decide_application))
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

/// Create a new guild.
#[utoipa::path(
    post, path = "/api/guilds", tag = "guilds",
    request_body(content = serde_json::Value),
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn create_guild(
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
            membership_mode: body.membership_mode.unwrap_or_else(|| "application".into()),
            cofounder_ids: body.cofounder_ids,
        },
    )
    .await?;

    // Notify co-founders
    for uid in &created.cofounders_added {
        let _ = crate::services::notify::send(
            &state,
            crate::services::notify::Recipient::User(*uid),
            "guild.cofounder_added",
        )
        .arg("tag", created.guild.tag.clone())
        .arg("guild", created.guild.name.clone())
        .payload(json!({ "guild_id": created.guild.id, "guild_slug": created.guild.slug }))
        .execute()
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

/// List guilds for leaderboard.
#[utoipa::path(
    get, path = "/api/guilds", tag = "guilds",
    responses((status = 200, body = serde_json::Value)),
)]
pub async fn list_for_leaderboard(
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

/// Get a guild by slug.
#[utoipa::path(
    get, path = "/api/guilds/{slug}", tag = "guilds",
    params(("slug" = String, Path)),
    responses((status = 200, body = serde_json::Value)),
)]
pub async fn get_by_slug(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<Value>, AppError> {
    let g = guild::by_slug(&state.db, &slug).await?;
    Ok(Json(build_response(json!({ "guild": g }))))
}

/// List guild members.
#[utoipa::path(
    get, path = "/api/guilds/{id}/members", tag = "guilds",
    params(("id" = uuid::Uuid, Path)),
    responses((status = 200, body = serde_json::Value)),
    operation_id = "guildListMembers",
)]
pub async fn list_members(
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

/// Change a guild member's role.
#[utoipa::path(
    post, path = "/api/guilds/{id}/members/{user_id}/role", tag = "guilds",
    params(("id" = uuid::Uuid, Path), ("user_id" = uuid::Uuid, Path)),
    request_body(content = serde_json::Value),
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn promote_member(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((guild_id, target_id)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    Json(body): Json<PromoteBody>,
) -> Result<Json<Value>, AppError> {
    guild::promote(&state.db, guild_id, auth.user_id, target_id, &body.role).await?;
    let guild_slug: Option<String> = sqlx::query_scalar("SELECT slug FROM guilds WHERE id = $1")
        .bind(guild_id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten();
    let _ = crate::services::notify::send(
        &state,
        crate::services::notify::Recipient::User(target_id),
        "guild.role_changed",
    )
    .arg("role", body.role.clone())
    .payload(json!({
        "guild_id": guild_id,
        "new_role": body.role,
        "guild_slug": guild_slug,
    }))
    .execute()
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

/// Kick a member from a guild.
#[utoipa::path(
    delete, path = "/api/guilds/{id}/members/{user_id}", tag = "guilds",
    params(("id" = uuid::Uuid, Path), ("user_id" = uuid::Uuid, Path)),
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn kick_member(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((guild_id, target_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>, AppError> {
    guild::kick_member(&state.db, guild_id, auth.user_id, target_id).await?;
    let guild_name: String = sqlx::query_scalar("SELECT name FROM guilds WHERE id = $1")
        .bind(guild_id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .unwrap_or_default();
    // No button on this one: there is nothing to click, and one would be a
    // taunt. See migration 0161.
    let _ = crate::services::notify::send(
        &state,
        crate::services::notify::Recipient::User(target_id),
        "guild.kicked",
    )
    .arg("guild", guild_name)
    .payload(json!({ "guild_id": guild_id }))
    .execute()
    .await;
    Ok(Json(build_response(json!({ "kicked": true }))))
}

/// Leave your current guild.
#[utoipa::path(
    post, path = "/api/guilds/me/leave", tag = "guilds",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn leave_guild(
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

/// Send a direct guild invitation to a user.
#[utoipa::path(
    post, path = "/api/guilds/{id}/invitations", tag = "guilds",
    params(("id" = uuid::Uuid, Path)),
    request_body(content = serde_json::Value),
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn invite_direct(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(guild_id): Path<Uuid>,
    headers: HeaderMap,
    Json(body): Json<InviteDirectBody>,
) -> Result<Json<Value>, AppError> {
    let invite =
        guild::invite_direct(&state.db, auth.user_id, guild_id, body.invited_user_id).await?;
    let g = guild::by_id(&state.db, guild_id).await?;
    let inviter_name: String = sqlx::query_scalar("SELECT display_name FROM users WHERE id = $1")
        .bind(auth.user_id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .unwrap_or_default();
    let _ = crate::services::notify::send(
        &state,
        crate::services::notify::Recipient::User(body.invited_user_id),
        "guild.invitation",
    )
    .arg("guild", g.name.clone())
    .arg("inviter", inviter_name)
    .payload(json!({
        "guild_id": guild_id,
        "guild_slug": g.slug,
        "invitation_id": invite.id,
    }))
    .execute()
    .await;
    if analytics_consent(&headers) {
        state.analytics.track(
            auth.user_id,
            events::GUILD_INVITE_SENT,
            props(&[("guild_id", json!(guild_id)), ("kind", json!("direct"))]),
        );
    }
    Ok(Json(build_response(json!({ "invitation": invite }))))
}

/// Create a shareable invitation token link.
#[utoipa::path(
    post, path = "/api/guilds/{id}/invitations/link", tag = "guilds",
    params(("id" = uuid::Uuid, Path)),
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn create_token_link(
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
            props(&[("guild_id", json!(guild_id)), ("kind", json!("token"))]),
        );
    }
    Ok(Json(build_response(json!({ "invitation": invite }))))
}

/// Accept a direct guild invitation.
#[utoipa::path(
    post, path = "/api/guild-invitations/{id}/accept", tag = "guilds",
    params(("id" = uuid::Uuid, Path)),
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
    operation_id = "guildAcceptInvite",
)]
pub async fn accept_invite(
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

/// Payload of the invitation-revocation routes.
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct RevokedInvitation {
    /// Always true on success. Revoking an already-revoked invitation is a
    /// success too — see `services::guild::revoke_invitation`.
    pub revoked: bool,
    pub invitation_id: Uuid,
}

/// DELETE /api/guild-invitations/{id} — owner/officer révoque une invitation
/// direct ou un lien token avant `expires_at`. Trello BE-P0-41.
#[utoipa::path(
    delete, path = "/api/guild-invitations/{id}", tag = "guilds",
    params(("id" = uuid::Uuid, Path, description = "Invitation id")),
    responses(
        (status = 200, description = "Invitation revoked", body = crate::api_response::ApiResponse<RevokedInvitation>),
        (status = 403, description = "Not an officer of the guild", body = crate::api_response::ErrorResponse),
        (status = 404, description = "Unknown invitation", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn revoke_invitation(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(invitation_id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    guild::revoke_invitation(&state.db, invitation_id, auth.user_id).await?;
    Ok(Json(build_response(json!({
        "revoked": true,
        "invitation_id": invitation_id,
    }))))
}

/// SKI-289 — the guild-scoped spelling the front end calls.
///
/// Same behaviour as the flat route above, plus a check that the
/// invitation belongs to the guild in the path. Both are kept: the flat
/// one is already in use, and an id is enough to identify an invitation.
#[utoipa::path(
    delete, path = "/api/guilds/{id}/invitations/{invitation_id}", tag = "guilds",
    params(
        ("id" = uuid::Uuid, Path, description = "Guild id"),
        ("invitation_id" = uuid::Uuid, Path, description = "Invitation id"),
    ),
    responses(
        (status = 200, description = "Invitation revoked", body = crate::api_response::ApiResponse<RevokedInvitation>),
        (status = 403, description = "Not an officer of the guild", body = crate::api_response::ErrorResponse),
        (status = 404, description = "Unknown invitation, or not in this guild", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn revoke_guild_invitation(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((guild_id, invitation_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>, AppError> {
    guild::revoke_invitation_in_guild(&state.db, guild_id, invitation_id, auth.user_id).await?;
    Ok(Json(build_response(json!({
        "revoked": true,
        "invitation_id": invitation_id,
    }))))
}

#[derive(Deserialize)]
struct JoinByTokenBody {
    token: String,
}

/// Join a guild via a shareable token.
#[utoipa::path(
    post, path = "/api/guilds/join-by-token", tag = "guilds",
    request_body(content = serde_json::Value),
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn join_by_token(
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
            props(&[("guild_id", json!(guild_id)), ("via", json!("token"))]),
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

/// Apply to join a guild.
#[utoipa::path(
    post, path = "/api/guilds/{id}/applications",
    operation_id = "guildApply",
    tag = "guilds",
    params(("id" = uuid::Uuid, Path)),
    request_body(content = serde_json::Value),
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn apply(
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

/// A user named in an invitation or an application.
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct GuildUserRef {
    pub id: Uuid,
    pub username: Option<String>,
    pub display_name: Option<String>,
}

/// The shareable token of a link invitation.
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct InvitationToken {
    pub value: String,
}

/// One pending invitation.
///
/// SKI-289 — typed because the front could otherwise only guess this
/// shape, and guessing a contract is what produced the empty-page bugs
/// this ticket cites. Exactly one of `invitee` / `token` is set: a direct
/// invitation names a user, a link invitation carries a token.
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct GuildInvitationView {
    pub id: Uuid,
    /// Set for a direct invitation, `null` for a shareable link.
    pub invitee: Option<GuildUserRef>,
    /// Set for a shareable link, `null` for a direct invitation.
    pub token: Option<InvitationToken>,
    /// RFC 3339.
    pub expires_at: String,
    pub sent_at: String,
}

/// Payload of `GET /api/guilds/{id}/invitations`.
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct GuildInvitationsData {
    pub invitations: Vec<GuildInvitationView>,
}

/// One pending application to join a guild.
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct GuildApplicationView {
    pub id: Uuid,
    pub applicant: GuildUserRef,
    pub message: Option<String>,
    /// Always `pending` on this endpoint — decided applications drop out.
    pub status: String,
    /// RFC 3339.
    pub applied_at: String,
}

/// Payload of `GET /api/guilds/{id}/applications`.
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct GuildApplicationsData {
    pub applications: Vec<GuildApplicationView>,
}

/// BE-P0-40 : list pending invitations for a guild (owner/officer only).
/// Missing endpoint made the front's "Invitations" tab a black hole for
/// duplicate detection / revocation.
#[utoipa::path(
    get, path = "/api/guilds/{id}/invitations", tag = "guilds",
    params(("id" = uuid::Uuid, Path)),
    responses(
        (status = 200, description = "Pending invitations", body = crate::api_response::ApiResponse<GuildInvitationsData>),
        (status = 403, description = "Not an officer of the guild", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn list_invitations(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(guild_id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let is_officer: Option<(String,)> = sqlx::query_as(
        "SELECT role FROM guild_members
         WHERE guild_id = $1 AND user_id = $2 AND role IN ('founder', 'officer')",
    )
    .bind(guild_id)
    .bind(auth.user_id)
    .fetch_optional(&state.db)
    .await?;
    if is_officer.is_none() && auth.role != "admin" {
        return Err(AppError::Forbidden);
    }

    let rows: Vec<InvitationRow> = sqlx::query_as(
        r#"
        SELECT gi.id, gi.invited_user_id, u.username, u.display_name,
               gi.token, gi.expires_at, gi.created_at
        FROM guild_invitations gi
        LEFT JOIN users u ON u.id = gi.invited_user_id
        WHERE gi.guild_id = $1 AND gi.accepted_at IS NULL AND gi.revoked_at IS NULL
        ORDER BY gi.created_at DESC
        "#,
    )
    .bind(guild_id)
    .fetch_all(&state.db)
    .await?;

    let items: Vec<Value> = rows
        .into_iter()
        .map(
            |(id, invited_user_id, username, display_name, token, expires_at, created_at)| {
                json!({
                    "id": id,
                    "invitee": invited_user_id.map(|uid| json!({
                        "id": uid,
                        "username": username,
                        "display_name": display_name,
                    })),
                    "token": token.map(|t| json!({ "value": t })),
                    "expires_at": expires_at.to_rfc3339(),
                    "sent_at": created_at.to_rfc3339(),
                })
            },
        )
        .collect();

    Ok(Json(build_response(json!({ "invitations": items }))))
}

/// BE-P0-39 : founder/officer of a guild lists the pending applications.
/// Missing endpoint was making the "Applications" tab in the guild page
/// forever empty on the front (the ID needed by `/decide` was never
/// discoverable).
#[utoipa::path(
    get, path = "/api/guilds/{id}/applications", tag = "guilds",
    params(("id" = uuid::Uuid, Path)),
    responses(
        (status = 200, description = "Pending applications", body = crate::api_response::ApiResponse<GuildApplicationsData>),
        (status = 403, description = "Not an officer of the guild", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
    operation_id = "guildListApplications",
)]
pub async fn list_applications(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(guild_id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    // Owner/officer gate — same rule as the decide endpoint.
    let is_officer: Option<(String,)> = sqlx::query_as(
        "SELECT role FROM guild_members
         WHERE guild_id = $1 AND user_id = $2 AND role IN ('founder', 'officer')",
    )
    .bind(guild_id)
    .bind(auth.user_id)
    .fetch_optional(&state.db)
    .await?;
    if is_officer.is_none() && auth.role != "admin" {
        return Err(AppError::Forbidden);
    }

    let rows: Vec<ApplicationRow> = sqlx::query_as(
        r#"
            SELECT ga.id, ga.applicant_id, ga.message, ga.status, ga.created_at,
                   u.username, u.display_name
            FROM guild_applications ga
            JOIN users u ON u.id = ga.applicant_id
            WHERE ga.guild_id = $1 AND ga.status = 'pending'
            ORDER BY ga.created_at ASC
            "#,
    )
    .bind(guild_id)
    .fetch_all(&state.db)
    .await?;

    let items: Vec<Value> = rows
        .into_iter()
        .map(
            |(id, applicant_id, message, status, created_at, username, display_name)| {
                json!({
                    "id": id,
                    "applicant": {
                        "id": applicant_id,
                        "username": username,
                        "display_name": display_name,
                    },
                    "message": message,
                    "status": status,
                    "applied_at": created_at.to_rfc3339(),
                })
            },
        )
        .collect();

    Ok(Json(build_response(json!({ "applications": items }))))
}

/// Body of `POST /guild-applications/{id}/decide`.
///
/// SKI-293 — the request body was published as an untyped blob, so the only
/// way to learn the field name was to probe the API until it stopped
/// answering `missing field 'accept'`. A plausible guess like
/// `{"decision": "accept"}` compiles, passes a test written against the same
/// guess, and fails on first real use.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = GuildDecideBody)]
pub struct DecideBody {
    /// `true` accepts the applicant into the guild, `false` rejects it.
    pub accept: bool,
}

/// Envelope returned once the application is decided.
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct DecidedApplicationData {
    pub application: crate::services::guild::GuildApplication,
}

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct DecidedApplicationResponse {
    pub data: DecidedApplicationData,
    pub meta: crate::api_response::MetaInfo,
}

/// Decide (accept/reject) a guild application. Officers only.
#[utoipa::path(
    post, path = "/api/guild-applications/{id}/decide", tag = "guilds",
    params(("id" = uuid::Uuid, Path, description = "Application UUID")),
    request_body = DecideBody,
    responses(
        (status = 200, description = "Application decided", body = DecidedApplicationResponse),
        (status = 400, description = "Application already decided", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Caller is not an officer of that guild", body = crate::api_response::ErrorResponse),
        (status = 404, description = "No such application", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn decide_application(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(application_id): Path<Uuid>,
    headers: HeaderMap,
    Json(body): Json<DecideBody>,
) -> Result<Json<Value>, AppError> {
    let app =
        guild::decide_application(&state.db, application_id, auth.user_id, body.accept).await?;
    let (decided_guild_name, decided_guild_slug): (String, String) =
        sqlx::query_as("SELECT name, slug FROM guilds WHERE id = $1")
            .bind(app.guild_id)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten()
            .unwrap_or_default();

    // The outcome is a translated word rather than two titles: a
    // translator reading `notification.guild.application_decided.body` sees
    // one sentence with a placeholder, not a branch to keep in sync.
    let decision_word = crate::services::i18n::t(
        "fr",
        if body.accept {
            "guild.decision.accepted"
        } else {
            "guild.decision.rejected"
        },
    );
    let _ = crate::services::notify::send(
        &state,
        crate::services::notify::Recipient::User(app.applicant_id),
        "guild.application_decided",
    )
    .arg("guild", decided_guild_name.clone())
    .arg("decision", decision_word)
    .payload(json!({
        "guild_id": app.guild_id,
        "guild_slug": decided_guild_slug,
        "accepted": body.accept,
    }))
    .execute()
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

/// Propose a guild war.
#[utoipa::path(
    post, path = "/api/guild-wars", tag = "guilds",
    request_body(content = serde_json::Value),
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn propose_war(
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
    let (challenger_name, defender_name, defender_slug) = {
        let names: Vec<(Uuid, String, String)> =
            sqlx::query_as("SELECT id, name, slug FROM guilds WHERE id = ANY($1)")
                .bind(vec![body.challenger_guild_id, body.defender_guild_id])
                .fetch_all(&state.db)
                .await
                .unwrap_or_default();
        let pick = |id: Uuid| {
            names
                .iter()
                .find(|(gid, _, _)| *gid == id)
                .map(|(_, n, s)| (n.clone(), s.clone()))
                .unwrap_or_default()
        };
        let (cname, _) = pick(body.challenger_guild_id);
        let (dname, dslug) = pick(body.defender_guild_id);
        (cname, dname, dslug)
    };

    for (uid,) in &officers {
        let _ = crate::services::notify::send(
            &state,
            crate::services::notify::Recipient::User(*uid),
            "guild.war_proposed",
        )
        .arg("challenger", challenger_name.clone())
        .arg("defender", defender_name.clone())
        .arg("stake", body.stake_gp.to_string())
        .payload(json!({
            "war_id": war.id,
            "challenger_guild_id": body.challenger_guild_id,
            "stake_gp": body.stake_gp,
            "guild_slug": defender_slug,
        }))
        .execute()
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

/// List guild wars.
#[utoipa::path(
    get, path = "/api/guild-wars", tag = "guilds",
    responses((status = 200, body = serde_json::Value)),
)]
pub async fn list_wars(
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

/// Respond to a proposed guild war.
#[utoipa::path(
    post, path = "/api/guild-wars/{id}/respond", tag = "guilds",
    params(("id" = uuid::Uuid, Path)),
    request_body(content = serde_json::Value),
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn respond_war(
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
            props(&[("war_id", json!(war.id)), ("accepted", json!(body.accept))]),
        );
    }
    Ok(Json(build_response(json!({ "war": war }))))
}

#[derive(Deserialize)]
struct ConcludeBody {
    winner_guild_id: Uuid,
}

/// Conclude a guild war (admin or winner officer).
#[utoipa::path(
    post, path = "/api/guild-wars/{id}/conclude", tag = "guilds",
    params(("id" = uuid::Uuid, Path)),
    request_body(content = serde_json::Value),
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn conclude_war(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(war_id): Path<Uuid>,
    headers: HeaderMap,
    Json(body): Json<ConcludeBody>,
) -> Result<Json<Value>, AppError> {
    // BE-P0-38 : admin OR founder/officer of the winner guild can conclude.
    // (Sprint 6 will add automatic scoring based on evidence submissions.)
    if auth.role != "admin" {
        let is_officer: Option<(String,)> = sqlx::query_as(
            "SELECT role FROM guild_members
             WHERE guild_id = $1 AND user_id = $2 AND role IN ('founder', 'officer')",
        )
        .bind(body.winner_guild_id)
        .bind(auth.user_id)
        .fetch_optional(&state.db)
        .await?;
        if is_officer.is_none() {
            return Err(AppError::Forbidden);
        }
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

/// Admin: dissolve a guild.
#[utoipa::path(
    post, path = "/api/admin/guilds/{id}/dissolve", tag = "guilds",
    params(("id" = uuid::Uuid, Path)),
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn admin_dissolve(
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
/// Get the skill composition matrix of a guild.
#[utoipa::path(
    get, path = "/api/guilds/{slug}/composition", tag = "guilds",
    params(("slug" = String, Path)),
    responses((status = 200, body = serde_json::Value)),
)]
pub async fn guild_composition(
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
