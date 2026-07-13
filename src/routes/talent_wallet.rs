//! P13.1 — Endpoints wallet talent.
//!
//! - GET /api/users/me/wallet : solde EUR + XOF + statut providers.
//! - GET /api/users/me/wallet/transactions?limit=20 : ledger récent.
//! - POST /api/users/me/wallet/residency { country: "CI" } : déclare la
//!   résidence (utilisée pour choisir le canal payout par défaut).
//!
//! Les withdraw endpoints (Stripe / Momo) sont dans P13.2 et P13.3.

use axum::extract::{Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::talent_wallet;

pub fn talent_wallet_routes() -> Router<AppState> {
    Router::new()
        .route("/users/me/wallet", get(my_wallet))
        .route("/users/me/wallet/transactions", get(my_wallet_transactions))
        .route("/users/me/wallet/residency", post(set_my_residency))
}

fn build_response(data: Value) -> Value {
    json!({
        "data": data,
        "meta": {
            "request_id": uuid::Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })
}

#[derive(Debug, Deserialize)]
struct TxQuery {
    limit: Option<i64>,
}

async fn my_wallet(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let wallet = talent_wallet::get_or_init_wallet(&state.db, auth.user_id).await?;
    Ok(Json(build_response(json!({ "wallet": wallet }))))
}

async fn my_wallet_transactions(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<TxQuery>,
) -> Result<Json<Value>, AppError> {
    let txs = talent_wallet::list_transactions(
        &state.db,
        auth.user_id,
        q.limit.unwrap_or(20),
    )
    .await?;
    Ok(Json(build_response(json!({ "transactions": txs }))))
}

#[derive(Debug, Deserialize)]
struct ResidencyBody {
    country: String,
}

async fn set_my_residency(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<ResidencyBody>,
) -> Result<Json<Value>, AppError> {
    let wallet =
        talent_wallet::set_residency_country(&state.db, auth.user_id, &body.country).await?;
    Ok(Json(build_response(json!({ "wallet": wallet }))))
}
