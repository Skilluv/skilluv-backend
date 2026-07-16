//! P14.5 — Admin fraud dashboard.
//!
//! Endpoints admin pour la modération anti-fraude :
//! - GET /api/admin/fraud/queue : deliverables flaggés + users suspects.
//! - POST /api/admin/fraud/deliverables/{id}/mark-valid : lève le flag.
//! - POST /api/admin/fraud/deliverables/{id}/revoke : marque revoked.
//! - POST /api/admin/fraud/users/{id}/mark-valid : lève le suspected_multi_account.
//! - POST /api/admin/fraud/scan-deliverable/{id} : (re-)lance un scan de similarité.
//! - POST /api/admin/fraud/detect-multi-accounts : lance le job de détection.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use bigdecimal::BigDecimal;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::{fingerprint, plagiarism};

pub fn admin_fraud_routes() -> Router<AppState> {
    Router::new()
        .route("/admin/fraud/queue", get(fraud_queue))
        .route(
            "/admin/fraud/deliverables/{id}/mark-valid",
            post(mark_deliverable_valid),
        )
        .route("/admin/fraud/deliverables/{id}/revoke", post(revoke_deliverable))
        .route("/admin/fraud/users/{id}/mark-valid", post(mark_user_valid))
        .route(
            "/admin/fraud/scan-deliverable/{id}",
            post(scan_deliverable_endpoint),
        )
        .route(
            "/admin/fraud/detect-multi-accounts",
            post(detect_multi_accounts_endpoint),
        )
        .route(
            "/admin/fraud/llm-evaluate/{id}",
            post(llm_evaluate_endpoint),
        )
        // IA-B — deep plagiarism scan (LLM-assisted AST + embeddings via IA).
        // Complémentaire au cosine local rapide (P14.3). Accessible admin
        // OU plagiarism_reviewer (P25).
        .route(
            "/admin/fraud/deep-scan/{id}",
            post(deep_plagiarism_scan_endpoint),
        )
}

fn build_response(data: Value) -> Value {
    json!({
        "data": data,
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })
}

// P21.1 : délègue à user_capabilities (source de vérité canonique).
// Note: signature devient async, tous les call sites `require_admin(&state, &auth).await?`
// ont été mis à jour en `require_admin(&state, &auth).await?`.
async fn require_admin(state: &AppState, auth: &AuthUser) -> Result<(), AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await
}

// ═══════════════════════════════════════════════════════════════════
// GET /admin/fraud/queue
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct QueueQuery {
    threshold: Option<String>,
    limit: Option<i64>,
}

async fn fraud_queue(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<QueueQuery>,
) -> Result<Json<Value>, AppError> {
    require_admin(&state, &auth).await?;

    let threshold: BigDecimal = q
        .threshold
        .as_deref()
        .and_then(|s| BigDecimal::try_from(s.parse::<f64>().ok()?).ok())
        .unwrap_or_else(|| BigDecimal::try_from(0.9f64).unwrap());
    let limit = q.limit.unwrap_or(50);

    let plag = plagiarism::list_flagged(&state.db, threshold, limit).await?;

    let suspects: Vec<(Uuid, chrono::DateTime<chrono::Utc>, Option<String>)> = sqlx::query_as(
        r#"
        SELECT id, suspected_multi_account_at, suspected_multi_account_reason
        FROM users
        WHERE suspected_multi_account = TRUE
        ORDER BY suspected_multi_account_at DESC NULLS LAST
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(build_response(json!({
        "flagged_deliverables": plag.into_iter().map(|(id, score, similar)| json!({
            "deliverable_id": id,
            "plagiarism_score": score,
            "similar_to": similar,
        })).collect::<Vec<_>>(),
        "suspected_users": suspects.into_iter().map(|(id, at, reason)| json!({
            "user_id": id,
            "flagged_at": at,
            "reason": reason,
        })).collect::<Vec<_>>(),
    }))))
}

// ═══════════════════════════════════════════════════════════════════
// POST /admin/fraud/deliverables/{id}/mark-valid
// ═══════════════════════════════════════════════════════════════════

async fn mark_deliverable_valid(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    require_admin(&state, &auth).await?;
    let res = sqlx::query(
        "UPDATE deliverables
         SET plagiarism_score = NULL,
             plagiarism_similar_to = NULL,
             plagiarism_scanned_at = NOW()
         WHERE id = $1",
    )
    .bind(id)
    .execute(&state.db)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound("deliverable not found".into()));
    }
    Ok(Json(build_response(json!({ "marked_valid": true }))))
}

// ═══════════════════════════════════════════════════════════════════
// POST /admin/fraud/deliverables/{id}/revoke
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct RevokeBody {
    #[serde(default)]
    reason: Option<String>,
}

