//! Reported vulnerabilities, seen from the side that decides.
//!
//! ## Three different people, not one "admin"
//!
//! The word `admin` in the path is about which surface this is, not about who
//! may reach it. Three roles work here and they are deliberately unequal:
//!
//!   * **`security_triager`** — reads the incoming queue and decides what is
//!     worth a reviewer's afternoon. High volume, mostly refusals. May not
//!     confirm anything.
//!   * **`security_reviewer:{family}`** — reproduces, confirms, argues
//!     severity, opens rounds, rules duplicates. The judgement.
//!   * **`admin`** — publishes, withholds, grants an extension, curates the
//!     catalogue. The decisions that are irreversible or that commit the
//!     platform to something.
//!
//! Migration 0557 explains why triage is a capability of its own rather than
//! the bottom of the reviewer ladder.
//!
//! ## Why publication is the only thing an administrator alone can do
//!
//! Because the internet keeps a copy. Every other transition here can be
//! corrected by making another one; publishing cannot, and the person doing it
//! should be somebody who answers for the platform rather than for one finding.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use utoipa::IntoParams;
use uuid::Uuid;

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::middleware::capabilities::require_any_capability;
use crate::services::{
    security_external_bounties, security_findings, security_lab_generator, security_research,
};

pub fn admin_security_routes() -> Router<AppState> {
    Router::new()
        .route("/admin/security/findings", get(queue))
        .route("/admin/security/findings/{id}", get(detail))
        .route("/admin/security/findings/{id}/transition", post(transition))
        .route("/admin/security/findings/{id}/severity", post(severity))
        .route("/admin/security/findings/{id}/rounds", post(open_round))
        .route(
            "/admin/security/findings/{id}/rounds/resolve",
            post(resolve_round),
        )
        .route(
            "/admin/security/findings/{id}/vendor-notified",
            post(vendor_notified),
        )
        .route(
            "/admin/security/findings/{id}/extension",
            post(request_extension),
        )
        .route(
            "/admin/security/findings/{id}/extension/grant",
            post(grant_extension),
        )
        .route("/admin/security/findings/{id}/withhold", post(withhold))
        .route("/admin/security/findings/{id}/rescan", post(rescan))
        .route("/admin/security/dedup-queue", get(dedup_queue))
        .route("/admin/security/embargo-sweep", post(embargo_sweep))
        .route("/admin/security/challenges", post(create_challenge))
        .route(
            "/admin/security/external-bounties",
            get(list_bounties).post(curate_bounty),
        )
        .route(
            "/admin/security/research-tokens/{id}/revoke",
            post(revoke_token),
        )
        .route(
            "/admin/security/findings/{id}/blue-lab",
            post(lab_from_finding),
        )
        .route("/admin/security/bounty-claims", get(bounty_claims))
        .route(
            "/admin/security/bounty-claims/{id}/verify",
            post(verify_bounty_claim),
        )
        .route(
            "/admin/security/bounty-claims/{id}/refuse",
            post(refuse_bounty_claim),
        )
}

// ═══════════════════════════════════════════════════════════════════
// Who is asking
// ═══════════════════════════════════════════════════════════════════

/// Anybody who works on findings: triager, reviewer of any family, curator of
/// the domain, or administrator.
async fn require_reader(state: &AppState, auth: &AuthUser) -> Result<(), AppError> {
    if require_any_capability(
        &state.db,
        auth.user_id,
        &[
            "admin",
            "security_triager",
            "security_reviewer:all",
            "domain_curator:security",
            "domain_curator:all",
            "challenge_validator:security",
        ],
    )
    .await
    .is_ok()
    {
        return Ok(());
    }

    // Or a reviewer of any one security family. Checked separately because the
    // families are derived rows and listing them here would be a copy that
    // drifts every time an orientation is added.
    let family: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM user_capabilities
              WHERE user_id = $1 AND capability LIKE 'security_reviewer:%'
                AND revoked_at IS NULL
                AND (expires_at IS NULL OR expires_at > NOW()))",
    )
    .bind(auth.user_id)
    .fetch_one(&state.db)
    .await?;

    if family { Ok(()) } else { Err(AppError::Forbidden) }
}

