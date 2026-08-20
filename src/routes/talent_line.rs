//! The three products that finish the talent line: entitlements, trial
//! periods, and reverse recruitment.
//!
//! They share a file because they share a story — an enterprise engaging with
//! a person before, during and instead of a hire — and because splitting
//! three small surfaces into three modules would mean three copies of the
//! same enterprise gate.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use bigdecimal::BigDecimal;
use serde::Deserialize;
use serde_json::{Value, json};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::{entitlements, reverse_recruitment, trials};

pub fn talent_line_routes() -> Router<AppState> {
    Router::new()
        // Entitlements — what a subscription includes.
        .route("/enterprise/entitlements", get(my_entitlements))
        // Trials.
        .route(
            "/enterprise/trials",
            get(enterprise_trials).post(start_trial),
        )
        .route("/enterprise/trials/{id}/conclude", post(conclude_trial))
        .route("/trials/hours/{id}/decision", post(decide_hours))
        .route("/users/me/trials", get(my_trials))
        .route("/trials/{id}/hours", get(trial_hours).post(log_hours))
        // Reverse recruitment.
        .route(
            "/reverse-recruitment/postings",
            get(browse_postings).post(post_wanted),
        )
        .route("/users/me/reverse-recruitment", get(my_posting))
        .route("/reverse-recruitment/postings/{id}/pitch", post(send_pitch))
        .route("/users/me/pitches", get(my_pitches))
        .route("/pitches/{id}/respond", post(respond_to_pitch))
}

