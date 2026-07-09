//! SCIM 2.0 provisioning endpoints for enterprise SSO.
//!
//! Scope: Users + Groups + service provider discovery.
//! Auth: `Authorization: Bearer <scim_token>` — one active token per enterprise.
//! PATCH ops supported: `replace` on `active`, `name.givenName`, `name.familyName`,
//! `displayName`, and `members` (on Groups).
//!
//! Token management endpoints (`POST /enterprise/sso/scim/token`) live on the
//! same router but are protected by the owner-auth flow rather than the SCIM
//! bearer.

use axum::extract::{FromRequestParts, Path, Query, State};
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::audit::{self, ActorType, AuditEntry};
use crate::services::scim;

pub fn scim_routes() -> Router<AppState> {
    Router::new()
        // Owner-authenticated token mgmt + group-to-role mapping
        .route("/enterprise/sso/scim/token", post(create_scim_token))
        .route("/enterprise/sso/scim/token", delete(revoke_scim_token))
        .route(
            "/enterprise/sso/scim/groups/{id}/mapped-role",
            put(set_group_role_mapping),
        )
        // SCIM v2 (bearer-authenticated)
        .route("/scim/v2/ServiceProviderConfig", get(sp_config))
        .route("/scim/v2/ResourceTypes", get(resource_types))
        .route("/scim/v2/Schemas", get(schemas))
        .route("/scim/v2/Users", get(list_users).post(create_user))
        .route(
            "/scim/v2/Users/{id}",
            get(get_user).put(replace_user).patch(patch_user).delete(delete_user),
        )
        .route("/scim/v2/Groups", get(list_groups).post(create_group))
        .route(
            "/scim/v2/Groups/{id}",
            get(get_group).put(replace_group).patch(patch_group).delete(delete_group),
        )
}

/// Best-effort audit log for a SCIM operation. Actor is always
/// `ActorType::Enterprise` (the IdP acts on behalf of the enterprise) ;
/// enterprise_id and any interesting IDs go into `metadata`.
async fn audit_scim(
    state: &AppState,
    action: &'static str,
    enterprise_id: Uuid,
    target_type: Option<&'static str>,
    target_id: Option<Uuid>,
    extra: Value,
) {
    let mut meta = json!({ "enterprise_id": enterprise_id, "channel": "scim" });
    if let (Some(obj), Some(ex)) = (meta.as_object_mut(), extra.as_object()) {
        for (k, v) in ex {
            obj.insert(k.clone(), v.clone());
        }
    }
    audit::record(
        &state.db,
        AuditEntry {
            actor_type: ActorType::Enterprise,
            actor_id: None,
            action,
            target_type,
            target_id,
            metadata: Some(meta),
            headers: None,
        },
    )
    .await;
}

// ─── Bearer auth extractor ───────────────────────────────────────

pub struct ScimAuth {
    pub enterprise_id: Uuid,
}

impl FromRequestParts<AppState> for ScimAuth {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "))
            .ok_or(AppError::Unauthorized)?;
        let enterprise_id = scim::resolve_token(&state.db, token).await?;
        Ok(ScimAuth { enterprise_id })
    }
}

// ─── Owner-authenticated token management ────────────────────────

async fn create_scim_token(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let enterprise =
        crate::routes::enterprise::require_enterprise_owner_pub(&state, &auth).await?;
    let (cleartext, hash) = scim::generate_token();
    scim::set_token(&state.db, enterprise.id, &hash).await?;
    audit_scim(
        &state,
        "scim.token.rotated",
        enterprise.id,
        Some("enterprise_sso_config"),
        Some(enterprise.id),
        json!({ "actor_user_id": auth.user_id }),
    )
    .await;
    Ok(Json(json!({
        "data": {
            "token": cleartext,
            "message": "Store this token securely — it will not be shown again.",
            "scim_base_url": format!("{}/api/scim/v2", state.config.base_url),
        },
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })))
}

async fn revoke_scim_token(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let enterprise =
        crate::routes::enterprise::require_enterprise_owner_pub(&state, &auth).await?;
    scim::clear_token(&state.db, enterprise.id).await?;
    audit_scim(
        &state,
        "scim.token.revoked",
        enterprise.id,
        Some("enterprise_sso_config"),
        Some(enterprise.id),
        json!({ "actor_user_id": auth.user_id }),
    )
    .await;
    Ok(Json(json!({ "data": { "revoked": true } })))
}

