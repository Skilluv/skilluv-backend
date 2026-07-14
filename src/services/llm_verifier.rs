//! P15.2 — LLM evaluation d'un deliverable via skilluv-ia (gRPC).
//!
//! On NE recode PAS le modèle LLM ici : skilluv-ia expose déjà
//! `code_reviewer.py` (proto: CodeReviewService.review_code). Ce module
//! est un wrapper Rust qui :
//!   1. Charge le deliverable + le challenge_template + le code depuis
//!      `artifact_metadata.code_content`.
//!   2. Appelle `AiClient::review_code` (existant depuis Phase 4).
//!   3. Update `deliverables.verification_status` selon le score renvoyé,
//!      en ajoutant le rapport LLM dans `verification_signal`.
//!
//! Sans `AiClient` connecté (grpc_ai_url absent en dev), on log un warning
//! et laisse le deliverable en 'pending_manual_review' — safe fallback.

use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::grpc::AiClient;

/// Résultat d'une évaluation LLM.
#[derive(Debug, Clone, serde::Serialize)]
pub struct LlmEvaluationOutcome {
    pub deliverable_id: Uuid,
    pub new_status: String,
    pub score: Option<f64>,
    pub llm_reachable: bool,
    pub notes: Option<String>,
}

/// Threshold en dessous duquel on passe en review humaine.
const AUTO_VERIFY_THRESHOLD: f64 = 0.7;

