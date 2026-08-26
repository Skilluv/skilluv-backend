//! Connecting a design tool, and reading what a pasted link points at.
//!
//! The reading half works today and needs nothing. The connecting half is
//! complete up to the wall: Skilluv has no developer account on Figma, Miro
//! or Webflow, so the two calls that need a client secret answer 503 naming
//! the variable that is missing rather than failing obscurely.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::services::design_cloud::{self, Provider};

pub fn design_cloud_routes() -> Router<AppState> {
    Router::new()
        .route("/design/cloud/connections", get(list_connections))
        .route("/design/cloud/{provider}/start", get(start))
        .route("/design/cloud/{provider}/disconnect", post(disconnect))
        .route("/design/cloud/inspect", get(inspect))
}

/// What this person has connected.
#[utoipa::path(
    get, path = "/api/design/cloud/connections", tag = "design",
    responses(
        (status = 200, body = ApiResponse<Vec<design_cloud::Connection>>),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn list_connections(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<Vec<design_cloud::Connection>>>, AppError> {
    let connections = design_cloud::list_for(&state.db, auth.user_id).await?;
    Ok(Json(ApiResponse::new(connections)))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct StartResponse {
    /// Where to send the person to approve the connection.
    pub authorize_url: String,
    /// Echoed back and checked at the callback. Without it, anybody could
    /// hand somebody a callback URL and attach their own account.
    pub state: String,
}

/// Begin connecting a tool.
///
/// Answers 503 when the deployment has no credentials for that provider,
/// naming the variable. A button that silently does nothing is worse than a
/// button that says why.
#[utoipa::path(
    get, path = "/api/design/cloud/{provider}/start", tag = "design",
    params(("provider" = String, Path, description = "figma, miro or webflow")),
    responses(
        (status = 200, body = ApiResponse<StartResponse>),
        (status = 400, description = "A tool with no OAuth flow", body = crate::api_response::ErrorResponse),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
        (status = 503, description = "Not configured on this deployment", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn start(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(provider): Path<String>,
) -> Result<Json<ApiResponse<StartResponse>>, AppError> {
    let provider = Provider::parse(&provider)?;

    // Bound to the person, so a callback cannot attach somebody else's
    // account, and unguessable, so it cannot be forged.
    let state_token = format!("{}:{}", auth.user_id, uuid::Uuid::new_v4());

    let redirect_uri = format!(
        "{}/api/design/cloud/{}/callback",
        state.config.base_url.trim_end_matches('/'),
        provider.as_str()
    );

    let authorize_url = design_cloud::authorize_url(provider, &redirect_uri, &state_token)?;

    Ok(Json(ApiResponse::new(StartResponse {
        authorize_url,
        state: state_token,
    })))
}

/// Disconnect a tool, wiping its tokens.
#[utoipa::path(
    post, path = "/api/design/cloud/{provider}/disconnect",
    operation_id = "designCloudDisconnect",
    tag = "design",
    params(("provider" = String, Path, description = "figma, miro or webflow")),
    responses(
        (status = 204, description = "Disconnected, or was not connected"),
        (status = 400, description = "A tool with no OAuth flow", body = crate::api_response::ErrorResponse),
        (status = 401, description = "Unauthenticated", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn disconnect(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(provider): Path<String>,
) -> Result<axum::http::StatusCode, AppError> {
    let provider = Provider::parse(&provider)?;
    design_cloud::revoke(&state.db, auth.user_id, provider).await?;
    // 204 whether or not there was one: disconnecting something already
    // disconnected is not an error, and answering 404 would tell a caller
    // whether an account was connected.
    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct InspectQuery {
    #[param(max_length = 2048)]
    pub url: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct InspectResponse {
    /// Null when the URL is not a design tool link at all.
    pub source: Option<design_cloud::CloudSource>,
    /// Said plainly, because the person pasting is the only one who can fix
    /// it and the moment they paste is the only moment they will.
    pub warning: Option<String>,
}

/// Read a pasted link.
///
/// Public and unauthenticated: it parses a string and touches nothing. The
/// point is that the front can warn about a private Figma link *before* a
/// deliverable is submitted rather than after a reviewer has failed to open
/// it.
#[utoipa::path(
    get, path = "/api/design/cloud/inspect", tag = "design",
    params(InspectQuery),
    responses(
        (status = 200, body = ApiResponse<InspectResponse>),
        (status = 400, description = "URL too long", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn inspect(
    Query(q): Query<InspectQuery>,
) -> Result<Json<ApiResponse<InspectResponse>>, AppError> {
    crate::validators::check_max_len(&q.url, "url", 2048)?;

    let source = design_cloud::read_url(&q.url);

    let warning = match &source {
        None => Some(
            "Ce lien ne pointe vers aucun outil de design connu. Vérifie que c'est bien \
             l'adresse du livrable."
                .to_string(),
        ),
        Some(source) if !source.opens_without_account => Some(format!(
            "Un lien {} n'est visible que si le fichier est partagé publiquement. Vérifie le \
             partage avant de rendre : un relecteur qui ne peut pas ouvrir ton travail ne peut \
             pas le valider.",
            source.provider
        )),
        Some(_) => None,
    };

    Ok(Json(ApiResponse::new(InspectResponse { source, warning })))
}