/// Which actor this person is, taking the strongest they hold.
///
/// Returned rather than checked, because the state machine in
/// `services::security_findings` is what decides what each actor may do — and
/// keeping that decision in one place is the reason it is a table there rather
/// than a series of `if` statements here.
async fn actor_for(
    state: &AppState,
    auth: &AuthUser,
    finding_id: Uuid,
) -> Result<security_findings::Actor, AppError> {
    use security_findings::Actor;

    if require_any_capability(&state.db, auth.user_id, &["admin"])
        .await
        .is_ok()
    {
        return Ok(Actor::Admin);
    }

    let reviewer: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM user_capabilities
              WHERE user_id = $1
                AND (capability LIKE 'security_reviewer:%'
                     OR capability = 'challenge_validator:security')
                AND revoked_at IS NULL
                AND (expires_at IS NULL OR expires_at > NOW()))",
    )
    .bind(auth.user_id)
    .fetch_one(&state.db)
    .await?;
    if reviewer {
        return Ok(Actor::Reviewer);
    }

    if require_any_capability(&state.db, auth.user_id, &["security_triager"])
        .await
        .is_ok()
    {
        return Ok(Actor::Triager);
    }

    // The reporter of this finding, if that is who they are. Last, because
    // somebody can be both and the stronger role is the useful one.
    let is_reporter: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM security_findings
                         WHERE id = $1 AND reporter_user_id = $2)",
    )
    .bind(finding_id)
    .bind(auth.user_id)
    .fetch_one(&state.db)
    .await?;
    if is_reporter {
        return Ok(Actor::Reporter);
    }

    Err(AppError::Forbidden)
}

// ═══════════════════════════════════════════════════════════════════
// The queue
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, IntoParams)]
#[serde(deny_unknown_fields)]
pub struct QueueQuery {
    #[param(nullable)]
    pub status: Option<String>,
    #[param(nullable)]
    pub severity: Option<String>,
    #[param(nullable)]
    pub target_kind: Option<String>,
    /// Only the ones a scanner thought resembled something else.
    #[serde(default)]
    pub suspected_duplicates: bool,
    #[param(nullable)]
    pub limit: Option<i64>,
}

/// The incoming queue.
///
/// Ordered by severity and then by age, which is the order somebody working
/// through it wants — not by arrival, which buries a critical filed on a
/// Friday under a week of informationals.
#[utoipa::path(
    get, path = "/api/admin/security/findings",
    operation_id = "adminSecurityQueue", tag = "admin",
    params(QueueQuery),
    responses(
        (status = 200, body = ApiResponse<serde_json::Value>),
        (status = 403, description = "Not a triager or reviewer", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn queue(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<QueueQuery>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require_reader(&state, &auth).await?;
    crate::validators::check_max_len_opt(&q.status, "status", 20)?;
    crate::validators::check_max_len_opt(&q.severity, "severity", 15)?;
    crate::validators::check_max_len_opt(&q.target_kind, "target_kind", 20)?;

    let findings: Vec<Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object(
                    'id', f.id, 'title', f.title, 'status', f.status,
                    'severity_tier', f.severity_tier,
                    'severity_reported_tier', f.severity_reported_tier,
                    'cvss_vector', f.cvss_vector, 'cvss_score', f.cvss_score,
                    'cwe_id', f.cwe_id,
                    'target_kind', f.target_kind, 'target_host', f.target_host,
                    'affected_endpoint', f.affected_endpoint,
                    'reporter', jsonb_build_object(
                        'username', u.username,
                        'anonymous', f.reporter_is_anonymous,
                        'rank', (SELECT r.rank FROM user_ranks r
                                  WHERE r.user_id = f.reporter_user_id)),
                    'triage_skipped_reason', f.triage_skipped_reason,
                    'dedup_state', f.dedup_state,
                    'similar_count', cardinality(f.similar_finding_ids),
                    'age_hours', round(EXTRACT(EPOCH FROM (NOW() - f.created_at)) / 3600),
                    'created_at', f.created_at,
                    'open_round', EXISTS (SELECT 1 FROM security_finding_rounds r
                                           WHERE r.finding_id = f.id
                                             AND r.resolved_at IS NULL))
           FROM security_findings f
           JOIN users u ON u.id = f.reporter_user_id
          WHERE ($1::TEXT IS NULL OR f.status = $1)
            AND ($2::TEXT IS NULL OR f.severity_tier = $2)
            AND ($3::TEXT IS NULL OR f.target_kind = $3)
            AND ($4::BOOLEAN IS FALSE OR f.dedup_state = 'suspected')
          ORDER BY CASE f.severity_tier
                       WHEN 'critical' THEN 5 WHEN 'high' THEN 4
                       WHEN 'medium' THEN 3 WHEN 'low' THEN 2 ELSE 1 END DESC,
                   f.created_at ASC
          LIMIT $5",
    )
    .bind(q.status.as_deref())
    .bind(q.severity.as_deref())
    .bind(q.target_kind.as_deref())
    .bind(q.suspected_duplicates)
    .bind(q.limit.unwrap_or(50).clamp(1, 200))
    .fetch_all(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(json!({ "findings": findings }))))
}