/// Admin surface, mounted behind the admin gate.
pub fn admin_talent_line_routes() -> Router<AppState> {
    Router::new().route(
        "/admin/enterprise-products/{id}/entitlements",
        post(grant_entitlement),
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

// ═══════════════════════════════════════════════════════════════════
// Entitlements
// ═══════════════════════════════════════════════════════════════════

/// What the caller's enterprise is entitled to, and what is left of it.
#[utoipa::path(
    get, path = "/api/enterprise/entitlements", tag = "enterprise",
    responses(
        (status = 200, body = serde_json::Value),
        (status = 403, description = "Not acting for an enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn my_entitlements(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
    let rows = entitlements::for_enterprise(&state.db, enterprise.id).await?;

    let items: Vec<Value> = rows
        .into_iter()
        .map(|e| {
            // A quota reports what is left; a ceiling reports a limit. Said
            // in different fields so a caller cannot render them alike.
            let remaining = entitlements::has_remainder(&e.nature)
                .then(|| e.granted.clone().map(|g| g - e.consumed.clone()))
                .flatten();
            json!({
                "kind": e.kind,
                "label": e.label,
                "description": e.description,
                "nature": e.nature,
                "unit": e.unit,
                "granted": e.granted,
                "consumed": entitlements::has_remainder(&e.nature).then_some(e.consumed),
                "remaining": remaining,
                "from_product": e.product_type,
            })
        })
        .collect();

    Ok(Json(build_response(json!({ "entitlements": items }))))
}

/// Admin: attach an entitlement to an engagement.
#[utoipa::path(
    post, path = "/api/admin/enterprise-products/{id}/entitlements", tag = "admin",
    params(("id" = Uuid, Path, description = "Engagement id")),
    request_body(content = serde_json::Value, description = "GrantInput"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "A flag with an amount, or an amount-carrying kind without one", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn grant_entitlement(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(product_id): Path<Uuid>,
    Json(input): Json<entitlements::GrantInput>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    entitlements::grant(&state.db, product_id, &input).await?;
    Ok(Json(build_response(json!({ "granted": input.kind }))))
}

// ═══════════════════════════════════════════════════════════════════
// Trials
// ═══════════════════════════════════════════════════════════════════

/// Start a paid trial.
#[utoipa::path(
    post, path = "/api/enterprise/trials", tag = "enterprise",
    request_body(content = serde_json::Value, description = "StartInput"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Unpaid, too long, or already running", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn start_trial(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<trials::StartInput>,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
    let talent = input.talent_user_id;
    let trial = trials::start(&state.db, enterprise.id, input).await?;

    // What this can cost at most, before anybody starts. A client agreeing to
    // a trial is agreeing to an exposure and should see it now rather than on
    // the first invoice.
    let ceiling = trials::maximum_cost(trial.duration_weeks, &trial.hourly_rate);

    let _ = crate::services::notify::send(
        &state,
        crate::services::notify::Recipient::User(talent),
        "trial.started",
    )
    .arg("weeks", trial.duration_weeks.to_string())
    .payload(json!({ "trial_id": trial.id }))
    .execute()
    .await;

    Ok(Json(build_response(json!({
        "trial": trial,
        "maximum_cost": ceiling,
    }))))
}

/// The caller's enterprise's trials.
#[utoipa::path(
    get, path = "/api/enterprise/trials", tag = "enterprise",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn enterprise_trials(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
    let rows = trials::for_enterprise(&state.db, enterprise.id).await?;
    Ok(Json(build_response(json!({ "trials": rows }))))
}

/// The caller's own trials.
#[utoipa::path(
    get, path = "/api/users/me/trials", tag = "profile",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn my_trials(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let rows = trials::for_talent(&state.db, auth.user_id).await?;
    Ok(Json(build_response(json!({ "trials": rows }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct LogHoursBody {
    pub worked_on: chrono::NaiveDate,
    #[schema(value_type = String)]
    pub hours: BigDecimal,
    /// What was done. It is what the client approves against.
    #[schema(max_length = 2000)]
    pub summary: String,
}

/// Claim a day's work.
#[utoipa::path(
    post, path = "/api/trials/{id}/hours", tag = "profile",
    params(("id" = Uuid, Path, description = "Trial id")),
    request_body = LogHoursBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Outside the trial window, or no summary", body = crate::api_response::ErrorResponse),
        (status = 404, description = "Not your trial", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn log_hours(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<LogHoursBody>,
) -> Result<Json<Value>, AppError> {
    let entry = trials::log_hours(
        &state.db,
        id,
        auth.user_id,
        body.worked_on,
        body.hours,
        &body.summary,
    )
    .await?;
    Ok(Json(build_response(json!({ "entry_id": entry }))))
}

/// The timesheet. Both parties read it.
#[utoipa::path(
    get, path = "/api/trials/{id}/hours", tag = "profile",
    params(("id" = Uuid, Path, description = "Trial id")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 403, description = "Not a party to this trial", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn trial_hours(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let trial = trials::by_id(&state.db, id).await?;
    if trial.talent_user_id != auth.user_id {
        let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
        if trial.enterprise_id != enterprise.id {
            return Err(AppError::Forbidden);
        }
    }
    let hours = trials::hours_of(&state.db, id).await?;
    Ok(Json(build_response(json!({
        "hours": hours,
        "approved_total": trial.approved_hours,
        "pending_total": trial.pending_hours,
    }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct HoursDecisionBody {
    pub approve: bool,
    /// Required to refuse.
    #[schema(max_length = 2000)]
    pub reason: Option<String>,
}

/// The client approves or refuses a day.
#[utoipa::path(
    post, path = "/api/trials/hours/{id}/decision", tag = "enterprise",
    params(("id" = Uuid, Path, description = "Timesheet entry id")),
    request_body = HoursDecisionBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Refused with no reason", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn decide_hours(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<HoursDecisionBody>,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
    trials::decide_hours(
        &state.db,
        id,
        enterprise.id,
        auth.user_id,
        body.approve,
        body.reason.as_deref(),
    )
    .await?;
    Ok(Json(build_response(json!({ "approved": body.approve }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ConcludeBody {
    /// `converted_hire`, `declined_by_enterprise`, `declined_by_talent` or
    /// `lapsed`.
    #[schema(max_length = 30)]
    pub outcome: String,
    #[schema(max_length = 2000)]
    pub note: Option<String>,
}

/// End a trial and settle the approved hours.
#[utoipa::path(
    post, path = "/api/enterprise/trials/{id}/conclude", tag = "enterprise",
    params(("id" = Uuid, Path, description = "Trial id")),
    request_body = ConcludeBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Unknown outcome", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not this trial's enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn conclude_trial(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ConcludeBody>,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
    let trial = trials::by_id(&state.db, id).await?;
    if trial.enterprise_id != enterprise.id {
        return Err(AppError::Forbidden);
    }

    let concluded = trials::conclude(&state.db, id, &body.outcome, body.note.as_deref()).await?;
    let (talent_owed, platform) = trials::settle(&state.db, id).await?;

    Ok(Json(build_response(json!({
        "trial": concluded,
        "talent_owed": talent_owed,
        "platform_share": platform,
        // The reduced rate this unlocks, so the client sees the reason to
        // convert rather than hiring around the platform.
        "converted_success_fee_percent": concluded.converted_success_fee_percent,
    }))))
}

// ═══════════════════════════════════════════════════════════════════
// Reverse recruitment
// ═══════════════════════════════════════════════════════════════════

/// Post what you are looking for.
#[utoipa::path(
    post, path = "/api/reverse-recruitment/postings", tag = "profile",
    request_body(content = serde_json::Value, description = "PostingInput"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Below the rank threshold, or an unknown trade", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn post_wanted(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<reverse_recruitment::PostingInput>,
) -> Result<Json<Value>, AppError> {
    let posting = reverse_recruitment::post(&state.db, auth.user_id, input).await?;
    Ok(Json(build_response(json!({ "posting": posting }))))
}

/// The caller's own posting, if they have one.
#[utoipa::path(
    get, path = "/api/users/me/reverse-recruitment", tag = "profile",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn my_posting(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let posting = reverse_recruitment::mine(&state.db, auth.user_id).await?;
    Ok(Json(build_response(json!({ "posting": posting }))))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct BrowseQuery {
    #[param(max_length = 30)]
    pub domain: Option<String>,
    #[param(max_length = 100)]
    pub orientation: Option<String>,
    pub remote_only: Option<bool>,
    #[param(max_length = 2)]
    pub country: Option<String>,
    #[serde(default = "default_browse_limit")]
    #[param(minimum = 1, maximum = 100)]
    pub limit: i64,
}

fn default_browse_limit() -> i64 {
    30
}

/// The postings a company can answer.
///
/// Requires an enterprise: these are people stating terms, and a public
/// listing of who is looking for work is a listing that reaches their current
/// employer.
#[utoipa::path(
    get, path = "/api/reverse-recruitment/postings", tag = "enterprise",
    params(BrowseQuery),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 403, description = "Not acting for an enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn browse_postings(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<BrowseQuery>,
) -> Result<Json<Value>, AppError> {
    // A person with no company gets 403, not the 404 `require_enterprise`
    // returns when it cannot resolve one. Both hide the list, and only one of
    // them is true: this surface exists and is not theirs. 404 here would say
    // the endpoint does not exist, which is the wrong thing to tell somebody
    // whose employer might be reading over their shoulder — the reason the
    // list is private in the first place.
    crate::routes::enterprise::require_enterprise(&state, &auth)
        .await
        .map_err(|e| match e {
            AppError::NotFound(_) => AppError::Forbidden,
            other => other,
        })?;
    crate::validators::check_max_len_opt(&q.domain, "domain", 30)?;
    crate::validators::check_max_len_opt(&q.orientation, "orientation", 100)?;

    let postings = reverse_recruitment::browse(
        &state.db,
        &reverse_recruitment::BrowseFilter {
            domain: q.domain,
            orientation: q.orientation,
            remote_only: q.remote_only,
            country: q.country,
        },
        q.limit,
    )
    .await?;

    Ok(Json(build_response(json!({ "postings": postings }))))
}

/// Argue for yourself.
#[utoipa::path(
    post, path = "/api/reverse-recruitment/postings/{id}/pitch", tag = "enterprise",
    params(("id" = Uuid, Path, description = "Posting id")),
    request_body(content = serde_json::Value, description = "PitchInput"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Too short, already pitched, or the monthly ceiling is reached", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn send_pitch(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<reverse_recruitment::PitchInput>,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
    let posting = reverse_recruitment::by_id(&state.db, id).await?;

    let pitch_id =
        reverse_recruitment::pitch(&state.db, id, enterprise.id, auth.user_id, input).await?;

    let _ = crate::services::notify::send(
        &state,
        crate::services::notify::Recipient::User(posting.talent_user_id),
        "reverse_recruitment.pitch_received",
    )
    .arg("company", enterprise.company_name.clone())
    .payload(json!({ "pitch_id": pitch_id, "posting_id": id }))
    .execute()
    .await;

    Ok(Json(build_response(json!({
        "pitch_id": pitch_id,
        "credits_spent": reverse_recruitment::PITCH_COST_CREDITS,
    }))))
}

/// The pitches the caller has received.
#[utoipa::path(
    get, path = "/api/users/me/pitches", tag = "profile",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn my_pitches(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let Some(posting) = reverse_recruitment::mine(&state.db, auth.user_id).await? else {
        return Ok(Json(build_response(json!({ "pitches": [] }))));
    };
    let pitches = reverse_recruitment::pitches_for(&state.db, posting.id).await?;

    // Marked read as they are handed over. A company that spent credits is
    // owed the knowledge that their argument was opened — which is not an
    // answer, and is not presented as one.
    for pitch in &pitches {
        if pitch.status == "sent" {
            let _ = reverse_recruitment::mark_read(&state.db, pitch.id, auth.user_id).await;
        }
    }

    Ok(Json(build_response(json!({ "pitches": pitches }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PitchResponseBody {
    pub interested: bool,
    /// Optional. Somebody declining ten pitches should not have to justify
    /// each one.
    #[schema(max_length = 2000)]
    pub reason: Option<String>,
}

/// Answer a pitch.
#[utoipa::path(
    post, path = "/api/pitches/{id}/respond", tag = "profile",
    params(("id" = Uuid, Path, description = "Pitch id")),
    request_body = PitchResponseBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "Not your pitch, or already answered", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn respond_to_pitch(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<PitchResponseBody>,
) -> Result<Json<Value>, AppError> {
    reverse_recruitment::respond(
        &state.db,
        id,
        auth.user_id,
        body.interested,
        body.reason.as_deref(),
    )
    .await?;

    // The company spent credits and is owed the outcome, either way.
    let sender: Option<Uuid> =
        sqlx::query_scalar("SELECT sent_by FROM reverse_recruitment_pitches WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await?
            .flatten();
    if let Some(sender) = sender {
        let _ = crate::services::notify::send(
            &state,
            crate::services::notify::Recipient::User(sender),
            "reverse_recruitment.pitch_answered",
        )
        .payload(json!({ "pitch_id": id, "interested": body.interested }))
        .execute()
        .await;
    }

    Ok(Json(build_response(json!({
        "recorded": true,
        "interested": body.interested,
    }))))
}
