//! The mission board — paid work published by enterprises.
//!
//! Three audiences, three sets of routes:
//!
//!   * anybody, logged in or not, reads the open board;
//!   * a member reads a mission and applies to it;
//!   * the enterprise that published it reads the applications and decides.
//!
//! The enterprise side goes through the same gate as every other enterprise
//! surface — verified email, strong second factor — rather than a check
//! written again here.

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::{AuthUser, extract_ip};
use crate::services::{mission_nda, missions};

pub fn mission_routes() -> Router<AppState> {
    Router::new()
        .route("/missions/types", get(list_types))
        .route("/missions", get(list_missions).post(create_mission))
        .route("/missions/{slug}", get(get_mission))
        .route("/missions/{slug}/apply", post(apply_to_mission))
        .route("/missions/{slug}/nda", get(read_nda).post(sign_nda))
        .route("/missions/{slug}/nda/signature", get(my_nda_signature))
        .route("/missions/{slug}/applications", get(list_applications))
        .route("/missions/{slug}/status", post(set_mission_status))
        .route("/mission-applications/{id}/decision", post(decide))
        .route(
            "/missions/{slug}/invoices",
            get(list_invoices).post(issue_invoice),
        )
        .route("/mission-invoices/{id}/checkout", post(pay_invoice))
        .route("/users/me/missions", get(my_missions))
        .route(
            "/missions/{slug}/deliveries",
            get(list_rounds).post(deliver_round),
        )
        .route("/missions/{slug}/deliveries/accept", post(accept_round))
        .route(
            "/missions/{slug}/deliveries/request-changes",
            post(request_changes),
        )
        .route("/missions/{slug}/ratings", get(list_ratings).post(rate))
        .route("/users/{username}/mission-standing", get(standing))
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

/// The kinds of paid work that exist, per domain.
#[utoipa::path(
    get, path = "/api/missions/types", tag = "missions",
    responses((status = 200, body = serde_json::Value)),
    operation_id = "missionsListTypes",
)]
pub async fn list_types(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let types: Vec<(String, String, String, String)> = sqlx::query_as(
        "SELECT slug, skill_domain, name, description FROM mission_types
          WHERE is_active = TRUE ORDER BY skill_domain, sort_order",
    )
    .fetch_all(&state.db)
    .await?;
    let types: Vec<Value> = types
        .into_iter()
        .map(|(slug, domain, name, description)| {
            json!({
                "slug": slug,
                "skill_domain": domain,
                "name": name,
                "description": description,
            })
        })
        .collect();
    Ok(Json(build_response(json!({ "mission_types": types }))))
}

/// Mirrors the CHECK on `missions.urgency` (migration 0192).
#[derive(Debug, Clone, Copy, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Urgency {
    Normal,
    Soon,
    Urgent,
}

impl Urgency {
    fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Soon => "soon",
            Self::Urgent => "urgent",
        }
    }
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct MissionQuery {
    #[param(max_length = 30)]
    pub skill_domain: Option<String>,
    #[param(max_length = 60)]
    pub mission_type: Option<String>,
    #[param(max_length = 40)]
    pub language: Option<String>,
    #[param(max_length = 60)]
    pub framework: Option<String>,
    #[param(max_length = 100)]
    pub orientation: Option<String>,
    #[param(max_length = 40)]
    pub ip_terms: Option<String>,
    #[param(max_length = 30)]
    pub payment_model: Option<String>,
    /// Education: `beginner`, `junior`, `mid`, `senior`, `mixed`.
    #[param(max_length = 20)]
    pub target_audience: Option<String>,
    pub min_budget_eur: Option<f64>,
    pub remote_only: Option<bool>,
    /// One of the three the column allows. A free string here silently
    /// matches nothing, which reads to a caller as an empty board.
    #[param(inline)]
    pub urgency: Option<Urgency>,
    #[serde(default = "default_limit")]
    #[param(minimum = 1, maximum = 100)]
    pub limit: i64,
    #[serde(default)]
    #[param(minimum = 0, maximum = 10000)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    30
}