/// One finding, in full: the report, the history, the rounds, the look-alikes.
#[utoipa::path(
    get, path = "/api/admin/security/findings/{id}",
    operation_id = "adminSecurityDetail", tag = "admin",
    params(("id" = Uuid, Path, description = "Finding")),
    responses(
        (status = 200, body = ApiResponse<serde_json::Value>),
        (status = 404, description = "No such finding", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn detail(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require_reader(&state, &auth).await?;

    let finding: Option<Value> = sqlx::query_scalar(
        "SELECT to_jsonb(f) - 'reporter_user_id'
                || jsonb_build_object(
                       'reporter', jsonb_build_object(
                           'username', u.username,
                           'display_name', u.display_name,
                           'anonymous', f.reporter_is_anonymous,
                           'rank', (SELECT r.rank FROM user_ranks r
                                     WHERE r.user_id = f.reporter_user_id),
                           'confirmed_findings', (
                               SELECT count(*) FROM security_findings p
                                WHERE p.reporter_user_id = f.reporter_user_id
                                  AND p.status IN ('confirmed','fixed','published'))))
           FROM security_findings f
           JOIN users u ON u.id = f.reporter_user_id
          WHERE f.id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?;

    let Some(finding) = finding else {
        return Err(AppError::NotFound("no such finding".into()));
    };

    let events: Vec<Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object(
                    'event', e.event, 'from', e.from_status, 'to', e.to_status,
                    'reason', e.reason, 'detail', e.detail,
                    'at', e.occurred_at,
                    'actor', u.username)
           FROM security_finding_events e
           LEFT JOIN users u ON u.id = e.actor_user_id
          WHERE e.finding_id = $1
          ORDER BY e.occurred_at",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;

    let rounds: Vec<Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object(
                    'round_no', r.round_no, 'kind', r.kind, 'name', k.name,
                    'notes_md', r.notes_md, 'requested_at', r.requested_at,
                    'answer_md', r.answer_md, 'answered_at', r.answered_at,
                    'resolution', r.resolution, 'resolved_at', r.resolved_at)
           FROM security_finding_rounds r
           LEFT JOIN revision_round_kinds k ON k.slug = r.kind
          WHERE r.finding_id = $1
          ORDER BY r.round_no",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;

    let similar: Vec<Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object(
                    'id', s.id, 'title', s.title, 'status', s.status,
                    'severity_tier', s.severity_tier,
                    'created_at', s.created_at,
                    'score', sc)
           FROM security_findings f
           CROSS JOIN LATERAL unnest(f.similar_finding_ids, f.similarity_scores)
                              AS t(sid, sc)
           JOIN security_findings s ON s.id = t.sid
          WHERE f.id = $1
          ORDER BY sc DESC",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(
        ApiResponse::new(json!({
            "finding": finding,
            "events": events,
            "rounds": rounds,
            "similar": similar,
        }))
    ))
}

// ═══════════════════════════════════════════════════════════════════
// Deciding
// ═══════════════════════════════════════════════════════════════════

/// Move a finding along.
#[utoipa::path(
    post, path = "/api/admin/security/findings/{id}/transition", tag = "admin",
    params(("id" = Uuid, Path, description = "Finding")),
    request_body = security_findings::TransitionInput,
    responses(
        (status = 200, body = ApiResponse<serde_json::Value>),
        (status = 409, description = "Not a legal move for you", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn transition(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<security_findings::TransitionInput>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require_reader(&state, &auth).await?;
    let actor = actor_for(&state, &auth, id).await?;
    let reason = input.reason.clone();
    let status = security_findings::transition(&state.db, auth.user_id, actor, id, input).await?;

    // Tell the reporter. Transactional, so this is an obligation rather than a
    // nicety: a reporter who is never told whether their report was read does
    // not file a second one, and that is how a disclosure programme dies.
    if let Some(kind) = security_findings::notification_for(&status) {
        match security_findings::notifiable(&state.db, id).await {
            Ok(f) => {
                let _ = crate::services::notify::send(
                    &state,
                    crate::services::notify::Recipient::User(f.reporter_user_id),
                    kind,
                )
                .arg("title", f.title)
                .arg("severity", f.severity_tier)
                .arg("days", security_findings::TRIAGE_SLA_DAYS.to_string())
                .arg(
                    "reason",
                    reason.unwrap_or_else(|| "No reason was recorded.".to_string()),
                )
                .payload(json!({ "finding_id": id, "status": status }))
                .execute()
                .await;
            }
            Err(e) => tracing::warn!(finding = %id, error = %e,
                "a finding moved and its reporter was not told"),
        }
    }

    // The proof engine, after the fact: a confirmation may have earned a badge
    // or a rank, and the person who earned it should be told in the same
    // minute. Best-effort — a notification failure must not undo a decision.
    let db = state.db.clone();
    tokio::spawn(async move {
        let reporter: Option<Uuid> = sqlx::query_scalar(
            "SELECT reporter_user_id FROM security_findings WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&db)
        .await
        .ok()
        .flatten();
        if let Some(reporter) = reporter {
            if let Err(e) = crate::services::proof_hooks::recompute_all_for_user(&db, reporter).await
            {
                tracing::warn!(user = %reporter, error = %e,
                    "proof recompute after a finding transition failed");
            }
        }
    });

    Ok(Json(ApiResponse::new(json!({ "status": status }))))
}

/// Change a severity, with the argument written down.
#[utoipa::path(
    post, path = "/api/admin/security/findings/{id}/severity", tag = "admin",
    params(("id" = Uuid, Path, description = "Finding")),
    request_body = security_findings::SeverityOverride,
    responses(
        (status = 200, body = ApiResponse<serde_json::Value>),
        (status = 400, description = "No argument given", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn severity(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<security_findings::SeverityOverride>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    // A severity decides a payout tier, so a triager may not set one.
    let actor = actor_for(&state, &auth, id).await?;
    if !matches!(
        actor,
        security_findings::Actor::Reviewer | security_findings::Actor::Admin
    ) {
        return Err(AppError::Forbidden);
    }
    let before = security_findings::notifiable(&state.db, id)
        .await
        .map(|f| f.severity_tier)
        .unwrap_or_default();
    let reason = input.reason.clone();
    let tier = security_findings::override_severity(&state.db, auth.user_id, id, input).await?;

    // A severity decides a payout tier. Changing one without telling the
    // person is the thing researchers leave a platform over.
    if before != tier {
        if let Ok(f) = security_findings::notifiable(&state.db, id).await {
            let _ = crate::services::notify::send(
                &state,
                crate::services::notify::Recipient::User(f.reporter_user_id),
                "security.severity_changed",
            )
            .arg("title", f.title)
            .arg("before", before)
            .arg("after", tier.clone())
            .arg("reason", reason)
            .payload(json!({ "finding_id": id }))
            .execute()
            .await;
        }
    }

    Ok(Json(ApiResponse::new(json!({ "severity_tier": tier }))))
}

/// Ask the researcher for something.
#[utoipa::path(
    post, path = "/api/admin/security/findings/{id}/rounds", tag = "admin",
    params(("id" = Uuid, Path, description = "Finding")),
    request_body = security_findings::RoundRequest,
    responses(
        (status = 200, body = ApiResponse<serde_json::Value>),
        (status = 409, description = "A round is already open, or five have been used", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn open_round(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<security_findings::RoundRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require_reader(&state, &auth).await?;
    let asked_for = input.notes_md.clone();
    let round = security_findings::open_round(&state.db, auth.user_id, id, input).await?;

    // The round is a question, and a question nobody hears is a report that
    // times out for no reason the reporter could have known about.
    if let Ok(f) = security_findings::notifiable(&state.db, id).await {
        let _ = crate::services::notify::send(
            &state,
            crate::services::notify::Recipient::User(f.reporter_user_id),
            "security.finding_round",
        )
        .arg("title", f.title)
        .arg("reason", asked_for)
        .payload(json!({ "finding_id": id, "round_no": round }))
        .execute()
        .await;
    }

    Ok(Json(ApiResponse::new(json!({ "round_no": round }))))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RoundResolution {
    /// `satisfied` or `insufficient`.
    pub resolution: String,
    #[serde(default)]
    pub note: Option<String>,
}

/// Close the open round.
#[utoipa::path(
    post, path = "/api/admin/security/findings/{id}/rounds/resolve", tag = "admin",
    params(("id" = Uuid, Path, description = "Finding")),
    request_body = RoundResolution,
    responses((status = 200, body = ApiResponse<serde_json::Value>)),
    security(("cookie_auth" = [])),
)]
pub async fn resolve_round(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<RoundResolution>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require_reader(&state, &auth).await?;
    security_findings::resolve_round(
        &state.db,
        auth.user_id,
        id,
        &body.resolution,
        body.note.as_deref(),
    )
    .await?;
    Ok(Json(ApiResponse::new(json!({ "resolved": true }))))
}

/// Record that the owner of the system has been told.
#[utoipa::path(
    post, path = "/api/admin/security/findings/{id}/vendor-notified", tag = "admin",
    params(("id" = Uuid, Path, description = "Finding")),
    responses((status = 200, body = ApiResponse<serde_json::Value>)),
    security(("cookie_auth" = [])),
)]
pub async fn vendor_notified(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require_reader(&state, &auth).await?;
    security_findings::notify_vendor(&state.db, auth.user_id, id).await?;
    Ok(Json(ApiResponse::new(json!({ "notified": true }))))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ExtensionRequest {
    pub reason: String,
}

/// The owner asks for more time.
#[utoipa::path(
    post, path = "/api/admin/security/findings/{id}/extension", tag = "admin",
    params(("id" = Uuid, Path, description = "Finding")),
    request_body = ExtensionRequest,
    responses((status = 200, body = ApiResponse<serde_json::Value>)),
    security(("cookie_auth" = [])),
)]
pub async fn request_extension(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ExtensionRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require_reader(&state, &auth).await?;
    security_findings::request_extension(&state.db, auth.user_id, id, &body.reason).await?;
    Ok(Json(ApiResponse::new(json!({ "requested": true }))))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ExtensionGrant {
    pub days: i16,
}

/// Grant it, moving the clock.
///
/// Administrator only. An extension is the platform telling a researcher that
/// the promise it made them has changed, and the person doing that should be
/// the one who made the promise.
#[utoipa::path(
    post, path = "/api/admin/security/findings/{id}/extension/grant", tag = "admin",
    params(("id" = Uuid, Path, description = "Finding")),
    request_body = ExtensionGrant,
    responses(
        (status = 200, body = ApiResponse<serde_json::Value>),
        (status = 409, description = "Nothing was requested", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn grant_extension(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ExtensionGrant>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require_any_capability(&state.db, auth.user_id, &["admin"]).await?;
    security_findings::grant_extension(&state.db, auth.user_id, id, body.days).await?;
    Ok(Json(ApiResponse::new(json!({ "granted_days": body.days }))))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct WithholdInput {
    pub reason: String,
}

/// Decide this one is never published.
#[utoipa::path(
    post, path = "/api/admin/security/findings/{id}/withhold", tag = "admin",
    params(("id" = Uuid, Path, description = "Finding")),
    request_body = WithholdInput,
    responses((status = 200, body = ApiResponse<serde_json::Value>)),
    security(("cookie_auth" = [])),
)]
pub async fn withhold(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<WithholdInput>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require_any_capability(&state.db, auth.user_id, &["admin"]).await?;
    security_findings::withhold(&state.db, auth.user_id, id, &body.reason).await?;
    Ok(Json(ApiResponse::new(json!({ "withheld": true }))))
}

/// Look again for look-alikes.
#[utoipa::path(
    post, path = "/api/admin/security/findings/{id}/rescan", tag = "admin",
    params(("id" = Uuid, Path, description = "Finding")),
    responses((status = 200, body = ApiResponse<serde_json::Value>)),
    security(("cookie_auth" = [])),
)]
pub async fn rescan(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require_reader(&state, &auth).await?;
    let found = security_findings::scan_similar(&state.db, id).await?;
    Ok(Json(ApiResponse::new(json!({ "candidates": found }))))
}

/// Everything a scanner thought resembled something else.
#[utoipa::path(
    get, path = "/api/admin/security/dedup-queue", tag = "admin",
    responses((status = 200, body = ApiResponse<serde_json::Value>)),
    security(("cookie_auth" = [])),
)]
pub async fn dedup_queue(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require_reader(&state, &auth).await?;
    let pairs: Vec<Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object(
                    'id', f.id, 'title', f.title, 'created_at', f.created_at,
                    'severity_tier', f.severity_tier,
                    'candidates', (
                        SELECT jsonb_agg(jsonb_build_object(
                                   'id', s.id, 'title', s.title,
                                   'created_at', s.created_at, 'score', t.sc)
                               ORDER BY t.sc DESC)
                          FROM unnest(f.similar_finding_ids, f.similarity_scores)
                               AS t(sid, sc)
                          JOIN security_findings s ON s.id = t.sid))
           FROM security_findings f
          WHERE f.dedup_state = 'suspected'
            AND f.status NOT IN ('withdrawn', 'not_applicable')
          ORDER BY f.created_at
          LIMIT 100",
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(
        ApiResponse::new(json!({
            "pairs": pairs,
            "note": "Nothing here is merged automatically. A merge decides who \
                     is paid.",
        }))
    ))
}

/// Walk the embargo clocks now rather than waiting for the sweep.
#[utoipa::path(
    post, path = "/api/admin/security/embargo-sweep", tag = "admin",
    responses((status = 200, body = ApiResponse<serde_json::Value>)),
    security(("cookie_auth" = [])),
)]
pub async fn embargo_sweep(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require_any_capability(&state.db, auth.user_id, &["admin"]).await?;
    let sweep = security_findings::sweep_embargoes(&state.db).await?;
    Ok(Json(
        ApiResponse::new(json!({
            "expired": sweep.expired,
            "reminded": sweep.reminded,
            "note": "Nothing was published. An expired embargo is an item on \
                     this list, not an automatic disclosure.",
        }))
    ))
}

