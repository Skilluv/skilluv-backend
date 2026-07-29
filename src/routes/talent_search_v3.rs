//! P16.4 — Search recruteur v3 : orientation + skills + mode + available.
//!
//! Ce que v3 apporte face à v2 (`/talents/search/v2`) :
//!   - Filtre par `orientation` (slug d'orientation métier P16.1). JOIN
//!     `user_orientations` — sélectionne les users qui déclarent ce métier.
//!   - Filtre par `skills` (CSV de slugs). JOIN `user_skills` — sélectionne
//!     les users qui ont prouvé au moins ces skills (avec proficiency >= min).
//!   - `mode` (default 'active') : exclut par défaut les orientations
//!     `learning` (aspirationnelles) pour ne pas polluer le search recruteur
//!     avec des profils sans preuves. `mode=both` inclut les apprenants.
//!   - `only_primary` : ne renvoie que les users dont c'est l'orientation
//!     principale (utile pour matcher un profil très spécialisé).
//!
//! Le tri par défaut privilégie le "WPC cumulé sur les skills matchés" — plus
//! l'user a prouvé les skills demandés, plus il remonte.
//!
//! **Historisation en filtre gratuit** : les orientations avec `ended_at`
//! sont ignorées → un user en reconversion active sur X mais historiquement
//! sur Y n'apparaîtra que pour X.

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::AppState;
use crate::api_response::MetaInfo;
use crate::errors::AppError;

