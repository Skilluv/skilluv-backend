//! Red-team findings, and the process that decides when they come out.
//!
//! ## Why the transitions are here and not left to the caller
//!
//! Migration 0200 states which states need which dates, and a CHECK can only
//! refuse an inconsistent row — it cannot refuse an inconsistent *move*. That
//! a finding must be notified before it can be embargoed, and embargoed
//! before it is published, is a rule about order, and order lives here.
//!
//! The ninety-day default is applied when nothing else is agreed. It is a
//! default, not a law: a provider who fixes in a week should not wait twelve,
//! and the caller can pass a date.

use axum::extract::{Path, State};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::routes::benchmarks::{
    author_of, require_reviewer_of_someone_elses_work, require_worked_on,
};

/// The window the industry settled on, applied when no other date is agreed.
const DEFAULT_EMBARGO_DAYS: i64 = 90;

pub fn ai_safety_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/slices/{slice_id}/safety-reports",
            get(list_reports).post(record_report),
        )
        .route("/safety-reports/{id}/disclosure", patch(update_disclosure))
        .route("/safety-reports/{id}/reproduce", post(reproduce_report))
}

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct SafetyReportRow {
    pub id: Uuid,
    pub slice_id: Uuid,
    pub target_model: String,
    pub target_version: String,
    pub attack_type: String,
    pub reproduction_md: String,
    pub observed_output: String,
    pub attempts: i32,
    pub successes: i32,
    pub severity_tier: String,
    pub severity_rationale_md: String,
    pub mitigation_proposed_md: String,
    pub disclosure_status: String,
    pub vendor_notified_at: Option<DateTime<Utc>>,
    pub embargo_until: Option<DateTime<Utc>>,
    pub published_at: Option<DateTime<Utc>>,
    pub withheld_reason_md: Option<String>,
    pub reproduced_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RecordReportBody {
    #[schema(max_length = 120)]
    pub target_model: String,
    /// The version or snapshot date. A target without one is untestable six
    /// months later, because the provider has redeployed since.
    #[schema(max_length = 60)]
    pub target_version: String,
    /// `prompt_injection`, `jailbreak`, `data_extraction`, `tool_misuse`,
    /// `bias`, `hallucination`, `adversarial_input`, `other`.
    #[schema(max_length = 40)]
    pub attack_type: String,
    pub reproduction_md: String,
    pub observed_output: String,
    pub attempts: i32,
    pub successes: i32,
    /// `low`, `medium`, `high`, `critical`.
    #[schema(max_length = 10)]
    pub severity_tier: String,
    pub severity_rationale_md: String,
    pub mitigation_proposed_md: String,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DisclosureBody {
    /// `vendor_notified`, `embargoed`, `published`, `withheld`.
    #[schema(max_length = 20)]
    pub status: String,
    /// Agreed publication date, for `embargoed`. Ninety days from the
    /// notification when absent.
    pub embargo_until: Option<DateTime<Utc>>,
    /// Required for `withheld`. Withholding with no stated ground is
    /// indistinguishable from burying a finding.
    pub withheld_reason_md: Option<String>,
}

/// Findings attached to one slice.
#[utoipa::path(
    get, path = "/api/slices/{slice_id}/safety-reports", tag = "slices",
    params(("slice_id" = Uuid, Path, description = "Slice id")),
    responses(
        (status = 200, description = "Findings on this slice", body = ApiResponse<Vec<SafetyReportRow>>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn list_reports(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(slice_id): Path<Uuid>,
) -> Result<Json<ApiResponse<Vec<SafetyReportRow>>>, AppError> {
    // Authenticated, deliberately. An unpublished finding is a working
    // attack, and a public listing of those would make this endpoint the
    // fastest place on the internet to shop for one.
    let rows = sqlx::query_as::<_, SafetyReportRow>(
        r#"
        SELECT id, slice_id, target_model, target_version, attack_type,
               reproduction_md, observed_output, attempts, successes,
               severity_tier, severity_rationale_md, mitigation_proposed_md,
               disclosure_status, vendor_notified_at, embargo_until,
               published_at, withheld_reason_md, reproduced_at
          FROM ai_safety_reports
         WHERE slice_id = $1
         ORDER BY created_at DESC
        "#,
    )
    .bind(slice_id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(rows)))
}

/// Record a finding on work you did.
#[utoipa::path(
    post, path = "/api/slices/{slice_id}/safety-reports", tag = "slices",
    params(("slice_id" = Uuid, Path, description = "Slice id")),
    request_body = RecordReportBody,
    responses(
        (status = 200, description = "Recorded", body = ApiResponse<SafetyReportRow>),
        (status = 400, description = "Incomplete finding", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not your work", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn record_report(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slice_id): Path<Uuid>,
    Json(body): Json<RecordReportBody>,
) -> Result<Json<ApiResponse<SafetyReportRow>>, AppError> {
    require_worked_on(&state, auth.user_id, slice_id).await?;

    // The database refuses successes above attempts and empty prose. What it
    // cannot say is that zero successes is not a finding — it is a model
    // behaving, which is worth knowing and is not this table.
    if body.successes == 0 {
        return Err(AppError::Validation(
            "a finding with no successful attempt is a model behaving as it \
             should, not a vulnerability"
                .into(),
        ));
    }

    let row = sqlx::query_as::<_, SafetyReportRow>(
        r#"
        INSERT INTO ai_safety_reports
            (slice_id, target_model, target_version, attack_type,
             reproduction_md, observed_output, attempts, successes,
             severity_tier, severity_rationale_md, mitigation_proposed_md)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
        RETURNING id, slice_id, target_model, target_version, attack_type,
                  reproduction_md, observed_output, attempts, successes,
                  severity_tier, severity_rationale_md, mitigation_proposed_md,
                  disclosure_status, vendor_notified_at, embargo_until,
                  published_at, withheld_reason_md, reproduced_at
        "#,
    )
    .bind(slice_id)
    .bind(&body.target_model)
    .bind(&body.target_version)
    .bind(&body.attack_type)
    .bind(&body.reproduction_md)
    .bind(&body.observed_output)
    .bind(body.attempts)
    .bind(body.successes)
    .bind(&body.severity_tier)
    .bind(&body.severity_rationale_md)
    .bind(&body.mitigation_proposed_md)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(ApiResponse::new(row)))
}

/// Move a finding along the disclosure process.
///
/// Only forwards, and only one step at a time. Going straight from `private`
/// to `published` is how a working attack reaches the internet before the
/// person who could fix it has heard of it, and the schema alone would have
/// allowed it.
#[utoipa::path(
    patch, path = "/api/safety-reports/{id}/disclosure", tag = "slices",
    params(("id" = Uuid, Path, description = "Finding id")),
    request_body = DisclosureBody,
    responses(
        (status = 200, description = "Disclosure updated", body = ApiResponse<SafetyReportRow>),
        (status = 400, description = "Not a legal move", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not your work", body = crate::api_response::ErrorResponse),
        (status = 404, description = "No such finding", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn update_disclosure(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<DisclosureBody>,
) -> Result<Json<ApiResponse<SafetyReportRow>>, AppError> {
    let current: Option<(Uuid, String, Option<DateTime<Utc>>)> = sqlx::query_as(
        "SELECT slice_id, disclosure_status, vendor_notified_at
           FROM ai_safety_reports WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?;

    let Some((slice_id, from, notified_at)) = current else {
        return Err(AppError::NotFound(format!("finding {id} not found")));
    };
    require_worked_on(&state, auth.user_id, slice_id).await?;

    if !is_legal_move(&from, &body.status) {
        return Err(AppError::Validation(format!(
            "'{from}' cannot become '{}': a finding is notified before it is \
             embargoed, and embargoed before it is published",
            body.status
        )));
    }

    let now = Utc::now();
    let notified = match body.status.as_str() {
        "vendor_notified" => Some(now),
        _ => notified_at,
    };
    let embargo = match body.status.as_str() {
        "embargoed" => Some(
            body.embargo_until
                .unwrap_or_else(|| notified.unwrap_or(now) + Duration::days(DEFAULT_EMBARGO_DAYS)),
        ),
        _ => body.embargo_until,
    };
    let published = (body.status == "published").then_some(now);

    let row = sqlx::query_as::<_, SafetyReportRow>(
        r#"
        UPDATE ai_safety_reports
           SET disclosure_status  = $2,
               vendor_notified_at = $3,
               embargo_until      = COALESCE($4, embargo_until),
               published_at       = COALESCE($5, published_at),
               withheld_reason_md = COALESCE($6, withheld_reason_md)
         WHERE id = $1
        RETURNING id, slice_id, target_model, target_version, attack_type,
                  reproduction_md, observed_output, attempts, successes,
                  severity_tier, severity_rationale_md, mitigation_proposed_md,
                  disclosure_status, vendor_notified_at, embargo_until,
                  published_at, withheld_reason_md, reproduced_at
        "#,
    )
    .bind(id)
    .bind(&body.status)
    .bind(notified)
    .bind(embargo)
    .bind(published)
    .bind(body.withheld_reason_md.as_deref())
    .fetch_one(&state.db)
    .await?;

    // Leaving `private` is half of what the attestation needs; the other half
    // is the reproduction. Whichever came second, this runs the engines.
    if let Some(author) = author_of(&state, slice_id).await? {
        let db = state.db.clone();
        tokio::spawn(async move {
            let _ = crate::services::proof_hooks::recompute_all_for_user(&db, author).await;
        });
    }

    Ok(Json(ApiResponse::new(row)))
}

/// Confirm you followed somebody else's reproduction and saw the same thing.
#[utoipa::path(
    post, path = "/api/safety-reports/{id}/reproduce", tag = "slices",
    params(("id" = Uuid, Path, description = "Finding id")),
    responses(
        (status = 200, description = "Reproduction recorded", body = ApiResponse<SafetyReportRow>),
        (status = 400, description = "Your own finding", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not a safety reviewer", body = crate::api_response::ErrorResponse),
        (status = 404, description = "No such finding", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn reproduce_report(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<SafetyReportRow>>, AppError> {
    let slice_id: Option<Uuid> =
        sqlx::query_scalar("SELECT slice_id FROM ai_safety_reports WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await?;
    let Some(slice_id) = slice_id else {
        return Err(AppError::NotFound(format!("finding {id} not found")));
    };

    require_reviewer_of_someone_elses_work(&state, auth.user_id, slice_id).await?;

    let row = sqlx::query_as::<_, SafetyReportRow>(
        r#"
        UPDATE ai_safety_reports
           SET reproduced_at = NOW(), reproduced_by_user_id = $2
         WHERE id = $1
        RETURNING id, slice_id, target_model, target_version, attack_type,
                  reproduction_md, observed_output, attempts, successes,
                  severity_tier, severity_rationale_md, mitigation_proposed_md,
                  disclosure_status, vendor_notified_at, embargo_until,
                  published_at, withheld_reason_md, reproduced_at
        "#,
    )
    .bind(id)
    .bind(auth.user_id)
    .fetch_one(&state.db)
    .await?;

    if let Some(author) = author_of(&state, slice_id).await? {
        let db = state.db.clone();
        tokio::spawn(async move {
            let _ = crate::services::proof_hooks::recompute_all_for_user(&db, author).await;
        });
    }

    Ok(Json(ApiResponse::new(row)))
}

/// Which disclosure moves are allowed.
///
/// Forwards only. There is no way back to `private`: once a vendor has been
/// told, pretending otherwise would let somebody rewrite the history of a
/// disclosure, and the record of when they were told is the whole point.
///
/// `withheld` is reachable from any notified state, because the decision not
/// to publish is often taken after seeing the response.
fn is_legal_move(from: &str, to: &str) -> bool {
    matches!(
        (from, to),
        ("private", "vendor_notified")
            | ("vendor_notified", "embargoed")
            | ("vendor_notified", "published")
            | ("vendor_notified", "withheld")
            | ("embargoed", "published")
            | ("embargoed", "withheld")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_finding_cannot_be_published_before_anyone_is_told() {
        // The schema allows the row; the order is what makes it wrong.
        assert!(!is_legal_move("private", "published"));
        assert!(!is_legal_move("private", "embargoed"));
        assert!(!is_legal_move("private", "withheld"));
    }

    #[test]
    fn the_normal_path_is_open() {
        assert!(is_legal_move("private", "vendor_notified"));
        assert!(is_legal_move("vendor_notified", "embargoed"));
        assert!(is_legal_move("embargoed", "published"));
    }

    #[test]
    fn a_vendor_who_fixes_fast_does_not_need_an_embargo() {
        assert!(is_legal_move("vendor_notified", "published"));
    }

    #[test]
    fn nothing_goes_back() {
        // A disclosure whose history can be rewritten is not a disclosure.
        assert!(!is_legal_move("published", "private"));
        assert!(!is_legal_move("embargoed", "vendor_notified"));
        assert!(!is_legal_move("vendor_notified", "private"));
        assert!(!is_legal_move("published", "withheld"));
        assert!(!is_legal_move("withheld", "published"));
    }
}
