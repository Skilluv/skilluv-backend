//! Contesting a payment — the endpoints the release window promised.
//!
//! Every state change here notifies the other party, because a dispute is
//! the one flow where silence is the failure mode: money is frozen, a clock
//! is running, and the person who does not know cannot act.

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::disputes::{self, Dispute, Outcome};
use crate::services::notify::{self, Recipient};

pub fn dispute_routes() -> Router<AppState> {
    Router::new()
        .route("/disputes", get(mine).post(raise))
        .route("/disputes/{id}/concede", post(concede))
        .route("/disputes/{id}/contest", post(contest))
        .route("/disputes/{id}/withdraw", post(withdraw))
        .route("/admin/disputes", get(queue))
        .route("/admin/disputes/{id}/decide", post(decide))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RaiseRequest {
    /// What the payment was for: `mentorship_session`, `bounty_slice`, …
    pub subject_type: String,
    pub subject_id: Uuid,
    /// What went wrong, in the payer's own words. The recipient answers it.
    pub reason: String,
}

/// Freeze a payment and ask the recipient to answer.
#[utoipa::path(
    post, path = "/api/disputes", tag = "payments",
    request_body = RaiseRequest,
    responses(
        (status = 200, description = "Dispute opened, funds frozen", body = serde_json::Value),
        (status = 400, description = "Too late, or no reason given", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not the person who paid", body = crate::api_response::ErrorResponse),
        (status = 409, description = "Already disputed", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn raise(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<RaiseRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let id = disputes::raise(
        &state.db,
        &body.subject_type,
        body.subject_id,
        auth.user_id,
        &body.reason,
    )
    .await?;

    let hold = disputes::load(&state.db, id).await?;

    // The recipient has a clock running and cannot act on what they do not
    // know. Transactional on every channel for that reason.
    let _ = notify::send(
        &state,
        Recipient::User(hold.beneficiary_id),
        "dispute.opened",
    )
    .arg("amount", format!("{} {}", hold.amount, hold.currency))
    .arg("reason", body.reason.trim())
    .payload(json!({
        "dispute_id": id,
        "subject_type": hold.subject_type,
        "subject_id": hold.subject_id,
    }))
    .execute()
    .await;

    Ok(Json(
        json!({ "data": { "dispute_id": id, "status": "open" } }),
    ))
}

/// The recipient agrees. Refunds immediately, with no operator involved.
#[utoipa::path(
    post, path = "/api/disputes/{id}/concede", tag = "payments",
    params(("id" = Uuid, Path, description = "Dispute id")),
    responses(
        (status = 200, description = "Refunded", body = serde_json::Value),
        (status = 403, description = "Not the recipient", body = crate::api_response::ErrorResponse),
        (status = 409, description = "Not open", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn concede(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let hold = disputes::load(&state.db, id).await?;
    disputes::concede(&state.db, id, auth.user_id).await?;
    notify_resolution(&state, &hold, id, "refunded").await;
    Ok(Json(json!({ "data": { "status": "refunded" } })))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ContestRequest {
    /// The recipient's account of what happened.
    pub response: String,
}

/// The recipient disagrees. A human decides.
#[utoipa::path(
    post, path = "/api/disputes/{id}/contest", tag = "payments",
    params(("id" = Uuid, Path, description = "Dispute id")),
    request_body = ContestRequest,
    responses(
        (status = 200, description = "Sent to an operator", body = serde_json::Value),
        (status = 403, description = "Not the recipient", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn contest(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ContestRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    disputes::contest(&state.db, id, auth.user_id, &body.response).await?;

    let _ = notify::send(
        &state,
        Recipient::Capability("admin"),
        "dispute.needs_review",
    )
    .arg("count", "1")
    .payload(json!({ "dispute_id": id }))
    .execute()
    .await;

    Ok(Json(json!({ "data": { "status": "contested" } })))
}

/// The payer changes their mind. The money resumes its normal course.
#[utoipa::path(
    post, path = "/api/disputes/{id}/withdraw", tag = "payments",
    params(("id" = Uuid, Path, description = "Dispute id")),
    responses(
        (status = 200, description = "Withdrawn, funds released", body = serde_json::Value),
        (status = 403, description = "Not the person who raised it", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn withdraw(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let hold = disputes::load(&state.db, id).await?;
    disputes::withdraw(&state.db, id, auth.user_id).await?;
    notify_resolution(&state, &hold, id, "withdrawn").await;
    Ok(Json(json!({ "data": { "status": "withdrawn" } })))
}

/// Every dispute the caller is party to, from either side.
#[utoipa::path(
    get, path = "/api/disputes", tag = "payments",
    responses(
        (status = 200, description = "Disputes", body = ApiResponse<Vec<Dispute>>),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn mine(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<Vec<Dispute>>>, AppError> {
    Ok(Json(ApiResponse::new(
        disputes::for_user(&state.db, auth.user_id).await?,
    )))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct DecideRequest {
    /// `payer` refunds; `recipient` hands the money over.
    pub in_favour_of: String,
    /// Both parties read this. Required.
    pub note: String,
}

/// Decide a contested dispute.
#[utoipa::path(
    post, path = "/api/admin/disputes/{id}/decide", tag = "admin",
    params(("id" = Uuid, Path, description = "Dispute id")),
    request_body = DecideRequest,
    responses(
        (status = 200, description = "Decided", body = serde_json::Value),
        (status = 400, description = "Unknown outcome, or no note", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not an operator", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn decide(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<DecideRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;

    let outcome = match body.in_favour_of.as_str() {
        "payer" => Outcome::Payer,
        "recipient" => Outcome::Recipient,
        other => {
            return Err(AppError::Validation(format!(
                "in_favour_of must be 'payer' or 'recipient', not '{other}'"
            )));
        }
    };

    let hold = disputes::load(&state.db, id).await?;
    disputes::decide(&state.db, id, auth.user_id, outcome, &body.note).await?;

    let status = match outcome {
        Outcome::Payer => "refunded",
        Outcome::Recipient => "released",
    };
    notify_resolution(&state, &hold, id, status).await;

    Ok(Json(json!({ "data": { "status": status } })))
}

/// The operator queue: contested disputes, oldest first.
#[utoipa::path(
    get, path = "/api/admin/disputes", tag = "admin",
    responses(
        (status = 200, description = "Contested disputes", body = serde_json::Value),
        (status = 403, description = "Not an operator", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn queue(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;

    /// One line of the operator queue.
    #[derive(sqlx::FromRow, serde::Serialize)]
    struct Awaiting {
        id: Uuid,
        reason: String,
        recipient_response: Option<String>,
        /// Text rather than a number: the queue is read, not summed, and a
        /// float would round money on the way to a screen.
        amount: String,
        currency: String,
        created_at: chrono::DateTime<chrono::Utc>,
    }

    let disputes: Vec<Awaiting> = sqlx::query_as(
        "SELECT d.id, d.reason, d.recipient_response,
                p.amount::TEXT AS amount, p.currency, d.created_at
           FROM disputes d
           JOIN pending_releases p ON p.id = d.pending_release_id
          WHERE d.status = 'contested'
          ORDER BY d.created_at
          LIMIT 200",
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(json!({ "data": { "disputes": disputes } })))
}

/// Tell both sides how it ended.
///
/// Both, always. The one who lost needs to know why more than the one who
/// won, and telling only the winner is how a platform earns a reputation
/// for deciding things in the dark.
async fn notify_resolution(
    state: &AppState,
    hold: &disputes::DisputedHold,
    dispute_id: Uuid,
    status: &str,
) {
    let payload = json!({
        "dispute_id": dispute_id,
        "subject_type": hold.subject_type,
        "subject_id": hold.subject_id,
    });

    let mut recipients = vec![hold.beneficiary_id];
    if let Some(payer) = hold.payer_id {
        recipients.push(payer);
    }

    // One kind per outcome rather than one kind with the outcome as an
    // argument: a word passed in is a word in one language, and it would
    // land untranslated inside a translated sentence.
    let kind = match status {
        "refunded" => "dispute.refunded",
        "released" => "dispute.released",
        _ => "dispute.withdrawn",
    };

    for user in recipients {
        let _ = notify::send(state, Recipient::User(user), kind)
            .arg("amount", format!("{} {}", hold.amount, hold.currency))
            .payload(payload.clone())
            .execute()
            .await;
    }
}
