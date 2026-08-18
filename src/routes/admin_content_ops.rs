//! Content ops admin endpoints — declencheurs manuels des services background.
//!
//! Priorite basse strategy doc : donner a l'admin panel un moyen de forcer
//! l'execution des services habituellement lances en cron (mirror hello wall,
//! sync profile readme, recompute badges d'un user). Utile en cas de :
//!
//! - Debug d'un user particulier (badge non decroche, README pas sync)
//! - Post-incident (cron down pendant N heures, recuperer le retard)
//! - Bulk retry (queue GitHub bloquee par rate limit, relance apres)
//!
//! Toutes les routes sont admin-gated (le nesting via admin_gate() applique
//! Origin + 2FA). En plus, chaque handler check `require_capability("admin")`
//! au cas ou un futur middleware s'egarerait.

use axum::extract::{Path, State};
use axum::routing::post;
use axum::{Json, Router};
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::middleware::AuthUser;
use crate::routes::admin::require_admin;
use crate::services::{badge_engine, hello_wall_mirror, profile_readme_sync};

pub fn admin_content_ops_routes() -> Router<AppState> {
    Router::new()
        .route("/admin/hello-wall/mirror-run", post(hello_wall_mirror_run))
        .route(
            "/admin/profile-readme/sync-run",
            post(profile_readme_sync_run),
        )
        .route(
            "/admin/badges/recompute/{user_id}",
            post(recompute_badges_for_user),
        )
}


