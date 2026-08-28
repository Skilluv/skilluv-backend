//! The security domain, from a contributor's side.
//!
//! ## What is public and what is not
//!
//! Three things are public and unauthenticated, because a disclosure programme
//! that requires an account to read the scope is a programme nobody reads:
//! `/security/scope`, `/security/hall-of-fame`, and the card of a confirmed
//! finding. Everything else needs a session, because everything else is either
//! somebody's report or somebody's evidence.
//!
//! ## Who reviews what
//!
//! Not `require_admin`, except for the one transition that publishes. A finding
//! is triaged by `security_triager`, judged by `security_reviewer:{family}` and
//! published by an administrator — the routing 0404's derived capabilities were
//! built for, and the reason the admin surface of this domain
//! (`routes::admin_security`) is not gated on `admin` alone.
//!
//! ## The upload that comes before the report
//!
//! Proof files are uploaded first and referenced by key in the submission. That
//! is the shape of the form — you screenshot the exploit while you have it —
//! and it is why an orphan sweep exists.

use axum::extract::{Multipart, Path, Query, State};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use utoipa::IntoParams;
use uuid::Uuid;

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::middleware::{AuthUser, RateLimiter};
use crate::services::{
    security_external_bounties, security_findings, security_practice, security_profile,
    security_proofs, security_research,
};

pub fn security_routes() -> Router<AppState> {
    Router::new()
        // Public.
        .route("/security/reference", get(reference))
        .route("/security/scope", get(scope))
        .route("/security/hall-of-fame", get(hall_of_fame))
        .route("/security/findings/{id}", get(finding_card))
        .route("/security/ctf/scoreboard", get(scoreboard))
        .route("/security/external-bounties", get(external_bounties))
        .route(
            "/security/external-bounties/claims",
            get(my_bounty_claims).post(claim_bounty),
        )
        .route("/trust/summary", get(trust_summary))
        .route("/users/{username}/security-profile", get(profile))
        // Reporting.
        .route("/security/reports", get(my_reports).post(submit_report))
        .route("/security/reports/uploads", post(upload_proof))
        .route("/security/proofs", get(download_proof))
        .route("/security/reports/{id}/withdraw", post(withdraw))
        .route("/security/reports/{id}/answer-round", post(answer_round))
        // Practice.
        .route("/security/challenges/{id}/flag", post(submit_flag))
        .route("/security/challenges/{id}/answers", post(submit_answers))
        .route("/security/challenges/{id}/artifact", get(lab_artifact))
        // Research mode.
        .route(
            "/security/research-token",
            get(current_token).post(issue_token),
        )
        .route("/security/research-token", delete(revoke_token))
}

// ═══════════════════════════════════════════════════════════════════
// The vocabulary
// ═══════════════════════════════════════════════════════════════════