#[derive(Deserialize)]
struct MappedRoleRequest {
    /// One of "recruiter", "enterprise", or null to clear the mapping.
    mapped_role: Option<String>,
}

async fn set_group_role_mapping(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(group_id): Path<Uuid>,
    Json(body): Json<MappedRoleRequest>,
) -> Result<Json<Value>, AppError> {
    let enterprise =
        crate::routes::enterprise::require_enterprise_owner_pub(&state, &auth).await?;
    let affected =
        scim::set_group_mapped_role(&state.db, enterprise.id, group_id, body.mapped_role.as_deref())
            .await?;
    audit_scim(
        &state,
        "scim.group.role_mapping_changed",
        enterprise.id,
        Some("scim_group"),
        Some(group_id),
        json!({
            "mapped_role": body.mapped_role,
            "affected_users": affected.len(),
        }),
    )
    .await;
    Ok(Json(json!({
        "data": {
            "group_id": group_id,
            "mapped_role": body.mapped_role,
            "affected_users": affected.len(),
        }
    })))
}

// ─── Discovery endpoints ─────────────────────────────────────────

async fn sp_config(_scim: ScimAuth) -> Json<Value> {
    Json(json!({
        "schemas": ["urn:ietf:params:scim:schemas:core:2.0:ServiceProviderConfig"],
        "documentationUri": "https://tools.ietf.org/html/rfc7644",
        "patch": { "supported": true },
        "bulk": { "supported": false, "maxOperations": 0, "maxPayloadSize": 0 },
        "filter": { "supported": true, "maxResults": 200 },
        "changePassword": { "supported": false },
        "sort": { "supported": false },
        "etag": { "supported": false },
        "authenticationSchemes": [{
            "type": "oauthbearertoken",
            "name": "OAuth Bearer Token",
            "description": "Authentication scheme using the OAuth Bearer Token Standard",
            "primary": true,
        }],
    }))
}

async fn resource_types(_scim: ScimAuth) -> Json<Value> {
    Json(json!({
        "schemas": ["urn:ietf:params:scim:api:messages:2.0:ListResponse"],
        "totalResults": 2,
        "Resources": [
            {
                "schemas": ["urn:ietf:params:scim:schemas:core:2.0:ResourceType"],
                "id": "User",
                "name": "User",
                "endpoint": "/Users",
                "schema": "urn:ietf:params:scim:schemas:core:2.0:User",
            },
            {
                "schemas": ["urn:ietf:params:scim:schemas:core:2.0:ResourceType"],
                "id": "Group",
                "name": "Group",
                "endpoint": "/Groups",
                "schema": "urn:ietf:params:scim:schemas:core:2.0:Group",
            }
        ]
    }))
}

async fn schemas(_scim: ScimAuth) -> Json<Value> {
    Json(json!({
        "schemas": ["urn:ietf:params:scim:api:messages:2.0:ListResponse"],
        "totalResults": 2,
        "Resources": [
            user_schema(),
            group_schema(),
        ],
    }))
}

fn user_schema() -> Value {
    json!({
        "id": "urn:ietf:params:scim:schemas:core:2.0:User",
        "name": "User",
        "description": "User account",
        "attributes": [
            { "name": "userName", "type": "string", "required": true, "uniqueness": "server" },
            { "name": "active", "type": "boolean", "required": false },
            { "name": "name", "type": "complex", "required": false, "subAttributes": [
                { "name": "givenName", "type": "string" },
                { "name": "familyName", "type": "string" },
            ]},
            { "name": "displayName", "type": "string", "required": false },
            { "name": "emails", "type": "complex", "multiValued": true, "required": true, "subAttributes": [
                { "name": "value", "type": "string", "required": true },
                { "name": "primary", "type": "boolean" },
            ]},
        ],
    })
}

fn group_schema() -> Value {
    json!({
        "id": "urn:ietf:params:scim:schemas:core:2.0:Group",
        "name": "Group",
        "attributes": [
            { "name": "displayName", "type": "string", "required": true },
            { "name": "members", "type": "complex", "multiValued": true, "subAttributes": [
                { "name": "value", "type": "string" },
                { "name": "type", "type": "string" },
            ]},
        ],
    })
}

// ─── SCIM Users ─────────────────────────────────────────────────

