#![recursion_limit = "512"]
// BE-P1-CONTRACT — route handlers are marked `pub async fn` so that utoipa
// can generate their OpenAPI schema, but many of their internal request /
// response DTOs remain private inside the route module (they are only
// referenced through utoipa's `request_body(content = serde_json::Value)`
// fast-lane path, never re-exported). Silence the resulting `private_interfaces`
// warnings crate-wide rather than pub-ing 100+ single-use structs.
#![allow(private_interfaces)]

rust_i18n::i18n!("locales", fallback = "en");

pub mod api_response;
pub mod config;
pub mod errors;
pub mod grpc;
pub mod middleware;
pub mod models;
pub mod observability;
pub mod openapi;
pub mod routes;
pub mod services;
pub mod validators;
pub mod websocket;

use std::sync::Arc;

use axum::Router;
use axum::http::{HeaderValue, Method, header};
use redis::aio::ConnectionManager;
use sqlx::PgPool;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::trace::TraceLayer;

use grpc::AiClient;
use services::{
    AnalyticsService, EmailService, GeoService, QueueService, SandboxService, StorageService,
    WebauthnService,
};
use websocket::WsManager;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub redis: ConnectionManager,
    pub config: AppStateConfig,
    pub sandbox: Arc<SandboxService>,
    pub storage: Arc<StorageService>,
    pub email: Arc<EmailService>,
    pub ai: Option<Arc<AiClient>>,
    pub queue: Arc<QueueService>,
    pub geo: Arc<GeoService>,
    pub analytics: AnalyticsService,
    pub ws: WsManager,
    pub webauthn: Arc<WebauthnService>,
}

#[derive(Clone)]
pub struct AppStateConfig {
    pub jwt_secret: String,
    pub base_url: String,
    /// 32-byte AES-256-GCM key for enterprise SSO client_secret at-rest encryption.
    /// `None` in dev when the env var is unset ; SSO endpoints error out cleanly.
    pub sso_encryption_key: Option<[u8; 32]>,
    /// External PDF renderer service URL — `None` returns 503 on `/pdf` endpoints.
    pub pdf_renderer_url: Option<String>,
}

