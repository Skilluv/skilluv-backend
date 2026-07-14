//! P16.3 — Routes API pour orientations (métiers) + user_orientations.
//!
//! Le contrat produit :
//!   - Catalogue public : `GET /api/orientations` (paginé, filtre domain/tag)
//!     et `GET /api/orientations/{slug}` (détail + skills recommandés).
//!   - Onboarding : `POST /api/users/me/orientations` inscrit 1 orientation
//!     (cap à 3 actives — au-delà, HTTP 409). `PATCH` modifie mode/primary,
//!     `DELETE` termine (sans supprimer — historisation via ended_at).
//!
//! Cap de 3 orientations non-ended = règle applicative (pas de CHECK DB pour
//! garder la flexibilité admin d'over-ride via SQL direct).

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, patch};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;

const MAX_ACTIVE_ORIENTATIONS: i64 = 3;

pub fn orientation_routes() -> Router<AppState> {
    Router::new()
        .route("/orientations", get(list_orientations))
        .route("/orientations/{slug}", get(get_orientation))
        .route("/users/me/orientations", get(my_orientations).post(register_orientation))
        .route(
            "/users/me/orientations/{slug}",
            patch(update_orientation).delete(end_orientation),
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

// ═══════════════════════════════════════════════════════════════════
// GET /orientations — catalogue public paginé
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct CatalogQuery {
    domain: Option<String>,
    tag: Option<String>,
    #[serde(default = "default_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
    #[serde(default)]
    include_archived: bool,
}

fn default_limit() -> i64 { 50 }

#[derive(Debug, Serialize, sqlx::FromRow)]
struct OrientationRow {
    id: Uuid,
    slug: String,
    name: String,
    description: String,
    primary_domain: String,
    secondary_domains: Vec<String>,
    tags: Vec<String>,
    is_curated: bool,
    is_archived: bool,
}

async fn list_orientations(
    State(state): State<AppState>,
    Query(q): Query<CatalogQuery>,
) -> Result<Json<Value>, AppError> {
    let limit = q.limit.clamp(1, 200);
    let rows = sqlx::query_as::<_, OrientationRow>(
        r#"
        SELECT id, slug, name, description, primary_domain,
               secondary_domains, tags, is_curated, is_archived
        FROM orientations
        WHERE is_curated = TRUE
          AND ($1::BOOLEAN OR is_archived = FALSE)
          AND ($2::VARCHAR IS NULL OR primary_domain = $2)
          AND ($3::VARCHAR IS NULL OR $3 = ANY(tags))
        ORDER BY primary_domain, name
        LIMIT $4 OFFSET $5
        "#,
    )
    .bind(q.include_archived)
    .bind(q.domain.as_deref())
    .bind(q.tag.as_deref())
    .bind(limit)
    .bind(q.offset.max(0))
    .fetch_all(&state.db)
    .await?;

    Ok(Json(wrap(json!({
        "orientations": rows,
        "pagination": { "limit": limit, "offset": q.offset.max(0) },
    }))))
}

// ═══════════════════════════════════════════════════════════════════
// GET /orientations/{slug} — détail + skills recommandés
// ═══════════════════════════════════════════════════════════════════

async fn get_orientation(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<Value>, AppError> {
    let orientation = sqlx::query_as::<_, OrientationRow>(
        "SELECT id, slug, name, description, primary_domain, secondary_domains,
                tags, is_curated, is_archived
         FROM orientations WHERE slug = $1",
    )
    .bind(&slug)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("orientation '{slug}' not found")))?;

    let skills: Vec<(String, String, bool, bool, f32)> = sqlx::query_as(
        r#"
        SELECT sn.slug, sn.display_name, osm.is_core, osm.is_recommended, osm.weight
        FROM orientation_skill_map osm
        JOIN skill_nodes sn ON sn.id = osm.skill_id
        WHERE osm.orientation_id = $1
        ORDER BY osm.is_core DESC, osm.weight DESC, sn.slug
        "#,
    )
    .bind(orientation.id)
    .fetch_all(&state.db)
    .await?;

    let skills_json: Vec<Value> = skills
        .into_iter()
        .map(|(slug, name, core, rec, w)| json!({
            "slug": slug,
            "display_name": name,
            "is_core": core,
            "is_recommended": rec,
            "weight": w,
        }))
        .collect();

    Ok(Json(wrap(json!({
        "orientation": orientation,
        "skills": skills_json,
    }))))
}

// ═══════════════════════════════════════════════════════════════════
// GET /users/me/orientations — les miennes
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Serialize, sqlx::FromRow)]
struct UserOrientationRow {
    orientation_slug: String,
    orientation_name: String,
    mode: String,
    is_primary: bool,
    started_at: chrono::DateTime<chrono::Utc>,
    ended_at: Option<chrono::DateTime<chrono::Utc>>,
    working_languages: Vec<String>,
    timezone: Option<String>,
    notes: Option<String>,
}

