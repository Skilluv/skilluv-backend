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
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::middleware::AuthUser;

const MAX_ACTIVE_ORIENTATIONS: i64 = 3;

pub fn orientation_routes() -> Router<AppState> {
    Router::new()
        .route("/orientations", get(list_orientations))
        .route("/orientations/{slug}", get(get_orientation))
        .route(
            "/users/me/orientations",
            get(my_orientations).post(register_orientation),
        )
        .route(
            "/users/me/orientations/{slug}",
            patch(update_orientation).delete(end_orientation),
        )
        .route(
            "/users/me/orientations/{slug}/playlist",
            get(orientation_playlist),
        )
        // FE-M1 — projection publique des orientations d'un user (profil public).
        .route("/users/{id}/orientations", get(public_user_orientations))
}

// ═══════════════════════════════════════════════════════════════════
// Types partagés
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct CatalogQuery {
    #[param(max_length = 100)]
    pub domain: Option<String>,
    #[param(max_length = 100)]
    pub tag: Option<String>,
    #[serde(default = "default_limit")]
    #[param(minimum = 1, maximum = 200)]
    pub limit: i64,
    #[serde(default)]
    #[param(minimum = 0, maximum = 100000)]
    pub offset: i64,
    #[serde(default)]
    pub include_archived: bool,
}

