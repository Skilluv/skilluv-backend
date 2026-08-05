//! P17.5 — API polymorphique badges.
//!
//! Contrat frontend :
//!   `GET /api/users/{id}/badges` retourne un objet regroupant toutes les
//!   familles pour l'user : rank courant, skill patches actifs, medals,
//!   compteurs seals/stamps. Chaque item inclut sa rarity et (optionnellement)
//!   les IDs de preuves source pour traçabilité UX.
//!
//!   `GET /api/badge-rules` expose le catalogue public des rules non-deprecated
//!   pour affichage "voici tous les badges gagnables".

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;

pub fn badge_routes() -> Router<AppState> {
    Router::new()
        .route("/users/{id}/badges", get(user_badges))
        .route("/badge-rules", get(list_rules))
}

/// Single badge item as returned inside the polymorphic user-badges
/// response. `output_type` is what tells the front whether this is a
/// skill patch, medal, seal, stamp, or guild crest.
#[derive(Debug, Clone, Serialize, sqlx::FromRow, ToSchema)]
pub struct BadgeItem {
    pub rule_slug: Option<String>,
    /// `"skill_patch"`, `"medal"`, `"challenge_seal"`, `"event_stamp"`, `"guild_crest"`.
    pub output_type: Option<String>,
    pub output_variant: Option<String>,
    pub display_name: Option<String>,
    /// `"common"`, `"rare"`, `"epic"`, `"legendary"`.
    pub rarity: String,
    pub earned_at: chrono::DateTime<chrono::Utc>,
    pub source_proofs_count: i64,
}

/// Current user rank + optional previous rank for the "promoted from"
/// UI accent. Falls back to a stub `apprenti` row for users pre-P18.
#[derive(Debug, Clone, Serialize, sqlx::FromRow, ToSchema)]
pub struct RankRow {
    /// One of `apprenti`, `compagnon`, `maitre`, `doyen`.
    pub rank: String,
    pub achieved_at: chrono::DateTime<chrono::Utc>,
    pub previous_rank: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserBadgesResponse {
    pub user_id: Uuid,
    pub rank: RankRow,
    pub skill_patches: Vec<BadgeItem>,
    pub medals: Vec<BadgeItem>,
    /// Aggregated count (not the full list — challenge seals can be
    /// numerous, front pages them separately).
    pub challenge_seals_count: usize,
    pub event_stamps_count: usize,
    pub guild_crests: Vec<BadgeItem>,
    pub total_badges: usize,
}

/// Polymorphic badges endpoint — returns every badge family the user
/// has earned, plus their current rank. Falls back to a stub
/// `apprenti` rank for accounts predating the P18 auto-creation
/// trigger so the front contract stays stable.
#[utoipa::path(
    get,
    path = "/api/users/{id}/badges",
    tag = "profile",
    params(("id" = Uuid, Path, description = "User UUID")),
    responses(
        (status = 200, description = "User badges + rank snapshot", body = ApiResponse<UserBadgesResponse>),
    ),
)]
pub async fn user_badges(
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
) -> Result<Json<ApiResponse<UserBadgesResponse>>, AppError> {
    let items: Vec<BadgeItem> = sqlx::query_as(
        r#"
        SELECT
            br.slug          AS rule_slug,
            br.output_type   AS output_type,
            br.output_variant AS output_variant,
            br.display_name  AS display_name,
            ub.rarity        AS rarity,
            ub.earned_at     AS earned_at,
            COALESCE(array_length(ub.source_proofs, 1), 0)::BIGINT AS source_proofs_count
        FROM user_badges ub
        LEFT JOIN badge_rules br ON br.id = ub.rule_id
        WHERE ub.user_id = $1 AND ub.revoked_at IS NULL
        ORDER BY ub.earned_at DESC
        "#,
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await?;

    // Fallback : les users créés après la migration 0092 n'ont pas de ligne
    // (le trigger d'auto-création arrivera en P18). En attendant on renvoie
    // apprenti par défaut pour un contrat frontend stable.
    let rank: RankRow = sqlx::query_as(
        "SELECT rank, achieved_at, previous_rank FROM user_ranks WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(&state.db)
    .await?
    .unwrap_or(RankRow {
        rank: "apprenti".to_string(),
        achieved_at: chrono::Utc::now(),
        previous_rank: None,
    });

    // Split par output_type pour lecture frontend simple.
    let mut skill_patches: Vec<BadgeItem> = Vec::new();
    let mut medals: Vec<BadgeItem> = Vec::new();
    let mut seals: Vec<BadgeItem> = Vec::new();
    let mut stamps: Vec<BadgeItem> = Vec::new();
    let mut crests: Vec<BadgeItem> = Vec::new();
    let total = items.len();
    for it in items {
        match it.output_type.as_deref() {
            Some("skill_patch") => skill_patches.push(it),
            Some("medal") => medals.push(it),
            Some("challenge_seal") => seals.push(it),
            Some("event_stamp") => stamps.push(it),
            Some("guild_crest") => crests.push(it),
            _ => {}
        }
    }

    Ok(Json(ApiResponse::new(UserBadgesResponse {
        user_id,
        rank,
        skill_patches,
        medals,
        challenge_seals_count: seals.len(),
        event_stamps_count: stamps.len(),
        guild_crests: crests,
        total_badges: total,
    })))
}

/// Row from the public badge-rules catalog. Conditions payload is
/// left as free-form JSON since each `output_type` defines its own
/// shape (see `badge_rules_engine`).
#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct RuleCatalogRow {
    pub slug: String,
    pub output_type: String,
    pub output_variant: Option<String>,
    pub display_name: String,
    pub description: String,
    pub icon_key: Option<String>,
    pub rarity: String,
    /// Free-form condition payload (schema depends on output_type).
    pub conditions: serde_json::Value,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RulesCatalogResponse {
    pub rules: Vec<RuleCatalogRow>,
}

/// Public catalog of every non-deprecated badge rule. Front uses it
/// for the "voici tous les badges gagnables" screen.
#[utoipa::path(
    get,
    path = "/api/badge-rules",
    tag = "profile",
    responses(
        (status = 200, description = "Badge rules catalog", body = ApiResponse<RulesCatalogResponse>),
    ),
)]
pub async fn list_rules(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<RulesCatalogResponse>>, AppError> {
    let rows: Vec<RuleCatalogRow> = sqlx::query_as(
        "SELECT slug, output_type, output_variant, display_name, description,
                icon_key, rarity, conditions
         FROM badge_rules WHERE deprecated_at IS NULL
         ORDER BY output_type, slug",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(ApiResponse::new(RulesCatalogResponse { rules: rows })))
}
