//! IA-C.2 + IA-C.3 — Routes user-facing branchées sur skilluv-ai.
//!
//! - `GET /api/users/me/performance` (IA-C.2) — analyse coach IA du profil,
//!   cache Redis 24h, refresh manuel via `?refresh=1` (rate-limit 1/heure/user).
//! - `POST /api/users/me/orientations/suggest` (IA-C.3) — suggère 3 orientations
//!   métier basées sur les skills, cache Redis 7 jours, appel Haiku 4.5.
//!
//! Le backend agrège les snapshots (deliverables, skills, orientations, rank)
//! et l'IA ajoute la sémantique. Voir docs/BACKEND-INTEGRATION.md §6.

use axum::extract::{Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use redis::AsyncCommands;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;

// Type aliases pour clippy::type_complexity (rangées sqlx::query_as).
type AiCoachRow179 = (
    String,
    i32,
    i32,
    chrono::DateTime<chrono::Utc>,
    chrono::DateTime<chrono::Utc>,
);
type AiCoachRow302 = (
    String,
    i32,
    i32,
    chrono::DateTime<chrono::Utc>,
    chrono::DateTime<chrono::Utc>,
);

pub fn ai_coach_routes() -> Router<AppState> {
    Router::new()
        .route("/users/me/performance", get(my_performance))
        .route("/users/me/orientations/suggest", post(suggest_orientations))
}

// ═══════════════════════════════════════════════════════════════════
// IA-C.2 — GET /users/me/performance
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct PerfQuery {
    #[serde(default)]
    refresh: bool,
}

async fn my_performance(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<PerfQuery>,
) -> Result<Json<Value>, AppError> {
    let cache_key = format!("ai:performance:{}", auth.user_id);
    let mut redis = state.redis.clone();

    // Refresh manuel : rate-limit 1/heure/user pour éviter les abus coût LLM.
    if q.refresh {
        crate::middleware::RateLimiter::check(
            &mut redis,
            "ai_performance_refresh",
            &auth.user_id.to_string(),
            1,
            3600,
        )
        .await?;
        let _: () = redis.del(&cache_key).await?;
    }

    // Cache hit (TTL 24h).
    let cached: Option<String> = redis.get(&cache_key).await.ok();
    if let Some(json_str) = cached
        && let Ok(v) = serde_json::from_str::<Value>(&json_str)
    {
        return Ok(Json(json!({ "data": v, "cached": true })));
    }

    // Cache miss : agrège snapshots + appel IA.
    let ai = state
        .ai
        .as_deref()
        .ok_or_else(|| AppError::Internal("AI client not connected".into()))?;

    let request = build_analyze_request(&state.db, auth.user_id).await?;
    let started = std::time::Instant::now();
    let result = ai.analyze_performance(request).await;
    let model_version = result.as_ref().ok().map(|r| r.model_version.clone());
    crate::services::ai_log::record(
        &state.db,
        "AnalyzePerformance",
        None,
        Some(auth.user_id),
        started.elapsed(),
        &result,
        model_version.as_deref(),
    )
    .await;
    let resp = result.map_err(|s| AppError::Internal(format!("analyze_performance gRPC: {s}")))?;

    let payload = json!({
        "user_id": auth.user_id,
        "overall_score": resp.overall_score,
        "strengths": resp.strengths.iter().map(|s| json!({
            "skill_slug": s.skill_slug,
            "evidence_count": s.evidence_count,
            "wpc_total": s.wpc_total,
        })).collect::<Vec<_>>(),
        "gaps": resp.gaps.iter().map(|g| json!({
            "skill_slug": g.skill_slug,
            "importance": g.importance,
            "reason": g.reason,
        })).collect::<Vec<_>>(),
        "next_actions": resp.next_actions.iter().map(|a| json!({
            "action_type": a.action_type,
            "target_slug": a.target_slug,
            "priority": a.priority,
        })).collect::<Vec<_>>(),
        "rank_readiness": resp.rank_readiness.as_ref().map(|r| json!({
            "current_rank": r.current_rank,
            "next_rank": r.next_rank,
            "missing_criteria": r.missing_criteria.iter().map(|m| json!({
                "criterion": m.criterion,
                "current_value": m.current_value,
                "threshold": m.threshold,
            })).collect::<Vec<_>>(),
            "estimated_days_to_promotion": r.estimated_days_to_promotion,
        })),
        "model_version": resp.model_version,
        "generated_at": chrono::Utc::now().to_rfc3339(),
    });

    // Cache 24h.
    let payload_str = serde_json::to_string(&payload).unwrap_or_default();
    let _: Result<(), _> = redis.set_ex(&cache_key, &payload_str, 86400).await;

    metrics::counter!("skilluv_ai_performance_calls_total").increment(1);

    Ok(Json(json!({ "data": payload, "cached": false })))
}

async fn build_analyze_request(
    db: &sqlx::PgPool,
    user_id: Uuid,
) -> Result<crate::grpc::proto::AnalyzePerformanceRequest, AppError> {
    // 1. Deliverables récents (top 50, verified).
    let deliverables: Vec<(
        Uuid,
        String,
        i32,
        chrono::DateTime<chrono::Utc>,
        String,
        i16,
    )> = sqlx::query_as(
        r#"
        SELECT d.id,
               COALESCE(sn.slug, ''),
               COALESCE(us.weighted_proven_count, 0),
               d.verified_at,
               COALESCE(d.verifiable_by, 'human_review'),
               COALESCE(ct.difficulty, 3)
        FROM deliverables d
        LEFT JOIN challenge_templates ct ON ct.id = d.challenge_id
        LEFT JOIN slice_skills ss ON ss.slice_id = d.slice_id
        LEFT JOIN skill_nodes sn ON sn.id = ss.skill_id
        LEFT JOIN user_skills us ON us.user_id = d.user_id AND us.skill_id = sn.id
        WHERE d.user_id = $1 AND d.verification_status = 'verified'
        ORDER BY d.verified_at DESC
        LIMIT 50
        "#,
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    let deliverable_snapshots = deliverables
        .into_iter()
        .map(
            |(id, slug, wpc, va, vb, diff)| crate::grpc::proto::DeliverableSnapshot {
                deliverable_id: id.to_string(),
                skill_slug: slug,
                wpc,
                verified_at: va.to_rfc3339(),
                verifiable_by: vb,
                difficulty: diff as i32,
            },
        )
        .collect();

    // 2. Skills agrégés.
    let skills: Vec<AiCoachRow179> = sqlx::query_as(
        r#"
        SELECT sn.slug,
               us.weighted_proven_count,
               us.proven_count,
               COALESCE(us.first_proven_at, NOW()),
               COALESCE(us.last_proven_at, NOW())
        FROM user_skills us
        JOIN skill_nodes sn ON sn.id = us.skill_id
        WHERE us.user_id = $1
        "#,
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    let skill_snapshots = skills
        .into_iter()
        .map(
            |(slug, wpc, cnt, first, last)| crate::grpc::proto::SkillSnapshot {
                skill_slug: slug,
                wpc_total: wpc,
                evidence_count: cnt,
                first_evidence_at: first.to_rfc3339(),
                last_evidence_at: last.to_rfc3339(),
            },
        )
        .collect();

    // 3. Orientations actives.
    let orientations: Vec<(String, String)> = sqlx::query_as(
        r#"
        SELECT o.slug, uo.mode
        FROM user_orientations uo
        JOIN orientations o ON o.id = uo.orientation_id
        WHERE uo.user_id = $1 AND uo.ended_at IS NULL
        "#,
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    let orientation_snapshots = orientations
        .into_iter()
        .map(|(slug, mode)| crate::grpc::proto::OrientationSnapshot {
            orientation_slug: slug,
            // Le backend n'a pas encore de completion_ratio par orientation
            // (P16 posait la base, ratio à calculer). Placeholder 0.5 =
            // "en cours", l'IA se débrouille avec les autres signaux.
            completion_ratio: if mode == "active" { 1.0 } else { 0.5 },
        })
        .collect();

    // 4. Rank courant.
    let current_rank: String = sqlx::query_scalar("SELECT rank FROM user_ranks WHERE user_id = $1")
        .bind(user_id)
        .fetch_optional(db)
        .await?
        .unwrap_or_else(|| "apprenti".to_string());

    Ok(crate::grpc::proto::AnalyzePerformanceRequest {
        user_id: user_id.to_string(),
        deliverables: deliverable_snapshots,
        skills: skill_snapshots,
        orientations: orientation_snapshots,
        current_rank,
    })
}

// ═══════════════════════════════════════════════════════════════════
// IA-C.3 — POST /users/me/orientations/suggest
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct SuggestBody {
    #[serde(default)]
    target_market: Option<String>, // 'africa' | 'international'
    #[serde(default)]
    max_suggestions: Option<i32>,
    #[serde(default)]
    refresh: bool,
}

async fn suggest_orientations(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<SuggestBody>,
) -> Result<Json<Value>, AppError> {
    let cache_key = format!("ai:career:{}", auth.user_id);
    let mut redis = state.redis.clone();

    if body.refresh {
        // Rate-limit refresh 1/heure (Haiku est moins cher, mais on cap quand même).
        crate::middleware::RateLimiter::check(
            &mut redis,
            "ai_career_refresh",
            &auth.user_id.to_string(),
            1,
            3600,
        )
        .await?;
        let _: () = redis.del(&cache_key).await?;
    }

    // Cache hit (TTL 7j).
    let cached: Option<String> = redis.get(&cache_key).await.ok();
    if let Some(json_str) = cached
        && let Ok(v) = serde_json::from_str::<Value>(&json_str)
    {
        return Ok(Json(json!({ "data": v, "cached": true })));
    }

    let ai = state
        .ai
        .as_deref()
        .ok_or_else(|| AppError::Internal("AI client not connected".into()))?;

    // Agrège skills user + langues.
    let skills: Vec<AiCoachRow302> = sqlx::query_as(
        r#"
        SELECT sn.slug,
               us.weighted_proven_count,
               us.proven_count,
               COALESCE(us.first_proven_at, NOW()),
               COALESCE(us.last_proven_at, NOW())
        FROM user_skills us
        JOIN skill_nodes sn ON sn.id = us.skill_id
        WHERE us.user_id = $1
        "#,
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await?;

    let skill_snapshots: Vec<_> = skills
        .into_iter()
        .map(
            |(slug, wpc, cnt, first, last)| crate::grpc::proto::SkillSnapshot {
                skill_slug: slug,
                wpc_total: wpc,
                evidence_count: cnt,
                first_evidence_at: first.to_rfc3339(),
                last_evidence_at: last.to_rfc3339(),
            },
        )
        .collect();

    // Langues du user (via user_orientations.working_languages ou default).
    let working_languages: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT DISTINCT lang
        FROM user_orientations uo, UNNEST(uo.working_languages) AS lang
        WHERE uo.user_id = $1 AND uo.ended_at IS NULL
        "#,
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await?;
    let working_languages = if working_languages.is_empty() {
        vec!["fr".to_string(), "en".to_string()]
    } else {
        working_languages
    };

    let request = crate::grpc::proto::CareerPathRequest {
        user_id: auth.user_id.to_string(),
        skills: skill_snapshots,
        working_languages,
        target_market: body
            .target_market
            .clone()
            .unwrap_or_else(|| "international".into()),
        max_suggestions: body.max_suggestions.unwrap_or(3).clamp(1, 10),
    };
    let started = std::time::Instant::now();
    let result = ai.suggest_career_path(request).await;
    let model_version = result.as_ref().ok().map(|r| r.model_version.clone());
    crate::services::ai_log::record(
        &state.db,
        "SuggestCareerPath",
        None,
        Some(auth.user_id),
        started.elapsed(),
        &result,
        model_version.as_deref(),
    )
    .await;
    let resp = result.map_err(|s| AppError::Internal(format!("suggest_career_path gRPC: {s}")))?;

    // Validation : chaque orientation_slug retourné doit exister dans notre catalogue
    // (défense en profondeur — le catalogue IA peut diverger, voir doc §6.3).
    let known: std::collections::HashSet<String> = sqlx::query_scalar(
        "SELECT slug FROM orientations WHERE is_curated = TRUE AND is_archived = FALSE",
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default()
    .into_iter()
    .collect();

    let validated_suggestions: Vec<_> = resp
        .suggestions
        .iter()
        .filter(|s| known.contains(&s.orientation_slug))
        .map(|s| {
            json!({
                "orientation_slug": s.orientation_slug,
                "confidence": s.confidence,
                "match_reason": s.match_reason,
                "required_skills_missing": s.required_skills_missing,
                "transition_effort": s.transition_effort,
                "timeline_estimate_months": s.timeline_estimate_months,
            })
        })
        .collect();
    let primary = if known.contains(&resp.primary_recommendation) {
        Some(resp.primary_recommendation.clone())
    } else {
        None
    };

    let payload = json!({
        "user_id": auth.user_id,
        "suggestions": validated_suggestions,
        "primary_recommendation": primary,
        "secondary_recommendations": resp
            .secondary_recommendations
            .iter()
            .filter(|s| known.contains(*s))
            .cloned()
            .collect::<Vec<_>>(),
        "model_version": resp.model_version,
        "generated_at": chrono::Utc::now().to_rfc3339(),
    });

    // Cache 7j (Haiku peu cher + mais suggestions stables pour l'user).
    let payload_str = serde_json::to_string(&payload).unwrap_or_default();
    let _: Result<(), _> = redis.set_ex(&cache_key, &payload_str, 7 * 86400).await;

    metrics::counter!("skilluv_ai_career_suggestions_total").increment(1);

    Ok(Json(json!({ "data": payload, "cached": false })))
}