/// The open board: missions anybody can still apply to.
#[utoipa::path(
    get, path = "/api/missions", tag = "missions",
    params(MissionQuery),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Invalid filter", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn list_missions(
    State(state): State<AppState>,
    Query(q): Query<MissionQuery>,
) -> Result<Json<Value>, AppError> {
    if !(1..=100).contains(&q.limit) {
        return Err(AppError::Validation(
            "limit must be between 1 and 100".into(),
        ));
    }
    if !(0..=10_000).contains(&q.offset) {
        return Err(AppError::Validation(
            "offset must be between 0 and 10000".into(),
        ));
    }
    // The lengths are declared in the contract, so they have to be enforced
    // here: a filter longer than the column it matches finds nothing, and
    // answering 200 with an empty list tells a caller the board is empty
    // rather than that their query was malformed.
    for (name, value, max) in [
        ("skill_domain", &q.skill_domain, 30),
        ("mission_type", &q.mission_type, 60),
        ("language", &q.language, 40),
        ("framework", &q.framework, 60),
        ("orientation", &q.orientation, 100),
        ("ip_terms", &q.ip_terms, 40),
        ("payment_model", &q.payment_model, 30),
        ("target_audience", &q.target_audience, 20),
    ] {
        crate::validators::check_max_len_opt(value, name, max)?;
    }

    let filter = missions::MissionFilter {
        skill_domain: q.skill_domain,
        mission_type: q.mission_type,
        language: q.language,
        framework: q.framework,
        orientation: q.orientation,
        ip_terms: q.ip_terms,
        payment_model: q.payment_model,
        min_budget_eur: q
            .min_budget_eur
            .and_then(|v| bigdecimal::BigDecimal::try_from(v).ok()),
        remote_only: q.remote_only,
        urgency: q.urgency.map(|u| u.as_str().to_string()),
        target_audience: q.target_audience,
    };
    let rows = missions::list_open(&state.db, &filter, q.limit, q.offset).await?;
    Ok(Json(build_response(json!({ "missions": rows }))))
}