// ═══════════════════════════════════════════════════════════════════
// The catalogue
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NewChallenge {
    pub title: String,
    pub description: String,
    pub instructions: String,
    /// `ctf_flag` or `defensive_lab`. The write-up kinds are seeded and curated
    /// like every other domain's catalogue; these two are the ones that need a
    /// secret, which is why they are created rather than seeded.
    pub kind: String,
    pub difficulty: i16,
    pub difficulty_tier: String,
    pub reward_fragments: i32,
    #[serde(default)]
    pub duration_minutes: Option<i32>,
    /// The flag itself, for `ctf_flag`. Hashed here and never stored.
    #[serde(default)]
    pub flag: Option<String>,
    #[serde(default)]
    pub flag_format: Option<String>,
    #[serde(default)]
    pub target_url: Option<String>,
    /// For `defensive_lab`: the artefact key, and the questions with their
    /// answers in plaintext. The answers are hashed here and never stored.
    #[serde(default)]
    pub lab_artifact_key: Option<String>,
    #[serde(default)]
    pub lab_artifact_bytes: Option<i64>,
    #[serde(default)]
    pub questions: Vec<NewQuestion>,
    #[serde(default)]
    pub pass_percent: Option<i16>,
    #[serde(default)]
    pub max_attempts: Option<i16>,
    #[serde(default)]
    pub attribution_md: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NewQuestion {
    pub id: String,
    /// `text` or `choice`.
    pub kind: String,
    pub question: String,
    /// In plaintext, from whoever solved the lab. Hashed before it is stored.
    pub answer: String,
    #[serde(default)]
    pub choices: Vec<String>,
    #[serde(default)]
    pub hint: Option<String>,
    #[serde(default)]
    pub case_sensitive: bool,
}

/// Create a machine-graded challenge.
///
/// The one place a flag or a set of answers enters the system, and the reason
/// it is an endpoint rather than a migration: whoever creates it has to have
/// solved it, and a migration author guessing an answer ships a challenge
/// nobody can ever pass (0558 says this at length).
///
/// The secrets are hashed here. Nothing stores the plaintext, so this response
/// is the last time anybody sees it.
#[utoipa::path(
    post, path = "/api/admin/security/challenges",
    operation_id = "adminSecurityCreateChallenge", tag = "admin",
    request_body = NewChallenge,
    responses(
        (status = 200, body = ApiResponse<serde_json::Value>),
        (status = 400, description = "Missing what the kind needs", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn create_challenge(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<NewChallenge>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require_any_capability(
        &state.db,
        auth.user_id,
        &["admin", "domain_curator:security", "domain_curator:all"],
    )
    .await?;

    use sha2::{Digest, Sha256};
    let hash = |s: &str| {
        let mut h = Sha256::new();
        h.update(s.as_bytes());
        hex::encode(h.finalize())
    };

    if !matches!(input.kind.as_str(), "ctf_flag" | "defensive_lab") {
        return Err(AppError::Validation(
            "only ctf_flag and defensive_lab are created here. The kinds that \
             are graded by a person reading a write-up are seeded like every \
             other domain's catalogue"
                .into(),
        ));
    }

    let (flag_hash, questions_json) = match input.kind.as_str() {
        "ctf_flag" => {
            let flag = input.flag.as_deref().ok_or_else(|| {
                AppError::Validation("a flag challenge needs the flag".into())
            })?;
            if flag.trim().len() < 4 {
                return Err(AppError::Validation("that flag is too short".into()));
            }
            if input.flag_format.is_none() || input.target_url.is_none() {
                return Err(AppError::Validation(
                    "a flag challenge names its format and its target".into(),
                ));
            }
            (Some(hash(flag.trim())), None)
        }
        _ => {
            if input.questions.is_empty() {
                return Err(AppError::Validation("a lab needs questions".into()));
            }
            if input.lab_artifact_key.is_none() {
                return Err(AppError::Validation(
                    "a lab needs the artefact people will analyse".into(),
                ));
            }
            let questions: Vec<Value> = input
                .questions
                .iter()
                .map(|q| {
                    let normalised = if q.case_sensitive {
                        q.answer.trim().to_string()
                    } else {
                        q.answer.trim().to_lowercase()
                    };
                    json!({
                        "id": q.id,
                        "kind": q.kind,
                        "question": q.question,
                        "expected_answer_hash": hash(&normalised),
                        "choices": q.choices,
                        "hint": q.hint,
                        "case_sensitive": q.case_sensitive,
                    })
                })
                .collect();
            (None, Some(Value::Array(questions)))
        }
    };

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO challenge_templates (
             title, description, instructions, skill_domain, difficulty,
             status, is_training, ai_policy, created_by, duration_minutes,
             reward_fragments,
             security_kind, security_difficulty_tier,
             security_flag_hash, security_flag_format, security_target_url,
             security_lab_artifact_key, security_lab_artifact_bytes,
             security_lab_questions, security_lab_pass_percent,
             security_lab_max_attempts, security_attribution_md)
         VALUES ($1, $2, $3, 'security', $4,
                 'draft', TRUE, 'disclosure_required', $5, $6,
                 $7,
                 $8, $9,
                 $10, $11, $12,
                 $13, $14,
                 $15, $16,
                 $17, $18)
         RETURNING id",
    )
    .bind(input.title.trim())
    .bind(input.description.trim())
    .bind(input.instructions.trim())
    .bind(input.difficulty)
    .bind(auth.user_id)
    .bind(input.duration_minutes)
    .bind(input.reward_fragments)
    .bind(&input.kind)
    .bind(&input.difficulty_tier)
    .bind(flag_hash.as_deref())
    .bind(input.flag_format.as_deref())
    .bind(input.target_url.as_deref())
    .bind(input.lab_artifact_key.as_deref())
    .bind(input.lab_artifact_bytes)
    .bind(questions_json.as_ref())
    .bind(input.pass_percent.or(Some(80)))
    .bind(input.max_attempts.or(Some(3)))
    .bind(input.attribution_md.as_deref())
    .fetch_one(&state.db)
    .await?;

    Ok(Json(
        ApiResponse::new(json!({
            "id": id,
            "status": "draft",
            "note": "Created as a draft. Publish it once somebody other than \
                     you has solved it from the instructions alone.",
        }))
    ))
}

// ═══════════════════════════════════════════════════════════════════
// Curated programmes elsewhere
// ═══════════════════════════════════════════════════════════════════

/// Every curated programme, including the retired ones.
#[utoipa::path(
    get, path = "/api/admin/security/external-bounties",
    operation_id = "adminSecurityListBounties", tag = "admin",
    responses((status = 200, body = ApiResponse<serde_json::Value>)),
    security(("cookie_auth" = [])),
)]
pub async fn list_bounties(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require_reader(&state, &auth).await?;
    let rows: Vec<Value> = sqlx::query_scalar(
        "SELECT to_jsonb(b) FROM external_bounty_programs b
          ORDER BY b.is_active DESC, b.curated_at DESC LIMIT 500",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(ApiResponse::new(json!({ "programmes": rows }))))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CuratedBounty {
    pub platform: String,
    pub program_slug: String,
    pub program_url: String,
    pub organisation_name: String,
    #[serde(default)]
    pub scope_summary: Option<String>,
    #[serde(default)]
    pub skill_topics: Vec<String>,
    #[serde(default)]
    pub payout_range: Option<String>,
    #[serde(default = "yes")]
    pub pays_money: bool,
    #[serde(default)]
    pub discloses_reports: bool,
    #[serde(default = "yes")]
    pub is_active: bool,
    #[serde(default)]
    pub retired_reason: Option<String>,
}

fn yes() -> bool {
    true
}

/// Add a programme, or re-date one that was already there.
///
/// `curated_at` moves on every write, because the date is the whole claim: a
/// programme nobody has looked at for a year is shown with that date rather
/// than presented as current.
#[utoipa::path(
    post, path = "/api/admin/security/external-bounties", tag = "admin",
    request_body = CuratedBounty,
    responses((status = 200, body = ApiResponse<serde_json::Value>)),
    security(("cookie_auth" = [])),
)]
pub async fn curate_bounty(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<CuratedBounty>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require_any_capability(
        &state.db,
        auth.user_id,
        &["admin", "domain_curator:security", "domain_curator:all"],
    )
    .await?;

    if !input.program_url.starts_with("https://") {
        return Err(AppError::Validation(
            "a programme link has to be an https link".into(),
        ));
    }

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO external_bounty_programs
             (platform, program_slug, program_url, organisation_name,
              scope_summary, skill_topics, payout_range, pays_money,
              discloses_reports, is_active, retired_reason, curated_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
         ON CONFLICT (platform, program_slug) DO UPDATE SET
             program_url = EXCLUDED.program_url,
             organisation_name = EXCLUDED.organisation_name,
             scope_summary = EXCLUDED.scope_summary,
             skill_topics = EXCLUDED.skill_topics,
             payout_range = EXCLUDED.payout_range,
             pays_money = EXCLUDED.pays_money,
             discloses_reports = EXCLUDED.discloses_reports,
             is_active = EXCLUDED.is_active,
             retired_reason = EXCLUDED.retired_reason,
             curated_by = EXCLUDED.curated_by,
             curated_at = NOW(),
             updated_at = NOW()
         RETURNING id",
    )
    .bind(&input.platform)
    .bind(&input.program_slug)
    .bind(&input.program_url)
    .bind(&input.organisation_name)
    .bind(input.scope_summary.as_deref())
    .bind(&input.skill_topics)
    .bind(input.payout_range.as_deref())
    .bind(input.pays_money)
    .bind(input.discloses_reports)
    .bind(input.is_active)
    .bind(input.retired_reason.as_deref())
    .bind(auth.user_id)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(json!({ "id": id }))))
}