/// Évalue un deliverable via LLM. Retourne l'outcome + met à jour la DB.
///
/// Contrats :
/// - Le deliverable doit avoir `verifiable_by = 'llm_evaluation'`.
/// - Le deliverable doit avoir `artifact_metadata.code_content`.
/// - Si `ai_client` est None → status='pending_manual_review' + notes.
pub async fn evaluate_deliverable(
    db: &PgPool,
    ai_client: Option<&AiClient>,
    deliverable_id: Uuid,
) -> Result<LlmEvaluationOutcome, AppError> {
    let row: Option<(String, Option<Uuid>, Option<serde_json::Value>)> = sqlx::query_as(
        "SELECT verifiable_by, challenge_id, artifact_metadata
         FROM deliverables WHERE id = $1",
    )
    .bind(deliverable_id)
    .fetch_optional(db)
    .await?
    .map(|(vb, cid, meta): (String, Option<Uuid>, Option<serde_json::Value>)| (vb, cid, meta));

    let (verifiable_by, challenge_id, metadata) = row.ok_or_else(|| {
        AppError::NotFound("deliverable not found".into())
    })?;

    if verifiable_by != "llm_evaluation" {
        return Err(AppError::Validation(format!(
            "deliverable verifiable_by is '{verifiable_by}', expected 'llm_evaluation'"
        )));
    }

    let code = metadata
        .as_ref()
        .and_then(|m| m.get("code_content"))
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_string();
    let language = metadata
        .as_ref()
        .and_then(|m| m.get("language"))
        .and_then(|l| l.as_str())
        .unwrap_or("text")
        .to_string();

    if code.trim().is_empty() {
        return Err(AppError::Validation(
            "artifact_metadata.code_content is empty — nothing to evaluate".into(),
        ));
    }

    // Charge le challenge_template pour title + instructions + rubric.
    let (title, instructions, difficulty, rubric): (String, String, i16, Option<serde_json::Value>) = if let Some(cid) = challenge_id {
        sqlx::query_as(
            "SELECT title, instructions, difficulty, evaluation_rubric
             FROM challenge_templates WHERE id = $1",
        )
        .bind(cid)
        .fetch_optional(db)
        .await?
        .unwrap_or_else(|| ("(no template)".into(), "".into(), 3i16, None))
    } else {
        ("(no template)".into(), "".into(), 3i16, None)
    };

    // Concatène instructions + rubric pour donner du contexte au LLM.
    let full_instructions = if let Some(r) = rubric.as_ref() {
        format!("{instructions}\n\nEvaluation rubric (JSON):\n{}", r)
    } else {
        instructions.clone()
    };

    let Some(ai) = ai_client else {
        // Fallback : marque pending_manual_review avec explanation.
        sqlx::query(
            "UPDATE deliverables
             SET verification_status = 'pending_manual_review',
                 verification_signal = COALESCE(verification_signal, '{}'::jsonb)
                     || jsonb_build_object(
                         'llm_verifier', jsonb_build_object(
                             'status', 'skipped',
                             'reason', 'ai_client_not_connected'
                         ))
             WHERE id = $1",
        )
        .bind(deliverable_id)
        .execute(db)
        .await?;
        return Ok(LlmEvaluationOutcome {
            deliverable_id,
            new_status: "pending_manual_review".into(),
            score: None,
            llm_reachable: false,
            notes: Some("AI client not connected — deliverable flagged for manual review".into()),
        });
    };

    match ai
        .review_code(
            &deliverable_id.to_string(),
            &code,
            &language,
            &title,
            &full_instructions,
            difficulty as i32,
        )
        .await
    {
        Ok(resp) => {
            // Score attendu dans [0.0, 1.0]. Le proto expose des champs
            // score/feedback via CodeReviewResponse — on lit permissivement.
            let score: f64 = extract_score(&resp);
            let feedback = extract_feedback(&resp);
            let (new_status, notes) = if score >= AUTO_VERIFY_THRESHOLD {
                ("verified", format!("LLM auto-approved with score {score:.2}"))
            } else {
                (
                    "pending_manual_review",
                    format!("LLM score {score:.2} < {AUTO_VERIFY_THRESHOLD} — flagged for review"),
                )
            };
            let signal = json!({
                "llm_verifier": {
                    "status": "evaluated",
                    "score": score,
                    "feedback": feedback,
                    "threshold": AUTO_VERIFY_THRESHOLD,
                }
            });
            let verified_at_expr = if new_status == "verified" {
                "NOW()"
            } else {
                "verified_at"
            };
            let sql = format!(
                "UPDATE deliverables
                 SET verification_status = $1,
                     verified_at = {verified_at_expr},
                     verification_signal = COALESCE(verification_signal, '{{}}'::jsonb) || $2::jsonb
                 WHERE id = $3"
            );
            sqlx::query(&sql)
                .bind(new_status)
                .bind(&signal)
                .bind(deliverable_id)
                .execute(db)
                .await?;
            metrics::counter!(
                "skilluv_llm_evaluations_total",
                "result" => new_status.to_string()
            )
            .increment(1);
            Ok(LlmEvaluationOutcome {
                deliverable_id,
                new_status: new_status.to_string(),
                score: Some(score),
                llm_reachable: true,
                notes: Some(notes),
            })
        }
        Err(status) => {
            tracing::warn!(
                error = %status, deliverable_id = %deliverable_id,
                "LLM evaluation gRPC call failed — falling back to manual review"
            );
            sqlx::query(
                "UPDATE deliverables
                 SET verification_status = 'pending_manual_review',
                     verification_signal = COALESCE(verification_signal, '{}'::jsonb)
                         || jsonb_build_object(
                             'llm_verifier', jsonb_build_object(
                                 'status', 'error',
                                 'error', $1
                             ))
                 WHERE id = $2",
            )
            .bind(status.to_string())
            .bind(deliverable_id)
            .execute(db)
            .await?;
            Ok(LlmEvaluationOutcome {
                deliverable_id,
                new_status: "pending_manual_review".into(),
                score: None,
                llm_reachable: false,
                notes: Some(format!("gRPC error: {status}")),
            })
        }
    }
}

/// Extrait le score depuis la réponse LLM. Défaut 0.5 si absent.
fn extract_score(resp: &crate::grpc::proto::CodeReviewResponse) -> f64 {
    // Le score dans le proto vit sous quality_score (0-100). On normalise en 0-1.
    let q = resp.quality_score as f64;
    (q / 100.0).clamp(0.0, 1.0)
}

fn extract_feedback(resp: &crate::grpc::proto::CodeReviewResponse) -> String {
    // Le proto expose summary + strengths + improvements. On sérialise simple.
    let strengths: Vec<String> = resp.strengths.iter().take(3).cloned().collect();
    let improvements: Vec<String> = resp.improvements.iter().take(3).cloned().collect();
    format!(
        "{}\nStrengths:\n{}\nImprovements:\n{}",
        resp.summary,
        strengths.join("\n"),
        improvements.join("\n")
    )
}