/// One mission in full.
#[utoipa::path(
    get, path = "/api/missions/{slug}", tag = "missions",
    params(("slug" = String, Path, description = "Mission slug")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "No such mission", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn get_mission(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<Value>, AppError> {
    let mission = missions::by_slug(&state.db, &slug).await?;
    // A draft belongs to the enterprise that is still writing it.
    if mission.status == "draft" {
        return Err(AppError::NotFound("mission not found".into()));
    }
    Ok(Json(build_response(json!({ "mission": mission }))))
}

/// Publish a mission. The enterprise gate applies: verified email, strong
/// second factor.
#[utoipa::path(
    post, path = "/api/missions", tag = "missions",
    request_body(content = serde_json::Value, description = "CreateMissionInput"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Invalid mission", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not acting for an enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn create_mission(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<missions::CreateMissionInput>,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
    let mission = missions::create(&state.db, enterprise.id, auth.user_id, input).await?;
    metrics::counter!("skilluv_missions_created_total").increment(1);
    Ok(Json(build_response(json!({ "mission": mission }))))
}

#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = MissionsStatusBody)]
pub struct StatusBody {
    #[schema(max_length = 30)]
    pub status: String,
    /// Required to cancel.
    #[schema(max_length = 2000)]
    pub reason: Option<String>,
}

/// Move a mission along: publish it, close applications, mark it delivered,
/// close it, or cancel it with a reason.
#[utoipa::path(
    post, path = "/api/missions/{slug}/status", tag = "missions",
    params(("slug" = String, Path, description = "Mission slug")),
    request_body = StatusBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Transition not allowed, or cancelled with no reason", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not this mission's enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn set_mission_status(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(body): Json<StatusBody>,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
    let mission = missions::by_slug(&state.db, &slug).await?;
    if mission.enterprise_id != enterprise.id {
        return Err(AppError::Forbidden);
    }
    let updated =
        missions::set_status(&state.db, mission.id, &body.status, body.reason.as_deref()).await?;
    Ok(Json(build_response(json!({ "mission": updated }))))
}

/// Apply to a mission.
#[utoipa::path(
    post, path = "/api/missions/{slug}/apply", tag = "missions",
    params(("slug" = String, Path, description = "Mission slug")),
    request_body(content = serde_json::Value, description = "ApplyInput"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Mission closed, or an empty application", body = crate::api_response::ErrorResponse),
        (status = 404, description = "No such mission", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn apply_to_mission(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(input): Json<missions::ApplyInput>,
) -> Result<Json<Value>, AppError> {
    let mission = missions::by_slug(&state.db, &slug).await?;
    let application = missions::apply(&state.db, mission.id, auth.user_id, input).await?;

    // The enterprise hears about it now rather than the next time somebody
    // remembers to open the board.
    let owner: Option<Uuid> = sqlx::query_scalar("SELECT owner_id FROM enterprises WHERE id = $1")
        .bind(mission.enterprise_id)
        .fetch_optional(&state.db)
        .await?;
    if let Some(owner) = owner {
        let _ = crate::services::notify::send(
            &state,
            crate::services::notify::Recipient::User(owner),
            "mission.application_received",
        )
        .arg("mission", mission.title.clone())
        .payload(json!({
            "mission_id": mission.id,
            "mission_slug": mission.slug,
            "application_id": application.id,
        }))
        .execute()
        .await;
    }

    metrics::counter!("skilluv_mission_applications_total").increment(1);
    Ok(Json(build_response(json!({ "application": application }))))
}

// ═══════════════════════════════════════════════════════════════════
// The confidentiality agreement
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, IntoParams)]
#[serde(deny_unknown_fields)]
pub struct NdaQuery {
    /// `en` or `fr`. A template with no version in that language falls back to
    /// English rather than refusing.
    #[param(nullable)]
    pub locale: Option<String>,
}

/// The agreement this mission asks for, with the hash of what is served.
///
/// The hash is quoted back when signing, which is what makes the signature
/// name a document rather than a moment.
#[utoipa::path(
    get, path = "/api/missions/{slug}/nda",
    operation_id = "missionsReadNda",
    tag = "missions",
    params(("slug" = String, Path, description = "Mission slug"), NdaQuery),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "This mission asks for no agreement", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn read_nda(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(slug): Path<String>,
    Query(q): Query<NdaQuery>,
) -> Result<Json<Value>, AppError> {
    let mission = missions::by_slug(&state.db, &slug).await?;
    let agreement = mission_nda::agreement_for(
        &state.db,
        &state.storage,
        mission.id,
        q.locale.as_deref().unwrap_or("en"),
    )
    .await?;

    Ok(Json(build_response(json!({
        "agreement": agreement,
        // Said in the response and not only in a document, because the whole
        // failure mode of a self-drafted agreement is that everybody assumes
        // somebody checked.
        "notice": if agreement.is_reviewed {
            "This text has been reviewed."
        } else {
            "This text is a draft. No lawyer has reviewed it."
        },
    }))))
}

/// Sign it.
#[utoipa::path(
    post, path = "/api/missions/{slug}/nda",
    operation_id = "missionsSignNda",
    tag = "missions",
    params(("slug" = String, Path, description = "Mission slug")),
    request_body = mission_nda::SignatureInput,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 409, description = "The agreement changed since it was shown to you", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn sign_nda(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    headers: HeaderMap,
    Json(input): Json<mission_nda::SignatureInput>,
) -> Result<Json<Value>, AppError> {
    let mission = missions::by_slug(&state.db, &slug).await?;

    // Parsed rather than stored as text, and absent rather than invented when
    // there is no forwarded address to trust. Migration 0557 says why.
    let ip = extract_ip(&headers).parse().ok();
    let user_agent = headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok());

    let id = mission_nda::sign(
        &state.db,
        &state.storage,
        mission.id,
        auth.user_id,
        ip,
        user_agent,
        input,
    )
    .await?;

    Ok(Json(build_response(json!({ "signature_id": id }))))
}