pub fn talent_search_v3_routes() -> Router<AppState> {
    Router::new().route("/talents/search/v3", get(search_v3))
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct QueryV3 {
    /// Orientation slug — mandatory for a real match. Without it,
    /// filter is skipped and every user matches.
    pub orientation: Option<String>,
    /// CSV of skill slugs, e.g. `react,typescript`. Users must have
    /// proven ALL of them at `min_proficiency`.
    pub skills: Option<String>,
    /// `active` (default), `learning`, or `both`.
    #[serde(default = "default_mode")]
    pub mode: String,
    /// True → only match users whose orientation is their primary.
    #[serde(default)]
    pub only_primary: bool,
    #[serde(default = "default_min_proficiency")]
    pub min_proficiency: i16,
    pub working_language: Option<String>,
    #[serde(default = "default_per_page")]
    pub per_page: i64,
    #[serde(default = "default_page")]
    pub page: i64,
}
fn default_mode() -> String {
    "active".into()
}
fn default_min_proficiency() -> i16 {
    1
}
fn default_per_page() -> i64 {
    20
}
fn default_page() -> i64 {
    1
}

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct TalentRow {
    pub user_id: Uuid,
    pub username: String,
    pub display_name: String,
    pub orientation_slug: String,
    /// `active` or `learning`.
    pub orientation_mode: String,
    pub is_primary: bool,
    /// Count of skills that matched the filter (or all skills if none
    /// requested).
    pub matched_skills_count: i64,
    /// Cumulative weighted_proven_count over the matched skills.
    pub matched_wpc_total: i64,
    pub working_languages: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SearchV3Pagination {
    pub page: i64,
    pub per_page: i64,
    /// Number of rows actually returned (may be < per_page on last page).
    pub returned: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct FiltersApplied {
    pub orientation: Option<String>,
    pub skills: Vec<String>,
    pub mode: String,
    pub only_primary: bool,
    pub min_proficiency: i16,
    pub working_language: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SearchV3Data {
    pub talents: Vec<TalentRow>,
    pub pagination: SearchV3Pagination,
    /// Echo of the filters actually applied (defaults resolved) so the
    /// front can show a "clear filter" chip.
    pub filters_applied: FiltersApplied,
}

/// The v3 search returns a bespoke `{ data, meta }` envelope (not the
/// generic `ApiResponse<T>`) because `data` embeds pagination and
/// filters_applied at the same level for legacy compat.
#[derive(Debug, Serialize, ToSchema)]
pub struct SearchV3Response {
    pub data: SearchV3Data,
    pub meta: MetaInfo,
}

/// Recruiter talent search v3 — filters by orientation + skills +
/// mode + language, sorted by matched WPC total. See file header for
/// design notes.
#[utoipa::path(
    get,
    path = "/api/talents/search/v3",
    tag = "enterprise",
    params(QueryV3),
    responses(
        (status = 200, description = "Talent search results", body = SearchV3Response),
        (status = 400, description = "Invalid mode", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn search_v3(
    State(state): State<AppState>,
    Query(q): Query<QueryV3>,
) -> Result<Json<SearchV3Response>, AppError> {
    if !matches!(q.mode.as_str(), "active" | "learning" | "both") {
        return Err(AppError::Validation(
            "mode must be one of: active | learning | both".into(),
        ));
    }
    let per_page = q.per_page.clamp(1, 50);
    let page = q.page.max(1);
    let offset = (page - 1) * per_page;

    let skills: Vec<String> = q
        .skills
        .as_deref()
        .map(|s| {
            s.split(',')
                .map(|x| x.trim().to_string())
                .filter(|x| !x.is_empty())
                .collect()
        })
        .unwrap_or_default();

    // Le mode filter : 'both' = pas de filtre, sinon exact match.
    let mode_filter: Option<&str> = if q.mode == "both" {
        None
    } else {
        Some(&q.mode)
    };

    // WPC est calculé même si aucun skill demandé (fallback : SUM sur tous
    // les skills de l'user).
    let rows = sqlx::query_as::<_, TalentRow>(
        r#"
        WITH matched AS (
            SELECT us.user_id,
                   COUNT(*) FILTER (WHERE us.proficiency_level >= $5)::BIGINT AS matched_count,
                   COALESCE(SUM(us.weighted_proven_count) FILTER
                       (WHERE us.proficiency_level >= $5), 0)::BIGINT AS matched_wpc
            FROM user_skills us
            JOIN skill_nodes sn ON sn.id = us.skill_id
            WHERE ($2::TEXT[] IS NULL OR sn.slug = ANY($2))
            GROUP BY us.user_id
        )
        SELECT u.id AS user_id, u.username, u.display_name,
               o.slug AS orientation_slug, uo.mode AS orientation_mode,
               uo.is_primary, uo.working_languages,
               COALESCE(m.matched_count, 0) AS matched_skills_count,
               COALESCE(m.matched_wpc, 0)   AS matched_wpc_total
        FROM user_orientations uo
        JOIN orientations o ON o.id = uo.orientation_id
        JOIN users u        ON u.id = uo.user_id
        LEFT JOIN matched m ON m.user_id = u.id
        WHERE uo.ended_at IS NULL
          AND ($1::VARCHAR IS NULL OR o.slug = $1)
          AND ($3::VARCHAR IS NULL OR uo.mode = $3)
          AND ($4::BOOLEAN IS FALSE OR uo.is_primary = TRUE)
          AND ($6::VARCHAR IS NULL OR $6 = ANY(uo.working_languages))
          AND (
            $2::TEXT[] IS NULL
            OR (COALESCE(m.matched_count, 0) >= array_length($2, 1))
          )
        ORDER BY
            (uo.mode = 'active') DESC,
            uo.is_primary DESC,
            matched_wpc_total DESC,
            matched_skills_count DESC,
            u.username
        LIMIT $7 OFFSET $8
        "#,
    )
    .bind(q.orientation.as_deref())
    .bind(if skills.is_empty() {
        None
    } else {
        Some(&skills)
    })
    .bind(mode_filter)
    .bind(q.only_primary)
    .bind(q.min_proficiency)
    .bind(q.working_language.as_deref())
    .bind(per_page)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let returned = rows.len() as i64;
    Ok(Json(SearchV3Response {
        data: SearchV3Data {
            talents: rows,
            pagination: SearchV3Pagination {
                page,
                per_page,
                returned,
            },
            filters_applied: FiltersApplied {
                orientation: q.orientation,
                skills,
                mode: q.mode,
                only_primary: q.only_primary,
                min_proficiency: q.min_proficiency,
                working_language: q.working_language,
            },
        },
        meta: MetaInfo::now(),
    }))
}
