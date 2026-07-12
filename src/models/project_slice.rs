//! Unité de travail réelle sur un projet curated.
//!
//! Voir `docs/challenges-target-model-and-roadmap.md` section B.4 pour le rationale
//! et partie G.1 pour le workflow "PR mergée → deliverable auto-vérifié".

use bigdecimal::BigDecimal;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Une slice = un scope de travail claim-able exclusivement.
///
/// Généralise le pattern éprouvé de `oss_bounties` (voir migration 0042). La bounty
/// n'est plus qu'un type de slice avec `credits_reward > 0`.
///
/// Workflow : `draft` → `open` → `claimed` (par un user, 7j exclusif) → `in_review`
/// → `merged` (via webhook GitHub ou review humaine) ou `expired` (retour au pool).
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ProjectSlice {
    pub id: Uuid,
    pub project_id: Uuid,

    pub slice_type: String,
    pub external_ref: Option<String>,
    pub external_metadata: Option<serde_json::Value>,

    pub title: String,
    pub description: String,
    pub acceptance_criteria: Option<String>,

    pub primary_domain: String,
    pub difficulty: i16,
    pub estimated_hours: Option<i32>,
    pub fragments_reward: i32,
    pub credits_reward: BigDecimal,

    pub status: String,
    pub claimed_by_user_id: Option<Uuid>,
    /// P10.1 : claim par une team persistente (XOR avec claimed_by_user_id).
    pub claimed_by_team_id: Option<Uuid>,
    pub claimed_at: Option<DateTime<Utc>>,
    pub claim_expires_at: Option<DateTime<Utc>>,

    pub created_by_user_id: Option<Uuid>,
    pub ingested_from: String,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
}

/// Types de slice (miroir du CHECK constraint SQL).
///
/// Détermine comment l'artefact est produit et vérifié (webhook GitHub pour
/// `GithubIssue`, review humaine pour les autres).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SliceType {
    GithubIssue,
    FigmaFrame,
    GameLevel,
    GameAsset,
    SecTarget,
    CliTask,
    DesignToken,
    Documentation,
    Other,
}

/// Lien M2M slice ↔ skill avec poids d'exercice (1-5).
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SliceSkill {
    pub slice_id: Uuid,
    pub skill_id: Uuid,
    /// Intensité de l'exercice sur ce skill par cette slice :
    /// 1 = effleuré, 3 = contribue clairement (défaut), 5 = cœur de la slice
    pub weight: i16,
    pub is_primary: bool,
}