#[derive(Deserialize)]
struct ListQuery {
    filter: Option<String>,
    #[serde(rename = "startIndex")]
    start_index: Option<i64>,
    count: Option<i64>,
}

/// Parses SCIM filters we support: `userName eq "x"` and `displayName eq "x"`.
/// Anything else is silently ignored (returns no filter) to keep IdPs happy.
fn parse_eq_filter<'a>(filter: &'a str, attr: &str) -> Option<&'a str> {
    let prefix = format!("{attr} eq \"");
    let rest = filter.strip_prefix(&prefix)?;
    rest.strip_suffix('"')
}

async fn list_users(
    State(state): State<AppState>,
    scim: ScimAuth,
    Query(q): Query<ListQuery>,
) -> Result<Json<Value>, AppError> {
    let start_index = q.start_index.unwrap_or(1).max(1);
    let count = q.count.unwrap_or(50).clamp(1, 200);
    let filter_username = q
        .filter
        .as_deref()
        .and_then(|f| parse_eq_filter(f, "userName"));
    let (users, total) =
        scim::list_users(&state.db, scim.enterprise_id, filter_username, start_index, count)
            .await?;
    Ok(Json(json!({
        "schemas": ["urn:ietf:params:scim:api:messages:2.0:ListResponse"],
        "totalResults": total,
        "startIndex": start_index,
        "itemsPerPage": users.len(),
        "Resources": users.iter().map(user_to_scim).collect::<Vec<_>>(),
    })))
}

#[derive(Deserialize)]
struct ScimUserRequest {
    #[serde(rename = "userName")]
    user_name: String,
    #[serde(default)]
    #[serde(rename = "externalId")]
    external_id: Option<String>,
    #[serde(default)]
    name: Option<ScimName>,
    #[serde(default)]
    #[serde(rename = "displayName")]
    display_name: Option<String>,
    #[serde(default)]
    emails: Vec<ScimEmail>,
    #[serde(default = "default_active")]
    active: bool,
}
fn default_active() -> bool { true }

#[derive(Deserialize)]
struct ScimName {
    #[serde(default)]
    #[serde(rename = "givenName")]
    given_name: Option<String>,
    #[serde(default)]
    #[serde(rename = "familyName")]
    family_name: Option<String>,
}

#[derive(Deserialize)]
struct ScimEmail {
    value: String,
    #[serde(default)]
    primary: Option<bool>,
}

async fn create_user(
    State(state): State<AppState>,
    scim: ScimAuth,
    Json(body): Json<ScimUserRequest>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let primary_email = body
        .emails
        .iter()
        .find(|e| e.primary.unwrap_or(false))
        .or_else(|| body.emails.first())
        .ok_or_else(|| AppError::Validation("emails[] is required".into()))?
        .value
        .clone();

    // Default role for SCIM-provisioned users lives on the SSO config.
    let default_role: (String,) = sqlx::query_as(
        "SELECT default_role FROM enterprise_sso_configs WHERE enterprise_id = $1",
    )
    .bind(scim.enterprise_id)
    .fetch_one(&state.db)
    .await?;

    let user_id = scim::provision_user(
        &state.db,
        scim::NewScimUser {
            enterprise_id: scim.enterprise_id,
            external_id: body.external_id.as_deref(),
            user_name: &body.user_name,
            email: &primary_email,
            given_name: body.name.as_ref().and_then(|n| n.given_name.as_deref()),
            family_name: body.name.as_ref().and_then(|n| n.family_name.as_deref()),
            display_name: body.display_name.as_deref(),
            default_role: &default_role.0,
            active: body.active,
        },
    )
    .await
    .map_err(|e| match e {
        AppError::Validation(msg) if msg.contains("already exists") => {
            AppError::Validation(format!("SCIM_CONFLICT: {msg}"))
        }
        other => other,
    })?;

    let view = scim::get_user(&state.db, scim.enterprise_id, user_id)
        .await?
        .ok_or_else(|| AppError::Internal("provisioned user disappeared".into()))?;
    audit_scim(
        &state,
        "scim.user.provisioned",
        scim.enterprise_id,
        Some("user"),
        Some(view.id),
        json!({ "user_name": view.user_name, "email": view.email, "active": view.active }),
    )
    .await;
    Ok((StatusCode::CREATED, Json(user_to_scim(&view))))
}

