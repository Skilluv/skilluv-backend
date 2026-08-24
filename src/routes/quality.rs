//! The quality domain: defect reports, imported test runs, cross-domain
//! routing, and the profile behind them.
//!
//! ## Who is allowed to review what
//!
//! Not `require_admin`. A defect report is judged by somebody who can read the
//! kind of system it is about, and that is
//! `quality_reviewer:{reviewer_group}` — derived from the trade on the slice
//! by migration 0404's trigger. Routing on the trade rather than on the
//! subtype is deliberate: a defect report against a game build and one against
//! an API are both `bug_report`, and the two people who can read them are
//! different.
//!
//! Administrators reach these through `quality_reviewer:all`, which they can
//! be granted like anybody else, rather than through a bypass that would make
//! the review record say "an admin decided" when what matters is which trade
//! decided.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use utoipa::IntoParams;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::{quality_practice, quality_profile};

pub fn quality_routes() -> Router<AppState> {
    Router::new()
        .route("/quality/reference", get(reference))
        .route("/quality/reports", get(reports))
        .route("/users/{username}/quality-profile", get(profile))
        // Defect reports.
        .route("/quality/bugs", get(my_bugs).post(file_bug))
        .route("/quality/bugs/{id}/fix", post(link_fix))
        .route("/quality/bugs/{id}/confirm", post(confirm_fix))
        .route("/quality/bugs/{id}/review", post(review_bug))
        .route("/quality/bugs/review-queue", get(review_queue))
        // Imported test runs.
        .route("/quality/test-runs", post(import_run))
        .route("/quality/slices/{slice_id}/test-runs", get(slice_runs))
        .route("/quality/test-runs/{id}/verify", post(verify_run))
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

/// The vocabulary of the domain, so a client does not hard-code it.
#[utoipa::path(
    get, path = "/api/quality/reference", tag = "work",
    responses((status = 200, body = serde_json::Value)),
)]
pub async fn reference(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let orientations: Vec<Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object(
                    'slug', slug, 'name', name, 'description', description,
                    'reviewer_group', reviewer_group, 'tags', tags,
                    'secondary_domains', secondary_domains
                )
           FROM orientations
          WHERE primary_domain = 'quality' AND is_curated AND NOT is_archived
          ORDER BY reviewer_group, name",
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(build_response(json!({
        "orientations": orientations,
        "reviewer_groups": quality_practice::REVIEWER_GROUPS,
        "report_subtypes": quality_practice::SUBTYPES,
        "severities": quality_practice::SEVERITIES,
        "reproducibilities": quality_practice::REPRODUCIBILITIES,
        "test_run_sources": quality_practice::RUN_SOURCES,
    }))))
}

