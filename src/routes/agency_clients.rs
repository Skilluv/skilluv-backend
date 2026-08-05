//! P24.2 — Routes CRUD pour agency_clients (workflow staffing_agency).
//!
//! Contraintes :
//!   - Nécessite un user authentifié rattaché à une enterprise `staffing_agency`.
//!   - Le trigger PG `agency_clients_enforce_type` bloque l'insertion si
//!     l'enterprise n'est pas de type staffing_agency (défense en profondeur).
//!   - Toutes les routes filtrent par l'enterprise active du user.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, patch};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::middleware::AuthUser;

pub fn agency_client_routes() -> Router<AppState> {
    Router::new()
        .route("/enterprises/me/agency-clients", get(list).post(create))
        .route(
            "/enterprises/me/agency-clients/{id}",
            patch(update).delete(deactivate),
        )
        // P24.3 — config JSONB par type
        .route(
            "/enterprises/me/type-config",
            get(get_type_config).patch(patch_type_config),
        )
}

// ═══════════════════════════════════════════════════════════════════
// P24.3 — GET / PATCH /enterprises/me/type-config
// ═══════════════════════════════════════════════════════════════════

/// Clés autorisées par type. Toute clé hors de cette allowlist est rejetée.
fn allowed_keys_for(ent_type: &str) -> &'static [&'static str] {
    match ent_type {
        "staffing_agency" => &["commission_rate", "brand_white_label", "default_client_id"],
        "remote_international" => &[
            "eor_provider",
            "preferred_currency",
            "timezone_requirement",
            "tax_withholding_country",
        ],
        _ => &[],
    }
}