/// Everything a client would otherwise hard-code.
#[utoipa::path(
    get, path = "/api/security/reference",
    operation_id = "securityReference", tag = "security",
    responses((status = 200, body = ApiResponse<serde_json::Value>)),
)]
pub async fn reference(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let orientations: Vec<Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object(
                    'slug', slug, 'name', name, 'description', description,
                    'reviewer_group', reviewer_group, 'tags', tags,
                    'secondary_domains', secondary_domains)
           FROM orientations
          WHERE primary_domain = 'security' AND is_curated AND NOT is_archived
          ORDER BY reviewer_group, name",
    )
    .fetch_all(&state.db)
    .await?;

    let round_kinds: Vec<Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object('slug', slug, 'name', name,
                                   'description', description)
           FROM revision_round_kinds
          WHERE skill_domain = 'security'
          ORDER BY sort_order",
    )
    .fetch_all(&state.db)
    .await?;

    let bases: Vec<Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object('basis', basis, 'title', title,
                                   'description', description,
                                   'requires_deliverable', requires_deliverable)
           FROM attestation_bases
          WHERE skill_domain = 'security'
          ORDER BY sort_order",
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(json!({
        "orientations": orientations,
        "severity_tiers": ["critical", "high", "medium", "low", "informational"],
        "finding_statuses": ["submitted", "triaged", "confirmed", "duplicate",
                             "not_applicable", "withdrawn", "fixed", "published"],
        "disclosure_stages": ["embargoed", "extension_requested",
                              "partially_disclosed", "public", "withheld"],
        "challenge_kinds": ["ctf_flag", "defensive_lab", "machine_walkthrough",
                            "training_ground", "analysis_exercise", "audit_exercise"],
        "difficulty_tiers": ["easy", "medium", "hard", "insane"],
        "slice_subtypes": ["finding_hunt", "code_audit", "threat_model",
                           "governance_review", "detection_engineering",
                           "purple_exercise", "incident_analysis"],
        "round_kinds": round_kinds,
        "attestation_bases": bases,
        "fragments_by_severity": {
            "critical": security_findings::fragments_for("critical"),
            "high": security_findings::fragments_for("high"),
            "medium": security_findings::fragments_for("medium"),
            "low": security_findings::fragments_for("low"),
            "informational": security_findings::fragments_for("informational"),
        },
        "triage_sla_days": security_findings::TRIAGE_SLA_DAYS,
    }))))
}

/// The scope, machine-readable.
///
/// Unauthenticated on purpose, and the reason is the whole point of T-01: a
/// researcher decides what to touch before they have an account, and a scope
/// behind a login is a scope nobody reads. The same list that refuses a
/// submission is what is served here, so the document and the enforcement
/// cannot drift.
#[utoipa::path(
    get, path = "/api/security/scope", tag = "security",
    responses((status = 200, body = ApiResponse<serde_json::Value>)),
)]
pub async fn scope() -> Json<ApiResponse<Value>> {
    Json(ApiResponse::new(json!({
        "in_scope_hosts": security_findings::scope_hosts(),
        "policy_url": format!("{}/security", crate::config::PUBLIC_SITE_URL),
        "contact": "security@skill-uv.com",
        "triage_sla_days": security_findings::TRIAGE_SLA_DAYS,
        "default_embargo_days": 90,
        "out_of_scope": [
            "denial of service of any kind, including load testing",
            "brute force beyond the published rate limits",
            "social engineering of users or staff",
            "physical attacks",
            "third-party accounts and services",
            "reports produced only by a scanner, with no reachability shown",
        ],
        "research_mode": {
            "header": crate::middleware::security_research::TOKEN_HEADER,
            "handle_header": crate::middleware::security_research::HANDLE_HEADER,
            "multiplier": security_research::RATE_LIMIT_MULTIPLIER,
            "how": format!("{}/security/research-mode",
                           crate::config::PUBLIC_SITE_URL),
        },
    })))
}

// ═══════════════════════════════════════════════════════════════════
// Reporting
// ═══════════════════════════════════════════════════════════════════

