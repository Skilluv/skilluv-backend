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
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;

pub fn explore_routes() -> Router<AppState> {
    Router::new().route("/explore", get(explore))
}

#[derive(Debug, Deserialize)]
struct ExploreQuery {
    /// Filtre optionnel sur le kind ('slice' | 'challenge'). Sinon les deux.
    kind: Option<String>,
    /// Filtre domaine ('code', 'design', 'game', 'security', 'ops', 'ai', 'soft_skills').
    domain: Option<String>,
    /// Difficulté (1-5).
    difficulty: Option<i16>,
    /// Langue de programmation (challenges uniquement).
    language: Option<String>,
    /// Filtrer par project_id (slices uniquement).
    project_id: Option<Uuid>,
    /// Recherche texte simple ILIKE sur title.
    q: Option<String>,
    /// Pagination.
    page: Option<i64>,
    per_page: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
struct ExploreItem {
    kind: &'static str,
    id: Uuid,
    title: String,
    domain: String,
    difficulty: i16,
    /// Détails supplémentaires spécifiques au kind.
    payload: Value,
    created_at: chrono::DateTime<chrono::Utc>,
}

async fn explore(
    State(state): State<AppState>,
    Query(q): Query<ExploreQuery>,
) -> Result<Json<Value>, AppError> {
    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(20).clamp(1, 100);
    // Chaque source SQL retourne assez d'items pour couvrir jusqu'à la page
    // demandée après le merge + tri en mémoire — sinon la pagination cross-source
    // ne ferait pas remonter les items plus anciens en page 2+.
    let limit_each = (page * per_page).min(500);

    let want_slices = q.kind.as_deref().map_or(true, |k| k == "slice");
    let want_challenges = q.kind.as_deref().map_or(true, |k| k == "challenge");

    let text_pattern: Option<String> = q.q.as_deref().map(|s| format!("%{s}%"));

    let mut items: Vec<ExploreItem> = Vec::new();

    if want_slices {
        // On restreint aux slices open + non-archivées (via project.archived_at NULL).
        let rows: Vec<(
            Uuid,
            String,
            String,
            i16,
            Uuid,
            String,
            i32,
            i32,
            chrono::DateTime<chrono::Utc>,
        )> = sqlx::query_as(
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
        let rows: Vec<(
            Uuid,
            String,
            String,
            i16,
            Option<String>,
            i32,
            bool,
            chrono::DateTime<chrono::Utc>,
        )> = sqlx::query_as(
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
    items.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    let offset = ((page - 1) * per_page) as usize;
    let page_slice: Vec<&ExploreItem> = items.iter().skip(offset).take(per_page as usize).collect();

    Ok(Json(json!({
        "data": {
            "items": page_slice,
            "page": page,
            "per_page": per_page,
            "returned": page_slice.len(),
        },
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })))
}
