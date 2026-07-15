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
use serde_json::{json, Value};
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
async fn resolve_staffing_agency(
    state: &AppState,
    auth: &AuthUser,
) -> Result<Uuid, AppError> {
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

    let (enterprise_id, ent_type) = ent.ok_or_else(|| {
        AppError::Forbidden
    })?;

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

async fn list(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
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
    Ok((StatusCode::CREATED, Json(wrap(json!({ "id": id, "client_name": body.client_name })))))
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