async fn my_orientations(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let rows = sqlx::query_as::<_, UserOrientationRow>(
        r#"
        SELECT o.slug AS orientation_slug, o.name AS orientation_name,
               uo.mode, uo.is_primary, uo.started_at, uo.ended_at,
               uo.working_languages, uo.timezone, uo.notes
        FROM user_orientations uo
        JOIN orientations o ON o.id = uo.orientation_id
        WHERE uo.user_id = $1
        ORDER BY uo.ended_at NULLS FIRST, uo.is_primary DESC, uo.started_at DESC
        "#,
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(wrap(json!({ "orientations": rows }))))
}

// ═══════════════════════════════════════════════════════════════════
// POST /users/me/orientations — s'inscrire (cap 3 actives)
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct RegisterBody {
    slug: String,
    #[serde(default = "default_mode")]
    mode: String,          // 'learning' | 'active'
    #[serde(default)]
    is_primary: bool,
    #[serde(default)]
    working_languages: Option<Vec<String>>,
    #[serde(default)]
    timezone: Option<String>,
    #[serde(default)]
    notes: Option<String>,
}
fn default_mode() -> String { "learning".into() }

async fn register_orientation(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<RegisterBody>,
) -> Result<impl IntoResponse, AppError> {
    if !matches!(body.mode.as_str(), "learning" | "active") {
        return Err(AppError::Validation("mode must be 'learning' or 'active'".into()));
    }

    let orientation: Option<(Uuid, bool)> = sqlx::query_as(
        "SELECT id, is_archived FROM orientations WHERE slug = $1 AND is_curated = TRUE",
    )
    .bind(&body.slug)
    .fetch_optional(&state.db)
    .await?;
    let (orientation_id, archived) = orientation.ok_or_else(|| {
        AppError::NotFound(format!("orientation '{}' not found or not curated", body.slug))
    })?;
    if archived {
        return Err(AppError::Validation(
            "this orientation is archived — cannot be selected".into(),
        ));
    }

    // Cap : max MAX_ACTIVE_ORIENTATIONS non-ended.
    let active_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_orientations
         WHERE user_id = $1 AND ended_at IS NULL",
    )
    .bind(auth.user_id)
    .fetch_one(&state.db)
    .await?;
    if active_count >= MAX_ACTIVE_ORIENTATIONS {
        return Err(AppError::Validation(format!(
            "max {MAX_ACTIVE_ORIENTATIONS} active orientations reached — end one first or prove more artifacts to unlock more"
        )));
    }

    let mut tx = state.db.begin().await?;
    // Si is_primary demandé, dé-flag les autres primaires actives.
    if body.is_primary {
        sqlx::query(
            "UPDATE user_orientations SET is_primary = FALSE
             WHERE user_id = $1 AND ended_at IS NULL",
        )
        .bind(auth.user_id)
        .execute(&mut *tx)
        .await?;
    }
    // Si aucun primary et c'est la 1re orientation, on l'auto-promeut primary.
    let final_is_primary = body.is_primary || active_count == 0;

    let inserted: (String, String) = sqlx::query_as(
        r#"
        INSERT INTO user_orientations
            (user_id, orientation_id, mode, is_primary, working_languages, timezone, notes)
        VALUES
            ($1, $2, $3, $4, COALESCE($5, ARRAY['fr']::TEXT[]), $6, $7)
        ON CONFLICT (user_id, orientation_id) DO UPDATE
            SET mode = EXCLUDED.mode,
                is_primary = EXCLUDED.is_primary,
                working_languages = EXCLUDED.working_languages,
                timezone = EXCLUDED.timezone,
                notes = EXCLUDED.notes,
                ended_at = NULL
        RETURNING mode, is_primary::TEXT
        "#,
    )
    .bind(auth.user_id)
    .bind(orientation_id)
    .bind(&body.mode)
    .bind(final_is_primary)
    .bind(body.working_languages.as_ref())
    .bind(body.timezone.as_deref())
    .bind(body.notes.as_deref())
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok((
        StatusCode::CREATED,
        Json(wrap(json!({
            "slug": body.slug,
            "mode": inserted.0,
            "is_primary": final_is_primary,
        }))),
    ))
}