async fn revoke_deliverable(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<RevokeBody>,
) -> Result<Json<Value>, AppError> {
    require_admin(&state, &auth).await?;
    let res = sqlx::query(
        "UPDATE deliverables
         SET revoked_at = NOW(),
             revocation_reason = COALESCE($1, 'admin_fraud_revoke')
         WHERE id = $2",
    )
    .bind(body.reason.as_deref())
    .bind(id)
    .execute(&state.db)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound("deliverable not found".into()));
    }
    metrics::counter!("skilluv_fraud_deliverables_revoked_total").increment(1);
    Ok(Json(build_response(json!({ "revoked": true }))))
}

// ═══════════════════════════════════════════════════════════════════
// POST /admin/fraud/users/{id}/mark-valid
// ═══════════════════════════════════════════════════════════════════

async fn mark_user_valid(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    require_admin(&state, &auth).await?;
    let res = sqlx::query(
        "UPDATE users
         SET suspected_multi_account = FALSE,
             suspected_multi_account_reason = 'cleared_by_admin'
         WHERE id = $1",
    )
    .bind(id)
    .execute(&state.db)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound("user not found".into()));
    }
    Ok(Json(build_response(json!({ "marked_valid": true }))))
}

// ═══════════════════════════════════════════════════════════════════
// POST /admin/fraud/scan-deliverable/{id}
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct ScanQuery {
    threshold: Option<f32>,
    window_days: Option<i32>,
}

async fn scan_deliverable_endpoint(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Query(q): Query<ScanQuery>,
) -> Result<Json<Value>, AppError> {
    require_admin(&state, &auth).await?;
    let threshold = q.threshold.unwrap_or(0.9);
    let window = q.window_days.unwrap_or(30);
    let res = plagiarism::scan_deliverable(&state.db, id, threshold, window).await?;
    Ok(Json(build_response(json!({
        "deliverable_id": id,
        "best_match_id": res.best_match_id,
        "best_score": res.best_score,
        "compared_count": res.compared_count,
    }))))
}

// ═══════════════════════════════════════════════════════════════════
// POST /admin/fraud/detect-multi-accounts
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct DetectBody {
    #[serde(default)]
    window_hours: Option<i32>,
    #[serde(default)]
    min_group_size: Option<i32>,
}

async fn detect_multi_accounts_endpoint(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<DetectBody>,
) -> Result<Json<Value>, AppError> {
    require_admin(&state, &auth).await?;
    let groups = fingerprint::detect_multi_accounts(
        &state.db,
        body.window_hours.unwrap_or(24),
        body.min_group_size.unwrap_or(3),
    )
    .await?;
    let total_users: usize = groups.iter().map(|g| g.user_ids.len()).sum();
    Ok(Json(build_response(json!({
        "groups_detected": groups.len(),
        "users_flagged": total_users,
        "groups": groups,
    }))))
}

// ═══════════════════════════════════════════════════════════════════
// POST /admin/fraud/llm-evaluate/{id} — P15.2 déclenche évaluation LLM
// ═══════════════════════════════════════════════════════════════════

async fn llm_evaluate_endpoint(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    require_admin(&state, &auth).await?;
    let outcome = crate::services::llm_verifier::evaluate_deliverable(
        &state.db,
        state.ai.as_deref(),
        id,
    )
    .await?;
    Ok(Json(build_response(json!(outcome))))
}

// ═══════════════════════════════════════════════════════════════════
// IA-B — POST /admin/fraud/deep-scan/{id}
// ═══════════════════════════════════════════════════════════════════
//
// Deep plagiarism scan via IA (AST + embeddings). Complémentaire au cosine
// local (P14.3) qui reste la 1re ligne de défense automatique. Ce endpoint
// sert 2 usages :
//
//   1. **Manual admin review** : un plagiarism_reviewer inspecte un cas gris
//      (cosine score 0.3-0.7) et veut une seconde opinion IA.
//   2. **Auto-escalation** : le hook post-cosine peut appeler ce endpoint
//      quand `cosine_score >= 0.7` pour affiner la décision. (Câblage
//      automatique post-MVP — on garde l'endpoint manuel au MVP.)
//
// Le résultat est mergé dans `deliverables.verification_signal.deep_plagiarism`
// (JSONB) — préserve le cosine_score existant, ajoute une clé distincte.
// Accessible : `admin` OU `plagiarism_reviewer` (require_any_capability).

#[derive(Debug, Deserialize)]
struct DeepScanQuery {
    /// Threshold IA (0.0-1.0). Défaut 0.80 (aligné avec skilluv-ia settings).
    /// Override utile pour scans stricts (0.95) sur cas déjà escaladés.
    threshold: Option<f32>,
    /// Fenêtre pour construire le comparison_pool (défaut 30 jours).
    window_days: Option<i32>,
    /// Cap du comparison_pool (défaut 200, voir doc §5).
    pool_cap: Option<i64>,
}

