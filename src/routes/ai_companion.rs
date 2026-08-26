//! SKI-44 (Post-MVP T3-01) — disclosed AI learning companion.
//!
//! Endpoints:
//!   POST /api/assistant/ask                  (auth)
//!   GET  /api/users/me/assistant-interactions (auth — my disclosure ledger)
//!   GET  /api/users/me/assistant-quota       (auth)
//!
//! Order of operations in `ask` is deliberate:
//!
//!   1. validate — reject a malformed request before spending anything;
//!   2. cache lookup — a hit costs nothing and consumes no quota;
//!   3. burst limit, then daily quota — cheap check before expensive one;
//!   4. gRPC call;
//!   5. record the interaction, always, including on failure.
//!
//! Step 5 is the one that matters: an AI interaction that is not recorded
//! is an undisclosed one, and disclosure is the entire justification for
//! shipping this feature at all.

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use redis::AsyncCommands;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::{AuthUser, RateLimiter};
use crate::services::{ai_companion, ranks};

pub fn ai_companion_routes() -> Router<AppState> {
    Router::new()
        .route("/assistant/ask", post(ask))
        .route("/users/me/assistant-interactions", get(list_interactions))
        .route("/users/me/assistant-quota", get(quota))
}

fn wrap(data: serde_json::Value) -> serde_json::Value {
    json!({
        "data": data,
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AssistantAskBody)]
pub struct AskBody {
    /// `explain` | `generate_exercises` | `pre_review` | `debug_help`
    pub interaction_type: String,
    pub prompt: String,
    /// Code the question is about. Required in practice for `pre_review`
    /// and `debug_help`; the worker decides what to do without it.
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub skill_slug: Option<String>,
    /// Answer language. Defaults to `fr`.
    #[serde(default)]
    pub locale: Option<String>,
}

/// Ask the assistant. Every exchange lands in the caller's own
/// disclosure ledger, which is the point of routing it through here.
#[utoipa::path(
    post, path = "/api/assistant/ask", tag = "ai",
    request_body = AskBody,
    responses(
        (status = 200, description = "The assistant answered, and the exchange was recorded"),
        (status = 503, description = "No assistant worker is reachable", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn ask(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<AskBody>,
) -> Result<impl IntoResponse, AppError> {
    ai_companion::validate_interaction_type(&body.interaction_type)?;

    let prompt = body.prompt.trim();
    if prompt.is_empty() {
        return Err(AppError::Validation("prompt must not be empty".into()));
    }
    if prompt.chars().count() > ai_companion::MAX_PROMPT_CHARS {
        return Err(AppError::Validation(format!(
            "prompt must be at most {} characters",
            ai_companion::MAX_PROMPT_CHARS
        )));
    }
    let code = body.code.unwrap_or_default();
    if code.chars().count() > ai_companion::MAX_CODE_CHARS {
        return Err(AppError::Validation(format!(
            "code must be at most {} characters",
            ai_companion::MAX_CODE_CHARS
        )));
    }
    let language = body.language.unwrap_or_default();
    let skill_slug = body.skill_slug.unwrap_or_default();
    let locale = body.locale.unwrap_or_else(|| "fr".to_string());

    let hash = ai_companion::request_hash(
        &body.interaction_type,
        prompt,
        &code,
        &language,
        &skill_slug,
        &locale,
    );
    let mut redis = state.redis.clone();

    // Cache hit: no LLM call, no quota consumed. The interaction is still
    // recorded — the learner received the help either way, so it is still
    // disclosable.
    let cached: Option<String> = redis.get(ai_companion::cache_key(&hash)).await.ok();
    if let Some(raw) = cached
        && let Some((answer, items, label, model)) = ai_companion::decode_cache(&raw)
    {
        let interaction = ai_companion::record(
            &state.db,
            auth.user_id,
            ai_companion::RecordParams {
                skill_slug: (!skill_slug.is_empty()).then_some(skill_slug.as_str()),
                disclosure_label: &label,
                model_version: model.as_deref(),
                request_hash: Some(&hash),
                cached: true,
                ..ai_companion::RecordParams::call(&body.interaction_type, prompt)
            },
        )
        .await?;

        let used = ai_companion::used_today(&state.db, auth.user_id).await?;
        metrics::counter!("skilluv_ai_companion_cache_hits_total").increment(1);

        return Ok(Json(wrap(json!(ai_companion::CompanionAnswer {
            interaction_id: interaction.id,
            answer_markdown: answer,
            items,
            disclosure_label: label,
            model_version: model,
            cached: true,
            quota_remaining: (ai_companion::DAILY_QUOTA - used).max(0),
        }))));
    }

    // Burst limit first — cheapest rejection.
    //
    // Both guard rails record the refusal before returning (SKI-298). A
    // rate-limited row does not count towards the quota (`used_today`
    // filters on `status = 'ok'`) and is never attached to a deliverable
    // (`disclose_on_deliverable` does the same), so the only thing it
    // changes is that "how often does the limit actually fire" now has an
    // answer. Without it, the one number that says whether 10/day is the
    // right number was unobservable.
    if let Err(err) = RateLimiter::check(
        &mut redis,
        "ai_companion_burst",
        &auth.user_id.to_string(),
        ai_companion::BURST_MAX,
        ai_companion::BURST_WINDOW_SECS,
    )
    .await
    {
        record_refusal(
            &state,
            auth.user_id,
            &body.interaction_type,
            prompt,
            (!skill_slug.is_empty()).then_some(skill_slug.as_str()),
            &hash,
            "burst",
        )
        .await?;
        return Err(err);
    }

    // Daily quota, counted from the durable ledger rather than Redis, so a
    // cache flush cannot hand out a fresh allowance.
    let used = ai_companion::used_today(&state.db, auth.user_id).await?;
    if used >= ai_companion::DAILY_QUOTA {
        record_refusal(
            &state,
            auth.user_id,
            &body.interaction_type,
            prompt,
            (!skill_slug.is_empty()).then_some(skill_slug.as_str()),
            &hash,
            "daily_quota",
        )
        .await?;
        return Err(AppError::Validation(format!(
            "daily AI companion quota reached ({} per 24h) — it resets on a rolling window",
            ai_companion::DAILY_QUOTA
        )));
    }

    let Some(ai) = state.ai.as_deref() else {
        // Recorded even though nothing happened: "the learner asked and got
        // nothing" is operationally useful, and keeps the ledger honest.
        ai_companion::record(
            &state.db,
            auth.user_id,
            ai_companion::RecordParams {
                skill_slug: (!skill_slug.is_empty()).then_some(skill_slug.as_str()),
                status: "unavailable",
                request_hash: Some(&hash),
                ..ai_companion::RecordParams::call(&body.interaction_type, prompt)
            },
        )
        .await?;
        return Err(AppError::ServiceUnavailable(
            "the AI companion is not available right now".into(),
        ));
    };

    let user_rank = ranks::effective_rank(&state.db, auth.user_id).await?;
    let request = crate::grpc::proto::CompanionRequest {
        user_id: auth.user_id.to_string(),
        interaction_type: body.interaction_type.clone(),
        prompt: prompt.to_string(),
        code: code.clone(),
        language: language.clone(),
        skill_slug: skill_slug.clone(),
        user_rank,
        locale: locale.clone(),
    };

    let started = std::time::Instant::now();
    let result = ai.companion_ask(request).await;
    let model_version = result.as_ref().ok().map(|r| r.model_version.clone());

    // Operational telemetry, alongside the disclosure ledger. Different
    // questions, both worth answering.
    crate::services::ai_log::record(
        &state.db,
        "CompanionAsk",
        None,
        Some(auth.user_id),
        started.elapsed(),
        &result,
        model_version.as_deref(),
    )
    .await;

    let resp = match result {
        Ok(r) => r,
        Err(status) => {
            // `Unimplemented` means the worker is running but has not
            // shipped this RPC yet — the deployment state this ticket
            // explicitly lists as a prerequisite. Reported as 503, same as
            // an outage, because from the caller's side it is one.
            let recorded_status = match status.code() {
                tonic::Code::Unimplemented | tonic::Code::Unavailable => "unavailable",
                _ => "error",
            };
            ai_companion::record(
                &state.db,
                auth.user_id,
                ai_companion::RecordParams {
                    skill_slug: (!skill_slug.is_empty()).then_some(skill_slug.as_str()),
                    status: recorded_status,
                    request_hash: Some(&hash),
                    ..ai_companion::RecordParams::call(&body.interaction_type, prompt)
                },
            )
            .await?;
            return Err(AppError::ServiceUnavailable(
                "the AI companion is not available right now".into(),
            ));
        }
    };

    let items: Vec<ai_companion::CompanionItem> = resp
        .items
        .iter()
        .map(|i| ai_companion::CompanionItem {
            title: i.title.clone(),
            body_markdown: i.body_markdown.clone(),
            kind: i.kind.clone(),
            priority: i.priority,
        })
        .collect();

    // A worker that returns no label still produces a disclosed
    // interaction — we supply the default rather than storing an empty
    // disclosure.
    let disclosure_label = if resp.disclosure_label.trim().is_empty() {
        format!(
            "Assistance IA Skilluv ({}) utilisée pendant la préparation de ce travail.",
            body.interaction_type
        )
    } else {
        resp.disclosure_label.clone()
    };

    let interaction = ai_companion::record(
        &state.db,
        auth.user_id,
        ai_companion::RecordParams {
            skill_slug: (!skill_slug.is_empty()).then_some(skill_slug.as_str()),
            disclosure_label: &disclosure_label,
            model_version: model_version.as_deref(),
            tokens_used: resp.tokens_used,
            request_hash: Some(&hash),
            ..ai_companion::RecordParams::call(&body.interaction_type, prompt)
        },
    )
    .await?;

    // Cache last: only a complete, successful, recorded exchange is worth
    // replaying.
    let encoded = ai_companion::encode_cache(
        &resp.answer_markdown,
        &items,
        &disclosure_label,
        model_version.as_deref(),
    );
    let _: Result<(), _> = redis
        .set_ex(
            ai_companion::cache_key(&hash),
            &encoded,
            ai_companion::CACHE_TTL_SECS,
        )
        .await;

    metrics::counter!(
        "skilluv_ai_companion_calls_total",
        "interaction_type" => body.interaction_type.clone(),
    )
    .increment(1);
    metrics::counter!("skilluv_ai_companion_tokens_total")
        .increment(resp.tokens_used.max(0) as u64);

    Ok(Json(wrap(json!(ai_companion::CompanionAnswer {
        interaction_id: interaction.id,
        answer_markdown: resp.answer_markdown,
        items,
        disclosure_label,
        model_version,
        cached: false,
        quota_remaining: (ai_companion::DAILY_QUOTA - used - 1).max(0),
    }))))
}

/// Write a refused request into the ledger (SKI-298).
///
/// Kept out of `ask` so the two guard rails cannot drift apart: the whole
/// value of the metric is that "refused by burst" and "refused by quota"
/// are counted the same way.
#[allow(clippy::too_many_arguments)]
async fn record_refusal(
    state: &AppState,
    user_id: Uuid,
    interaction_type: &str,
    prompt: &str,
    skill_slug: Option<&str>,
    hash: &str,
    refusal_kind: &str,
) -> Result<(), AppError> {
    ai_companion::record(
        &state.db,
        user_id,
        ai_companion::RecordParams {
            skill_slug,
            status: "rate_limited",
            request_hash: Some(hash),
            refusal_kind: Some(refusal_kind),
            ..ai_companion::RecordParams::call(interaction_type, prompt)
        },
    )
    .await?;
    metrics::counter!(
        "skilluv_ai_companion_refusals_total",
        "kind" => refusal_kind.to_string(),
    )
    .increment(1);
    Ok(())
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct ListInteractionsQuery {
    #[serde(default)]
    pub limit: Option<i64>,
    /// Only interactions not yet attached to a deliverable.
    #[serde(default)]
    pub undisclosed_only: bool,
}

/// The caller's own disclosure ledger.
///
/// Readable by the user themselves so the disclosure is something they can
/// inspect, not something done to them.
#[utoipa::path(
    get, path = "/api/users/me/assistant-interactions", tag = "ai",
    params(ListInteractionsQuery),
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn list_interactions(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<ListInteractionsQuery>,
) -> Result<impl IntoResponse, AppError> {
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let interactions: Vec<ai_companion::AiInteraction> = sqlx::query_as(
        r#"
        SELECT * FROM ai_interactions
         WHERE user_id = $1
           AND (NOT $2::BOOLEAN OR disclosed_on_deliverable_id IS NULL)
         ORDER BY created_at DESC
         LIMIT $3
        "#,
    )
    .bind(auth.user_id)
    .bind(q.undisclosed_only)
    .bind(limit)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(wrap(json!({ "interactions": interactions }))))
}

/// What is left of the caller's assistant allowance for the current window.
#[utoipa::path(
    get, path = "/api/users/me/assistant-quota", tag = "ai",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn quota(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<impl IntoResponse, AppError> {
    let used = ai_companion::used_today(&state.db, auth.user_id).await?;
    Ok(Json(wrap(json!({
        "daily_quota": ai_companion::DAILY_QUOTA,
        "used": used,
        "remaining": (ai_companion::DAILY_QUOTA - used).max(0),
        "disclosure_window_days": ai_companion::DISCLOSURE_WINDOW_DAYS,
    }))))
}

// ═══════════════════════════════════════════════════════════════════
// SKI-298 (T3-01b) — admin projection
// ═══════════════════════════════════════════════════════════════════
//
// Two surfaces, two different questions:
//
//   * `GET /api/admin/assistant/stats` answers "what is this costing us
//     and are the guard rails holding" — aggregates only, no prompt text.
//   * `GET /api/admin/users/{id}/assistant-interactions` answers "was this
//     specific piece of work assisted" — the disclosure ledger of one user,
//     which is exactly what instructing a disclosure dispute requires, and
//     which nothing but the user themselves could read before.
//
// The split matters: the first is an operations dashboard anyone with
// `admin` reads daily, the second is reading someone's questions. Merging
// them would have made the cheap one carry the cost of the expensive one.

/// Mounted behind `admin_gate` in `lib.rs`.
pub fn admin_ai_companion_routes() -> Router<AppState> {
    Router::new()
        .route("/admin/assistant/stats", get(admin_stats))
        .route(
            "/admin/users/{user_id}/assistant-interactions",
            get(admin_user_interactions),
        )
}

#[derive(Debug, Deserialize)]
pub struct AdminStatsQuery {
    /// Sliding window, in days. Defaults to 30, capped at a year.
    #[serde(default)]
    pub window_days: Option<i64>,
    /// How many consumers the top list carries. Defaults to 10.
    #[serde(default)]
    pub top: Option<i64>,
}

async fn admin_stats(
    _gate: crate::middleware::admin_gate::AdminGate,
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<AdminStatsQuery>,
) -> Result<impl IntoResponse, AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await?;

    let stats =
        ai_companion::admin_stats(&state.db, q.window_days.unwrap_or(30), q.top.unwrap_or(10))
            .await?;

    Ok(Json(wrap(json!({
        "stats": stats,
        // Echoed so the dashboard can render "3 / 10 used" style copy
        // without hardcoding the policy it is reporting on.
        "policy": {
            "daily_quota": ai_companion::DAILY_QUOTA,
            "burst_max": ai_companion::BURST_MAX,
            "burst_window_secs": ai_companion::BURST_WINDOW_SECS,
            "cache_ttl_secs": ai_companion::CACHE_TTL_SECS,
        },
    }))))
}

#[derive(Debug, Deserialize)]
pub struct AdminLedgerQuery {
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
    /// Only interactions not yet attached to a deliverable.
    #[serde(default)]
    pub undisclosed_only: bool,
}

/// One user's disclosure ledger, read as an admin.
///
/// Mirrors `GET /api/users/me/assistant-interactions` field for field, so
/// the admin panel and the learner are looking at the same object and a
/// dispute cannot turn on the two views disagreeing.
async fn admin_user_interactions(
    _gate: crate::middleware::admin_gate::AdminGate,
    State(state): State<AppState>,
    auth: AuthUser,
    Path(user_id): Path<Uuid>,
    Query(q): Query<AdminLedgerQuery>,
) -> Result<impl IntoResponse, AppError> {
    crate::middleware::capabilities::require_capability(&state.db, auth.user_id, "admin").await?;

    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let offset = q.offset.unwrap_or(0).max(0);

    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE id = $1)")
        .bind(user_id)
        .fetch_one(&state.db)
        .await?;
    if !exists {
        return Err(AppError::NotFound(format!("user {user_id} not found")));
    }

    let interactions: Vec<ai_companion::AiInteraction> = sqlx::query_as(
        r#"
        SELECT * FROM ai_interactions
         WHERE user_id = $1
           AND (NOT $2::BOOLEAN OR disclosed_on_deliverable_id IS NULL)
         ORDER BY created_at DESC
         LIMIT $3 OFFSET $4
        "#,
    )
    .bind(user_id)
    .bind(q.undisclosed_only)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ai_interactions WHERE user_id = $1")
        .bind(user_id)
        .fetch_one(&state.db)
        .await?;

    Ok(Json(wrap(json!({
        "user_id": user_id,
        "interactions": interactions,
        "total": total,
        "limit": limit,
        "offset": offset,
    }))))
}