// ═══════════════════════════════════════════════════════════════════
// PATCH /users/me/orientations/{slug}
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct UpdateBody {
    mode: Option<String>,
    is_primary: Option<bool>,
    working_languages: Option<Vec<String>>,
    timezone: Option<String>,
    notes: Option<String>,
}

async fn update_orientation(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(body): Json<UpdateBody>,
) -> Result<Json<Value>, AppError> {
    if let Some(m) = &body.mode {
        if !matches!(m.as_str(), "learning" | "active") {
            return Err(AppError::Validation("mode must be 'learning' or 'active'".into()));
        }
    }

    let mut tx = state.db.begin().await?;
    if let Some(true) = body.is_primary {
        sqlx::query(
            "UPDATE user_orientations SET is_primary = FALSE
             WHERE user_id = $1 AND ended_at IS NULL",
        )
        .bind(auth.user_id)
        .execute(&mut *tx)
        .await?;
    }
    let updated = sqlx::query(
        r#"
        UPDATE user_orientations uo
        SET mode = COALESCE($3, uo.mode),
            is_primary = COALESCE($4, uo.is_primary),
            working_languages = COALESCE($5, uo.working_languages),
            timezone = COALESCE($6, uo.timezone),
            notes = COALESCE($7, uo.notes)
        FROM orientations o
        WHERE uo.user_id = $1 AND uo.orientation_id = o.id
          AND o.slug = $2 AND uo.ended_at IS NULL
        "#,
    )
    .bind(auth.user_id)
    .bind(&slug)
    .bind(body.mode.as_deref())
    .bind(body.is_primary)
    .bind(body.working_languages.as_ref())
    .bind(body.timezone.as_deref())
    .bind(body.notes.as_deref())
    .execute(&mut *tx)
    .await?;
    if updated.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("active orientation '{slug}' not found for this user")));
    }
    tx.commit().await?;
    Ok(Json(wrap(json!({ "updated": true, "slug": slug }))))
}

// ═══════════════════════════════════════════════════════════════════
// DELETE /users/me/orientations/{slug} — historise (ended_at = NOW)
// ═══════════════════════════════════════════════════════════════════

async fn end_orientation(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> Result<Json<Value>, AppError> {
    let res = sqlx::query(
        r#"
        UPDATE user_orientations uo
        SET ended_at = NOW(), is_primary = FALSE
        FROM orientations o
        WHERE uo.user_id = $1 AND uo.orientation_id = o.id
          AND o.slug = $2 AND uo.ended_at IS NULL
        "#,
    )
    .bind(auth.user_id)
    .bind(&slug)
    .execute(&state.db)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("active orientation '{slug}' not found")));
    }
    Ok(Json(wrap(json!({ "ended": true, "slug": slug }))))
}