async fn deep_plagiarism_scan_endpoint(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Query(q): Query<DeepScanQuery>,
) -> Result<Json<Value>, AppError> {
    crate::middleware::capabilities::require_any_capability(
        &state.db,
        auth.user_id,
        &["admin", "plagiarism_reviewer"],
    )
    .await?;

    // Rate-limit destructif (une deep scan coûte 2-5s IA + capacity queue Redis).
    crate::middleware::admin_destructive::enforce_admin_destructive(&state, auth.user_id).await?;

    // 1. Charge le deliverable cible + son code.
    let target: Option<(Uuid, Option<Uuid>, Option<serde_json::Value>)> = sqlx::query_as(
        "SELECT id, challenge_id, artifact_metadata FROM deliverables WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?;
    let (deliverable_id, challenge_id, metadata) = target
        .ok_or_else(|| AppError::NotFound("deliverable not found".into()))?;

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
            "artifact_metadata.code_content vide — rien à scanner".into(),
        ));
    }

    let ai = state.ai.as_deref().ok_or_else(|| {
        AppError::Internal("AI client not connected (grpc_ai_url absent en dev)".into())
    })?;

    // 2. Construit le comparison_pool (challenges similaires, cap 200).
    let pool_cap = q.pool_cap.unwrap_or(200).clamp(1, 500);
    let window = q.window_days.unwrap_or(30);
    let pool_rows: Vec<(Uuid, String, chrono::DateTime<chrono::Utc>)> = if let Some(cid) = challenge_id {
        sqlx::query_as(
            r#"
            SELECT d.id,
                   COALESCE(d.artifact_metadata->>'code_content', ''),
                   d.verified_at
            FROM deliverables d
            WHERE d.challenge_id = $1
              AND d.id != $2
              AND d.verification_status = 'verified'
              AND d.verified_at >= NOW() - MAKE_INTERVAL(days => $3)
              AND (d.artifact_metadata->>'code_content') IS NOT NULL
            ORDER BY d.verified_at DESC
            LIMIT $4
            "#,
        )
        .bind(cid)
        .bind(deliverable_id)
        .bind(window)
        .bind(pool_cap)
        .fetch_all(&state.db)
        .await?
    } else {
        Vec::new()
    };

    // Note : `PreviousSubmission` v2 n'inclut PAS user_id (le contrat proto
    // délègue le lookup au backend via `similar_submission_id` retourné).
    // On passe la même language que le deliverable cible (assumption : pool
    // tenant-scoped d'un même challenge → même stack).
    let comparison_pool: Vec<crate::grpc::proto::PreviousSubmission> = pool_rows
        .into_iter()
        .map(|(sid, c, sub_at)| crate::grpc::proto::PreviousSubmission {
            submission_id: sid.to_string(),
            code: c,
            language: language.clone(),
            submitted_at: sub_at.to_rfc3339(),
        })
        .collect();

    let pool_size = comparison_pool.len();

    // 3. Appelle l'IA.
    let request = crate::grpc::proto::CheckPlagiarismRequest {
        submission_id: deliverable_id.to_string(),
        code,
        language,
        comparison_pool,
        threshold: q.threshold.unwrap_or(0.80) as f64,
    };
    let resp = ai
        .check_plagiarism(request)
        .await
        .map_err(|s| AppError::Internal(format!("gRPC check_plagiarism failed: {s}")))?;

    // 4. Merge le résultat dans verification_signal.deep_plagiarism.
    let signal = serde_json::json!({
        "deep_plagiarism": {
            "similarity_score": resp.similarity_score,
            "similar_submission_id": resp.similar_submission_id,
            "ast_similarity": resp.ast_similarity,
            "embedding_similarity": resp.embedding_similarity,
            "is_plagiarism": resp.is_plagiarism,
            "matched_ranges_count": resp.matched_ranges.len(),
            "model_version": resp.model_version,
            "scanned_by": auth.user_id,
            "scanned_at": chrono::Utc::now().to_rfc3339(),
        }
    });
    sqlx::query(
        "UPDATE deliverables
         SET verification_signal = COALESCE(verification_signal, '{}'::jsonb) || $2::jsonb
         WHERE id = $1",
    )
    .bind(deliverable_id)
    .bind(&signal)
    .execute(&state.db)
    .await?;

    metrics::counter!(
        "skilluv_deep_plagiarism_scans_total",
        "is_plagiarism" => resp.is_plagiarism.to_string(),
    )
    .increment(1);

    // Audit log (l'action est destructive au sens signal enrichi).
    crate::services::audit::record(
        &state.db,
        crate::services::audit::AuditEntry {
            actor_type: crate::services::audit::ActorType::Admin,
            actor_id: Some(auth.user_id),
            action: "deep_plagiarism_scan",
            target_type: Some("deliverable"),
            target_id: Some(deliverable_id),
            metadata: Some(serde_json::json!({
                "similarity_score": resp.similarity_score,
                "is_plagiarism": resp.is_plagiarism,
                "pool_size": pool_size,
            })),
            headers: None,
        },
    )
    .await;

    Ok(Json(build_response(json!({
        "deliverable_id": deliverable_id,
        "similarity_score": resp.similarity_score,
        "similar_submission_id": resp.similar_submission_id,
        "ast_similarity": resp.ast_similarity,
        "embedding_similarity": resp.embedding_similarity,
        "is_plagiarism": resp.is_plagiarism,
        "pool_size": pool_size,
        "model_version": resp.model_version,
    }))))
}
