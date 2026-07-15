//! BE-A + BE-C — Middleware "admin gate" pour les routes `/api/admin/*`.
//!
//! Deux vérifications combinées, à appliquer via `Router::layer(...)` sur
//! tous les routers admin :
//!
//! 1. **BE-C — Origin check** (`ensure_admin_origin`) : la requête doit provenir
//!    d'une origin autorisée (`admin.skilluv.com`, `localhost:5174` en dev, ou
//!    une entrée de l'env `ADMIN_ORIGINS`). Sinon 403 `AUTH_ADMIN_ORIGIN_REQUIRED`.
//!    Défense en profondeur en plus du CORS (qui n'est qu'un contrôle client).
//!
//! 2. **BE-A — 2FA mandatory** (`ensure_admin_2fa`) : si l'utilisateur
//!    authentifié a `role='admin'` et n'a **ni** TOTP activé **ni** un
//!    webauthn credential, 403 `AUTH_ADMIN_2FA_SETUP_REQUIRED`. Le login
//!    lui-même reste possible (via `requires_totp_setup` soft flag) pour
//!    que l'admin puisse atteindre `/auth/setup-2fa` côté front.
//!
//! Les deux fonctions sont exposées sous forme de `middleware::from_fn_with_state`
//! pour être composées facilement dans `lib.rs::build_router`.

use axum::extract::State;
use axum::http::{HeaderMap, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::body::Body;

use crate::AppState;
use crate::errors::AppError;

/// BE-C — vérifie que la requête provient d'une origin admin autorisée.
/// À appliquer via `Router::layer(middleware::from_fn_with_state(state, ensure_admin_origin))`.
pub async fn ensure_admin_origin(
    State(_state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    if !crate::routes::is_admin_origin(req.headers()) {
        return AppError::AdminOriginRequired.into_response();
    }
    next.run(req).await
}

/// BE-A — vérifie qu'un admin authentifié a bien un second facteur actif.
/// À appliquer APRÈS `ensure_admin_origin` sur les routes qui exigent auth admin.
///
/// Note : si la requête n'est pas authentifiée en tant qu'admin (session
/// absente ou role différent), on laisse passer — c'est aux routes elles-mêmes
/// de vérifier `require_capability("admin")` ou `require_admin`. Ce middleware
/// se contente d'empêcher l'admin sans 2FA de continuer.
pub async fn ensure_admin_2fa(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    // Extract user_id from JWT cookie (best-effort — si absent, on laisse passer).
    let Some(user_id) = extract_user_id_from_headers(req.headers(), &state) else {
        return next.run(req).await;
    };

    // Fetch role + totp_enabled + has_passkey en une query.
    let row: Option<(String, bool, bool)> = match sqlx::query_as(
        r#"
        SELECT u.role, u.totp_enabled,
               EXISTS(SELECT 1 FROM webauthn_credentials WHERE user_id = u.id)
        FROM users u WHERE u.id = $1
        "#,
    )
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(r) => r,
        Err(_) => return next.run(req).await, // ne pas casser la request sur DB error
    };

    let Some((role, totp_enabled, has_passkey)) = row else {
        return next.run(req).await;
    };

    if role == "admin" && !totp_enabled && !has_passkey {
        return AppError::AdminTwoFaSetupRequired.into_response();
    }

    next.run(req).await
}

/// Extrait user_id depuis le cookie `admin_access_token` OU `access_token`.
/// Version simplifiée (le vrai `AuthUser` extractor fait plus mais nécessite
/// un contexte handler). Ici on veut juste un lookup rapide dans un middleware.
fn extract_user_id_from_headers(headers: &HeaderMap, state: &AppState) -> Option<uuid::Uuid> {
    let cookie_header = headers.get("cookie")?.to_str().ok()?;
    let token = cookie_header
        .split(';')
        .map(|s| s.trim())
        .find_map(|c| {
            c.strip_prefix("admin_access_token=")
                .or_else(|| c.strip_prefix("access_token="))
        })?;
    // Decode JWT et parse le sub claim en Uuid.
    let claims = crate::services::AuthService::verify_access_token(token, &state.config.jwt_secret).ok()?;
    uuid::Uuid::parse_str(&claims.sub).ok()
}

// L'AppError renvoie déjà un JSON conforme via IntoResponse — pas besoin
// de wrapper `(StatusCode, Json)` manuellement.
#[allow(dead_code)]
const _NOTE: StatusCode = StatusCode::FORBIDDEN;