/// Revoke somebody's research token.
#[utoipa::path(
    post, path = "/api/admin/security/research-tokens/{id}/revoke",
    operation_id = "adminSecurityRevokeToken", tag = "admin",
    params(("id" = Uuid, Path, description = "Token")),
    responses((status = 200, body = ApiResponse<serde_json::Value>)),
    security(("cookie_auth" = [])),
)]
pub async fn revoke_token(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require_any_capability(&state.db, auth.user_id, &["admin"]).await?;
    security_research::revoke_by_id(&state.db, id, "by_operator").await?;
    Ok(Json(ApiResponse::new(json!({ "revoked": true }))))
}

// ═══════════════════════════════════════════════════════════════════
// The loop back to the blue side
// ═══════════════════════════════════════════════════════════════════

/// Turn a confirmed finding into a defensive exercise.
///
/// The artefact is supplied rather than extracted: the request log lives in the
/// reverse proxy and not in this database, and its redaction is a judgement
/// about other people's requests that nothing here should be making. What this
/// endpoint does is everything after the export — the challenge, the questions,
/// and the answers that are known because the finding is on the record.
#[utoipa::path(
    post, path = "/api/admin/security/findings/{id}/blue-lab",
    operation_id = "adminSecurityLabFromFinding",
    tag = "admin",
    params(("id" = Uuid, Path, description = "Finding")),
    request_body = security_lab_generator::LabFromFinding,
    responses(
        (status = 200, body = ApiResponse<serde_json::Value>),
        (status = 409, description = "The finding is not confirmed", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn lab_from_finding(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<security_lab_generator::LabFromFinding>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require_any_capability(
        &state.db,
        auth.user_id,
        &["admin", "domain_curator:security", "domain_curator:all"],
    )
    .await?;

    let challenge_id =
        security_lab_generator::draft_from_finding(&state.db, id, auth.user_id, input).await?;

    Ok(Json(ApiResponse::new(json!({
        "challenge_id": challenge_id,
        "status": "draft",
        "note": "Read the artefact and check that every question is answerable                  from it before publishing. A redaction that removed an answer                  is only visible from the other side.",
    }))))
}

// ═══════════════════════════════════════════════════════════════════
// Bounties claimed from elsewhere
// ═══════════════════════════════════════════════════════════════════

/// The claims waiting on somebody opening a disclosure.
#[utoipa::path(
    get, path = "/api/admin/security/bounty-claims",
    operation_id = "adminSecurityBountyClaims",
    tag = "admin",
    responses((status = 200, body = ApiResponse<serde_json::Value>)),
    security(("cookie_auth" = [])),
)]
pub async fn bounty_claims(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require_reader(&state, &auth).await?;
    let claims = security_external_bounties::awaiting_review(&state.db, 100).await?;
    Ok(Json(ApiResponse::new(json!({ "claims": claims }))))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BountyVerdict {
    /// The severity the reviewer settled on, which need not be the one the
    /// other platform rated it.
    pub severity: String,
}

/// Accept a claim.
#[utoipa::path(
    post, path = "/api/admin/security/bounty-claims/{id}/verify",
    operation_id = "adminSecurityVerifyBountyClaim",
    tag = "admin",
    params(("id" = Uuid, Path, description = "Claim")),
    request_body = BountyVerdict,
    responses(
        (status = 200, body = ApiResponse<serde_json::Value>),
        (status = 409, description = "Already decided", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn verify_bounty_claim(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<BountyVerdict>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require_reader(&state, &auth).await?;
    let code =
        security_external_bounties::verify(&state.db, auth.user_id, id, &body.severity).await?;
    Ok(Json(ApiResponse::new(json!({ "verification_code": code }))))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BountyRefusal {
    pub reason: String,
}

/// Refuse it, with the reason the person will read.
#[utoipa::path(
    post, path = "/api/admin/security/bounty-claims/{id}/refuse",
    operation_id = "adminSecurityRefuseBountyClaim",
    tag = "admin",
    params(("id" = Uuid, Path, description = "Claim")),
    request_body = BountyRefusal,
    responses((status = 200, body = ApiResponse<serde_json::Value>)),
    security(("cookie_auth" = [])),
)]
pub async fn refuse_bounty_claim(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<BountyRefusal>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    require_reader(&state, &auth).await?;
    security_external_bounties::refuse(&state.db, auth.user_id, id, &body.reason).await?;
    Ok(Json(ApiResponse::new(json!({ "refused": true }))))
}
