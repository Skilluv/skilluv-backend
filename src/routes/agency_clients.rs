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
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
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

async fn get_type_config(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let (ent_id, ent_type) = resolve_enterprise(&state, &auth).await?;
    let cfg: serde_json::Value =
        sqlx::query_scalar("SELECT type_config FROM enterprises WHERE id = $1")
            .bind(ent_id)
            .fetch_one(&state.db)
            .await?;
    Ok(Json(wrap(json!({
        "enterprise_type": ent_type,
        "type_config": cfg,
        "allowed_keys": allowed_keys_for(&ent_type),
    }))))
}

async fn patch_type_config(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(patch): Json<serde_json::Value>,
) -> Result<Json<Value>, AppError> {
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
    Ok(Json(wrap(
        json!({ "updated": true, "keys_set": patch_obj.keys().collect::<Vec<_>>() }),
    )))
}

fn wrap(data: Value) -> Value {
    json!({
        "data": data,
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })
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

#[derive(Debug, Serialize, sqlx::FromRow)]
struct AgencyClientRow {
    id: Uuid,
    client_name: String,
    client_contact_email: Option<String>,
    notes: Option<String>,
    active: bool,
    created_at: chrono::DateTime<chrono::Utc>,
}

async fn list(State(state): State<AppState>, auth: AuthUser) -> Result<Json<Value>, AppError> {
    let ent_id = resolve_staffing_agency(&state, &auth).await?;
    let rows: Vec<AgencyClientRow> = sqlx::query_as(
        "SELECT id, client_name, client_contact_email, notes, active, created_at
         FROM agency_clients WHERE enterprise_id = $1
         ORDER BY active DESC, created_at DESC",
    )
    .bind(ent_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(wrap(json!({ "clients": rows }))))
}

#[derive(Debug, Deserialize)]
struct CreateBody {
    client_name: String,
    #[serde(default)]
    client_contact_email: Option<String>,
    #[serde(default)]
    notes: Option<String>,
}

async fn create(
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
        Json(wrap(json!({ "id": id, "client_name": body.client_name }))),
    ))
}

#[derive(Debug, Deserialize)]
struct UpdateBody {
    #[serde(default)]
    client_name: Option<String>,
    #[serde(default)]
    client_contact_email: Option<String>,
    #[serde(default)]
    notes: Option<String>,
    #[serde(default)]
    active: Option<bool>,
}

async fn update(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateBody>,
) -> Result<Json<Value>, AppError> {
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
    Ok(Json(wrap(json!({ "updated": true }))))
}

async fn deactivate(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
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
    Ok(Json(wrap(json!({ "deactivated": true }))))
}