#[derive(Debug, Serialize, ToSchema)]
pub struct MirrorFailedDetail {
    pub id: Uuid,
    pub error: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct HelloWallMirrorReport {
    pub mirrored: usize,
    pub failed: usize,
    pub skipped: usize,
    pub mirrored_ids: Vec<Uuid>,
    pub failed_details: Vec<MirrorFailedDetail>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ProfileReadmeSyncReport {
    pub synced: usize,
    pub failed: usize,
    pub skipped_no_readme: usize,
    pub synced_ids: Vec<Uuid>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BadgeRecomputeReport {
    pub awarded: Vec<String>,
    pub revoked: Vec<String>,
    pub unchanged: usize,
}

/// Reused wrapper for the 3 content-ops responses: legacy shape kept
/// `{ ok: true, data: ... }` — front branches on `ok` to distinguish
/// success vs the ApiResponse envelope other endpoints use. Kept as-is
/// to avoid breaking the admin panel.
#[derive(Debug, Serialize, ToSchema)]
pub struct AdminOpsEnvelope<T: ToSchema> {
    pub ok: bool,
    pub data: T,
}

/// POST /api/admin/hello-wall/mirror-run
///
/// Prend les entrees `hello_wall_entries` `mirrored_at IS NULL` et pousse
/// chacune vers `skilluv-community/hello-wall/entries/{username}.md`.
/// Necessite `SKILLUV_BOT_GITHUB_TOKEN` en env — sans lui, renvoie 503.
#[utoipa::path(
    post,
    path = "/api/admin/hello-wall/mirror-run",
    tag = "admin",
    responses(
        (status = 200, description = "Mirror run report", body = AdminOpsEnvelope<HelloWallMirrorReport>),
        (status = 403, description = "Not an admin", body = crate::api_response::ErrorResponse),
        (status = 500, description = "SKILLUV_BOT_GITHUB_TOKEN env missing", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn hello_wall_mirror_run(
    _gate: crate::middleware::admin_gate::AdminGate,
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<AdminOpsEnvelope<HelloWallMirrorReport>>, AppError> {
    require_admin(&state, &auth).await?;

    let token = std::env::var("SKILLUV_BOT_GITHUB_TOKEN").map_err(|_| {
        AppError::Internal(
            "SKILLUV_BOT_GITHUB_TOKEN env var manquant — mirror impossible. Configurez la variable dans l'environnement backend.".into(),
        )
    })?;

    let report = hello_wall_mirror::mirror_pending_entries(&state.db, &token).await?;

    tracing::info!(
        actor_id = %auth.user_id,
        mirrored_count = report.mirrored.len(),
        failed_count = report.failed.len(),
        "admin triggered hello_wall mirror_run"
    );

    let failed_details: Vec<MirrorFailedDetail> = report
        .failed
        .into_iter()
        .map(|(id, error)| MirrorFailedDetail { id, error })
        .collect();

    Ok(Json(AdminOpsEnvelope {
        ok: true,
        data: HelloWallMirrorReport {
            mirrored: report.mirrored.len(),
            failed: failed_details.len(),
            skipped: report.skipped,
            mirrored_ids: report.mirrored,
            failed_details,
        },
    }))
}

/// POST /api/admin/profile-readme/sync-run
///
/// Sync les README GitHub des users en mode `github_sync`.
/// `SKILLUV_BOT_GITHUB_TOKEN` est optionnel — sans lui, fetch anonyme via
/// raw.githubusercontent (rate limit 60/h par IP).
#[utoipa::path(
    post,
    path = "/api/admin/profile-readme/sync-run",
    tag = "admin",
    responses(
        (status = 200, description = "Sync run report", body = AdminOpsEnvelope<ProfileReadmeSyncReport>),
        (status = 403, description = "Not an admin", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn profile_readme_sync_run(
    _gate: crate::middleware::admin_gate::AdminGate,
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<AdminOpsEnvelope<ProfileReadmeSyncReport>>, AppError> {
    require_admin(&state, &auth).await?;

    let token = std::env::var("SKILLUV_BOT_GITHUB_TOKEN").ok();

    let report = profile_readme_sync::sync_pending_readmes(&state.db, token.as_deref()).await?;

    tracing::info!(
        actor_id = %auth.user_id,
        synced_count = report.synced.len(),
        failed_count = report.failed.len(),
        skipped_no_readme = report.skipped_no_readme.len(),
        "admin triggered profile_readme sync_run"
    );

    Ok(Json(AdminOpsEnvelope {
        ok: true,
        data: ProfileReadmeSyncReport {
            synced: report.synced.len(),
            failed: report.failed.len(),
            skipped_no_readme: report.skipped_no_readme.len(),
            synced_ids: report.synced,
        },
    }))
}

/// POST /api/admin/badges/recompute/{user_id}
///
/// Force le recompute complet des badges pour un user. Utile en debug quand
/// le proof engine hooks n'a pas trigger (bug, event drop, etc.).
#[utoipa::path(
    post,
    path = "/api/admin/badges/recompute/{user_id}",
    tag = "admin",
    params(("user_id" = Uuid, Path, description = "Target user UUID")),
    responses(
        (status = 200, description = "Recompute report", body = AdminOpsEnvelope<BadgeRecomputeReport>),
        (status = 403, description = "Not an admin", body = crate::api_response::ErrorResponse),
    ),
    security(("cookie_auth" = [])),
)]
pub async fn recompute_badges_for_user(
    _gate: crate::middleware::admin_gate::AdminGate,
    State(state): State<AppState>,
    auth: AuthUser,
    Path(user_id): Path<Uuid>,
) -> Result<Json<AdminOpsEnvelope<BadgeRecomputeReport>>, AppError> {
    require_admin(&state, &auth).await?;

    let report = badge_engine::recompute_badges_for_user(&state.db, user_id).await?;

    tracing::info!(
        actor_id = %auth.user_id,
        target_user_id = %user_id,
        awarded = ?report.awarded,
        revoked = ?report.revoked,
        unchanged = report.unchanged,
        "admin triggered badge recompute"
    );

    Ok(Json(AdminOpsEnvelope {
        ok: true,
        data: BadgeRecomputeReport {
            awarded: report.awarded,
            revoked: report.revoked,
            unchanged: report.unchanged,
        },
    }))
}

// Silence unused-import — ApiResponse is not used here (legacy envelope).
#[allow(dead_code)]
type _ApiResponseTouch = ApiResponse<()>;