/// The signature I gave, if I gave one.
#[utoipa::path(
    get, path = "/api/missions/{slug}/nda/signature",
    operation_id = "missionsMyNdaSignature",
    tag = "missions",
    params(("slug" = String, Path, description = "Mission slug")),
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn my_nda_signature(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> Result<Json<Value>, AppError> {
    let mission = missions::by_slug(&state.db, &slug).await?;
    let signature = mission_nda::signature_of(&state.db, mission.id, auth.user_id).await?;
    Ok(Json(build_response(json!({ "signature": signature }))))
}

/// Every application to a mission. The publishing enterprise only.
#[utoipa::path(
    get, path = "/api/missions/{slug}/applications", tag = "missions",
    params(("slug" = String, Path, description = "Mission slug")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 403, description = "Not this mission's enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
    operation_id = "missionsListApplications",
)]
pub async fn list_applications(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
    let mission = missions::by_slug(&state.db, &slug).await?;
    if mission.enterprise_id != enterprise.id {
        return Err(AppError::Forbidden);
    }
    let applications = missions::applications_for(&state.db, mission.id).await?;
    Ok(Json(build_response(
        json!({ "applications": applications }),
    )))
}

#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = MissionsDecisionBody)]
pub struct DecisionBody {
    /// `shortlisted`, `selected` or `rejected`.
    #[schema(max_length = 20)]
    pub status: String,
    /// Required to reject.
    #[schema(max_length = 2000)]
    pub reason: Option<String>,
}

/// Shortlist, select or reject an application.
#[utoipa::path(
    post, path = "/api/mission-applications/{id}/decision", tag = "missions",
    params(("id" = Uuid, Path, description = "Application id")),
    request_body = DecisionBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Rejected with no reason, or the mission is taken", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not this mission's enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
    operation_id = "missionsDecide",
)]
pub async fn decide(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<DecisionBody>,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
    let owns: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM mission_applications a
               JOIN missions m ON m.id = a.mission_id
              WHERE a.id = $1 AND m.enterprise_id = $2)",
    )
    .bind(id)
    .bind(enterprise.id)
    .fetch_one(&state.db)
    .await?;
    if !owns {
        return Err(AppError::Forbidden);
    }

    let application = missions::decide(
        &state.db,
        id,
        auth.user_id,
        &body.status,
        body.reason.as_deref(),
    )
    .await?;

    // Everybody hears their outcome, including the ones who were not chosen.
    let kind = match application.status.as_str() {
        "selected" => Some("mission.application_selected"),
        "rejected" => Some("mission.application_rejected"),
        "shortlisted" => Some("mission.application_shortlisted"),
        _ => None,
    };
    if let Some(kind) = kind {
        let _ = crate::services::notify::send(
            &state,
            crate::services::notify::Recipient::User(application.user_id),
            kind,
        )
        .payload(json!({
            "application_id": application.id,
            "mission_id": application.mission_id,
            "reason": application.decision_reason,
        }))
        .execute()
        .await;
    }

    Ok(Json(build_response(json!({ "application": application }))))
}

// ─── Invoicing ───────────────────────────────────────────────────

