use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
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
    /// The security discipline a cyber challenge is — ctf_flag, defensive_lab,
    /// machine_walkthrough, training_ground, analysis_exercise, audit_exercise
    /// (migration on `challenge_templates`). NULL for every non-security
    /// challenge. Serialized so a client can tell a CTF target from a lab
    /// rather than submit a flag to something that has none (SKI-320).
    pub security_kind: Option<String>,
    /// The cyber difficulty tier, alongside `security_kind`. NULL off-domain.
    pub security_difficulty_tier: Option<String>,
    /// The questions of a defensive lab, as a client may see them —
    /// `[{ id, kind, question, choices, case_sensitive }]`, NULL off-lab.
    ///
    /// Stored with an `expected_answer_hash` and an author's `hint` per
    /// question; neither leaves this struct. The hash is stripped because a
    /// hash of a short answer — a port number, a process name, an address — is
    /// a plaintext answer to anybody with a wordlist, and publishing it would
    /// end the lab for everybody. The hint is stripped because it is what a
    /// wrong answer buys: `security_practice::LabOutcome.hints` returns the
    /// hints for the questions that were actually got wrong, and a hint shown
    /// next to the question before the first attempt is just a shorter
    /// question (SKI-332).
    ///
    /// The stripping is on serialisation rather than at each call site, so the
    /// six other endpoints that return a `ChallengeTemplate` cannot forget it.
    #[serde(serialize_with = "serialize_public_lab_questions")]
    #[schema(value_type = Option<Vec<PublicLabQuestion>>)]
    pub security_lab_questions: Option<serde_json::Value>,
    /// Size of the lab artefact, said before the download starts. A memory
    /// image on a metered connection is a decision, not a click (SKI-332).
    pub security_lab_artifact_bytes: Option<i64>,
    /// The share of the questions that has to be right, announced up front
    /// rather than discovered after a failed attempt.
    pub security_lab_pass_percent: Option<i16>,
    /// Attempts before the cooling-off period, for the same reason.
    pub security_lab_max_attempts: Option<i16>,
    /// P26 — Sas compagnonnage débutant. NULL = challenge non-beginner.
    /// 'sas' = review humaine du process ; 'free' = mode libre réservé
    /// aux verified_apprentice (voir migration 0118 + gate submit_challenge).
    pub beginner_stage: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// One question of a defensive lab, as a client is served it.
///
/// Documentation only: the column is JSONB and is serialised by
/// [`serialize_public_lab_questions`], which is the single place the shape is
/// produced. This type is what the OpenAPI document says that shape is, so a
/// generated client has `question` and `choices` as fields rather than an
/// untyped object — and so the day somebody widens the projection, the
/// document is a compile-adjacent reminder rather than silently stale.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct PublicLabQuestion {
    /// The id to answer under, in `POST /security/challenges/{id}/answers`.
    pub id: String,
    /// `text` or `choice`.
    pub kind: Option<String>,
    pub question: String,
    /// The options, for a `choice` question. Empty for a `text` one.
    #[serde(default)]
    pub choices: Vec<String>,
    /// Whether the answer is compared as typed. False for almost everything:
    /// most answers here are an address, a tool name or a count.
    #[serde(default)]
    pub case_sensitive: bool,
}

/// Serialise a lab's questions without the two fields a client may not have.
///
/// See [`ChallengeTemplate::security_lab_questions`]. Anything that is not an
/// array of objects is serialised as `null` rather than passed through: a lab
/// whose questions column holds an unexpected shape is a lab nobody can attempt
/// anyway, and passing the shape on is how an `expected_answer_hash` nested
/// somewhere unforeseen would reach a browser.
fn serialize_public_lab_questions<S>(
    value: &Option<serde_json::Value>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    /// The keys a client is served, in the order the front expects them.
    const PUBLIC_KEYS: &[&str] = &["id", "kind", "question", "choices", "case_sensitive"];

    let public = value.as_ref().and_then(|v| v.as_array()).map(|questions| {
        let items: Vec<serde_json::Value> = questions
            .iter()
            .filter_map(|q| q.as_object())
            .map(|q| {
                let mut out = serde_json::Map::new();
                for key in PUBLIC_KEYS {
                    if let Some(field) = q.get(*key) {
                        out.insert((*key).to_string(), field.clone());
                    }
                }
                serde_json::Value::Object(out)
            })
            .collect();
        serde_json::Value::Array(items)
    });

    public.serialize(serializer)
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
