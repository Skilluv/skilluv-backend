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
//! ## Mounted, and off by default
//!
//! This layer is mounted on the API router. Whether it *rejects* is read from
//! `CSRF_ENFORCE` at startup, and the default is no.
//!
//! That split exists because the two dangerous mistakes here are opposite.
//! Leaving the check unmounted -- where it sat for months, written and tested
//! and wired to nothing -- means it protects nothing and no one notices.
//! Mounting it enforcing, before every client is known to send the header,
//! 403s every write in production: a total outage, from a defence that was
//! not needed that day, because `SameSite=Strict` on the auth cookies already
//! blocks the classic attack path.
//!
//! So it runs on every request and, while `CSRF_ENFORCE` is off, records what
//! it *would* have refused as `skilluv_csrf_would_reject_total` and lets the
//! request through. Watch that counter; when it sits at zero across a real
//! week, set `CSRF_ENFORCE=true`. That is an environment change, not a
//! deploy, so turning it back off is immediate if it was premature.

use axum::extract::Request;
use axum::http::{HeaderMap, Method};
use axum::middleware::Next;
use axum::response::Response;

use crate::errors::AppError;

pub const CSRF_COOKIE_NAME: &str = "csrf_token";
pub const ADMIN_CSRF_COOKIE_NAME: &str = "admin_csrf_token";

/// The `Domain` attribute the CSRF cookie needs, or `None` on a single-origin
/// deployment.
///
/// This cookie is the one cookie in the system a browser script has to *read*.
/// Issued host-only from `api.skill-uv.com`, `document.cookie` on
/// `skill-uv.com` cannot see it, so the frontend could never echo a value it
/// was never able to learn -- the check would have refused every write from
/// the app it was written to protect.
///
/// Read from the environment rather than derived from `base_url`, because the
/// right value is a deployment fact: `skill-uv.com` covers both origins in
/// production, and localhost has no dot-domain to share, where the attribute
/// must simply be absent.
fn csrf_cookie_domain() -> Option<String> {
    std::env::var("CSRF_COOKIE_DOMAIN")
        .ok()
        .map(|d| d.trim().to_string())
        .filter(|d| !d.is_empty())
}

/// Whether a failed check refuses the request. See the module docs.
pub fn csrf_is_enforced() -> bool {
    matches!(
        std::env::var("CSRF_ENFORCE").ok().as_deref(),
        Some("1") | Some("true") | Some("TRUE")
    )
}

fn domain_attr() -> String {
    csrf_cookie_domain()
        .map(|d| format!(" Domain={d};"))
        .unwrap_or_default()
}

pub fn build_csrf_cookie(value: &str, path: &str, max_age_secs: i64) -> String {
    // NOT httpOnly: the SPA reads it from JS to echo in the request header.
    format!(
        "{CSRF_COOKIE_NAME}={value}; Secure; SameSite=Strict;{} Path={path}; Max-Age={max_age_secs}",
        domain_attr()
    )
}

/// Same as `build_csrf_cookie` but with an origin-bound prefix. Login handlers
/// pass `"admin_"` when the caller came from the admin frontend so the SPA
/// reads the right cookie name — the public app's `csrf_token` and the admin
/// app's `admin_csrf_token` live independently in the browser jar.
pub fn build_csrf_cookie_with_prefix(
    prefix: &str,
    value: &str,
    path: &str,
    max_age_secs: i64,
) -> String {
    format!(
        "{prefix}{CSRF_COOKIE_NAME}={value}; Secure; SameSite=Strict;{} Path={path}; Max-Age={max_age_secs}",
        domain_attr()
    )
}

/// Generate a fresh CSRF token (128-bit random hex).
pub fn generate_csrf_token() -> String {
    use uuid::Uuid;
    Uuid::new_v4().simple().to_string()
}

fn extract_csrf_cookie(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get("cookie")?.to_str().ok()?;
    // Same admin-first / public-fallback rule as the AuthUser cookie parser
    // (see middleware::auth). Whichever CSRF cookie the current session used
    // is what the client will echo in the header.
    raw.split(';')
        .map(|s| s.trim())
        .find_map(|s| s.strip_prefix(&format!("{ADMIN_CSRF_COOKIE_NAME}=")))
        .or_else(|| {
            raw.split(';')
                .map(|s| s.trim())
                .find_map(|s| s.strip_prefix(&format!("{CSRF_COOKIE_NAME}=")))
        })
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
            let verdict = match (
                extract_csrf_cookie(headers),
                headers.get("x-csrf-token").and_then(|v| v.to_str().ok()),
            ) {
                (None, _) => Some("no_cookie"),
                (Some(_), None) => Some("no_header"),
                (Some(cookie), Some(header)) if !constant_time_eq(&cookie, header) => {
                    Some("mismatch")
                }
                _ => None,
            };

            if let Some(reason) = verdict {
                if csrf_is_enforced() {
                    return Err(AppError::Forbidden);
                }
                // Off by default. Counting rather than refusing is what turns
                // "we think every client sends the header" into something a
                // person can read off a dashboard before flipping the switch.
                metrics::counter!("skilluv_csrf_would_reject_total", "reason" => reason)
                    .increment(1);
            }
            Ok(next.run(req).await)
        }
    }
}
