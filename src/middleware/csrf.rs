//! Double-submit CSRF token middleware.
//!
//! Auth cookies are already `SameSite=Strict`, which blocks the classic CSRF attack path in modern
//! browsers. This layer is defense-in-depth for the frontends that will run in the same site but
//! want an additional check, and for any future relaxation to `SameSite=Lax`.
//!
//! Contract:
//! - Server emits a `csrf_token` cookie (NOT httpOnly — the JS frontend must be able to read it).
//! - On any state-changing request (POST/PUT/PATCH/DELETE), the client echoes the value in the
//!   `X-CSRF-Token` header. Values must match (constant-time compare).
//! - GET/HEAD/OPTIONS bypass the check.
//!
//! Wire in as `.layer(axum::middleware::from_fn(require_csrf))` on the router branch you want
//! to protect. It is intentionally NOT applied globally to keep backward compatibility with
//! existing clients — flip it on once the frontend is updated to send the header.

use axum::extract::Request;
use axum::http::{HeaderMap, Method};
use axum::middleware::Next;
use axum::response::Response;

use crate::errors::AppError;

pub const CSRF_COOKIE_NAME: &str = "csrf_token";

pub fn build_csrf_cookie(value: &str, path: &str, max_age_secs: i64) -> String {
    // NOT httpOnly: the SPA reads it from JS to echo in the request header.
    format!(
        "{CSRF_COOKIE_NAME}={value}; Secure; SameSite=Strict; Path={path}; Max-Age={max_age_secs}"
    )
}

/// Generate a fresh CSRF token (128-bit random hex).
pub fn generate_csrf_token() -> String {
    use uuid::Uuid;
    Uuid::new_v4().simple().to_string()
}

fn extract_csrf_cookie(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get("cookie")?.to_str().ok()?;
    raw.split(';')
        .map(|s| s.trim())
        .find(|s| s.starts_with(&format!("{CSRF_COOKIE_NAME}=")))
        .and_then(|s| s.strip_prefix(&format!("{CSRF_COOKIE_NAME}=")))
        .map(|s| s.to_string())
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.bytes().zip(b.bytes()) {
        diff |= x ^ y;
    }
    diff == 0
}

pub async fn require_csrf(req: Request, next: Next) -> Result<Response, AppError> {
    match *req.method() {
        Method::GET | Method::HEAD | Method::OPTIONS => Ok(next.run(req).await),
        _ => {
            let headers = req.headers();
            let cookie_val = extract_csrf_cookie(headers).ok_or(AppError::Forbidden)?;
            let header_val = headers
                .get("x-csrf-token")
                .and_then(|v| v.to_str().ok())
                .ok_or(AppError::Forbidden)?;
            if !constant_time_eq(&cookie_val, header_val) {
                return Err(AppError::Forbidden);
            }
            Ok(next.run(req).await)
        }
    }
}