/// Report a vulnerability.
#[utoipa::path(
    post, path = "/api/security/reports", tag = "security",
    request_body = security_findings::SubmitInput,
    responses(
        (status = 200, body = ApiResponse<serde_json::Value>),
        (status = 400, description = "Out of scope, or a report nobody could follow", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn submit_report(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<security_findings::SubmitInput>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    // Five an hour. Enough for somebody having a very good afternoon, and the
    // ceiling a declared research token multiplies.
    let mut redis = state.redis.clone();
    RateLimiter::check(
        &mut redis,
        "security_report",
        &auth.user_id.to_string(),
        5,
        3600,
    )
    .await?;

    let submitted = security_findings::submit(&state.db, auth.user_id, input).await?;

    // The acknowledgement. Seventy-two hours is what the published policy
    // promises for this one, and the promise is only worth anything if the
    // message actually goes out.
    let _ = crate::services::notify::send(
        &state,
        crate::services::notify::Recipient::User(auth.user_id),
        "security.finding_received",
    )
    .arg("title", submitted.title.clone())
    .arg("days", security_findings::TRIAGE_SLA_DAYS.to_string())
    .payload(json!({ "finding_id": submitted.id }))
    .execute()
    .await;

    // And the queue somebody has to work. Anybody who can triage or review,
    // because a report waiting on "whoever is free" is one that waits for
    // nobody in particular.
    if !submitted.triage_skipped {
        let _ = crate::services::notify::send(
            &state,
            crate::services::notify::Recipient::AnyCapability(vec![
                "security_triager".to_string(),
                "security_reviewer:all".to_string(),
                "challenge_validator:security".to_string(),
            ]),
            "security.triage_queued",
        )
        .arg("count", "1")
        .arg("days", security_findings::TRIAGE_SLA_DAYS.to_string())
        .payload(json!({ "finding_id": submitted.id }))
        .execute()
        .await;
    }

    // The similarity scan is not on the request path: a reporter should not wait
    // on it, and its result is read by a triager minutes later at the earliest.
    let db = state.db.clone();
    let id = submitted.id;
    tokio::spawn(async move {
        if let Err(e) = security_findings::scan_similar(&db, id).await {
            tracing::warn!(finding = %id, error = %e, "similarity scan failed");
        }
    });

    Ok(Json(ApiResponse::new(json!({ "report": submitted }))))
}

/// My reports.
#[utoipa::path(
    get, path = "/api/security/reports",
    operation_id = "securityMyReports", tag = "security",
    responses((status = 200, body = ApiResponse<serde_json::Value>)),
    security(("cookie_auth" = [])),
)]
pub async fn my_reports(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let reports = security_findings::mine(&state.db, auth.user_id).await?;
    Ok(Json(ApiResponse::new(json!({ "reports": reports }))))
}

