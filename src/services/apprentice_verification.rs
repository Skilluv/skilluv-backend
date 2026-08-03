//! P26 — Sas compagnonnage débutant : service de vérification asynchrone.
//!
//! Workflow (voir migration 0118 + discussion produit) :
//!   1. Apprenti veut faire un challenge marqué `beginner_stage='sas'`.
//!   2. Il pull d'abord N questions aléatoires du pool actif du challenge
//!      via `pick_questions()`.
//!   3. Il enregistre une vidéo/audio par question, upload sur MinIO,
//!      soumet la liste `{question_id: media_url}` via `submit_verification()`.
//!   4. Une ligne pending atterrit dans la file compagnon.
//!   5. Un compagnon (capability `apprentice_verifier`) revoit via
//!      `list_pending()` puis `record_verdict()`.
//!   6. À `approved`, on déclenche `capabilities_engine::recompute_*` qui
//!      grant `verified_apprentice` si le seuil est atteint.
//!
//! Ce que ce service NE fait PAS :
//!   - Aucune permission check (routes/middleware s'en occupent).
//!   - Aucun upload direct (storage.rs déjà en place, l'URL est passée en
//!     paramètre — le service ne connaît que la string).
//!   - Aucune notif email/push (à câbler dans le hook plus tard si besoin).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::capabilities_engine;

// ─── Constants ────────────────────────────────────────────────────

/// Nombre de questions tirées par soumission. Petit (2) pour ne pas
/// dissuader l'apprenti (chaque question = 60s d'enregistrement +
/// re-take éventuel). Peut être bumped plus tard si les compagnons
/// remontent qu'ils n'ont pas assez de signal.
pub const QUESTIONS_PER_SUBMISSION: usize = 2;

/// Verdicts valides côté DB (miroir du CHECK sur apprentice_verifications).
pub const VERDICT_APPROVED: &str = "approved";
pub const VERDICT_REJECTED: &str = "rejected";
pub const VERDICT_ABSTAIN: &str = "abstain";

// ─── Domain types ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct VerificationQuestion {
    pub id: Uuid,
    pub template_id: Uuid,
    pub prompt_text: String,
    pub order_hint: i32,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ApprenticeVerification {
    pub id: Uuid,
    pub apprentice_user_id: Uuid,
    pub template_id: Uuid,
    pub submission_id: Option<Uuid>,
    pub reviewer_user_id: Option<Uuid>,
    pub answers: JsonValue,
    pub verdict: String,
    pub reviewer_notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub reviewed_at: Option<DateTime<Utc>>,
}

/// Vue "file compagnon" : la ligne verification + le titre du challenge et
/// le pseudo de l'apprenti, pour éviter un round-trip côté front.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct PendingVerificationView {
    pub id: Uuid,
    pub apprentice_user_id: Uuid,
    pub apprentice_username: String,
    pub template_id: Uuid,
    pub challenge_title: String,
    pub answers: JsonValue,
    pub created_at: DateTime<Utc>,
}

/// Vue "progression apprenti" : le compte d'approbations distinctes et
/// l'historique récent des soumissions.
#[derive(Debug, Clone, Serialize)]
pub struct ApprenticeProgress {
    pub approved_distinct: i64,
    pub threshold: i64,
    pub is_verified: bool,
    pub recent: Vec<ApprenticeVerification>,
}

// ─── Errors ───────────────────────────────────────────────────────

/// Erreur métier renvoyée pour les cas où l'entrée est légitime mais
/// que le workflow refuse la transition (à mapper en 4xx côté route).
#[derive(Debug, thiserror::Error)]
pub enum VerificationError {
    #[error("challenge is not part of the beginner sas (beginner_stage != 'sas')")]
    NotSasChallenge,
    #[error("a pending verification already exists for this apprentice and challenge")]
    PendingAlreadyExists,
    #[error("verification not found")]
    NotFound,
    #[error("verification already reviewed (verdict != pending)")]
    AlreadyReviewed,
    #[error("verdict must be one of: approved, rejected, abstain")]
    InvalidVerdict,
    #[error("answers payload must reference the questions returned by pick_questions")]
    AnswersMismatch,
}

impl From<VerificationError> for AppError {
    fn from(err: VerificationError) -> Self {
        AppError::Validation(err.to_string())
    }
}

// ─── Read-side ────────────────────────────────────────────────────

