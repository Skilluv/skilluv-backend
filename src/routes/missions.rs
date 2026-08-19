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
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::missions;

pub fn mission_routes() -> Router<AppState> {
    Router::new()
        .route("/missions/types", get(list_types))
        .route("/missions", get(list_missions).post(create_mission))
        .route("/missions/{slug}", get(get_mission))
        .route("/missions/{slug}/apply", post(apply_to_mission))
        .route("/missions/{slug}/applications", get(list_applications))
        .route("/missions/{slug}/status", post(set_mission_status))
        .route("/mission-applications/{id}/decision", post(decide))
        .route(
            "/missions/{slug}/invoices",
            get(list_invoices).post(issue_invoice),
        )
        .route("/mission-invoices/{id}/checkout", post(pay_invoice))
        .route("/users/me/missions", get(my_missions))
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