/// Take a report back.
#[utoipa::path(
    post, path = "/api/security/reports/{id}/withdraw",
    operation_id = "securityWithdraw", tag = "security",
    params(("id" = Uuid, Path, description = "Report")),
    responses(
        (status = 200, body = ApiResponse<serde_json::Value>),
        (status = 409, description = "Too late to withdraw", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn withdraw(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let status = security_findings::transition(
        &state.db,
        auth.user_id,
        security_findings::Actor::Reporter,
        id,
        security_findings::TransitionInput {
            to: "withdrawn".into(),
            ..Default::default()
        },
    )
    .await?;
    Ok(Json(ApiResponse::new(json!({ "status": status }))))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RoundAnswer {
    pub answer_md: String,
}

/// Answer what a reviewer asked for.
#[utoipa::path(
    post, path = "/api/security/reports/{id}/answer-round", tag = "security",
    params(("id" = Uuid, Path, description = "Report")),
    request_body = RoundAnswer,
    responses(
        (status = 200, body = ApiResponse<serde_json::Value>),
        (status = 404, description = "No open round of yours", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn answer_round(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<RoundAnswer>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let round =
        security_findings::answer_round(&state.db, auth.user_id, id, &body.answer_md).await?;
    Ok(Json(ApiResponse::new(json!({ "round_no": round }))))
}

// ═══════════════════════════════════════════════════════════════════
// Proofs
// ═══════════════════════════════════════════════════════════════════

/// Upload one proof file, and get back the key to put in the report.
#[utoipa::path(
    post, path = "/api/security/reports/uploads", tag = "security",
    responses(
        (status = 200, body = ApiResponse<serde_json::Value>),
        (status = 400, description = "Refused format, or too large", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn upload_proof(
    State(state): State<AppState>,
    auth: AuthUser,
    mut multipart: Multipart,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let mut redis = state.redis.clone();
    RateLimiter::check(
        &mut redis,
        "security_proof_upload",
        &auth.user_id.to_string(),
        security_proofs::UPLOADS_PER_HOUR,
        3600,
    )
    .await?;

    let mut filename: Option<String> = None;
    let mut bytes: Option<Vec<u8>> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::Validation(format!("malformed upload: {e}")))?
    {
        if field.name().unwrap_or_default() == "file" {
            filename = field.file_name().map(|f| f.to_string());
            let data = field
                .bytes()
                .await
                .map_err(|e| AppError::Validation(format!("file: {e}")))?;
            if data.len() > security_proofs::MAX_PROOF_BYTES {
                return Err(AppError::Validation("that file is too large".into()));
            }
            bytes = Some(data.to_vec());
        }
    }

    let (Some(filename), Some(bytes)) = (filename, bytes) else {
        return Err(AppError::Validation(
            "the upload needs a `file` part with a filename".into(),
        ));
    };

    let key = security_proofs::store(&state.storage, auth.user_id, &filename, &bytes).await?;

    Ok(Json(ApiResponse::new(json!({
        "key": key,
        "note": "put this key in `proof_keys` on the report. It is not a URL: \
                 a proof of an unfixed vulnerability does not get a stable \
                 address."
    }))))
}

/// A one-hour link to a proof, for whoever is allowed to see it.
/// The object key of a proof file — a bucket path such as
/// `security-proofs/{uploader}/{uuid}.png`. A query parameter rather than a
/// path segment because it contains slashes: an axum path capture would need a
/// `{*key}` wildcard, which OpenAPI cannot describe as a normal parameter, and
/// a document that cannot name the segment is one no generated client can call.
#[derive(Debug, Deserialize, IntoParams)]
pub struct ProofKeyQuery {
    pub key: String,
}

#[utoipa::path(
    get, path = "/api/security/proofs", tag = "security",
    params(ProofKeyQuery),
    params(("key" = String, Path, description = "Proof key from the upload")),
    responses(
        (status = 200, body = ApiResponse<serde_json::Value>),
        (status = 403, description = "Not yours and not a reviewer's", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn download_proof(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<ProofKeyQuery>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let key = q.key;
    if !key.starts_with("security-proofs/") || key.contains("..") {
        return Err(AppError::Validation("not a proof key".into()));
    }
    if !security_proofs::may_read(&state.db, auth.user_id, &key).await? {
        return Err(AppError::Forbidden);
    }
    let url = security_proofs::signed_url(&state.storage, &key).await?;
    Ok(Json(ApiResponse::new(json!({
        "url": url,
        "expires_in_seconds": security_proofs::SIGNED_URL_SECONDS,
    }))))
}

// ═══════════════════════════════════════════════════════════════════
// Practice
// ═══════════════════════════════════════════════════════════════════

/// Submit a captured flag.
#[utoipa::path(
    post, path = "/api/security/challenges/{id}/flag", tag = "security",
    params(("id" = Uuid, Path, description = "Challenge")),
    request_body = security_practice::FlagSubmission,
    responses(
        (status = 200, body = ApiResponse<serde_json::Value>),
        (status = 400, description = "Wrong flag, or too many attempts", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn submit_flag(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<security_practice::FlagSubmission>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let outcome = security_practice::submit_flag(&state.db, auth.user_id, id, &body.flag).await?;

    // A first solve is community news, and the only thing in this domain that
    // is broadcast rather than kept between a reporter and a reviewer. The
    // username is read rather than taken from the session, because `AuthUser`
    // carries an id and a role and nothing a person would recognise.
    if outcome.first_solve {
        let context: Option<(String, String)> = sqlx::query_as(
            "SELECT c.title, u.username
               FROM challenge_templates c, users u
              WHERE c.id = $1 AND u.id = $2",
        )
        .bind(id)
        .bind(auth.user_id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten();

        if let Some((title, username)) = context {
            state
                .ws
                .broadcast_all(crate::websocket::WsMessage {
                    event: "security.first_solve".to_string(),
                    room: None,
                    payload: json!({
                        "challenge_id": id,
                        "challenge_title": title,
                        "username": username,
                    }),
                })
                .await;
        }
    }

    Ok(Json(ApiResponse::new(json!({ "outcome": outcome }))))
}

/// Submit the answers to a graded lab.
#[utoipa::path(
    post, path = "/api/security/challenges/{id}/answers", tag = "security",
    params(("id" = Uuid, Path, description = "Challenge")),
    request_body = security_practice::LabSubmission,
    responses(
        (status = 200, body = ApiResponse<serde_json::Value>),
        (status = 400, description = "Attempts used up", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn submit_answers(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<security_practice::LabSubmission>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let outcome = security_practice::submit_answers(&state.db, auth.user_id, id, body).await?;
    Ok(Json(ApiResponse::new(json!({ "outcome": outcome }))))
}

/// A one-day link to the artefact of a defensive lab.
///
/// Authenticated, because the link is minted for whoever asked and expires:
/// the artefact is not secret — it was redacted and published to be analysed —
/// but a permanent public URL to a capture is a mirror somebody else hosts, and
/// then a lab this platform cannot revise.
///
/// The analysis happens on the reader's own machine, in the reader's own tools.
/// Nothing is uploaded back; only the answers return, through
/// `POST /security/challenges/{id}/answers`.
#[utoipa::path(
    get, path = "/api/security/challenges/{id}/artifact", tag = "security",
    params(("id" = Uuid, Path, description = "Challenge")),
    responses(
        (status = 200, body = ApiResponse<security_practice::LabArtifact>),
        (status = 400, description = "Not a lab", body = crate::api_response::ErrorResponse),
        (status = 404, description = "No such challenge", body = crate::api_response::ErrorResponse),
        (status = 409, description = "Not published", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn lab_artifact(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<security_practice::LabArtifact>>, AppError> {
    let artifact = security_practice::artifact_link(&state.db, &state.storage, id).await?;
    Ok(Json(ApiResponse::new(artifact)))
}

/// Who has solved what.
#[utoipa::path(
    get, path = "/api/security/ctf/scoreboard", tag = "security",
    responses((status = 200, body = ApiResponse<serde_json::Value>)),
)]
pub async fn scoreboard(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let board = security_practice::scoreboard(&state.db).await?;
    Ok(Json(ApiResponse::new(board)))
}

// ═══════════════════════════════════════════════════════════════════
// Public reading
// ═══════════════════════════════════════════════════════════════════

/// One finding, as a stranger may read it.
#[utoipa::path(
    get, path = "/api/security/findings/{id}", tag = "security",
    params(("id" = Uuid, Path, description = "Finding")),
    responses(
        (status = 200, body = ApiResponse<serde_json::Value>),
        (status = 404, description = "Not a confirmed finding", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn finding_card(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let card = security_findings::public_card(&state.db, id).await?;
    Ok(Json(ApiResponse::new(json!({ "finding": card }))))
}

/// The hall of fame.
///
/// Cached, because it is a heavy read of a slowly changing set and it is the
/// page a disclosure gets shared to.
#[utoipa::path(
    get, path = "/api/security/hall-of-fame", tag = "security",
    responses((status = 200, body = ApiResponse<serde_json::Value>)),
)]
pub async fn hall_of_fame(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let key = format!(
        "security:hall-of-fame:{}",
        state.db.connect_options().get_database().unwrap_or("db")
    );
    let mut redis = state.redis.clone();
    if let Some(cached) = crate::services::cache::get_json::<Value>(&mut redis, &key).await? {
        return Ok(Json(ApiResponse::new(cached)));
    }

    let board = security_findings::hall_of_fame(&state.db).await?;
    let _ = crate::services::cache::set_json(&mut redis, &key, &board, 300).await;
    Ok(Json(ApiResponse::new(board)))
}

/// The trust centre's figures (T-10).
///
/// The same rows the hall of fame reads, plus what the platform says about
/// itself. One source, so two pages cannot quote different numbers — which is
/// the failure a trust page most needs to avoid.
#[utoipa::path(
    get, path = "/api/trust/summary", tag = "security",
    responses((status = 200, body = ApiResponse<serde_json::Value>)),
)]
pub async fn trust_summary(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let key = format!(
        "security:trust-summary:{}",
        state.db.connect_options().get_database().unwrap_or("db")
    );
    let mut redis = state.redis.clone();
    if let Some(cached) = crate::services::cache::get_json::<Value>(&mut redis, &key).await? {
        return Ok(Json(ApiResponse::new(cached)));
    }

    let findings = security_findings::hall_of_fame(&state.db).await?;

    // Honest, and stated as claims with dates rather than as badges. Nothing
    // here says "certified" for anything nobody has certified.
    let summary = json!({
        "findings": findings["stats"],
        "scope": security_findings::scope_hosts(),
        "documents": {
            "security_policy": "/SECURITY.md",
            "privacy": "/PRIVACY.md",
            "incident_response": "/INCIDENT_RESPONSE.md",
            "threat_model": "/THREAT_MODEL.md",
            "disclosure_policy": "/docs/security/DISCLOSURE-POLICY.md",
        },
        "compliance": [
            { "framework": "GDPR", "state": "self_assessed",
              "note": "Lawful bases, retention and subject rights are documented in PRIVACY.md. No external audit." },
            { "framework": "SOC 2", "state": "not_started" },
            { "framework": "ISO 27001", "state": "not_started" },
        ],
        "contacts": {
            "security": "security@skill-uv.com",
            "privacy": "security@skill-uv.com",
        },
        "disclosure_programme": {
            "safe_harbour": true,
            "default_embargo_days": 90,
            "triage_sla_days": security_findings::TRIAGE_SLA_DAYS,
            "hall_of_fame": "/security/hall-of-fame",
        },
    });

    let _ = crate::services::cache::set_json(&mut redis, &key, &summary, 900).await;
    Ok(Json(ApiResponse::new(summary)))
}

/// Somebody's security profile, and the one number for it.
#[utoipa::path(
    get, path = "/api/users/{username}/security-profile",
    operation_id = "securityProfile", tag = "profile",
    params(("username" = String, Path, description = "Username")),
    responses(
        (status = 200, body = ApiResponse<serde_json::Value>),
        (status = 404, description = "No such person", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn profile(
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let profile = security_profile::build(&state.db, &username).await?;
    Ok(Json(ApiResponse::new(json!({ "profile": profile }))))
}

// ═══════════════════════════════════════════════════════════════════
// Curated bounty programmes elsewhere (T-13)
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, IntoParams)]
#[serde(deny_unknown_fields)]
pub struct BountyQuery {
    /// One of the curated platforms.
    #[param(nullable)]
    pub platform: Option<String>,
    /// A skill node slug the programme is tagged with.
    #[param(nullable)]
    pub topic: Option<String>,
    /// Only programmes that pay money. Some do not, and an evening spent on
    /// one when you needed to be paid is an evening lost.
    #[serde(default)]
    pub paid_only: bool,
}

/// Public bounty programmes worth a researcher's evening.
#[utoipa::path(
    get, path = "/api/security/external-bounties", tag = "security",
    params(BountyQuery),
    responses((status = 200, body = ApiResponse<serde_json::Value>)),
)]
pub async fn external_bounties(
    State(state): State<AppState>,
    Query(q): Query<BountyQuery>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    crate::validators::check_max_len_opt(&q.platform, "platform", 20)?;
    crate::validators::check_max_len_opt(&q.topic, "topic", 60)?;

    let programmes: Vec<Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object(
                    'id', id, 'platform', platform,
                    'organisation', organisation_name,
                    'url', program_url, 'scope_summary', scope_summary,
                    'skill_topics', skill_topics,
                    'payout_range', payout_range, 'pays_money', pays_money,
                    'discloses_reports', discloses_reports,
                    'curated_at', curated_at)
           FROM external_bounty_programs
          WHERE is_active
            AND ($1::TEXT IS NULL OR platform = $1)
            AND ($2::TEXT IS NULL OR $2 = ANY(skill_topics))
            AND ($3::BOOLEAN IS FALSE OR pays_money)
          ORDER BY curated_at DESC
          LIMIT 200",
    )
    .bind(q.platform.as_deref())
    .bind(q.topic.as_deref())
    .bind(q.paid_only)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(json!({
        "programmes": programmes,
        "note": "Curated, not endorsed. This platform does not run any of \
                 these and cannot help with a report filed on one.",
    }))))
}

// ═══════════════════════════════════════════════════════════════════
// Research mode
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct TokenRequest {
    /// What to call it. "Burp on the laptop".
    #[serde(default)]
    pub label: Option<String>,
    /// How long it lives, in days. Thirty by default.
    #[serde(default)]
    pub days: Option<i64>,
}

/// Issue a research token, replacing any live one.
#[utoipa::path(
    post, path = "/api/security/research-token", tag = "security",
    request_body = TokenRequest,
    responses(
        (status = 200, description = "The secret, shown once", body = ApiResponse<serde_json::Value>),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn issue_token(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<TokenRequest>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let (plaintext, view) = security_research::issue(
        &state.db,
        auth.user_id,
        body.label.as_deref(),
        body.days.unwrap_or(30),
    )
    .await?;

    Ok(Json(ApiResponse::new(json!({
        "token": plaintext,
        "details": view,
        "header": crate::middleware::security_research::TOKEN_HEADER,
        "note": "Shown once. It raises your rate limit and grants nothing \
                 else — denial of service stays out of scope.",
    }))))
}

/// The live token's details, without the secret.
#[utoipa::path(
    get, path = "/api/security/research-token", tag = "security",
    responses((status = 200, body = ApiResponse<serde_json::Value>)),
    security(("cookie_auth" = [])),
)]
pub async fn current_token(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let token = security_research::current(&state.db, auth.user_id).await?;
    Ok(Json(ApiResponse::new(json!({ "token": token }))))
}

/// Revoke it.
#[utoipa::path(
    delete, path = "/api/security/research-token",
    operation_id = "securityRevokeToken", tag = "security",
    responses(
        (status = 200, body = ApiResponse<serde_json::Value>),
        (status = 404, description = "You have no live token", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn revoke_token(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    security_research::revoke(&state.db, auth.user_id, "by_holder").await?;
    Ok(Json(ApiResponse::new(json!({ "revoked": true }))))
}

/// Claim a bounty earned on another platform.
///
/// It arrives claimed and stays claimed until a reviewer opens the public
/// disclosure — the same shape as a declared certification, and for the same
/// reason: the person filing it is the person it belongs to.
#[utoipa::path(
    post, path = "/api/security/external-bounties/claims",
    operation_id = "securityClaimBounty",
    tag = "security",
    request_body = security_external_bounties::ClaimInput,
    responses(
        (status = 200, body = ApiResponse<serde_json::Value>),
        (status = 400, description = "No public disclosure to check", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn claim_bounty(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<security_external_bounties::ClaimInput>,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let id = security_external_bounties::claim(&state.db, auth.user_id, input).await?;
    Ok(Json(ApiResponse::new(json!({
        "id": id,
        "state": "waiting",
        "note": "A reviewer will open the disclosure and check that it exists,                  that it names you, and that its severity is what you said.                  That is everything anybody can check from outside, and the                  attestation says as much.",
    }))))
}

/// My claims and where they got to.
#[utoipa::path(
    get, path = "/api/security/external-bounties/claims",
    operation_id = "securityMyBountyClaims",
    tag = "security",
    responses((status = 200, body = ApiResponse<serde_json::Value>)),
    security(("cookie_auth" = [])),
)]
pub async fn my_bounty_claims(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<Value>>, AppError> {
    let claims = security_external_bounties::mine(&state.db, auth.user_id).await?;
    Ok(Json(ApiResponse::new(json!({ "claims": claims }))))
}