/// What is owed on a mission. Readable by the enterprise that publishes it
/// and by the person doing the work — those are the two parties to it.
#[utoipa::path(
    get, path = "/api/missions/{slug}/invoices", tag = "missions",
    params(("slug" = String, Path, description = "Mission slug")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 403, description = "Not a party to this mission", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
    operation_id = "missionsListInvoices",
)]
pub async fn list_invoices(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> Result<Json<Value>, AppError> {
    let mission = missions::by_slug(&state.db, &slug).await?;
    let is_talent = mission.assigned_user_id == Some(auth.user_id);
    if !is_talent {
        let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
        if mission.enterprise_id != enterprise.id {
            return Err(AppError::Forbidden);
        }
    }
    let invoices = crate::services::mission_billing::for_mission(&state.db, mission.id).await?;
    Ok(Json(build_response(json!({ "invoices": invoices }))))
}

/// Put an amount on the mission's account: the whole job, a month of a
/// retainer, a batch of approved hours.
#[utoipa::path(
    post, path = "/api/missions/{slug}/invoices", tag = "missions",
    params(("slug" = String, Path, description = "Mission slug")),
    request_body(content = serde_json::Value, description = "IssueInput"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Nobody on the mission, or an invoice for nothing", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not this mission's enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn issue_invoice(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(input): Json<crate::services::mission_billing::IssueInput>,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
    let mission = missions::by_slug(&state.db, &slug).await?;
    if mission.enterprise_id != enterprise.id {
        return Err(AppError::Forbidden);
    }
    let invoice = crate::services::mission_billing::issue(&state.db, mission.id, input).await?;
    Ok(Json(build_response(json!({ "invoice": invoice }))))
}

/// Open a checkout for one invoice.
///
/// The corridor is chosen from the buyer's country, like everywhere else: a
/// Beninese enterprise paying a Beninese developer must not be sent to a card
/// form that will not take their card.
#[utoipa::path(
    post, path = "/api/mission-invoices/{id}/checkout", tag = "missions",
    params(("id" = Uuid, Path, description = "Invoice id")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Already paid or cancelled", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not this mission's enterprise", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn pay_invoice(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let enterprise = crate::routes::enterprise::require_enterprise(&state, &auth).await?;
    let invoice = crate::services::mission_billing::by_id(&state.db, id).await?;
    let mission = missions::by_id(&state.db, invoice.mission_id).await?;
    if mission.enterprise_id != enterprise.id {
        return Err(AppError::Forbidden);
    }
    if invoice.status != "issued" {
        return Err(AppError::Validation(format!(
            "this invoice is {} — there is nothing to pay",
            invoice.status
        )));
    }

    let buyer: (String, String, Option<String>, Option<String>) =
        sqlx::query_as("SELECT email, display_name, country_iso2, phone FROM users WHERE id = $1")
            .bind(auth.user_id)
            .fetch_one(&state.db)
            .await?;
    let (email, display_name, country, phone) = buyer;

    let currency: crate::services::ledger::Currency = invoice.currency.parse()?;
    let method = if currency == crate::services::ledger::Currency::Xof && phone.is_some() {
        crate::services::collect::Method::MobileMoney
    } else {
        crate::services::collect::Method::Card
    };

    let registry = crate::services::collect_adapters::registry_from_env();
    let provider = registry
        .resolve(&state.db, country.as_deref(), currency, method)
        .await?;

    let base = state.config.frontend_url.trim_end_matches('/').to_string();
    let success_url = format!("{base}/missions/{}?paid=1", mission.slug);
    let cancel_url = format!("{base}/missions/{}?canceled=1", mission.slug);
    // Keyed on the invoice: paying the same one twice is a mistake, paying
    // two months of a retainer is not.
    let idempotency_key = format!("mission_invoice:{}", invoice.id);
    let description = format!("Skilluv — {} ({})", mission.title, invoice.label);

    let session = crate::services::collect::start(
        &state.db,
        provider.as_ref(),
        method,
        crate::services::collect::CollectionRequest {
            payer_id: Some(auth.user_id),
            payer_enterprise_id: Some(enterprise.id),
            payer_email: &email,
            payer_name: &display_name,
            payer_country: country.as_deref(),
            payer_phone: phone.as_deref(),
            subject_type: "mission_invoice",
            subject_id: invoice.id,
            amount: &invoice.amount,
            currency,
            description: &description,
            success_url: &success_url,
            cancel_url: &cancel_url,
            idempotency_key: &idempotency_key,
            operator: None,
            credits: None,
            merchant_reference: None,
        },
    )
    .await?;

    Ok(Json(build_response(json!({
        "invoice_id": invoice.id,
        "checkout_url": session.redirect_url,
        "payment_id": session.payment_id,
        "provider": session.provider,
        "amount": invoice.amount,
        "currency": invoice.currency,
        "commission_percent": invoice.commission_percent,
    }))))
}

/// The caller's own missions: what they applied to, and what they are on.
#[utoipa::path(
    get, path = "/api/users/me/missions", tag = "missions",
    responses(
        (status = 200, body = serde_json::Value),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn my_missions(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let rows: Vec<(String, String, String, String, Option<String>)> = sqlx::query_as(
        r#"
        SELECT m.slug, m.title, m.status AS mission_status,
               a.status AS application_status, a.decision_reason
          FROM mission_applications a
          JOIN missions m ON m.id = a.mission_id
         WHERE a.user_id = $1
         ORDER BY a.created_at DESC
        "#,
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await?;

    let applications: Vec<Value> = rows
        .into_iter()
        .map(
            |(slug, title, mission_status, application_status, reason)| {
                json!({
                    "mission_slug": slug,
                    "mission_title": title,
                    "mission_status": mission_status,
                    "application_status": application_status,
                    "decision_reason": reason,
                })
            },
        )
        .collect();

    Ok(Json(build_response(json!({
        "applications": applications
    }))))
}

// ═══════════════════════════════════════════════════════════════════
// Delivery rounds
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct DeliverBody {
    /// Where the work is: an https link or a stored object.
    pub artifact_url: String,
    /// What changed since the previous round, and what to look at.
    #[serde(default)]
    pub notes_md: Option<String>,
}

/// Hand in a round.
///
/// Two or three rounds is the normal case for design work, not a failure. The
/// mission stays `in_progress` until a round is accepted — nothing about the
/// mission regresses, because the rounds live on the delivery.
#[utoipa::path(
    post, path = "/api/missions/{slug}/deliveries", tag = "missions",
    params(("slug" = String, Path)),
    request_body = DeliverBody,
    responses(
        (status = 201, description = "round handed in"),
        (status = 403, description = "not the person this mission is assigned to",
         body = crate::api_response::ErrorResponse),
        (status = 409, description = "not in progress, or a round is already waiting",
         body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn deliver_round(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(body): Json<DeliverBody>,
) -> Result<impl IntoResponse, AppError> {
    let mission = missions::by_slug(&state.db, &slug).await?;
    let delivery = crate::services::mission_delivery::deliver(
        &state.db,
        mission.id,
        auth.user_id,
        &body.artifact_url,
        body.notes_md.as_deref(),
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(build_response(json!({ "delivery": delivery }))),
    ))
}

/// Every round of a mission, oldest first — the trail an arbitration reads.
#[utoipa::path(
    get, path = "/api/missions/{slug}/deliveries", tag = "missions",
    params(("slug" = String, Path)),
    responses((status = 200, description = "the rounds, oldest first")),
    security(("cookie_auth" = [])),
)]
pub async fn list_rounds(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(slug): Path<String>,
) -> Result<Json<Value>, AppError> {
    let mission = missions::by_slug(&state.db, &slug).await?;
    let rounds = crate::services::mission_delivery::rounds_of(&state.db, mission.id).await?;
    Ok(Json(build_response(json!({ "rounds": rounds }))))
}

/// Accept the waiting round. The mission becomes `delivered`.
#[utoipa::path(
    post, path = "/api/missions/{slug}/deliveries/accept", tag = "missions",
    params(("slug" = String, Path)),
    responses(
        (status = 200, description = "accepted, and the mission is delivered"),
        (status = 403, description = "not a member of the enterprise that published it",
         body = crate::api_response::ErrorResponse),
        (status = 409, description = "no round is waiting",
         body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn accept_round(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
) -> Result<Json<Value>, AppError> {
    let mission = missions::by_slug(&state.db, &slug).await?;
    let delivery =
        crate::services::mission_delivery::accept(&state.db, mission.id, auth.user_id).await?;
    Ok(Json(build_response(json!({ "delivery": delivery }))))
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RequestChangesBody {
    /// What is wrong. At least twenty characters: "not quite" costs a round
    /// and teaches nothing.
    pub reason: String,
}

/// Ask for another round.
#[utoipa::path(
    post, path = "/api/missions/{slug}/deliveries/request-changes", tag = "missions",
    params(("slug" = String, Path)),
    request_body = RequestChangesBody,
    responses(
        (status = 200, description = "changes requested, and the mission stays in progress"),
        (status = 400, description = "no reason given", body = crate::api_response::ErrorResponse),
        (status = 409, description = "no round is waiting",
         body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn request_changes(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(body): Json<RequestChangesBody>,
) -> Result<Json<Value>, AppError> {
    let mission = missions::by_slug(&state.db, &slug).await?;
    let delivery = crate::services::mission_delivery::request_changes(
        &state.db,
        mission.id,
        auth.user_id,
        &body.reason,
    )
    .await?;
    Ok(Json(build_response(json!({ "delivery": delivery }))))
}

// ═══════════════════════════════════════════════════════════════════
// Ratings
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RateBody {
    /// 1 to 5.
    pub rating: i16,
    #[serde(default)]
    pub comment_md: Option<String>,
}

/// Rate the other side.
///
/// Written blind: nothing is readable until both sides have written, or until
/// fourteen days have passed. A rating one side can read before writing their
/// own is a negotiation, and a rating a silent client can suppress for ever is
/// worse.
#[utoipa::path(
    post, path = "/api/missions/{slug}/ratings", tag = "missions",
    params(("slug" = String, Path)),
    request_body = RateBody,
    responses(
        (status = 201, description = "recorded, and hidden until the other side writes"),
        (status = 403, description = "neither side of this mission",
         body = crate::api_response::ErrorResponse),
        (status = 409, description = "not delivered yet, or already rated",
         body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn rate(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(slug): Path<String>,
    Json(body): Json<RateBody>,
) -> Result<impl IntoResponse, AppError> {
    let mission = missions::by_slug(&state.db, &slug).await?;
    let rating = crate::services::mission_delivery::rate(
        &state.db,
        mission.id,
        auth.user_id,
        body.rating,
        body.comment_md.as_deref(),
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(build_response(json!({ "rating": rating }))),
    ))
}

/// The ratings on a mission, once they are readable.
///
/// An empty list while they are still blind: "nobody has said anything yet"
/// and "it is not your turn to read" look the same from outside, and the
/// difference is not worth leaking.
#[utoipa::path(
    get, path = "/api/missions/{slug}/ratings", tag = "missions",
    params(("slug" = String, Path)),
    responses((status = 200, description = "the ratings, or nothing while they are blind")),
)]
pub async fn list_ratings(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<Value>, AppError> {
    let mission = missions::by_slug(&state.db, &slug).await?;
    let ratings = crate::services::mission_delivery::ratings_of(&state.db, mission.id).await?;
    Ok(Json(build_response(json!({ "ratings": ratings }))))
}

/// What somebody's received ratings average to.
///
/// `average` is null rather than zero when there is nothing revealed yet: an
/// unrated person is not a badly rated one, and a zero on a profile would say
/// the opposite.
#[utoipa::path(
    get, path = "/api/users/{username}/mission-standing", tag = "missions",
    params(("username" = String, Path)),
    responses(
        (status = 200, description = "how many revealed ratings, and their average"),
        (status = 404, description = "no such account", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn standing(
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> Result<Json<Value>, AppError> {
    let user_id: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(&username)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("user '{username}' not found")))?;

    let standing = crate::services::mission_delivery::standing_of(&state.db, user_id).await?;
    Ok(Json(build_response(json!({ "standing": standing }))))
}
