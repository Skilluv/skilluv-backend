//! The Skilluv Code Awards — public reading, member nominations and votes,
//! curator shortlisting.
//!
//! Everything readable is public: the categories, the shortlist, the citation
//! behind each nomination, and the running count. An award whose standings
//! nobody can see is an announcement, and nobody believes announcements.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::awards;

pub fn award_routes() -> Router<AppState> {
    Router::new()
        .route("/awards/categories", get(list_categories))
        .route("/awards/{year}", get(edition_standings))
        .route("/awards/{year}/nominations", post(nominate))
        .route("/awards/nominees/{id}/vote", post(vote))
        .route("/awards/nominees/shortlist", post(shortlist))
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

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct CategoriesQuery {
    /// Narrow to one family's awards (plus the cross-cutting ones). One of the
    /// eight domains; omitted returns every category (SKI-314). The pattern
    /// keeps the schema as strict as the handler, so a contract fuzzer does not
    /// generate a 31-character "domain" the API then rejects.
    #[param(pattern = r"^(code|design|game|security|ops|ai|soft_skills|audio)$")]
    pub domain: Option<String>,
}

/// The award categories and what each one nominates. `?domain=` scopes to one
/// family's awards and the platform-wide ones.
#[utoipa::path(
    get, path = "/api/awards/categories", tag = "awards",
    params(CategoriesQuery),
    responses((status = 200, body = serde_json::Value)),
    operation_id = "awardsListCategories",
)]
pub async fn list_categories(
    State(state): State<AppState>,
    Query(q): Query<CategoriesQuery>,
) -> Result<Json<Value>, AppError> {
    crate::validators::check_skill_domain_opt(&q.domain, "domain")?;
    let categories = awards::categories(&state.db, q.domain.as_deref()).await?;
    Ok(Json(build_response(json!({ "categories": categories }))))
}

/// One edition: its state, its weights, and every nominee with the running
/// count behind it.
#[utoipa::path(
    get, path = "/api/awards/{year}", tag = "awards",
    // Bounded in the spec because the handler bounds it: an edition year is
    // stored as a smallint, and an unbounded integer in the contract promises
    // callers a range the database cannot hold.
    params(("year" = i32, Path, description = "Year the work happened in", minimum = 2000, maximum = 3000)),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "No edition for that year", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn edition_standings(
    State(state): State<AppState>,
    Path(year): Path<i32>,
) -> Result<Json<Value>, AppError> {
    let year =
        i16::try_from(year).map_err(|_| AppError::Validation("year is out of range".into()))?;
    let edition = awards::edition_of_year(&state.db, year).await?;
    let nominees = awards::nominees(&state.db, edition.id).await?;
    Ok(Json(build_response(
        json!({ "edition": edition, "nominees": nominees }),
    )))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct NominateBody {
    #[schema(max_length = 60)]
    pub category_slug: String,
    pub subject_id: Uuid,
    /// Why this deserves it. Required — voters cannot weigh a name.
    #[schema(max_length = 2000)]
    pub citation: String,
}

/// Put a piece of work forward, including your own.
#[utoipa::path(
    post, path = "/api/awards/{year}/nominations", tag = "awards",
    // Bounded in the spec because the handler bounds it: an edition year is
    // stored as a smallint, and an unbounded integer in the contract promises
    // callers a range the database cannot hold.
    params(("year" = i32, Path, description = "Year the work happened in", minimum = 2000, maximum = 3000)),
    request_body = NominateBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Nominations closed, or the subject is not what the category asks for", body = crate::api_response::ErrorResponse),
        (status = 404, description = "No edition or no such category", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn nominate(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(year): Path<i32>,
    Json(body): Json<NominateBody>,
) -> Result<Json<Value>, AppError> {
    let year =
        i16::try_from(year).map_err(|_| AppError::Validation("year is out of range".into()))?;
    let edition = awards::edition_of_year(&state.db, year).await?;
    let id = awards::nominate(
        &state.db,
        edition.id,
        auth.user_id,
        awards::NominateInput {
            category_slug: body.category_slug,
            subject_id: body.subject_id,
            citation: body.citation,
        },
    )
    .await?;
    Ok(Json(build_response(json!({ "nominee_id": id }))))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct VoteQuery {
    /// Cast this as a jury vote. Requires the `jury_tournament` capability;
    /// a juror also has a community vote, and casting one does not spend the
    /// other.
    #[serde(default)]
    pub jury: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct VoteAccepted {
    pub recorded: bool,
    pub ballot: String,
}

/// Vote for a shortlisted nominee.
#[utoipa::path(
    post, path = "/api/awards/nominees/{id}/vote", tag = "awards",
    params(("id" = Uuid, Path, description = "Nominee id"), VoteQuery),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 400, description = "Not shortlisted, edition not voting, or already voted", body = crate::api_response::ErrorResponse),
        (status = 403, description = "Not a juror", body = crate::api_response::ErrorResponse),
        (status = 404, description = "No such nominee", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn vote(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Query(q): Query<VoteQuery>,
) -> Result<Json<Value>, AppError> {
    awards::vote(&state.db, id, auth.user_id, q.jury).await?;
    Ok(Json(build_response(json!(VoteAccepted {
        recorded: true,
        ballot: if q.jury {
            "jury".into()
        } else {
            "community".into()
        },
    }))))
}

#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = AwardsShortlistBody)]
pub struct ShortlistBody {
    pub nominee_ids: Vec<Uuid>,
}

/// Fix the shortlist. Curators, not administrators: choosing which work
/// belongs on a ballot is an editorial judgement.
#[utoipa::path(
    post, path = "/api/awards/nominees/shortlist", tag = "awards",
    request_body = ShortlistBody,
    responses(
        (status = 200, body = serde_json::Value),
        (status = 403, description = "Not a curator", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn shortlist(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<ShortlistBody>,
) -> Result<Json<Value>, AppError> {
    crate::middleware::capabilities::require_any_capability(
        &state.db,
        auth.user_id,
        &["community_curator", "admin"],
    )
    .await?;
    let count = awards::shortlist(&state.db, &body.nominee_ids).await?;
    Ok(Json(build_response(json!({ "shortlisted": count }))))
}
