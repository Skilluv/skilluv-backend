//! SKI-45 (Post-MVP T3-02) — reverse marketplace endpoints.
//!
//! Endpoints:
//!   POST   /api/talent-offers          (auth, Artisan+)
//!   GET    /api/talent-offers          (public browse)
//!   GET    /api/users/me/talent-offers (auth)
//!   PATCH  /api/talent-offers/{id}     (owner)
//!   DELETE /api/talent-offers/{id}     (owner)
//!   GET    /api/admin/talent-offers   (moderator)
//!   POST   /api/admin/talent-offers/{id}/deactivate  (moderator)
//!   POST   /api/admin/talent-offers/{id}/reinstate   (moderator)
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

/// The five kinds an offer can be, as a type, for the same reason
/// `SkillDomain` exists: the document said `string`, the handler answered 400,
/// and both were right. Mirrors `services::talent_offers::OFFER_TYPES`, which
/// is what actually refuses a request; `the_offer_kinds_match_the_guard`
/// fails when they drift.
#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum OfferKind {
    PairProgramming,
    CodeReview,
    Whiteboard,
    MockInterview,
    CareerAdvice,
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct BrowseQuery {
    #[serde(default)]
    #[param(value_type = Option<OfferKind>)]
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

// ═══════════════════════════════════════════════════════════════════
// SKI-296 (T3-02b) — admin moderation
// ═══════════════════════════════════════════════════════════════════
//
// The owner-scoped surface left one abusive offer with no proportionate
// answer. `PATCH` and `DELETE` both filter on `user_id = $auth`, and the
// public browse only shows compliant offers, so an offer nobody could
// inspect was also an offer nobody could pull. The available lever was to
// ban the author or let them fall below Artisan — which takes down five
// offers to deal with one.
//
// So: a hold on the single offer, kept readable, plus a listing that can
// see what the browse hides. Both under `admin_gate`.

/// Capabilities allowed to moderate an offer. Same family as cohorts and
/// external signals — an offer description is user content.
const OFFER_MODERATOR_CAPS: &[&str] = &["admin", "community_moderator"];

/// Mounted behind `admin_gate` in `lib.rs`.
pub fn admin_talent_offer_routes() -> Router<AppState> {
    Router::new()
        .route("/admin/talent-offers", get(admin_browse))
        .route(
            "/admin/talent-offers/{id}/deactivate",
            post(admin_deactivate),
        )
        .route("/admin/talent-offers/{id}/reinstate", post(admin_reinstate))
}

#[derive(Debug, Deserialize)]
pub struct AdminBrowseQuery {
    #[serde(default)]
    pub offer_type: Option<String>,
    #[serde(default)]
    pub skill: Option<String>,
    #[serde(default)]
    pub user_id: Option<Uuid>,
    /// Include paused and held offers, and offers whose author is hidden,
    /// banned or below the rank bar.
    #[serde(default)]
    pub include_inactive: bool,
    /// Only offers currently under a moderation hold.
    #[serde(default)]
    pub held_only: bool,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
}

#[utoipa::path(
    get, path = "/api/admin/talent-offers", tag = "admin",
    operation_id = "adminTalentOffersList",
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn admin_browse(
    _gate: crate::middleware::admin_gate::AdminGate,
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<AdminBrowseQuery>,
) -> Result<impl IntoResponse, AppError> {
    crate::middleware::capabilities::require_any_capability(
        &state.db,
        auth.user_id,
        OFFER_MODERATOR_CAPS,
    )
    .await?;

    let limit = q.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let offset = q.offset.unwrap_or(0).max(0);

    let (offers, total) = talent_offers::admin_browse(
        &state.db,
        talent_offers::AdminBrowseFilter {
            offer_type: q.offer_type.as_deref(),
            skill_slug: q.skill.as_deref(),
            user_id: q.user_id,
            // `held_only` implies looking past the public filter; requiring
            // the caller to pass both flags would only be a way to get an
            // empty page by mistake.
            include_inactive: q.include_inactive || q.held_only,
            held_only: q.held_only,
            limit,
            offset,
        },
    )
    .await?;

    Ok(Json(wrap(json!({
        "offers": offers,
        "total": total,
        "limit": limit,
        "offset": offset,
    }))))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AdminDeactivateBody {
    /// At least 8 characters. It is shown to nobody automatically, but it
    /// is what an appeal is instructed against.
    pub reason: String,
}

/// Pull an offer from the marketplace.
///
/// Not a delete: the offer stays readable so a dispute over it can be
/// instructed against what was actually published, rather than against
/// somebody's recollection of it.
#[utoipa::path(
    post, path = "/api/admin/talent-offers/{id}/deactivate", tag = "admin",
    operation_id = "adminTalentOfferDeactivate",
    params(("id" = uuid::Uuid, Path)),
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn admin_deactivate(
    _gate: crate::middleware::admin_gate::AdminGate,
    State(state): State<AppState>,
    auth: AuthUser,
    headers: axum::http::HeaderMap,
    Path(id): Path<Uuid>,
    Json(body): Json<AdminDeactivateBody>,
) -> Result<impl IntoResponse, AppError> {
    crate::middleware::capabilities::require_any_capability(
        &state.db,
        auth.user_id,
        OFFER_MODERATOR_CAPS,
    )
    .await?;

    let offer = talent_offers::moderation_hold(&state.db, id, auth.user_id, &body.reason).await?;

    crate::services::audit::record(
        &state.db,
        crate::services::audit::AuditEntry {
            actor_type: crate::services::audit::ActorType::Admin,
            actor_id: Some(auth.user_id),
            action: "talent_offer.deactivate",
            target_type: Some("talent_offer"),
            target_id: Some(id),
            metadata: Some(json!({
                "author_id": offer.user_id,
                "offer_type": offer.offer_type,
                "reason": offer.moderation_reason,
            })),
            headers: Some(&headers),
        },
    )
    .await;

    Ok(Json(wrap(json!({ "offer": offer }))))
}

/// Lift a hold placed in error.
///
/// Deliberately a separate endpoint from the author's `PATCH`: the whole
/// value of the hold is that the person it was placed on cannot undo it.
#[utoipa::path(
    post, path = "/api/admin/talent-offers/{id}/reinstate", tag = "admin",
    operation_id = "adminTalentOfferReinstate",
    params(("id" = uuid::Uuid, Path)),
    responses((status = 200, body = serde_json::Value)),
    security(("cookie_auth" = [])),
)]
pub async fn admin_reinstate(
    _gate: crate::middleware::admin_gate::AdminGate,
    State(state): State<AppState>,
    auth: AuthUser,
    headers: axum::http::HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    crate::middleware::capabilities::require_any_capability(
        &state.db,
        auth.user_id,
        OFFER_MODERATOR_CAPS,
    )
    .await?;

    let offer = talent_offers::moderation_release(&state.db, id).await?;

    crate::services::audit::record(
        &state.db,
        crate::services::audit::AuditEntry {
            actor_type: crate::services::audit::ActorType::Admin,
            actor_id: Some(auth.user_id),
            action: "talent_offer.reinstate",
            target_type: Some("talent_offer"),
            target_id: Some(id),
            metadata: Some(json!({ "author_id": offer.user_id })),
            headers: Some(&headers),
        },
    )
    .await;

    Ok(Json(wrap(json!({ "offer": offer }))))
}

#[cfg(test)]
mod offer_kind_tests {
    use super::OfferKind;
    use crate::services::talent_offers::OFFER_TYPES;

    /// The document and the guard describe the same five kinds.
    #[test]
    fn the_offer_kinds_match_the_guard() {
        let schema = serde_json::to_value(<OfferKind as utoipa::PartialSchema>::schema()).unwrap();
        let documented: Vec<String> = schema["enum"]
            .as_array()
            .expect("a unit enum documents its values under `enum`")
            .iter()
            .map(|v| v.as_str().expect("each value is a string").to_string())
            .collect();
        assert_eq!(
            documented, OFFER_TYPES,
            "OfferKind and OFFER_TYPES have drifted"
        );
    }
}
