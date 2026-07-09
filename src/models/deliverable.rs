//! Artefact opposable produit par un user.
//!
//! Voir `docs/challenges-target-model-and-roadmap.md` section B.6 pour le rationale
//! et partie G.1 pour le workflow "PR mergée → deliverable auto-vérifié".
//!
//! Un deliverable vérifié est **immuable** sauf `revoked_at`, `featured`, `public`.
//! Les corrections légitimes passent par un nouveau deliverable qui supersede
//! l'ancien via `parent_deliverable_id`.

use bigdecimal::BigDecimal;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Un artefact réel vérifiable, remplace sémantiquement `challenge_submissions.code`.
///
/// Un deliverable est toujours rattaché à *au moins* :
/// - une `project_slice` (contribution à un vrai projet), OU
/// - un `challenge` (training onboarding ou capstone)
///
/// Une fois `verification_status = 'verified'`, l'artefact est immuable
/// (contrainte SQL + convention applicative).
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Deliverable {
    pub id: Uuid,

    pub slice_id: Option<Uuid>,
    pub challenge_id: Option<Uuid>,
    pub user_id: Uuid,
    pub team_id: Option<Uuid>,

    /// Chain de supersede — un deliverable peut remplacer un précédent révoqué
    /// pour permettre une correction transparente sans réécrire l'historique.
    pub parent_deliverable_id: Option<Uuid>,

    pub artifact_type: String,
    pub artifact_url: String,
    pub artifact_hash: Option<String>,
    pub artifact_metadata: Option<serde_json::Value>,

    pub verifiable_by: String,
    pub verification_status: String,
    pub verified_at: Option<DateTime<Utc>>,
    pub verified_by_user_id: Option<Uuid>,
    pub verification_signal: Option<serde_json::Value>,
    pub verification_notes: Option<String>,

    pub fragments_awarded: i32,
    pub credits_awarded: BigDecimal,

    /// Politique IA — déclarée par le user à la soumission ou dans une fenêtre
    /// de 7 jours post-vérification (voir partie G.1 étape 12).
    pub ai_assistance_level: Option<String>,
    pub ai_tools_used: Vec<String>,
    pub ai_disclosure_notes: Option<String>,
    pub ai_disclosure_prompted_at: Option<DateTime<Utc>>,
    pub ai_disclosure_deadline_at: Option<DateTime<Utc>>,

    pub public: bool,
    pub featured: bool,

    pub revoked_at: Option<DateTime<Utc>>,
    pub revoked_by_user_id: Option<Uuid>,
    pub revocation_reason: Option<String>,

    pub submitted_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

/// Statut de vérification (miroir du CHECK constraint SQL).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    Pending,
    PendingManualReview,
    PendingAdminEscalation,
    Verified,
    Rejected,
    Revoked,
}

/// Méthode de vérification (miroir du CHECK constraint SQL).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VerifiableBy {
    GithubWebhook,
    HumanReview,
    AutomatedDiff,
    ThirdPartyApi,
    CiStatus,
    Multi,
}

/// Niveau d'assistance IA déclaré par le user (aligné politique section 10 vision).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AiAssistanceLevel {
    None,
    Autocomplete,
    PairProgramming,
    GeneratedThenRefactored,
    GeneratedAsIs,
}
