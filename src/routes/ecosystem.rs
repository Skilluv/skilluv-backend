//! The ecosystem line: labels, the creators marketplace, sponsored cohorts.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::ecosystem;

pub fn ecosystem_routes() -> Router<AppState> {
    Router::new()
        // Public.
        .route("/certifications/programs", get(list_programs))
        .route("/certifications/live", get(list_live))
        .route("/marketplace/items", get(list_items).post(list_item))
        .route("/marketplace/items/{id}", get(read_item))
        .route("/marketplace/items/{id}/publish", post(publish_item))
        // Buyers.
        .route("/marketplace/items/{id}/purchase", post(purchase))
        .route("/marketplace/downloads/{token}", get(download))
        .route("/marketplace/uploads", post(request_upload))
        .route("/marketplace/purchases/{id}/rate", post(rate))
        // Applicants.
        .route("/certifications/request", post(request_certification))
}

pub fn admin_ecosystem_routes() -> Router<AppState> {
    Router::new()
        .route("/admin/certifications/{id}/audit", post(audit))
        .route("/admin/certifications/{id}/revoke", post(revoke))
        .route("/admin/certifications/expire-lapsed", post(expire_lapsed))
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
// Certifications
// ═══════════════════════════════════════════════════════════════════

/// What can be certified, at what price, against what pass mark.
///
/// The pass mark is published because a certification whose bar is private
/// is a certification nobody can judge the worth of.
#[utoipa::path(
    get, path = "/api/certifications/programs", tag = "work",
    responses((status = 200, body = serde_json::Value)),
)]
pub async fn list_programs(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let programs = ecosystem::programs(&state.db).await?;
    Ok(Json(build_response(json!({ "programs": programs }))))
}

/// Every certification currently live, so a badge can be checked rather than
/// believed.
#[utoipa::path(
    get, path = "/api/certifications/live", tag = "work",
    responses((status = 200, body = serde_json::Value)),
)]
pub async fn list_live(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let certifications = ecosystem::live_certifications(&state.db).await?;
    Ok(Json(build_response(
        json!({ "certifications": certifications }),
    )))
}