fn default_limit() -> i64 {
    50
}

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct OrientationRow {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub description: String,
    /// `code`, `design`, `game`, `security`, etc.
    pub primary_domain: String,
    pub secondary_domains: Vec<String>,
    pub tags: Vec<String>,
    pub is_curated: bool,
    pub is_archived: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CatalogPagination {
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OrientationsCatalogResponse {
    pub orientations: Vec<OrientationRow>,
    pub pagination: CatalogPagination,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OrientationSkillEntry {
    pub slug: String,
    pub display_name: String,
    pub is_core: bool,
    pub is_recommended: bool,
    pub weight: f32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OrientationDetailResponse {
    pub orientation: OrientationRow,
    pub skills: Vec<OrientationSkillEntry>,
}

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct UserOrientationRow {
    pub orientation_slug: String,
    pub orientation_name: String,
    /// `learning` or `active`.
    pub mode: String,
    pub is_primary: bool,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub ended_at: Option<chrono::DateTime<chrono::Utc>>,
    pub working_languages: Vec<String>,
    pub timezone: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MyOrientationsResponse {
    pub orientations: Vec<UserOrientationRow>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RegisterBody {
    #[schema(max_length = 10000)]
    pub slug: String,
    /// `learning` (default) or `active`.
    #[serde(default = "default_mode")]
    #[schema(max_length = 10000)]
    pub mode: String,
    #[serde(default)]
    pub is_primary: bool,
    #[serde(default)]
    pub working_languages: Option<Vec<String>>,
    #[serde(default)]
    #[schema(max_length = 10000)]
    pub timezone: Option<String>,
    #[serde(default)]
    #[schema(max_length = 10000)]
    pub notes: Option<String>,
}

fn default_mode() -> String {
    "learning".into()
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RegisterOrientationResponse {
    pub slug: String,
    pub mode: String,
    pub is_primary: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateBody {
    #[schema(max_length = 10000)]
    pub mode: Option<String>,
    pub is_primary: Option<bool>,
    pub working_languages: Option<Vec<String>>,
    #[schema(max_length = 10000)]
    pub timezone: Option<String>,
    #[schema(max_length = 10000)]
    pub notes: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UpdateOrientationResponse {
    pub updated: bool,
    pub slug: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EndOrientationResponse {
    pub ended: bool,
    pub slug: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PublicUserOrientationRow {
    pub orientation_slug: String,
    pub orientation_name: String,
    pub mode: String,
    pub is_primary: bool,
    /// RFC 3339 timestamp.
    pub picked_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PublicUserOrientationsResponse {
    pub orientations: Vec<PublicUserOrientationRow>,
}

// ═══════════════════════════════════════════════════════════════════
// GET /orientations — catalogue public paginé
// ═══════════════════════════════════════════════════════════════════

/// Public paginated catalogue of curated orientations. Optional
/// filters on `domain` and `tag`.
#[utoipa::path(
    get,
    path = "/api/orientations",
    tag = "profile",
    params(CatalogQuery),
    responses(
        (status = 200, description = "Orientations catalogue", body = ApiResponse<OrientationsCatalogResponse>),
    ),
)]
pub async fn list_orientations(
    State(state): State<AppState>,
    Query(q): Query<CatalogQuery>,
) -> Result<Json<ApiResponse<OrientationsCatalogResponse>>, AppError> {
    crate::validators::check_max_len_opt(&q.domain, "domain", 100)?;
    crate::validators::check_max_len_opt(&q.tag, "tag", 100)?;
    if !(1..=200).contains(&q.limit) {
        return Err(AppError::Validation(
            "limit must be between 1 and 200".into(),
        ));
    }
    if !(0..=100_000).contains(&q.offset) {
        return Err(AppError::Validation(
            "offset must be between 0 and 100000".into(),
        ));
    }
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

    Ok(Json(ApiResponse::new(OrientationsCatalogResponse {
        orientations: rows,
        pagination: CatalogPagination {
            limit,
            offset: q.offset.max(0),
        },
    })))
}

// ═══════════════════════════════════════════════════════════════════
// GET /orientations/{slug} — détail + skills recommandés
// ═══════════════════════════════════════════════════════════════════

/// Detail on a single orientation + the map of recommended /
/// core skills that unlock it.
#[utoipa::path(
    get,
    path = "/api/orientations/{slug}",
    tag = "profile",
    params(("slug" = String, Path, description = "Orientation slug")),
    responses(
        (status = 200, description = "Orientation detail", body = ApiResponse<OrientationDetailResponse>),
        (status = 404, description = "Slug unknown", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn get_orientation(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<ApiResponse<OrientationDetailResponse>>, AppError> {
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

    let skills_json: Vec<OrientationSkillEntry> = skills
        .into_iter()
        .map(|(slug, name, core, rec, w)| OrientationSkillEntry {
            slug,
            display_name: name,
            is_core: core,
            is_recommended: rec,
            weight: w,
        })
        .collect();

    Ok(Json(ApiResponse::new(OrientationDetailResponse {
        orientation,
        skills: skills_json,
    })))
}

// ═══════════════════════════════════════════════════════════════════
// GET /users/me/orientations — les miennes
// ═══════════════════════════════════════════════════════════════════

/// List every orientation the caller has ever picked (active + ended).
#[utoipa::path(
    get,
    path = "/api/users/me/orientations",
    tag = "profile",
    responses(
        (status = 200, description = "Caller's orientations", body = ApiResponse<MyOrientationsResponse>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn my_orientations(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<MyOrientationsResponse>>, AppError> {
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
    Ok(Json(ApiResponse::new(MyOrientationsResponse {
        orientations: rows,
    })))
}

// ═══════════════════════════════════════════════════════════════════
// POST /users/me/orientations — s'inscrire (cap 3 actives)
// ═══════════════════════════════════════════════════════════════════

/// Enroll the caller in an orientation. Idempotent on
/// `(user_id, orientation_id)` — re-POSTing an already-active row
/// refreshes the mode/languages/etc fields and un-ends it. Enforces
/// the 3-active cap.
#[utoipa::path(
    post,
    path = "/api/users/me/orientations",
    tag = "profile",
    request_body = RegisterBody,
    responses(
        (status = 201, description = "Orientation registered", body = ApiResponse<RegisterOrientationResponse>),
        (status = 400, description = "Invalid mode, archived orientation, or 3-active cap reached", body = crate::api_response::ErrorResponse),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
        (status = 404, description = "Slug unknown or not curated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn register_orientation(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<RegisterBody>,
) -> Result<impl IntoResponse, AppError> {
    if !matches!(body.mode.as_str(), "learning" | "active") {
        return Err(AppError::Validation(
            "mode must be 'learning' or 'active'".into(),
        ));
    }

    let orientation: Option<(Uuid, bool)> = sqlx::query_as(
        "SELECT id, is_archived FROM orientations WHERE slug = $1 AND is_curated = TRUE",
    )
    .bind(&body.slug)
    .fetch_optional(&state.db)
    .await?;
    let (orientation_id, archived) = orientation.ok_or_else(|| {
        AppError::NotFound(format!(
            "orientation '{}' not found or not curated",
            body.slug
        ))
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
        Json(ApiResponse::new(RegisterOrientationResponse {
            slug: body.slug,
            mode: inserted.0,
            is_primary: final_is_primary,
        })),
    ))
}

// ═══════════════════════════════════════════════════════════════════
// PATCH /users/me/orientations/{slug}
// ═══════════════════════════════════════════════════════════════════

/// Partial update on an active orientation. Setting `is_primary=true`
/// automatically un-flags any other active primary.
#[utoipa::path(
    patch,
    path = "/api/users/me/orientations/{slug}",
    tag = "profile",
    params(("slug" = String, Path, description = "Orientation slug")),
    request_body = UpdateBody,
    responses(
        (status = 200, description = "Updated", body = ApiResponse<UpdateOrientationResponse>),
        (status = 400, description = "Invalid mode", body = crate::api_response::ErrorResponse),
        (status = 404, description = "Active orientation not found for this user", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn update_orientation(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(body): Json<UpdateBody>,
) -> Result<Json<ApiResponse<UpdateOrientationResponse>>, AppError> {
    if let Some(m) = &body.mode
        && !matches!(m.as_str(), "learning" | "active")
    {
        return Err(AppError::Validation(
            "mode must be 'learning' or 'active'".into(),
        ));
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
        return Err(AppError::NotFound(format!(
            "active orientation '{slug}' not found for this user"
        )));
    }
    tx.commit().await?;
    Ok(Json(ApiResponse::new(UpdateOrientationResponse {
        updated: true,
        slug,
    })))
}

// ═══════════════════════════════════════════════════════════════════
// GET /users/me/orientations/{slug}/playlist — P16.5
// ═══════════════════════════════════════════════════════════════════

/// Personalised learning playlist for an orientation. The playlist
/// service returns a rich structured payload (recommended slices,
/// missing skills, next steps) — documented here as free-form JSON
/// since it evolves independently.
#[utoipa::path(
    get,
    path = "/api/users/me/orientations/{slug}/playlist",
    tag = "profile",
    params(("slug" = String, Path, description = "Orientation slug")),
    responses(
        (status = 200, description = "Playlist payload", body = serde_json::Value),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
        (status = 404, description = "Orientation not active for this user", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn orientation_playlist(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let playlist =
        crate::services::orientations_playlist::playlist_for(&state.db, auth.user_id, &slug)
            .await?;
    Ok(Json(serde_json::json!(playlist)))
}

/// End an active orientation (soft — sets `ended_at`, un-flags
/// `is_primary`).
#[utoipa::path(
    delete,
    path = "/api/users/me/orientations/{slug}",
    tag = "profile",
    params(("slug" = String, Path, description = "Orientation slug")),
    responses(
        (status = 200, description = "Ended", body = ApiResponse<EndOrientationResponse>),
        (status = 404, description = "No active orientation with this slug", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn end_orientation(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> Result<Json<ApiResponse<EndOrientationResponse>>, AppError> {
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
        return Err(AppError::NotFound(format!(
            "active orientation '{slug}' not found"
        )));
    }
    Ok(Json(ApiResponse::new(EndOrientationResponse {
        ended: true,
        slug,
    })))
}

// ═══════════════════════════════════════════════════════════════════
// FE-M1 — GET /users/{id}/orientations (public, projection minimale)
// ═══════════════════════════════════════════════════════════════════
//
// Retourne uniquement les orientations actives (ended_at IS NULL) et une
// projection minimale (pas de notes/timezone). Pas de flag privacy dédié
// (les orientations sont une info d'identité professionnelle, comme le
// job title public sur LinkedIn). Si un user veut cacher son profil, il
// utilise le mécanisme users.profile_active = FALSE (déjà existant).

/// Public projection of a user's active orientations. Returns an
/// empty list rather than 403 when the target's profile is inactive
/// (anti-enumeration).
#[utoipa::path(
    get,
    path = "/api/users/{id}/orientations",
    tag = "profile",
    params(("id" = Uuid, Path, description = "User UUID")),
    responses(
        (status = 200, description = "Active orientations (public projection)", body = ApiResponse<PublicUserOrientationsResponse>),
        (status = 404, description = "User not found or banned", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn public_user_orientations(
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
) -> Result<Json<ApiResponse<PublicUserOrientationsResponse>>, AppError> {
    // Si le user a désactivé son profil public, renvoie un tableau vide (pas
    // une 403 : évite l'énumération).
    let public: Option<bool> =
        sqlx::query_scalar("SELECT profile_active FROM users WHERE id = $1 AND is_banned = FALSE")
            .bind(user_id)
            .fetch_optional(&state.db)
            .await?;
    let Some(active) = public else {
        return Err(AppError::NotFound("user not found".into()));
    };
    if !active {
        return Ok(Json(ApiResponse::new(PublicUserOrientationsResponse {
            orientations: vec![],
        })));
    }

    let rows: Vec<(String, String, String, bool, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        r#"
        SELECT o.slug, o.name, uo.mode, uo.is_primary, uo.started_at
        FROM user_orientations uo
        JOIN orientations o ON o.id = uo.orientation_id
        WHERE uo.user_id = $1 AND uo.ended_at IS NULL
        ORDER BY uo.is_primary DESC, uo.started_at DESC
        "#,
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await?;

    let orientations: Vec<PublicUserOrientationRow> = rows
        .into_iter()
        .map(
            |(slug, name, mode, primary, picked)| PublicUserOrientationRow {
                orientation_slug: slug,
                orientation_name: name,
                mode,
                is_primary: primary,
                picked_at: picked.to_rfc3339(),
            },
        )
        .collect();

    Ok(Json(ApiResponse::new(PublicUserOrientationsResponse {
        orientations,
    })))
}
