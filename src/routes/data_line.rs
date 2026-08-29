//! The data line: consent, the metered API, reports, licences, white-label.
//!
//! Everything here describes people who are not the customer, so every route
//! passes through the same gate: a person's own, per-purpose, revocable
//! agreement. Two consequences show up in the shapes below and are worth
//! naming.
//!
//! **A person who has not opted in is not found.** Not "private", not
//! "restricted". Saying a user exists but is unshareable would let a client
//! enumerate everybody who declined, which is information about them they did
//! not agree to share either.
//!
//! **The consent routes are the person's own session only.** No admin route
//! grants consent on somebody's behalf, and there is no import path. If that
//! ever needs to exist, it should be hard to add — which is why it is absent
//! rather than present and guarded.

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::{api_metering, data_consent, data_licensing};

pub fn data_routes() -> Router<AppState> {
    Router::new()
        // The person's own decisions.
        .route("/data/purposes", get(list_purposes))
        .route("/users/me/data-consent", get(my_consent))
        .route("/users/me/data-consent/{purpose}", post(set_consent))
        .route("/users/me/unified-profile", get(my_unified_profile))
        .route(
            "/users/me/identity-partners",
            get(my_partners).post(set_partner),
        )
        // The public grid.
        .route("/api-plans", get(list_api_plans))
        // The metered public API. Key-authenticated, not cookie.
        .route("/public/v1/talent-score/{username}", get(talent_score))
        .route(
            "/public/v1/talent-attestations/{username}",
            get(talent_attestations),
        )
        .route("/public/v1/usage", get(key_usage))
}

pub fn admin_data_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/admin/data/reports",
            get(list_reports).post(commission_report),
        )
        .route("/admin/data/reports/{id}/deliver", post(deliver_report))
        .route(
            "/admin/data/licences",
            get(list_licences).post(open_licence),
        )
        .route("/admin/data/licences/{id}/settle", post(settle_licence))
        .route(
            "/admin/data/deployments",
            get(list_deployments).post(provision_deployment),
        )
        .route("/admin/data/deployments/{id}/go-live", post(go_live))
        .route("/admin/data/cohorts", get(cohort_sizes))
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

// ═══════════════════════════════════════════════════════════════════
// Consent
// ═══════════════════════════════════════════════════════════════════

/// What somebody can be asked to agree to, in the words they will be shown.
#[utoipa::path(
    get, path = "/api/data/purposes", tag = "profile",
    responses((status = 200, body = serde_json::Value)),
)]
pub async fn list_purposes(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let purposes = data_consent::purposes(&state.db).await?;
    Ok(Json(build_response(json!({ "purposes": purposes }))))
}

