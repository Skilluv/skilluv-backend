use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::services::geo::Country;

pub fn geo_routes() -> Router<AppState> {
    Router::new()
        .route("/geo/countries", get(list_countries))
        .route("/geo/cities", get(search_cities))
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct CitiesQuery {
    /// ISO 3166-1 alpha-2 or alpha-3 country code (e.g. `SN` or `SEN`).
    pub country: String,
    /// Optional case-insensitive prefix/substring filter on city name.
    pub q: Option<String>,
    /// Max rows to return, clamped to `[1, 50]`. Defaults to 20.
    pub limit: Option<usize>,
}

/// Owned city view returned by the search endpoint. Population is
/// pulled from the GeoNames dump baked into the binary.
#[derive(Debug, Serialize, ToSchema)]
pub struct CityOut {
    pub name: String,
    /// ISO 3166-1 alpha-2 country code.
    pub country: String,
    pub population: i64,
}

/// List every country the platform knows about (baked-in GeoNames
/// dataset). Used by the register / profile / talent-search forms.
#[utoipa::path(
    get,
    path = "/api/geo/countries",
    tag = "profile",
    responses(
        (status = 200, description = "Full country list", body = ApiResponse<Vec<Country>>),
    ),
)]
pub async fn list_countries(State(state): State<AppState>) -> Json<ApiResponse<Vec<Country>>> {
    Json(ApiResponse::new(state.geo.countries().to_vec()))
}

/// Search cities within a country by optional name prefix. Sorted by
/// population desc — the first hit is the largest match. Bounded to
/// 50 results max.
#[utoipa::path(
    get,
    path = "/api/geo/cities",
    tag = "profile",
    params(CitiesQuery),
    responses(
        (status = 200, description = "Matching cities", body = ApiResponse<Vec<CityOut>>),
        (status = 400, description = "Missing country param", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn search_cities(
    State(state): State<AppState>,
    Query(q): Query<CitiesQuery>,
) -> Result<Json<ApiResponse<Vec<CityOut>>>, AppError> {
    if q.country.trim().is_empty() {
        return Err(AppError::Validation(
            "country query parameter is required (ISO2 or ISO3)".into(),
        ));
    }
    let limit = q.limit.unwrap_or(20).clamp(1, 50);
    let results: Vec<CityOut> = state
        .geo
        .search_cities(&q.country, q.q.as_deref(), limit)
        .into_iter()
        .map(|c| CityOut {
            name: c.name.clone(),
            country: c.country.clone(),
            population: c.population,
        })
        .collect();
    Ok(Json(ApiResponse::new(results)))
}