/// Tire aléatoirement N questions du pool actif d'un challenge. Renvoie
/// une erreur métier si le challenge n'est pas `beginner_stage='sas'` ou
/// si le pool contient moins de N questions actives.
pub async fn pick_questions(
    db: &PgPool,
    template_id: Uuid,
) -> Result<Vec<VerificationQuestion>, AppError> {
    ensure_template_is_sas(db, template_id).await?;
    let rows: Vec<VerificationQuestion> = sqlx::query_as(
        "SELECT id, template_id, prompt_text, order_hint, active, created_at, updated_at
         FROM challenge_verification_questions
         WHERE template_id = $1 AND active = TRUE
         ORDER BY RANDOM()
         LIMIT $2",
    )
    .bind(template_id)
    .bind(QUESTIONS_PER_SUBMISSION as i64)
    .fetch_all(db)
    .await?;
    if rows.len() < QUESTIONS_PER_SUBMISSION {
        return Err(AppError::Validation(format!(
            "challenge question pool has {} active questions, need at least {}",
            rows.len(),
            QUESTIONS_PER_SUBMISSION
        )));
    }
    Ok(rows)
}

/// File des vérifications en attente, ordonnée FIFO. Aucun filtrage par
/// compagnon (tous les compagnons se partagent la même file — le premier
/// arrivé rend le verdict).
pub async fn list_pending(
    db: &PgPool,
    limit: i64,
    offset: i64,
) -> Result<Vec<PendingVerificationView>, AppError> {
    let rows: Vec<PendingVerificationView> = sqlx::query_as(
        "SELECT av.id, av.apprentice_user_id, u.username AS apprentice_username,
                av.template_id, ct.title AS challenge_title,
                av.answers, av.created_at
         FROM apprentice_verifications av
         JOIN users u ON u.id = av.apprentice_user_id
         JOIN challenge_templates ct ON ct.id = av.template_id
         WHERE av.verdict = 'pending'
         ORDER BY av.created_at ASC
         LIMIT $1 OFFSET $2",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// Progression de l'apprenti dans le sas + historique récent (10 dernières).
pub async fn get_progress(
    db: &PgPool,
    apprentice_user_id: Uuid,
) -> Result<ApprenticeProgress, AppError> {
    let approved_distinct: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT template_id) FROM apprentice_verifications
         WHERE apprentice_user_id = $1 AND verdict = 'approved'",
    )
    .bind(apprentice_user_id)
    .fetch_one(db)
    .await?;
    let threshold: i64 = std::env::var("SKILLUV_APPRENTICE_SAS_THRESHOLD")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3);
    let is_verified: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM user_capabilities
            WHERE user_id = $1 AND capability = 'verified_apprentice' AND revoked_at IS NULL
        )",
    )
    .bind(apprentice_user_id)
    .fetch_one(db)
    .await?;
    let recent: Vec<ApprenticeVerification> = sqlx::query_as(
        "SELECT id, apprentice_user_id, template_id, submission_id, reviewer_user_id,
                answers, verdict, reviewer_notes, created_at, reviewed_at
         FROM apprentice_verifications
         WHERE apprentice_user_id = $1
         ORDER BY created_at DESC
         LIMIT 10",
    )
    .bind(apprentice_user_id)
    .fetch_all(db)
    .await?;
    Ok(ApprenticeProgress {
        approved_distinct,
        threshold,
        is_verified,
        recent,
    })
}

// ─── Write-side ───────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SubmitPayload {
    pub template_id: Uuid,
    pub submission_id: Option<Uuid>,
    /// Mapping strict `question_id (UUID as string) -> media_url (String)`.
    /// Validé contre le pool actif du template : chaque clé doit être une
    /// question active du challenge, et TOUTES les questions attendues
    /// doivent être présentes (voir `QUESTIONS_PER_SUBMISSION`).
    pub answers: JsonValue,
}