/// Everything the caller has been asked, and how they answered.
#[utoipa::path(
    get, path = "/api/users/me/data-consent", tag = "profile",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn my_consent(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    // Revoked rows included: somebody who turned something off should see
    // that they did, and when.
    let consent = data_consent::for_user(&state.db, auth.user_id).await?;
    Ok(Json(build_response(json!({ "consent": consent }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct DataConsentBody {
    pub agree: bool,
}

/// Agree, or withdraw.
#[utoipa::path(
    post, path = "/api/users/me/data-consent/{purpose}", tag = "profile",
    params(("purpose" = String, Path, description = "Purpose slug")),
    request_body = DataConsentBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Not a purpose we ask about", body = crate::api_response::ErrorResponse),
        (status = 404, description = "Nothing to withdraw", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn set_consent(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(purpose): Path<String>,
    Json(body): Json<DataConsentBody>,
) -> Result<Json<Value>, AppError> {
    if body.agree {
        data_consent::grant(&state.db, auth.user_id, &purpose).await?;
    } else {
        data_consent::revoke(&state.db, auth.user_id, &purpose).await?;
    }

    let consent = data_consent::for_user(&state.db, auth.user_id).await?;
    Ok(Json(build_response(json!({
        "consent": consent,
        // Said plainly, because it is the part people get wrong: withdrawing
        // stops what has not been built yet. A dataset shipped last month
        // cannot be unshipped.
        "note": "Le retrait s'applique à tout ce qui n'a pas encore été produit. \
                 Un jeu de données déjà livré ne peut pas être rappelé.",
        "note_code": "withdrawal_is_not_retroactive",
    }))))
}

#[utoipa::path(
    get, path = "/api/users/me/unified-profile", tag = "profile",
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "Nothing computed yet", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn my_unified_profile(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    // Recomputed on the way out for the owner only: this is the one reader
    // who should always see the current figure, and the one who will want to
    // check what a partner would see.
    let score = data_consent::recompute(&state.db, auth.user_id).await?;
    let partners = data_consent::allowed_partners(&state.db, auth.user_id).await?;
    Ok(Json(build_response(
        json!({ "profile": score, "partners_allowed": partners }),
    )))
}

#[utoipa::path(
    get, path = "/api/users/me/identity-partners", tag = "profile",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn my_partners(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let partners = data_consent::allowed_partners(&state.db, auth.user_id).await?;
    Ok(Json(build_response(json!({ "partners": partners }))))
}

/// Name a partner, or take one back.
#[utoipa::path(
    post, path = "/api/users/me/identity-partners", tag = "profile",
    request_body(content = serde_json::Value, description = "PartnerInput"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "The unified profile has not been agreed to", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn set_partner(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<data_consent::PartnerInput>,
) -> Result<Json<Value>, AppError> {
    data_consent::set_partner(&state.db, auth.user_id, input).await?;
    let partners = data_consent::allowed_partners(&state.db, auth.user_id).await?;
    Ok(Json(build_response(json!({ "partners": partners }))))
}

// ═══════════════════════════════════════════════════════════════════
// The metered API
// ═══════════════════════════════════════════════════════════════════

/// What API access costs.
#[utoipa::path(
    get, path = "/api/api-plans", tag = "enterprise",
    responses((status = 200, body = serde_json::Value)),
)]
pub async fn list_api_plans(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let plans = api_metering::plans(&state.db).await?;
    Ok(Json(build_response(json!({ "plans": plans }))))
}

/// The key presented on this call, checked and counted.
async fn caller(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<api_metering::CallerKey, AppError> {
    let presented = headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .ok_or(AppError::Unauthorized)?;
    api_metering::authorise(&state.db, presented).await
}

/// One person's public figures — if they agreed to be readable.
#[utoipa::path(
    get, path = "/api/public/v1/talent-score/{username}", tag = "public",
    params(("username" = String, Path, description = "Skilluv username")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 401, description = "No or unknown API key", body = crate::api_response::ErrorResponse),
        (status = 404, description = "No such person, or one who has not opted in", body = crate::api_response::ErrorResponse),
        (status = 429, description = "Over the daily ceiling or the monthly quota", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn talent_score(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(username): Path<String>,
) -> Result<Json<Value>, AppError> {
    let key = caller(&state, &headers).await?;

    let profile =
        api_metering::readable_profile(&state.db, &username, &state.config.frontend_url).await?;

    // Not found rather than forbidden. "Exists but private" would let a
    // client enumerate everybody who declined.
    let profile = profile.ok_or_else(|| AppError::NotFound("no such profile".into()))?;

    Ok(Json(json!({
        "data": profile,
        "meta": {
            "attribution_required": key.attribution_required,
            "attribution": if key.attribution_required {
                Some("Données fournies par Skilluv — skill-uv.com")
            } else {
                None
            },
        }
    })))
}

/// Somebody's public attestations, for a verifier.
#[utoipa::path(
    get, path = "/api/public/v1/talent-attestations/{username}", tag = "public",
    params(("username" = String, Path, description = "Skilluv username")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 401, description = "No or unknown API key", body = crate::api_response::ErrorResponse),
        (status = 404, description = "No such person, or one who has not opted in", body = crate::api_response::ErrorResponse),
        (status = 429, description = "Over the daily ceiling or the monthly quota", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn talent_attestations(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(username): Path<String>,
) -> Result<Json<Value>, AppError> {
    caller(&state, &headers).await?;

    let readable =
        api_metering::readable_profile(&state.db, &username, &state.config.frontend_url).await?;
    if readable.is_none() {
        return Err(AppError::NotFound("no such profile".into()));
    }

    // Public and unrevoked only. The verification code is included because
    // that is the whole point: a reader should be able to check the claim
    // without trusting this response.
    let rows: Vec<Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object(
                    'title', a.title, 'type', a.attestation_type, 'basis', a.basis,
                    'issued_at', a.created_at, 'verification_code', a.verification_code
                )
           FROM attestations a
           JOIN users u ON u.id = a.user_id
          WHERE lower(u.username) = lower($1)
            AND a.public AND a.revoked_at IS NULL
          ORDER BY a.created_at DESC",
    )
    .bind(&username)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(build_response(json!({ "attestations": rows }))))
}

/// A client's own usage this month.
#[utoipa::path(
    get, path = "/api/public/v1/usage", tag = "public",
    responses(
        (status = 200, body = serde_json::Value),
        (status = 401, description = "No or unknown API key", body = crate::api_response::ErrorResponse),
    ),
    operation_id = "dataLineKeyUsage",
)]
pub async fn key_usage(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let key = caller(&state, &headers).await?;
    let (requests, throttled) = api_metering::month_to_date(&state.db, key.id).await?;
    Ok(Json(build_response(json!({
        "plan": key.plan,
        "requests_this_month": requests,
        "throttled_this_month": throttled,
    }))))
}

// ═══════════════════════════════════════════════════════════════════
// Reports, licences, deployments
// ═══════════════════════════════════════════════════════════════════

#[utoipa::path(
    get, path = "/api/admin/data/reports", tag = "admin",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
    operation_id = "dataLineListReports",
)]
pub async fn list_reports(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let reports = data_licensing::reports(&state.db).await?;
    Ok(Json(build_response(json!({ "reports": reports }))))
}

#[utoipa::path(
    post, path = "/api/admin/data/reports", tag = "admin",
    request_body(content = serde_json::Value, description = "ReportInput"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "An empty scope", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn commission_report(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<data_licensing::ReportInput>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let report = data_licensing::commission_report(&state.db, auth.user_id, input).await?;
    Ok(Json(build_response(json!({ "report": report }))))
}

#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = DataLineDeliverBody)]
pub struct DeliverBody {
    pub document_url: String,
    /// Which consent the figures rest on. Named rather than assumed: a report
    /// drawn from research consent and one drawn from commercial consent are
    /// different datasets with different people in them.
    pub purpose: String,
}

#[utoipa::path(
    post, path = "/api/admin/data/reports/{id}/deliver", tag = "admin",
    params(("id" = Uuid, Path, description = "Report id")),
    request_body = DeliverBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Too few people behind the figures, or a non-https document", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn deliver_report(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<DeliverBody>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let booked =
        data_licensing::deliver_report(&state.db, id, &body.document_url, &body.purpose).await?;
    Ok(Json(build_response(json!({ "revenue_booked": booked }))))
}

#[utoipa::path(
    get, path = "/api/admin/data/licences", tag = "admin",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn list_licences(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let licences = data_licensing::licences(&state.db).await?;
    Ok(Json(build_response(json!({ "licences": licences }))))
}

#[utoipa::path(
    post, path = "/api/admin/data/licences", tag = "admin",
    request_body(content = serde_json::Value, description = "LicenceInput"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Too few people consenting, a blank purpose, or a commercial licence paying nobody", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn open_licence(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<data_licensing::LicenceInput>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let licence = data_licensing::open_licence(&state.db, input).await?;
    Ok(Json(build_response(json!({ "licence": licence }))))
}

#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = DataLineSettleBody)]
pub struct SettleBody {
    pub period_start: chrono::NaiveDate,
    pub period_end: chrono::NaiveDate,
}

/// Pay the people in a dataset their share for a period.
#[utoipa::path(
    post, path = "/api/admin/data/licences/{id}/settle", tag = "admin",
    params(("id" = Uuid, Path, description = "Contract id")),
    request_body = SettleBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Unsigned, a backwards period, or an empty cohort", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn settle_licence(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<SettleBody>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let (paid, each) =
        data_licensing::settle_period(&state.db, id, body.period_start, body.period_end).await?;
    Ok(Json(build_response(
        json!({ "people_paid": paid, "amount_each": each }),
    )))
}

#[utoipa::path(
    get, path = "/api/admin/data/deployments", tag = "admin",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn list_deployments(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let deployments = data_licensing::deployments(&state.db).await?;
    Ok(Json(build_response(json!({ "deployments": deployments }))))
}

#[utoipa::path(
    post, path = "/api/admin/data/deployments", tag = "admin",
    request_body(content = serde_json::Value, description = "DeploymentInput"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "A recognition claim without a signed contract, or a host already deployed", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn provision_deployment(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<data_licensing::DeploymentInput>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let deployment = data_licensing::provision(&state.db, input).await?;
    Ok(Json(build_response(json!({ "deployment": deployment }))))
}

#[utoipa::path(
    post, path = "/api/admin/data/deployments/{id}/go-live", tag = "admin",
    params(("id" = Uuid, Path, description = "Deployment id")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "No signed contract", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn go_live(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let booked = data_licensing::go_live(&state.db, id).await?;
    Ok(Json(build_response(json!({ "setup_fee_booked": booked }))))
}

/// How many people each purpose currently covers, and whether that is enough
/// to publish anything at all.
#[utoipa::path(
    get, path = "/api/admin/data/cohorts", tag = "admin",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn cohort_sizes(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;

    let mut cohorts = Vec::new();
    for purpose in data_consent::PURPOSES {
        let size = data_consent::cohort_size(&state.db, purpose).await?;
        cohorts.push(json!({
            "purpose": purpose,
            "people": size,
            "publishable": data_consent::cohort_is_publishable(size),
        }));
    }

    Ok(Json(build_response(json!({
        "cohorts": cohorts,
        "floor": data_consent::COHORT_FLOOR,
    }))))
}
