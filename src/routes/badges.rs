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
use serde_json::{json, Value};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;

pub fn badge_routes() -> Router<AppState> {
    Router::new()
        .route("/users/{id}/badges", get(user_badges))
        .route("/badge-rules", get(list_rules))
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

#[derive(Debug, Serialize, sqlx::FromRow)]
struct BadgeItem {
    rule_slug: Option<String>,
    output_type: Option<String>,
    output_variant: Option<String>,
    display_name: Option<String>,
    rarity: String,
    earned_at: chrono::DateTime<chrono::Utc>,
    source_proofs_count: i64,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct RankRow {
    rank: String,
    achieved_at: chrono::DateTime<chrono::Utc>,
    previous_rank: Option<String>,
}

async fn user_badges(
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
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
    let mut skill_patches: Vec<&BadgeItem> = Vec::new();
    let mut medals: Vec<&BadgeItem> = Vec::new();
    let mut seals: Vec<&BadgeItem> = Vec::new();
    let mut stamps: Vec<&BadgeItem> = Vec::new();
    let mut crests: Vec<&BadgeItem> = Vec::new();
    for it in &items {
        match it.output_type.as_deref() {
            Some("skill_patch") => skill_patches.push(it),
            Some("medal") => medals.push(it),
            Some("challenge_seal") => seals.push(it),
            Some("event_stamp") => stamps.push(it),
            Some("guild_crest") => crests.push(it),
            _ => {}
        }
    }

    Ok(Json(wrap(json!({
        "user_id": user_id,
        "rank": rank,
        "skill_patches": skill_patches,
        "medals": medals,
        "challenge_seals_count": seals.len(),
        "event_stamps_count": stamps.len(),
        "guild_crests": crests,
        "total_badges": items.len(),
    }))))
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct RuleCatalogRow {
    slug: String,
    output_type: String,
    output_variant: Option<String>,
    display_name: String,
    description: String,
    icon_key: Option<String>,
    rarity: String,
    conditions: serde_json::Value,
}

async fn list_rules(
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    let rows: Vec<RuleCatalogRow> = sqlx::query_as(
        "SELECT slug, output_type, output_variant, display_name, description,
                icon_key, rarity, conditions
         FROM badge_rules WHERE deprecated_at IS NULL
         ORDER BY output_type, slug",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(wrap(json!({ "rules": rows }))))
}
