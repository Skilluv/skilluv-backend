//! Unité de travail réelle sur un projet curated.
//!
//! Voir `docs/challenges-target-model-and-roadmap.md` section B.4 pour le rationale
//! et partie G.1 pour le workflow "PR mergée → deliverable auto-vérifié".

use bigdecimal::BigDecimal;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// A slice = a scope of work claim-able exclusively.
///
/// Generalizes the pattern established by `oss_bounties` (see migration 0042). A
/// bounty is now just a slice with `credits_reward > 0`.
///
/// Workflow (P26 v2, decision 2026-08-06 — see migration 0119):
///
/// ```text
/// draft → open → claimed → in_progress → submitted → ci_green
///                                                        ↓
///                                                pending_validation
///                                                    ↙       ↘
///                                            validated       claimed (reject)
///                                                ↓
///                                            merged (bonus)
/// ```
///
/// **Challenge success status** = `validated` (fragments distributed +
/// attestation issued). The transition to `merged` is an independent **bonus**
/// (upstream maintainer merged the PR) that adds extra fragments but is NOT
/// required for the challenge to count as a success.
///
/// Terminal statuses: `merged`, `closed`, `expired`.
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

    /// P26 v2 SKI-77: timestamp of the challenge success (Skilluv validation).
    /// Distinct from `merged_at` (upstream bonus).
    pub validated_at: Option<DateTime<Utc>>,
    /// P26 v2 SKI-77: the Skilluv validator who approved the PR.
    pub validated_by_user_id: Option<Uuid>,

    /// P26 v2 SKI-79: if non-empty, only users with an active user_orientation
    /// matching one of these slugs may claim. Empty = no restriction. Slug
    /// shape validated at insert time by the service (no FK, so orientation
    /// renames don't orphan slices).
    #[serde(default)]
    pub required_orientation_slugs: Vec<String>,

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