/// The public quality profile.
#[utoipa::path(
    get, path = "/api/users/{username}/quality-profile", tag = "profile",
    params(("username" = String, Path, description = "Username")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "No such person", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn profile(
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> Result<Json<Value>, AppError> {
    let profile = quality_profile::build(&state.db, &username).await?;
    Ok(Json(build_response(json!({ "profile": profile }))))
}

/// `deny_unknown_fields` for the reason `tests/test_unknown_query_params.rs`
/// gives: an endpoint that silently drops an unknown filter answers 200 with
/// a full list to somebody who believes they narrowed it, which is the worst
/// of the three possible behaviours.
#[derive(Debug, Deserialize, IntoParams)]
#[serde(deny_unknown_fields)]
pub struct ReportsQuery {
    /// Restrict to work aimed at one domain. Absent means every domain,
    /// including the cross-domain artefacts that target none.
    #[param(nullable)]
    pub target_domain: Option<String>,
    #[param(nullable)]
    pub limit: Option<i64>,
}

/// Verified quality artefacts, optionally restricted to one target domain.
///
/// The listing the backlog called cross-domain sub-tagging (W-05). Verified
/// work only: an unverified report is somebody's claim, and a public listing
/// is where a stranger forms a judgement.
#[utoipa::path(
    get, path = "/api/quality/reports", tag = "work",
    params(ReportsQuery),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "A domain nothing declares", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn reports(
    State(state): State<AppState>,
    Query(q): Query<ReportsQuery>,
) -> Result<Json<Value>, AppError> {
    let reports = quality_practice::reports_by_target_domain(
        &state.db,
        q.target_domain.as_deref(),
        q.limit.unwrap_or(25),
    )
    .await?;
    Ok(Json(build_response(json!({ "reports": reports }))))
}

// ═══════════════════════════════════════════════════════════════════
// Defect reports
// ═══════════════════════════════════════════════════════════════════

#[utoipa::path(
    post, path = "/api/quality/bugs", tag = "work",
    request_body = quality_practice::BugReportInput,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "A report a stranger could not follow", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn file_bug(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<quality_practice::BugReportInput>,
) -> Result<Json<Value>, AppError> {
    let report = quality_practice::file_bug_report(&state.db, auth.user_id, input).await?;
    Ok(Json(build_response(json!({ "report": report }))))
}

#[utoipa::path(
    get, path = "/api/quality/bugs", tag = "work",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn my_bugs(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let reports = quality_practice::bug_reports_for(&state.db, auth.user_id).await?;
    Ok(Json(build_response(json!({ "reports": reports }))))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct FixBody {
    pub fix_url: String,
}

/// Record where the fix landed.
#[utoipa::path(
    post, path = "/api/quality/bugs/{id}/fix", tag = "work",
    params(("id" = Uuid, Path, description = "Defect report id")),
    request_body = FixBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "Not an open report of yours", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn link_fix(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<FixBody>,
) -> Result<Json<Value>, AppError> {
    let report = quality_practice::link_fix(&state.db, auth.user_id, id, &body.fix_url).await?;
    Ok(Json(build_response(json!({ "report": report }))))
}

/// Confirm the defect is gone.
///
/// The reporter, and nobody else. Recomputing the proof afterwards is what
/// turns the confirmation into an attestation, and it runs outside the write
/// so a feed failure cannot roll back somebody's confirmation.
#[utoipa::path(
    post, path = "/api/quality/bugs/{id}/confirm", tag = "work",
    params(("id" = Uuid, Path, description = "Defect report id")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Nothing to confirm yet", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn confirm_fix(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let report = quality_practice::confirm_fix(&state.db, auth.user_id, id).await?;

    // Best-effort: the confirmation is recorded either way, and the sweep in
    // the proof orchestrator picks up whatever this call did not.
    if let Err(e) =
        crate::services::proof_hooks::recompute_all_for_user(&state.db, auth.user_id).await
    {
        tracing::warn!(
            user_id = %auth.user_id, error = %e,
            "fix confirmed but the proof recompute did not run"
        );
    }

    Ok(Json(build_response(json!({ "report": report }))))
}

/// Judge a defect report.
///
/// Guarded by the trade behind the slice, not by an administrator role. A
/// report whose slice carries no quality orientation is refused rather than
/// routed to somebody chosen on its behalf: work in a queue nobody can read is
/// work nobody reads.
#[utoipa::path(
    post, path = "/api/quality/bugs/{id}/review", tag = "work",
    params(("id" = Uuid, Path, description = "Defect report id")),
    request_body = quality_practice::ReviewDecision,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 403, description = "Not a reviewer of this trade", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn review_bug(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(decision): Json<quality_practice::ReviewDecision>,
) -> Result<Json<Value>, AppError> {
    let orientation = quality_practice::reviewer_orientation_for_report(&state.db, id)
        .await?
        .ok_or_else(|| {
            AppError::Validation(
                "this report hangs off a slice with no quality trade on it — nobody can \
                 be said to be able to review it"
                    .into(),
            )
        })?;

    crate::middleware::capabilities::require_reviewer_for_orientation(
        &state.db,
        auth.user_id,
        &orientation,
    )
    .await?;

    let report = quality_practice::review_bug_report(&state.db, auth.user_id, id, decision).await?;

    // A review can turn a confirmed report into an attestable one, or take an
    // attestation's basis away by rejecting it. Both are the reporter's proof
    // changing, so it is recomputed for them rather than for the reviewer.
    if let Err(e) =
        crate::services::proof_hooks::recompute_all_for_user(&state.db, report.reporter_user_id)
            .await
    {
        tracing::warn!(
            user_id = %report.reporter_user_id, error = %e,
            "defect report reviewed but the proof recompute did not run"
        );
    }

    Ok(Json(build_response(json!({ "report": report }))))
}

/// What is waiting for a reviewer.
///
/// Open to anybody holding any quality review capability: seeing the queue is
/// not judging it, and a reviewer who can only judge one family still needs to
/// find the reports in it.
#[utoipa::path(
    get, path = "/api/quality/bugs/review-queue", tag = "work",
    responses(
        (status = 200, body = serde_json::Value),
        (status = 403, description = "No quality review capability", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn review_queue(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    require_any_quality_reviewer(&state, &auth).await?;
    let reports = quality_practice::unreviewed_bug_reports(&state.db).await?;
    Ok(Json(build_response(json!({ "reports": reports }))))
}

/// Holding any one of the quality review capabilities.
///
/// Built from `REVIEWER_GROUPS` rather than written out, so a sixth family
/// added to the catalogue reaches this guard without anybody remembering to
/// edit it — the drift migration 0404 exists to stop, in its request-path
/// form.
async fn require_any_quality_reviewer(state: &AppState, auth: &AuthUser) -> Result<(), AppError> {
    let mut caps: Vec<String> = quality_practice::REVIEWER_GROUPS
        .iter()
        .map(|g| format!("quality_reviewer:{g}"))
        .collect();
    caps.push("quality_reviewer:all".to_string());
    let refs: Vec<&str> = caps.iter().map(String::as_str).collect();

    crate::middleware::capabilities::require_any_capability(&state.db, auth.user_id, &refs).await
}

// ═══════════════════════════════════════════════════════════════════
// Imported test runs
// ═══════════════════════════════════════════════════════════════════

#[utoipa::path(
    post, path = "/api/quality/test-runs", tag = "work",
    request_body = quality_practice::TestRunInput,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "A figure with no source", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn import_run(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<quality_practice::TestRunInput>,
) -> Result<Json<Value>, AppError> {
    let run = quality_practice::import_test_run(&state.db, auth.user_id, input).await?;
    Ok(Json(build_response(json!({ "run": run }))))
}

#[utoipa::path(
    get, path = "/api/quality/slices/{slice_id}/test-runs", tag = "work",
    params(("slice_id" = Uuid, Path, description = "Slice id")),
    responses((status = 200, body = serde_json::Value)),
)]
pub async fn slice_runs(
    State(state): State<AppState>,
    Path(slice_id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let runs = quality_practice::test_runs_for_slice(&state.db, slice_id).await?;
    Ok(Json(build_response(json!({ "runs": runs }))))
}

/// Say a run is what its report says.
#[utoipa::path(
    post, path = "/api/quality/test-runs/{id}/verify", tag = "work",
    params(("id" = Uuid, Path, description = "Test run id")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 403, description = "No quality review capability", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn verify_run(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    require_any_quality_reviewer(&state, &auth).await?;
    let run = quality_practice::verify_test_run(&state.db, auth.user_id, id).await?;
    Ok(Json(build_response(json!({ "run": run }))))
}