async fn get_user(
    State(state): State<AppState>,
    scim: ScimAuth,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let view = scim::get_user(&state.db, scim.enterprise_id, id)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".into()))?;
    Ok(Json(user_to_scim(&view)))
}

async fn replace_user(
    State(state): State<AppState>,
    scim: ScimAuth,
    Path(id): Path<Uuid>,
    Json(body): Json<ScimUserRequest>,
) -> Result<Json<Value>, AppError> {
    // Ensure the user belongs to this enterprise.
    scim::get_user(&state.db, scim.enterprise_id, id)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".into()))?;

    scim::update_user_name(
        &state.db,
        id,
        body.name.as_ref().and_then(|n| n.given_name.as_deref()),
        body.name.as_ref().and_then(|n| n.family_name.as_deref()),
        body.display_name.as_deref(),
    )
    .await?;
    scim::set_user_active(&state.db, scim.enterprise_id, id, body.active).await?;

    let view = scim::get_user(&state.db, scim.enterprise_id, id)
        .await?
        .ok_or_else(|| AppError::Internal("user disappeared".into()))?;
    Ok(Json(user_to_scim(&view)))
}

#[derive(Deserialize)]
struct PatchRequest {
    #[serde(rename = "Operations")]
    operations: Vec<PatchOp>,
}

#[derive(Deserialize)]
struct PatchOp {
    op: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    value: Value,
}

async fn patch_user(
    State(state): State<AppState>,
    scim: ScimAuth,
    Path(id): Path<Uuid>,
    Json(body): Json<PatchRequest>,
) -> Result<Json<Value>, AppError> {
    scim::get_user(&state.db, scim.enterprise_id, id)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".into()))?;

    for op in &body.operations {
        if op.op.to_lowercase() != "replace" {
            // Silently accept add/remove on complex attrs we ignore.
            continue;
        }
        match op.path.as_deref() {
            Some("active") => {
                let active = op.value.as_bool().unwrap_or(true);
                scim::set_user_active(&state.db, scim.enterprise_id, id, active).await?;
                audit_scim(
                    &state,
                    if active { "scim.user.reactivated" } else { "scim.user.deactivated" },
                    scim.enterprise_id,
                    Some("user"),
                    Some(id),
                    json!({}),
                )
                .await;
            }
            Some("name.givenName") => {
                let given = op.value.as_str();
                scim::update_user_name(&state.db, id, given, None, None).await?;
            }
            Some("name.familyName") => {
                let family = op.value.as_str();
                scim::update_user_name(&state.db, id, None, family, None).await?;
            }
            Some("displayName") => {
                let display = op.value.as_str();
                scim::update_user_name(&state.db, id, None, None, display).await?;
            }
            None => {
                // Bulk replace object — accept { active, name, displayName }.
                if let Some(b) = op.value.get("active").and_then(|v| v.as_bool()) {
                    scim::set_user_active(&state.db, scim.enterprise_id, id, b).await?;
                }
                let given = op
                    .value
                    .get("name")
                    .and_then(|n| n.get("givenName"))
                    .and_then(|v| v.as_str());
                let family = op
                    .value
                    .get("name")
                    .and_then(|n| n.get("familyName"))
                    .and_then(|v| v.as_str());
                let display = op.value.get("displayName").and_then(|v| v.as_str());
                if given.is_some() || family.is_some() || display.is_some() {
                    scim::update_user_name(&state.db, id, given, family, display).await?;
                }
            }
            _ => {}
        }
    }

    let view = scim::get_user(&state.db, scim.enterprise_id, id)
        .await?
        .ok_or_else(|| AppError::Internal("user disappeared".into()))?;
    Ok(Json(user_to_scim(&view)))
}