#[utoipa::path(
    post, path = "/api/certifications/request", tag = "work",
    request_body(content = serde_json::Value, description = "CertificationInput"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "A subject the programme does not certify, or one already live", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn request_certification(
    State(state): State<AppState>,
    _auth: AuthUser,
    Json(input): Json<ecosystem::CertificationInput>,
) -> Result<Json<Value>, AppError> {
    let certification = ecosystem::request_certification(&state.db, input).await?;
    Ok(Json(build_response(json!({
        "certification": certification,
        // Said at the moment of ordering, because it is the part a buyer
        // assumes the other way round.
        "note": "Le paiement ne certifie pas. L'audit décide, et un échec laisse \
                 les frais engagés sans label.",
        "note_code": "payment_does_not_certify",
    }))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AuditBody {
    /// Every criterion looked at, with what the score rests on.
    #[schema(value_type = Vec<Object>)]
    pub findings: Vec<ecosystem::Finding>,
    #[serde(default)]
    pub notes: Option<String>,
}

/// Record an audit and decide in one call.
#[utoipa::path(
    post, path = "/api/admin/certifications/{id}/audit", tag = "admin",
    params(("id" = Uuid, Path, description = "Certification id")),
    request_body = AuditBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "No findings, or a criterion scored without evidence", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn audit(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<AuditBody>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let certification = ecosystem::audit(
        &state.db,
        id,
        auth.user_id,
        body.findings,
        body.notes.as_deref(),
    )
    .await?;

    metrics::counter!(
        "skilluv_certification_audits_total",
        "outcome" => certification.status.clone()
    )
    .increment(1);
    Ok(Json(build_response(
        json!({ "certification": certification }),
    )))
}

#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = EcosystemReasonBody)]
pub struct ReasonBody {
    pub reason: String,
}

#[utoipa::path(
    post, path = "/api/admin/certifications/{id}/revoke", tag = "admin",
    params(("id" = Uuid, Path, description = "Certification id")),
    request_body = ReasonBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "No reason given", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn revoke(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ReasonBody>,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    ecosystem::revoke(&state.db, id, &body.reason).await?;
    Ok(Json(build_response(json!({ "revoked": true }))))
}

#[utoipa::path(
    post, path = "/api/admin/certifications/expire-lapsed", tag = "admin",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn expire_lapsed(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    crate::routes::admin::require_admin(&state, &auth).await?;
    let expired = ecosystem::expire_lapsed(&state.db).await?;
    Ok(Json(build_response(json!({ "expired": expired }))))
}

// ═══════════════════════════════════════════════════════════════════
// The marketplace
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct ItemsQuery {
    #[serde(default)]
    pub domain: Option<String>,
}

#[utoipa::path(
    get, path = "/api/marketplace/items", tag = "work",
    params(ItemsQuery),
    responses((status = 200, body = serde_json::Value)),
)]
pub async fn list_items(
    State(state): State<AppState>,
    Query(q): Query<ItemsQuery>,
) -> Result<Json<Value>, AppError> {
    let items = ecosystem::published_items(&state.db, q.domain.as_deref()).await?;
    Ok(Json(build_response(json!({ "items": items }))))
}

#[utoipa::path(
    get, path = "/api/marketplace/items/{id}", tag = "work",
    params(("id" = Uuid, Path, description = "Item id")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "No such item", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn read_item(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let item = ecosystem::item(&state.db, id).await?;
    let (_, commission, payout) = ecosystem::split_sale(&item.price);
    Ok(Json(build_response(json!({
        "item": item,
        // Shown on the item's own page so a creator can work out their take
        // before listing rather than after selling.
        "creator_receives": payout,
        "platform_commission": commission,
    }))))
}

#[utoipa::path(
    post, path = "/api/marketplace/items", tag = "work",
    request_body(content = serde_json::Value, description = "ItemInput"),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "No files, no readable licence, a taken slug, or no price", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn list_item(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<ecosystem::ItemInput>,
) -> Result<Json<Value>, AppError> {
    let item = ecosystem::list_item(&state.db, auth.user_id, input).await?;
    Ok(Json(build_response(json!({ "item": item }))))
}

#[utoipa::path(
    post, path = "/api/marketplace/items/{id}/publish", tag = "work",
    params(("id" = Uuid, Path, description = "Item id")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "Not your item", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn publish_item(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let item = ecosystem::item(&state.db, id).await?;
    if item.creator_user_id != auth.user_id {
        return Err(AppError::NotFound("item not found".into()));
    }
    let item = ecosystem::publish_item(&state.db, id).await?;
    Ok(Json(build_response(json!({ "item": item }))))
}

#[utoipa::path(
    post, path = "/api/marketplace/items/{id}/purchase", tag = "work",
    params(("id" = Uuid, Path, description = "Item id")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Not on sale, or your own item", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn purchase(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let (purchase_id, token, _payout) =
        ecosystem::purchase(&state.db, id, Some(auth.user_id), None).await?;

    metrics::counter!("skilluv_marketplace_sales_total").increment(1);
    Ok(Json(build_response(json!({
        "purchase_id": purchase_id,
        "download_url": format!("/api/marketplace/downloads/{token}"),
        "valid_for_hours": ecosystem::DOWNLOAD_WINDOW_HOURS,
        "downloads_allowed": ecosystem::DOWNLOAD_LIMIT,
    }))))
}

/// Redeem a download token.
#[utoipa::path(
    get, path = "/api/marketplace/downloads/{token}", tag = "work",
    params(("token" = String, Path, description = "Download token")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Too many downloads", body = crate::api_response::ErrorResponse),
        (status = 404, description = "Expired or unknown", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn download(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(token): Path<String>,
) -> Result<Json<Value>, AppError> {
    let files = ecosystem::redeem_download(&state.db, &state.storage, &token).await?;
    Ok(Json(build_response(json!({ "files": files }))))
}

#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = MarketplaceUploadBody)]
pub struct UploadBody {
    /// The file's name, used only to make the stored key readable. The bytes
    /// go to the returned presigned URL, not through this call.
    pub filename: String,
}

/// Ask for somewhere to put a marketplace file.
///
/// Returns a presigned PUT and the key to name in `file_keys` when creating the
/// item. The marketplace had no upload path, so a creator could not deposit an
/// item from the front at all (SKI-330).
#[utoipa::path(
    post, path = "/api/marketplace/uploads", tag = "work",
    request_body = UploadBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "No filename", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn request_upload(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<UploadBody>,
) -> Result<Json<Value>, AppError> {
    if body.filename.trim().is_empty() {
        return Err(AppError::Validation("a file needs a name".into()));
    }
    crate::validators::check_max_len(&body.filename, "filename", 255)?;
    let target =
        ecosystem::upload_target(&state.storage, auth.user_id, body.filename.trim()).await?;
    Ok(Json(build_response(json!({ "upload": target }))))
}

#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = EcosystemRatingBody)]
pub struct RatingBody {
    pub rating: i16,
    #[serde(default)]
    pub review: Option<String>,
}

/// Rate something you bought.
#[utoipa::path(
    post, path = "/api/marketplace/purchases/{id}/rate", tag = "work",
    params(("id" = Uuid, Path, description = "Purchase id")),
    request_body = RatingBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "A rating outside 1 to 5", body = crate::api_response::ErrorResponse),
        (status = 404, description = "No purchase of yours here", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
    operation_id = "ecosystemRate",
)]
pub async fn rate(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<RatingBody>,
) -> Result<Json<Value>, AppError> {
    ecosystem::rate(
        &state.db,
        id,
        auth.user_id,
        body.rating,
        body.review.as_deref(),
    )
    .await?;
    Ok(Json(build_response(json!({ "rated": body.rating }))))
}
