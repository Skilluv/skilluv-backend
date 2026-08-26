//! SKI-45 (Post-MVP T3-02) — reverse marketplace endpoints.
//!
//! Endpoints:
//!   POST   /api/talent-offers          (auth, Artisan+)
//!   GET    /api/talent-offers          (public browse)
//!   GET    /api/users/me/talent-offers (auth)
//!   PATCH  /api/talent-offers/{id}     (owner)
//!   DELETE /api/talent-offers/{id}     (owner)
//!
//! Browse is public and unauthenticated on purpose: the point of inverting
//! the marketplace is that the offer finds the person, and requiring a
//! login to see who is available defeats it.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::talent_offers;

const MAX_LIMIT: i64 = 100;
const DEFAULT_LIMIT: i64 = 50;

pub fn talent_offer_routes() -> Router<AppState> {
    Router::new()
        .route("/talent-offers", post(create).get(browse))
        .route(
            "/talent-offers/{id}",
            axum::routing::patch(update).delete(remove),
        )
        .route("/users/me/talent-offers", get(list_mine))
}

fn wrap(data: serde_json::Value) -> serde_json::Value {
    json!({
        "data": data,
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })
}

/// Serde helper: missing field → `None`, JSON `null` → `Some(None)`,
/// value → `Some(Some(v))`. Needed so PATCH can tell "leave the price
/// alone" from "make this offer free".
fn deserialize_double_option<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: serde::Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    Ok(Some(Option::<T>::deserialize(deserializer)?))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateOfferBody {
    pub offer_type: String,
    #[serde(default)]
    pub skill_id: Option<Uuid>,
    /// Hours per week, 1..20. Defaults to 2.
    #[serde(default)]
    pub availability_hours: Option<i16>,
    /// Omit or null for a free offer. A price requires a verified payout
    /// account.
    #[serde(default)]
    pub price_cents_per_hour: Option<i64>,
    #[serde(default)]
    pub description: Option<String>,
}

/// Publish an offer to teach, review or mentor.
#[utoipa::path(
    post, path = "/api/talent-offers",
    operation_id = "talentOffersCreate",
    tag = "opportunities",
    request_body = CreateOfferBody,
    responses(
        (status = 201, description = "Published"),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn create(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateOfferBody>,
) -> Result<impl IntoResponse, AppError> {
    let description = body.description.unwrap_or_default();
    let offer = talent_offers::create(
        &state.db,
        auth.user_id,
        talent_offers::CreateOfferParams {
            offer_type: &body.offer_type,
            skill_id: body.skill_id,
            availability_hours: body.availability_hours.unwrap_or(2),
            price_cents_per_hour: body.price_cents_per_hour,
            description: &description,
        },
    )
    .await?;

    Ok((StatusCode::CREATED, Json(wrap(json!({ "offer": offer })))))
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct BrowseQuery {
    #[serde(default)]
    pub offer_type: Option<String>,
    /// Filter by skill slug (not id — this is a public browse surface).
    #[serde(default)]
    pub skill: Option<String>,
    /// Only free offers.
    #[serde(default)]
    pub free_only: bool,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
}

/// Public listing of what people offer to teach, review or mentor.
#[utoipa::path(
    get, path = "/api/talent-offers", tag = "opportunities",
    params(BrowseQuery),
    responses((status = 200, body = serde_json::Value)),
)]
pub async fn browse(
    State(state): State<AppState>,
    Query(q): Query<BrowseQuery>,
) -> Result<impl IntoResponse, AppError> {
    let limit = q.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let offset = q.offset.unwrap_or(0).max(0);

    let offers = talent_offers::browse(
        &state.db,
        talent_offers::BrowseFilter {
            offer_type: q.offer_type.as_deref(),
            skill_slug: q.skill.as_deref(),
            free_only: q.free_only,
            limit,
            offset,
        },
    )
    .await?;

    Ok(Json(wrap(json!({
        "offers": offers,
        "limit": limit,
        "offset": offset,
    }))))
}

/// The offers the caller published, including the closed ones.
#[utoipa::path(
    get, path = "/api/users/me/talent-offers",
    operation_id = "talentOffersListMine",
    tag = "opportunities",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn list_mine(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<impl IntoResponse, AppError> {
    let offers: Vec<talent_offers::TalentOffer> =
        sqlx::query_as("SELECT * FROM talent_offers WHERE user_id = $1 ORDER BY created_at DESC")
            .bind(auth.user_id)
            .fetch_all(&state.db)
            .await?;

    // Echoed so the client can explain why a paused offer cannot be
    // re-activated, instead of only failing on the attempt.
    let can_publish = talent_offers::assert_can_publish(&state.db, auth.user_id)
        .await
        .is_ok();

    Ok(Json(wrap(json!({
        "offers": offers,
        "can_publish": can_publish,
        "min_rank": talent_offers::MIN_RANK,
    }))))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateOfferBody {
    #[serde(default)]
    pub availability_hours: Option<i16>,
    /// Explicit `null` makes the offer free; omitting leaves it unchanged.
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub price_cents_per_hour: Option<Option<i64>>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub active: Option<bool>,
}

/// Change one of the caller's offers.
#[utoipa::path(
    patch, path = "/api/talent-offers/{id}",
    operation_id = "talentOffersUpdate",
    tag = "opportunities",
    params(("id" = uuid::Uuid, Path)),
    request_body = UpdateOfferBody,
    responses(
        (status = 200, description = "Updated"),
        (status = 404, description = "No offer of yours with that id", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn update(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateOfferBody>,
) -> Result<impl IntoResponse, AppError> {
    let offer = talent_offers::update(
        &state.db,
        id,
        auth.user_id,
        body.availability_hours,
        body.price_cents_per_hour,
        body.description.as_deref(),
        body.active,
    )
    .await?;
    Ok(Json(wrap(json!({ "offer": offer }))))
}

/// Withdraw one of the caller's offers.
#[utoipa::path(
    delete, path = "/api/talent-offers/{id}",
    operation_id = "talentOffersRemove",
    tag = "opportunities",
    params(("id" = uuid::Uuid, Path)),
    responses(
        (status = 204, description = "Withdrawn"),
        (status = 404, description = "No offer of yours with that id", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn remove(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let affected = sqlx::query("DELETE FROM talent_offers WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(auth.user_id)
        .execute(&state.db)
        .await?
        .rows_affected();
    if affected == 0 {
        return Err(AppError::NotFound(format!("offer {id} not found")));
    }
    Ok(StatusCode::NO_CONTENT)
}
