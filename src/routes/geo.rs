use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;

pub fn geo_routes() -> Router<AppState> {
    Router::new()
        .route("/geo/countries", get(list_countries))
        .route("/geo/cities", get(search_cities))
}

fn build_response(data: serde_json::Value) -> serde_json::Value {
    json!({
        "data": data,
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })
}

#[derive(Debug, Deserialize)]
struct CitiesQuery {
    country: String,
    q: Option<String>,
    limit: Option<usize>,
}

#[derive(Debug, Serialize)]
struct CityOut<'a> {
    name: &'a str,
    country: &'a str,
    population: i64,
}

async fn list_countries(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(build_response(json!(state.geo.countries())))
}

async fn search_cities(
    State(state): State<AppState>,
    Query(q): Query<CitiesQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
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
            name: &c.name,
            country: &c.country,
            population: c.population,
        })
        .collect();
    Ok(Json(build_response(json!(results))))
}
