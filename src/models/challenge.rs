use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ChallengeTemplate {
    pub id: Uuid,
    pub title: String,
    pub description: String,
    pub instructions: String,
    pub skill_domain: String,
    pub difficulty: i16,
    pub mode: String,
    pub duration_minutes: Option<i32>,
    /// Politique IA typée introduite en P3 (migration 0061). Remplace l'ancien
    /// `ai_allowed` (droppé en P8.3, migration 0070). Défaut : `disclosure_required`.
    /// Valeurs : unrestricted | disclosure_required | human_verified |
    /// no_ai_declared | ai_native. Voir docs partie 10.
    pub ai_policy: String,
    pub tone: String,
    pub language: Option<String>,
    pub reward_fragments: i32,
    pub is_onboarding: bool,
    /// Introduit en P3 (migration 0061). Flag "onboarding/training" hors règle
    /// dure #1 (aucun challenge published sans project_id sauf training).
    pub is_training: bool,
    /// Introduit en P3 (migration 0061). Flag capstone (chef-d'œuvre de fin de phase).
    pub is_capstone: bool,
    /// Introduit en P3 (migration 0061). Lien projet réel (règle dure #1).
    pub project_id: Option<Uuid>,
    pub status: String,
    pub test_cases: Option<serde_json::Value>,
    pub expected_output: Option<String>,
    pub is_community: bool,
    pub community_status: Option<String>,
    pub review_feedback: Option<String>,
    pub featured: bool,
    pub vote_count: i32,
    pub created_by: Option<Uuid>,
    /// P10.3 : composition team attendue (musicien + coder + …). JSONB array
    /// de { role_slug, role_display_name?, required_skill_slug?, min_proficiency_level, count }.
    /// NULL = pas de contrainte, team libre-forme.
    pub team_composition: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// P9.1 : `code|stdout|stderr` retirés (mig 0072) — le contenu de la submission
/// vit désormais dans `deliverables.artifact_metadata` (règle A.4 : immuabilité
/// des preuves). La ligne `challenge_submissions` sert de trace de progression
/// (status, fragments_earned, timestamps) uniquement.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ChallengeSubmission {
    pub id: Uuid,
    pub challenge_id: Uuid,
    pub user_id: Uuid,
    pub status: String,
    pub language: Option<String>,
    pub fragments_earned: i32,
    pub attempt_number: i32,
    pub started_at: DateTime<Utc>,
    pub submitted_at: Option<DateTime<Utc>>,
    pub evaluated_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub team_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SkillFragment {
    pub id: Uuid,
    pub user_id: Uuid,
    pub skill_domain: String,
    pub sub_skill: String,
    pub fragments: i32,
    pub updated_at: DateTime<Utc>,
}