async fn delete_user(
    State(state): State<AppState>,
    scim: ScimAuth,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    scim::get_user(&state.db, scim.enterprise_id, id)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".into()))?;
    // SCIM DELETE ≡ soft-delete: deactivate + revoke all sessions. The user
    // row persists for audit / RGPD reasons.
    scim::set_user_active(&state.db, scim.enterprise_id, id, false).await?;
    audit_scim(
        &state,
        "scim.user.deleted",
        scim.enterprise_id,
        Some("user"),
        Some(id),
        json!({}),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

fn user_to_scim(view: &scim::ScimUserView) -> Value {
    json!({
        "schemas": ["urn:ietf:params:scim:schemas:core:2.0:User"],
        "id": view.id,
        "externalId": view.external_id,
        "userName": view.user_name,
        "name": {
            "givenName": view.given_name,
            "familyName": view.family_name,
        },
        "displayName": view.display_name,
        "emails": [{
            "value": view.email,
            "primary": true,
        }],
        "active": view.active,
        "meta": {
            "resourceType": "User",
            "created": view.created_at.to_rfc3339(),
            "lastModified": view.updated_at.to_rfc3339(),
        }
    })
}

// ─── SCIM Groups ─────────────────────────────────────────────────

#[derive(Deserialize)]
struct ScimGroupRequest {
    #[serde(rename = "displayName")]
    display_name: String,
    #[serde(default)]
    #[serde(rename = "externalId")]
    external_id: Option<String>,
    #[serde(default)]
    members: Vec<ScimGroupMember>,
}

#[derive(Deserialize)]
struct ScimGroupMember {
    value: String, // user id (UUID)
}

async fn create_group(
    State(state): State<AppState>,
    scim: ScimAuth,
    Json(body): Json<ScimGroupRequest>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let group_id = scim::create_group(
        &state.db,
        scim.enterprise_id,
        body.external_id.as_deref(),
        &body.display_name,
    )
    .await?;
    let member_ids = parse_member_uuids(&body.members)?;
    if !member_ids.is_empty() {
        scim::add_group_members(&state.db, group_id, &member_ids).await?;
    }
    let view = scim::get_group(&state.db, scim.enterprise_id, group_id)
        .await?
        .ok_or_else(|| AppError::Internal("group disappeared".into()))?;
    audit_scim(
        &state,
        "scim.group.created",
        scim.enterprise_id,
        Some("scim_group"),
        Some(view.id),
        json!({ "display_name": view.display_name, "initial_members": view.members.len() }),
    )
    .await;
    Ok((StatusCode::CREATED, Json(group_to_scim(&view))))
}

async fn get_group(
    State(state): State<AppState>,
    scim: ScimAuth,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let view = scim::get_group(&state.db, scim.enterprise_id, id)
        .await?
        .ok_or_else(|| AppError::NotFound("Group not found".into()))?;
    Ok(Json(group_to_scim(&view)))
}

async fn list_groups(
    State(state): State<AppState>,
    scim: ScimAuth,
    Query(q): Query<ListQuery>,
) -> Result<Json<Value>, AppError> {
    let start_index = q.start_index.unwrap_or(1).max(1);
    let count = q.count.unwrap_or(50).clamp(1, 200);
    let filter = q.filter.as_deref().and_then(|f| parse_eq_filter(f, "displayName"));
    let (groups, total) =
        scim::list_groups(&state.db, scim.enterprise_id, filter, start_index, count).await?;
    Ok(Json(json!({
        "schemas": ["urn:ietf:params:scim:api:messages:2.0:ListResponse"],
        "totalResults": total,
        "startIndex": start_index,
        "itemsPerPage": groups.len(),
        "Resources": groups.iter().map(group_to_scim).collect::<Vec<_>>(),
    })))
}

async fn replace_group(
    State(state): State<AppState>,
    scim: ScimAuth,
    Path(id): Path<Uuid>,
    Json(body): Json<ScimGroupRequest>,
) -> Result<Json<Value>, AppError> {
    scim::get_group(&state.db, scim.enterprise_id, id)
        .await?
        .ok_or_else(|| AppError::NotFound("Group not found".into()))?;
    scim::update_group_display_name(&state.db, scim.enterprise_id, id, &body.display_name).await?;
    let members = parse_member_uuids(&body.members)?;
    scim::replace_group_members(&state.db, id, &members).await?;
    let view = scim::get_group(&state.db, scim.enterprise_id, id)
        .await?
        .ok_or_else(|| AppError::Internal("group disappeared".into()))?;
    Ok(Json(group_to_scim(&view)))
}

async fn patch_group(
    State(state): State<AppState>,
    scim: ScimAuth,
    Path(id): Path<Uuid>,
    Json(body): Json<PatchRequest>,
) -> Result<Json<Value>, AppError> {
    scim::get_group(&state.db, scim.enterprise_id, id)
        .await?
        .ok_or_else(|| AppError::NotFound("Group not found".into()))?;

    for op in &body.operations {
        let op_lc = op.op.to_lowercase();
        match (op_lc.as_str(), op.path.as_deref()) {
            ("replace", Some("displayName")) => {
                let name = op.value.as_str().unwrap_or("").to_string();
                if !name.is_empty() {
                    scim::update_group_display_name(&state.db, scim.enterprise_id, id, &name)
                        .await?;
                }
            }
            ("add", Some("members")) | ("add", None) => {
                let members = extract_member_ids_from_value(&op.value);
                if !members.is_empty() {
                    scim::add_group_members(&state.db, id, &members).await?;
                }
            }
            ("remove", Some(path)) if path.starts_with("members") => {
                // Okta sends: `path=members[value eq "<uuid>"]` with no body.
                // Azure AD sends: `path=members`, value=[{value: "<uuid>"}, …].
                if let Some(uid) = extract_member_id_from_filter_path(path) {
                    scim::remove_group_members(&state.db, id, &[uid]).await?;
                } else {
                    let members = extract_member_ids_from_value(&op.value);
                    if !members.is_empty() {
                        scim::remove_group_members(&state.db, id, &members).await?;
                    }
                }
            }
            ("remove", None) => {
                let members = extract_member_ids_from_value(&op.value);
                if !members.is_empty() {
                    scim::remove_group_members(&state.db, id, &members).await?;
                }
            }
            ("replace", Some("members")) => {
                let members = extract_member_ids_from_value(&op.value);
                scim::replace_group_members(&state.db, id, &members).await?;
            }
            _ => {}
        }
    }

    let view = scim::get_group(&state.db, scim.enterprise_id, id)
        .await?
        .ok_or_else(|| AppError::Internal("group disappeared".into()))?;
    Ok(Json(group_to_scim(&view)))
}

async fn delete_group(
    State(state): State<AppState>,
    scim: ScimAuth,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let deleted = scim::delete_group(&state.db, scim.enterprise_id, id).await?;
    if !deleted {
        return Err(AppError::NotFound("Group not found".into()));
    }
    audit_scim(
        &state,
        "scim.group.deleted",
        scim.enterprise_id,
        Some("scim_group"),
        Some(id),
        json!({}),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

fn parse_member_uuids(members: &[ScimGroupMember]) -> Result<Vec<Uuid>, AppError> {
    members
        .iter()
        .map(|m| {
            Uuid::parse_str(&m.value)
                .map_err(|_| AppError::Validation(format!("invalid member id: {}", m.value)))
        })
        .collect()
}

fn extract_member_ids_from_value(v: &Value) -> Vec<Uuid> {
    // Accepts either an array of member objects or a single object.
    let items: Vec<&Value> = match v {
        Value::Array(a) => a.iter().collect(),
        Value::Object(_) => vec![v],
        _ => vec![],
    };
    items
        .into_iter()
        .filter_map(|item| item.get("value").and_then(|s| s.as_str()))
        .filter_map(|s| Uuid::parse_str(s).ok())
        .collect()
}

fn extract_member_id_from_filter_path(path: &str) -> Option<Uuid> {
    // Format: `members[value eq "<uuid>"]`
    let start = path.find("\"")? + 1;
    let end = path.rfind('"')?;
    if end <= start {
        return None;
    }
    Uuid::parse_str(&path[start..end]).ok()
}

fn group_to_scim(view: &scim::ScimGroupView) -> Value {
    json!({
        "schemas": [
            "urn:ietf:params:scim:schemas:core:2.0:Group",
            "urn:skilluv:params:scim:schemas:extension:group:2.0:RoleMapping"
        ],
        "id": view.id,
        "externalId": view.external_id,
        "displayName": view.display_name,
        "members": view.members.iter().map(|uid| json!({
            "value": uid,
            "type": "User",
        })).collect::<Vec<_>>(),
        // Skilluv extension: role mapping set by the owner via
        // PUT /enterprise/sso/scim/groups/{id}/mapped-role.
        "urn:skilluv:params:scim:schemas:extension:group:2.0:RoleMapping": {
            "mappedRole": view.mapped_role,
        },
        "meta": {
            "resourceType": "Group",
            "created": view.created_at.to_rfc3339(),
            "lastModified": view.updated_at.to_rfc3339(),
        }
    })
}