pub fn build_router(state: AppState) -> Router {
    // BE-A + BE-C — helper qui applique les 2 middlewares "admin gate" sur
    // les routers réservés aux surfaces admin (origin check + 2FA mandatory).
    // L'ordre importe : `ensure_admin_origin` d'abord (rejette avant même de
    // consulter la DB), puis `ensure_admin_2fa` (lookup role + totp/passkey).
    // Les gates admin (origin + 2FA) sont désormais appliqués via
    // l'extractor `middleware::admin_gate::AdminGate`, ajouté en premier
    // paramètre de chaque handler admin. L'extractor s'exécute UNIQUEMENT
    // quand la (path, méthode) matche un handler enregistré — les
    // méthodes non-supportées retombent naturellement sur le 405 par
    // défaut d'axum (au lieu du 403 qu'aurait renvoyé un middleware
    // Router-level qui intercepte tout, y compris les 405). Voir la
    // doc `AdminGate` pour le raisonnement complet. Ce closure est
    // conservé en no-op le temps du refactor pour ne pas changer les
    // .nest() call sites, mais il ne fait plus rien.
    let admin_gate = |r: Router<AppState>| r;

    let router = Router::new()
        .nest("/api", routes::health_routes())
        .nest("/api", routes::auth_routes())
        .nest("/api", routes::email_prefs_routes())
        .nest("/api", routes::challenge_routes())
        .nest("/api", routes::sandbox_routes())
        .nest("/api", routes::slice_routes())
        .nest("/api", routes::deliverable_routes())
        .nest("/api", routes::review_queue_routes())
        .nest("/api", routes::track_routes())
        .nest("/api", routes::skill_routes())
        .nest("/api", routes::orientation_routes())
        .nest("/api", routes::onboarding_routes())
        .nest("/api", routes::attestation_routes())
        .nest("/api", routes::season_routes())
        .nest("/api", routes::portfolio_routes())
        .nest("/api", admin_gate(routes::admin_routes()))
        .nest("/api", routes::gamification_routes())
        .nest("/api", routes::badge_routes())
        .nest("/api", routes::capability_routes())
        .nest("/api", routes::agency_client_routes())
        .nest("/api", routes::ai_coach_routes())
        .nest("/api", routes::event_routes())
        .nest("/api", routes::geo_routes())
        .nest("/api", routes::legal_routes())
        .nest("/api", routes::i18n_routes())
        .nest("/api", routes::social_routes())
        .nest("/api", routes::dm_routes())
        .nest("/api", routes::feed_routes())
        .nest("/api", routes::explore_routes())
        .nest("/api", routes::talent_wallet_routes())
        .nest("/api", routes::forum_routes())
        .nest("/api", routes::guild_routes())
        .nest("/api", routes::github_routes())
        .nest("/api", routes::project_routes())
        .nest("/api", routes::tournament_routes())
        .nest("/api", routes::leaderboard_routes())
        .nest("/api", routes::profile_routes())
        .nest("/api", routes::enterprise_routes())
        .nest("/api", routes::enterprise_sso_routes())
        .nest("/api", routes::scim_routes())
        .nest("/api", routes::talent_search_routes())
        .nest("/api", routes::talent_list_routes())
        .nest("/api", routes::contact_routes())
        .nest("/api", routes::notification_routes())
        .nest("/api", routes::enterprise_dashboard_routes())
        .nest("/api", routes::user_profile_routes())
        .nest("/api", routes::profile_extras_routes())
        .nest("/api", routes::oauth_routes())
        .nest("/api", routes::report_routes())
        .nest("/api", admin_gate(routes::admin_moderation_routes()))
        .nest("/api", routes::challenge_tag_routes())
        .nest("/api", routes::community_routes())
        .nest("/api", admin_gate(routes::admin_community_routes()))
        // FE-M9 — modération inline (non admin-gated, require_any_capability).
        .nest("/api", routes::moderation_routes())
        .nest("/api", routes::challenge_team_routes())
        .nest("/api", routes::developer_routes())
        // Dev-mode-only helpers (verify token peek etc). Gated inside each
        // handler by SKILLUV_DEV_MODE=true env — endpoints exist unconditionally
        // but return 403 in prod. See src/routes/dev.rs.
        .nest("/api", routes::dev_routes())
        .nest("/api", routes::public_api_routes())
        // BE-P1-CONTRACT — the legacy routes::openapi_routes() serving a
        // hand-written spec at /api/docs/openapi.json is superseded by
        // openapi::attach() below (utoipa-generated spec at
        // /api/openapi.json + Swagger UI at /api/docs/*). Removed here to
        // avoid the 'Overlapping method route' axum panic between the two.
        .nest("/api", routes::sponsored_routes())
        .nest("/api", routes::enterprise_credits_routes())
        .nest("/api", routes::enterprise_pipeline_routes())
        .nest("/api", routes::enterprise_kyc_routes())
        .nest("/api", routes::talent_search_v2_routes())
        .nest("/api", routes::talent_search_v3_routes())
        .nest("/api", routes::magic_link_routes())
        .nest("/api", routes::webauthn_routes())
        .nest("/api", routes::push_routes())
        .nest("/api", admin_gate(routes::admin_dashboard_routes()))
        .nest("/api", admin_gate(routes::admin_fraud_routes()))
        .nest("/api", admin_gate(routes::admin_project_routes()))
        .nest("/api", admin_gate(routes::admin_content_ops_routes()))
        // Trello vx5q6jW4 — admin_tournament_routes (seasons + tournaments
        // admin) était mixé dans tournament_routes sans admin_gate. Split
        // out et wired ici. Les 6 autres admin_* modules (users, skills,
        // orientations, enterprises, badge_rules, ops) sont déjà merged
        // dans admin_routes() (lignes 55-65 de src/routes/admin.rs) — les
        // re-nest ici causerait un "Overlapping method route" panic axum.
        .nest("/api", admin_gate(routes::admin_tournament_routes()))
        // Phase 5
        .nest("/api", routes::bounty_routes())
        .nest("/api", routes::certification_routes())
        .nest("/api", routes::mentorship_routes())
        .nest("/api", routes::tenant_routes())
        .nest("/api", routes::ai_job_routes())
        .nest("/api", routes::enterprise_subscription_routes())
        .merge(routes::well_known_routes().with_state(state.clone()))
        .merge(routes::metrics_routes().with_state(state.clone()))
        .merge(websocket::ws_routes().with_state(state.clone()));

    // BE-P1-CONTRACT — attach the OpenAPI JSON + Swagger UI *before* the
    // security layers so they don't accidentally block schemathesis introspection.
    let router = openapi::attach(router);

    router
        // Rejette au niveau HTTP les methodes deprecated / dangereuses AVANT
        // toute autre middleware. Deux objectifs :
        //   1. Securite : TRACE est un vecteur XST connu (Cross-Site Tracing,
        //      RFC 7231 §4.3.8 recommande de le desactiver). CONNECT n'a pas
        //      de semantique pour une API REST — l'accepter serait suspect.
        //   2. Conformite REST : sans ce block, notre admin_gate intercepte
        //      TRACE en 403 (defense en profondeur) avant qu'axum ne puisse
        //      repondre 405. Retourner 405 uniformement respecte l'attente
        //      schemathesis unsupported_methods sans desactiver l'admin_gate.
        .layer(axum::middleware::from_fn(reject_deprecated_methods))
        .layer(middleware::SecurityHeadersLayer)
        .layer(TraceLayer::new_for_http())
        .layer(build_cors_layer())
        // Sentry layers (outermost so they wrap everything). NewSentryLayer creates a
        // per-request Hub so user/tag context set in handlers doesn't leak across requests.
        .layer(sentry_tower::SentryHttpLayer::new().enable_transaction())
        .layer(sentry_tower::NewSentryLayer::new_from_top())
        .with_state(state)
}