/// Crée une nouvelle vérification pending. Refuse si :
///   - le challenge n'est pas `beginner_stage='sas'`,
///   - il existe déjà une vérification pending pour ce (user, template),
///   - les answers ne référencent pas des questions actives valides,
///   - le nombre d'answers n'égale pas `QUESTIONS_PER_SUBMISSION`.
pub async fn submit_verification(
    db: &PgPool,
    apprentice_user_id: Uuid,
    payload: SubmitPayload,
) -> Result<ApprenticeVerification, AppError> {
    ensure_template_is_sas(db, payload.template_id).await?;

    // Un seul pending par (apprenti, challenge). Les rejetés autorisent un
    // re-submit ; l'unique partial index côté DB garantit l'invariant
    // mais on double-check ici pour renvoyer une 400 plutôt qu'une 500.
    let pending_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM apprentice_verifications
            WHERE apprentice_user_id = $1 AND template_id = $2 AND verdict = 'pending'
        )",
    )
    .bind(apprentice_user_id)
    .bind(payload.template_id)
    .fetch_one(db)
    .await?;
    if pending_exists {
        return Err(VerificationError::PendingAlreadyExists.into());
    }

    // Validation answers : object JSON avec N entrées, chaque clé UUID
    // référençant une question active du template. On accepte tout ordre.
    let obj = payload
        .answers
        .as_object()
        .ok_or(VerificationError::AnswersMismatch)?;
    if obj.len() != QUESTIONS_PER_SUBMISSION {
        return Err(VerificationError::AnswersMismatch.into());
    }
    let question_ids: Vec<Uuid> = obj
        .keys()
        .filter_map(|k| Uuid::parse_str(k).ok())
        .collect();
    if question_ids.len() != QUESTIONS_PER_SUBMISSION {
        return Err(VerificationError::AnswersMismatch.into());
    }
    let valid_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM challenge_verification_questions
         WHERE template_id = $1 AND active = TRUE AND id = ANY($2)",
    )
    .bind(payload.template_id)
    .bind(&question_ids)
    .fetch_one(db)
    .await?;
    if valid_count as usize != QUESTIONS_PER_SUBMISSION {
        return Err(VerificationError::AnswersMismatch.into());
    }

    // Chaque valeur doit être une string non vide (URL du media).
    for (_k, v) in obj {
        match v.as_str() {
            Some(s) if !s.trim().is_empty() => {}
            _ => return Err(VerificationError::AnswersMismatch.into()),
        }
    }

    let row: ApprenticeVerification = sqlx::query_as(
        "INSERT INTO apprentice_verifications
            (apprentice_user_id, template_id, submission_id, answers, verdict)
         VALUES ($1, $2, $3, $4, 'pending')
         RETURNING id, apprentice_user_id, template_id, submission_id,
                   reviewer_user_id, answers, verdict, reviewer_notes,
                   created_at, reviewed_at",
    )
    .bind(apprentice_user_id)
    .bind(payload.template_id)
    .bind(payload.submission_id)
    .bind(&payload.answers)
    .fetch_one(db)
    .await?;
    Ok(row)
}

#[derive(Debug, Deserialize)]
pub struct VerdictPayload {
    pub verdict: String,
    pub notes: Option<String>,
}

/// Rend le verdict compagnon sur une vérification pending. À `approved`,
/// déclenche un recompute des capabilities de l'apprenti — c'est là que
/// `verified_apprentice` est éventuellement accordée (voir P26.6 hook).
pub async fn record_verdict(
    db: &PgPool,
    verification_id: Uuid,
    reviewer_user_id: Uuid,
    payload: VerdictPayload,
) -> Result<ApprenticeVerification, AppError> {
    let verdict = payload.verdict.as_str();
    if verdict != VERDICT_APPROVED
        && verdict != VERDICT_REJECTED
        && verdict != VERDICT_ABSTAIN
    {
        return Err(VerificationError::InvalidVerdict.into());
    }

    // On lit la ligne pour valider qu'elle est pending — refuser un
    // second verdict (compagnon race condition).
    let current: Option<ApprenticeVerification> = sqlx::query_as(
        "SELECT id, apprentice_user_id, template_id, submission_id, reviewer_user_id,
                answers, verdict, reviewer_notes, created_at, reviewed_at
         FROM apprentice_verifications WHERE id = $1",
    )
    .bind(verification_id)
    .fetch_optional(db)
    .await?;
    let current = current.ok_or(VerificationError::NotFound)?;
    if current.verdict != "pending" {
        return Err(VerificationError::AlreadyReviewed.into());
    }

    let updated: ApprenticeVerification = sqlx::query_as(
        "UPDATE apprentice_verifications
         SET verdict = $1, reviewer_user_id = $2, reviewer_notes = $3, reviewed_at = NOW()
         WHERE id = $4 AND verdict = 'pending'
         RETURNING id, apprentice_user_id, template_id, submission_id, reviewer_user_id,
                   answers, verdict, reviewer_notes, created_at, reviewed_at",
    )
    .bind(verdict)
    .bind(reviewer_user_id)
    .bind(payload.notes.as_deref())
    .bind(verification_id)
    .fetch_one(db)
    .await?;

    // Hook P26.6 : sur approved, recompute → grant verified_apprentice si
    // seuil atteint. On ignore silencieusement une erreur du recompute
    // (le verdict, lui, doit rester enregistré ; le grant est
    // rattrapable par un sweep ultérieur).
    if verdict == VERDICT_APPROVED {
        if let Err(err) =
            capabilities_engine::recompute_capabilities_for_user(db, updated.apprentice_user_id)
                .await
        {
            tracing::warn!(
                apprentice_user_id = %updated.apprentice_user_id,
                verification_id = %verification_id,
                error = %err,
                "P26: verdict enregistré mais recompute capabilities a échoué"
            );
        }
    }

    Ok(updated)
}

// ─── Helpers ──────────────────────────────────────────────────────

async fn ensure_template_is_sas(db: &PgPool, template_id: Uuid) -> Result<(), AppError> {
    let stage: Option<String> = sqlx::query_scalar(
        "SELECT beginner_stage FROM challenge_templates WHERE id = $1",
    )
    .bind(template_id)
    .fetch_optional(db)
    .await?
    .flatten();
    if stage.as_deref() != Some("sas") {
        return Err(VerificationError::NotSasChallenge.into());
    }
    Ok(())
}
