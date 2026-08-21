//! P12.4 — GET /api/explore
//!
//! Recherche multi-critères pré-filtrée qui unifie deux types d'unités de
//! travail dans un seul endpoint :
//!   - `project_slices` (unités OSS réelles : issues GitHub, frames Figma, …)
//!   - `challenge_templates` (challenges de training / capstone)
//!
//! Utilisé par la page "Explore" côté frontend pour permettre aux users de
//! chercher indépendamment des recos personnalisées.

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::AppState;
use crate::api_response::MetaInfo;
use crate::errors::AppError;

// Type aliases pour clippy::type_complexity (rangées sqlx::query_as).
type ExploreRow76 = (
    Uuid,
    String,
    String,
    i16,
    Uuid,
    String,
    i32,
    i32,
    chrono::DateTime<chrono::Utc>,
);
type ExploreRow133 = (
    Uuid,
    String,
    String,
    i16,
    Option<String>,
    i32,
    bool,
    chrono::DateTime<chrono::Utc>,
);

pub fn explore_routes() -> Router<AppState> {
    Router::new().route("/explore", get(explore))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct ExploreQuery {
    /// Filtre optionnel sur le kind (`slice` | `challenge`). Sinon les deux.
    #[param(pattern = r"^(slice|challenge)$")]
    pub kind: Option<String>,
    /// One of the eight active domains. The handler checks it against
    /// `validators::SKILL_DOMAINS`; this pattern is the same list, and it had
    /// gone stale — a contract that understates what it accepts sends a caller
    /// looking for an endpoint that does not exist.
    #[param(pattern = r"^(code|design|game|security|ops|ai|soft_skills|audio)$")]
    pub domain: Option<String>,
    /// Difficulté (1-5).
    #[param(minimum = 1, maximum = 5)]
    pub difficulty: Option<i16>,
    /// Langue de programmation (challenges uniquement).
    #[param(max_length = 50)]
    pub language: Option<String>,
    /// Filtrer par project_id (slices uniquement).
    pub project_id: Option<Uuid>,
    /// Recherche texte simple ILIKE sur title.
    #[param(max_length = 200)]
    pub q: Option<String>,
    #[param(minimum = 1, maximum = 100000)]
    pub page: Option<i64>,
    #[param(minimum = 1, maximum = 100)]
    pub per_page: Option<i64>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ExploreItem {
    /// `slice` or `challenge`.
    pub kind: &'static str,
    pub id: Uuid,
    pub title: String,
    pub domain: String,
    pub difficulty: i16,
    /// Kind-specific payload:
    /// - slice: `{ project_id, slice_type, fragments_reward, credits_reward }`
    /// - challenge: `{ language, reward_fragments, is_capstone }`
    pub payload: Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ExplorePage {
    pub items: Vec<ExploreItem>,
    pub page: i64,
    pub per_page: i64,
    pub returned: usize,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ExploreResponse {
    pub data: ExplorePage,
    pub meta: MetaInfo,
}

/// Public multi-source explore endpoint — unifies open slices and
/// published challenge templates in one paginated feed. Both sources
/// pre-fetched then merged/sorted server-side by `created_at DESC`.
#[utoipa::path(
    get,
    path = "/api/explore",
    tag = "feed",
    params(ExploreQuery),
    responses(
        (status = 200, description = "Explore results", body = ExploreResponse),
    ),
)]
pub async fn explore(
    State(state): State<AppState>,
    Query(q): Query<ExploreQuery>,
) -> Result<Json<ExploreResponse>, AppError> {
    if let Some(k) = &q.kind
        && !matches!(k.as_str(), "slice" | "challenge")
    {
        return Err(AppError::Validation(
            "kind must be one of: slice, challenge".into(),
        ));
    }
    crate::validators::check_skill_domain_opt(&q.domain, "domain")?;
    crate::validators::check_max_len_opt(&q.language, "language", 50)?;
    crate::validators::check_max_len_opt(&q.q, "q", 200)?;
    crate::validators::check_range_opt(q.difficulty.map(i64::from), "difficulty", 1, 5)?;
    crate::validators::check_range_opt(q.page, "page", 1, 100_000)?;
    crate::validators::check_range_opt(q.per_page, "per_page", 1, 100)?;
    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(20).clamp(1, 100);
    // Chaque source SQL retourne assez d'items pour couvrir jusqu'à la page
    // demandée après le merge + tri en mémoire — sinon la pagination cross-source
    // ne ferait pas remonter les items plus anciens en page 2+.
    let limit_each = (page * per_page).min(500);

    let want_slices = q.kind.as_deref().is_none_or(|k| k == "slice");
    let want_challenges = q.kind.as_deref().is_none_or(|k| k == "challenge");

    let text_pattern: Option<String> = q.q.as_deref().map(|s| format!("%{s}%"));

    let mut items: Vec<ExploreItem> = Vec::new();

    if want_slices {
        // On restreint aux slices open + non-archivées (via project.archived_at NULL).
        let rows: Vec<ExploreRow76> = sqlx::query_as(
            r#"
            SELECT ps.id, ps.title, ps.primary_domain, ps.difficulty,
                   ps.project_id, ps.slice_type, ps.fragments_reward,
                   COALESCE(ps.credits_reward, 0)::INT,
                   ps.created_at
            FROM project_slices ps
            JOIN projects p ON p.id = ps.project_id
            WHERE ps.status = 'open'
              AND p.archived_at IS NULL
              AND ($1::TEXT IS NULL OR ps.primary_domain = $1)
              AND ($2::SMALLINT IS NULL OR ps.difficulty = $2)
              AND ($3::UUID IS NULL OR ps.project_id = $3)
              AND ($4::TEXT IS NULL OR ps.title ILIKE $4)
            ORDER BY ps.created_at DESC
            LIMIT $5
            "#,
        )
        .bind(q.domain.as_deref())
        .bind(q.difficulty)
        .bind(q.project_id)
        .bind(text_pattern.as_deref())
        .bind(limit_each)
        .fetch_all(&state.db)
        .await?;

        for (id, title, domain, difficulty, project_id, slice_type, frags, credits, created_at) in
            rows
        {
            items.push(ExploreItem {
                kind: "slice",
                id,
                title,
                domain,
                difficulty,
                created_at,
                payload: json!({
                    "project_id": project_id,
                    "slice_type": slice_type,
                    "fragments_reward": frags,
                    "credits_reward": credits,
                }),
            });
        }
    }

    if want_challenges {
        let rows: Vec<ExploreRow133> = sqlx::query_as(
            r#"
            SELECT ct.id, ct.title, ct.skill_domain, ct.difficulty,
                   ct.language, ct.reward_fragments, ct.is_capstone,
                   ct.created_at
            FROM challenge_templates ct
            WHERE ct.status = 'published'
              AND ($1::TEXT IS NULL OR ct.skill_domain = $1)
              AND ($2::SMALLINT IS NULL OR ct.difficulty = $2)
              AND ($3::TEXT IS NULL OR ct.language = $3)
              AND ($4::TEXT IS NULL OR ct.title ILIKE $4)
            ORDER BY ct.created_at DESC
            LIMIT $5
            "#,
        )
        .bind(q.domain.as_deref())
        .bind(q.difficulty)
        .bind(q.language.as_deref())
        .bind(text_pattern.as_deref())
        .bind(limit_each)
        .fetch_all(&state.db)
        .await?;

        for (id, title, domain, difficulty, language, reward, is_capstone, created_at) in rows {
            items.push(ExploreItem {
                kind: "challenge",
                id,
                title,
                domain,
                difficulty,
                created_at,
                payload: json!({
                    "language": language,
                    "reward_fragments": reward,
                    "is_capstone": is_capstone,
                }),
            });
        }
    }

    // Tri final unifié par created_at DESC + slice de page.
    items.sort_by_key(|i| std::cmp::Reverse(i.created_at));
    let offset = ((page - 1) * per_page) as usize;
    let page_items: Vec<ExploreItem> = items
        .into_iter()
        .skip(offset)
        .take(per_page as usize)
        .collect();

    let returned = page_items.len();
    Ok(Json(ExploreResponse {
        data: ExplorePage {
            items: page_items,
            page,
            per_page,
            returned,
        },
        meta: MetaInfo::now(),
    }))
}