/// Rejette avec 405 toute méthode HTTP hors du set standard REST supporté
/// par l'API. Allowlist plutôt que denylist car :
///   - Sécurité : TRACE (vecteur XST, RFC 7231), CONNECT (pas de sens REST),
///     QUERY (RFC 9110 extension search, non implémentée), PROPFIND/MOVE/etc.
///     (WebDAV, hors scope) doivent tous être refusés uniformément.
///   - Conformité schemathesis unsupported_methods : le check pingue toutes
///     les méthodes hors spec. Notre admin_gate court-circuite en 403 avant
///     qu'axum ne puisse répondre 405. Un allowlist en tête garantit la
///     bonne réponse quelle que soit la méthode exotique testée.
///   - Robustesse : un nouveau standard HTTP demain (ex : LINK/UNLINK
///     réactivés) échoue proprement sans qu'on ait à patcher.
async fn reject_deprecated_methods(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::http::{Method, StatusCode};
    let allowed = matches!(
        *req.method(),
        Method::GET
            | Method::POST
            | Method::PUT
            | Method::PATCH
            | Method::DELETE
            | Method::OPTIONS
            | Method::HEAD
    );
    if !allowed {
        return axum::response::Response::builder()
            .status(StatusCode::METHOD_NOT_ALLOWED)
            .header("Allow", "GET, POST, PUT, PATCH, DELETE, OPTIONS, HEAD")
            .body(axum::body::Body::empty())
            .expect("static response builds");
    }
    next.run(req).await
}

/// Build the CORS layer with an explicit origin allowlist. Reads
/// `ALLOWED_ORIGINS` from env — comma-separated, e.g.
/// `http://localhost:5173,http://localhost:5174,https://skilluv.com,https://admin.skilluv.com`.
/// Falls back to the two dev origins so `cargo run` on a fresh checkout works
/// out of the box. `credentials: true` is required for the httpOnly cookie
/// auth flow — the previous `permissive` layer set `Access-Control-Allow-*`
/// wildcards which browsers refuse to combine with credentials, meaning we
/// were quietly relying on same-origin requests.
fn build_cors_layer() -> CorsLayer {
    let raw = std::env::var("ALLOWED_ORIGINS")
        .unwrap_or_else(|_| "http://localhost:5173,http://localhost:5174".to_string());
    let origins: Vec<HeaderValue> = raw
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter_map(|s| HeaderValue::from_str(s).ok())
        .collect();
    tracing::info!(
        origins = ?origins.iter().map(|v| v.to_str().unwrap_or("<invalid>")).collect::<Vec<_>>(),
        "CORS allowlist active"
    );
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_credentials(true)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            header::CONTENT_TYPE,
            header::AUTHORIZATION,
            header::ACCEPT,
            header::HeaderName::from_static("x-csrf-token"),
            header::HeaderName::from_static("x-skilluv-tenant"),
        ])
        .expose_headers([header::HeaderName::from_static("x-request-id")])
}