/// Résout l'enterprise active du user (tous types).
async fn resolve_enterprise(state: &AppState, auth: &AuthUser) -> Result<(Uuid, String), AppError> {
    let ent: Option<(Uuid, String)> = sqlx::query_as(
        r#"
        SELECT e.id, e.enterprise_type
        FROM enterprise_members em
        JOIN enterprises e ON e.id = em.enterprise_id
        WHERE em.user_id = $1
        ORDER BY em.created_at DESC
        LIMIT 1
        "#,
    )
    .bind(auth.user_id)
    .fetch_optional(&state.db)
    .await?;
    ent.ok_or(AppError::Forbidden)
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TypeConfigResponse {
    /// `staffing_agency`, `remote_international`, or `direct_hire`.
    pub enterprise_type: String,
    /// Free-form JSONB — shape depends on enterprise_type. Only keys
    /// present in `allowed_keys` are honoured by PATCH.
    pub type_config: serde_json::Value,
    pub allowed_keys: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TypeConfigUpdatedResponse {
    pub updated: bool,
    pub keys_set: Vec<String>,
}

/// Read the caller enterprise's `type_config` JSONB and the enum of
/// keys the current enterprise_type allows.
#[utoipa::path(
    get,
    path = "/api/enterprises/me/type-config",
    tag = "enterprise",
    responses(
        (status = 200, description = "Type-config snapshot", body = ApiResponse<TypeConfigResponse>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Caller has no enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn get_type_config(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<TypeConfigResponse>>, AppError> {
    let (ent_id, ent_type) = resolve_enterprise(&state, &auth).await?;
    let cfg: serde_json::Value =
        sqlx::query_scalar("SELECT type_config FROM enterprises WHERE id = $1")
            .bind(ent_id)
            .fetch_one(&state.db)
            .await?;
    let allowed: Vec<String> = allowed_keys_for(&ent_type)
        .iter()
        .map(|s| s.to_string())
        .collect();
    Ok(Json(ApiResponse::new(TypeConfigResponse {
        enterprise_type: ent_type,
        type_config: cfg,
        allowed_keys: allowed,
    })))
}

/// Merge-patch the type_config JSONB. Only allowlisted keys per
/// enterprise_type are accepted; unknown keys yield 400.
#[utoipa::path(
    patch,
    path = "/api/enterprises/me/type-config",
    tag = "enterprise",
    request_body(content = serde_json::Value, description = "Partial JSON object; only allowlisted keys accepted"),
    responses(
        (status = 200, description = "type_config merged", body = ApiResponse<TypeConfigUpdatedResponse>),
        (status = 400, description = "Non-allowlisted key or bad body shape", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Caller has no enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn patch_type_config(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(patch): Json<serde_json::Value>,
) -> Result<Json<ApiResponse<TypeConfigUpdatedResponse>>, AppError> {
    let (ent_id, ent_type) = resolve_enterprise(&state, &auth).await?;
    let allowed = allowed_keys_for(&ent_type);
    if allowed.is_empty() {
        return Err(AppError::Validation(format!(
            "enterprise_type '{ent_type}' has no configurable type_config keys"
        )));
    }
    let patch_obj = patch
        .as_object()
        .ok_or_else(|| AppError::Validation("body must be a JSON object".into()))?;
    for key in patch_obj.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(AppError::Validation(format!(
                "key '{key}' not allowed for enterprise_type '{ent_type}' (allowed: {allowed:?})"
            )));
        }
    }

    // Merge : type_config = type_config || $patch (les nouvelles clés overwrite).
    sqlx::query(
        "UPDATE enterprises SET type_config = type_config || $2::jsonb, updated_at = NOW()
         WHERE id = $1",
    )
    .bind(ent_id)
    .bind(&patch)
    .execute(&state.db)
    .await?;
    let keys_set: Vec<String> = patch_obj.keys().cloned().collect();
    Ok(Json(ApiResponse::new(TypeConfigUpdatedResponse {
        updated: true,
        keys_set,
    })))
}

/// Résout l'enterprise active du user et vérifie qu'elle est staffing_agency.
/// Retourne l'enterprise_id.
async fn resolve_staffing_agency(state: &AppState, auth: &AuthUser) -> Result<Uuid, AppError> {
    let ent: Option<(Uuid, String)> = sqlx::query_as(
        r#"
        SELECT e.id, e.enterprise_type
        FROM enterprise_members em
        JOIN enterprises e ON e.id = em.enterprise_id
        WHERE em.user_id = $1
        ORDER BY em.created_at DESC
        LIMIT 1
        "#,
    )
    .bind(auth.user_id)
    .fetch_optional(&state.db)
    .await?;

    let (enterprise_id, ent_type) = ent.ok_or_else(|| AppError::Forbidden)?;

    if ent_type != "staffing_agency" {
        return Err(AppError::Validation(format!(
            "agency_clients only available for enterprise_type='staffing_agency' (yours is '{ent_type}')"
        )));
    }

    Ok(enterprise_id)
}

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct AgencyClientRow {
    pub id: Uuid,
    pub client_name: String,
    pub client_contact_email: Option<String>,
    pub notes: Option<String>,
    pub active: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AgencyClientsListResponse {
    pub clients: Vec<AgencyClientRow>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AgencyClientCreatedResponse {
    pub id: Uuid,
    pub client_name: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AgencyClientUpdatedResponse {
    pub updated: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AgencyClientDeactivatedResponse {
    pub deactivated: bool,
}

/// List every agency-client row for the caller's staffing_agency.
/// Ordered active-first then newest-first.
#[utoipa::path(
    get,
    path = "/api/enterprises/me/agency-clients",
    tag = "enterprise",
    responses(
        (status = 200, description = "Agency clients", body = ApiResponse<AgencyClientsListResponse>),
        (status = 400, description = "Enterprise is not a staffing_agency", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Caller has no enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn list(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<AgencyClientsListResponse>>, AppError> {
    let ent_id = resolve_staffing_agency(&state, &auth).await?;
    let rows: Vec<AgencyClientRow> = sqlx::query_as(
        "SELECT id, client_name, client_contact_email, notes, active, created_at
         FROM agency_clients WHERE enterprise_id = $1
         ORDER BY active DESC, created_at DESC",
    )
    .bind(ent_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(ApiResponse::new(AgencyClientsListResponse {
        clients: rows,
    })))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateBody {
    #[schema(max_length = 10000)]
    pub client_name: String,
    #[serde(default)]
    #[schema(max_length = 10000)]
    pub client_contact_email: Option<String>,
    #[serde(default)]
    #[schema(max_length = 10000)]
    pub notes: Option<String>,
}

/// Create an agency-client row. The PG trigger
/// `agency_clients_enforce_type` blocks insertion when the enterprise
/// is not staffing_agency (defense-in-depth on top of the app check).
#[utoipa::path(
    post,
    path = "/api/enterprises/me/agency-clients",
    tag = "enterprise",
    request_body = CreateBody,
    responses(
        (status = 201, description = "Agency client created", body = ApiResponse<AgencyClientCreatedResponse>),
        (status = 400, description = "Not a staffing_agency", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Caller has no enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn create(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateBody>,
) -> Result<impl IntoResponse, AppError> {
    let ent_id = resolve_staffing_agency(&state, &auth).await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO agency_clients (enterprise_id, client_name, client_contact_email, notes)
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(ent_id)
    .bind(&body.client_name)
    .bind(body.client_contact_email.as_deref())
    .bind(body.notes.as_deref())
    .fetch_one(&state.db)
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::new(AgencyClientCreatedResponse {
            id,
            client_name: body.client_name,
        })),
    ))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateBody {
    #[serde(default)]
    #[schema(max_length = 10000)]
    pub client_name: Option<String>,
    #[serde(default)]
    #[schema(max_length = 10000)]
    pub client_contact_email: Option<String>,
    #[serde(default)]
    #[schema(max_length = 10000)]
    pub notes: Option<String>,
    #[serde(default)]
    pub active: Option<bool>,
}

/// Partial update of an agency-client row (COALESCE per field).
#[utoipa::path(
    patch,
    path = "/api/enterprises/me/agency-clients/{id}",
    tag = "enterprise",
    params(("id" = Uuid, Path, description = "Agency-client UUID")),
    request_body = UpdateBody,
    responses(
        (status = 200, description = "Updated", body = ApiResponse<AgencyClientUpdatedResponse>),
        (status = 400, description = "Not a staffing_agency", body = crate::api_response::ErrorResponse),
        (status = 404, description = "Row not found under this enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn update(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateBody>,
) -> Result<Json<ApiResponse<AgencyClientUpdatedResponse>>, AppError> {
    let ent_id = resolve_staffing_agency(&state, &auth).await?;
    let res = sqlx::query(
        r#"
        UPDATE agency_clients
        SET client_name = COALESCE($3, client_name),
            client_contact_email = COALESCE($4, client_contact_email),
            notes = COALESCE($5, notes),
            active = COALESCE($6, active),
            updated_at = NOW()
        WHERE id = $1 AND enterprise_id = $2
        "#,
    )
    .bind(id)
    .bind(ent_id)
    .bind(body.client_name.as_deref())
    .bind(body.client_contact_email.as_deref())
    .bind(body.notes.as_deref())
    .bind(body.active)
    .execute(&state.db)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound("agency_client not found".into()));
    }
    Ok(Json(ApiResponse::new(AgencyClientUpdatedResponse {
        updated: true,
    })))
}

/// Soft-delete: flip `active = FALSE`. Historical rows stay in DB for
/// audit / reporting.
#[utoipa::path(
    delete,
    path = "/api/enterprises/me/agency-clients/{id}",
    tag = "enterprise",
    params(("id" = Uuid, Path, description = "Agency-client UUID")),
    responses(
        (status = 200, description = "Deactivated", body = ApiResponse<AgencyClientDeactivatedResponse>),
        (status = 404, description = "Row not found under this enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn deactivate(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<AgencyClientDeactivatedResponse>>, AppError> {
    let ent_id = resolve_staffing_agency(&state, &auth).await?;
    let res = sqlx::query(
        "UPDATE agency_clients SET active = FALSE, updated_at = NOW()
         WHERE id = $1 AND enterprise_id = $2",
    )
    .bind(id)
    .bind(ent_id)
    .execute(&state.db)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound("agency_client not found".into()));
    }
    Ok(Json(ApiResponse::new(AgencyClientDeactivatedResponse {
        deactivated: true,
    })))
}
