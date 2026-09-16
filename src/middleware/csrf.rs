//! Double-submit CSRF token middleware.
//!
//! Auth cookies are `SameSite=Lax`. That still blocks the classic CSRF attack
//! path - a cross-site POST never carries them - but it no longer blocks a
//! cross-site top-level GET, which `Strict` did.
//!
//! They were `Strict` until an OAuth return proved it unworkable: a browser
//! withholds a `Strict` cookie on a navigation another site began, and every
//! provider return is exactly that. The session survived the round trip and
//! the person landed signed out, which reads as "linking GitHub does not
//! work" and is not.
//!
//! So this layer is no longer belt-and-braces. It is the thing that has to
//! hold, and the relaxation this file anticipated has happened.
//!
//! Contract:
//! - Server emits a `csrf_token` cookie (NOT httpOnly - the JS frontend must be able to read it).
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
//! 403s every write in production: a total outage, from a defence nothing was
//! yet sending the header for.
//!
//! The balance has moved. While the cookies were `Strict` the default-off was
//! nearly free, because `Strict` was doing the work. It is not free now:
//! `Lax` leaves cross-site top-level GET carrying the session, and this layer
//! is what stands in for the difference. `CSRF_ENFORCE` should be turned on
//! once `skilluv_csrf_would_reject_total` has sat at zero across a real week,
//! and that is an environment change rather than a deploy, so it is immediate
//! to undo if it turns out to be premature.
//!
//! What makes the gap narrow in the meantime: this check bypasses GET, and so
//! does the risk - a `Lax` cookie rides a cross-site GET, and a GET is not
//! supposed to change anything. The endpoints that do accept a cross-site GET
//! and change state are the OAuth callbacks, and they authenticate on a Redis
//! `state` token bound to the user, which is the OAuth mechanism for exactly
//! this and does not depend on the cookie at all.
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
        "{CSRF_COOKIE_NAME}={value}; Secure; SameSite=Lax;{} Path={path}; Max-Age={max_age_secs}",
        domain_attr()
    )
}

/// Same as `build_csrf_cookie` but with an origin-bound prefix. Login handlers
/// pass `"admin_"` when the caller came from the admin frontend so the SPA
/// reads the right cookie name - the public app's `csrf_token` and the admin
/// app's `admin_csrf_token` live independently in the browser jar.
pub fn build_csrf_cookie_with_prefix(
    prefix: &str,
    value: &str,
    path: &str,
    max_age_secs: i64,
) -> String {
    format!(
        "{prefix}{CSRF_COOKIE_NAME}={value}; Secure; SameSite=Lax;{} Path={path}; Max-Age={max_age_secs}",
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

/// Does this request carry a session at all?
///
/// CSRF is forgery of an *authenticated* action: the attack is a page the
/// victim did not write, making the victim's browser spend the victim's
/// session. A request that carries no session spends nothing, so there is
/// nothing to forge and no reason to demand a token for it.
///
/// This is what lets `POST /auth/register` and `POST /auth/login` through
/// without a list of paths to maintain. At that point in the flow no
/// `csrf_token` cookie exists yet - the response to that very request is what
/// creates it - so a check that demanded one would refuse every registration
/// and every sign-in on the platform the moment enforcement was turned on.
///
/// It is a rule rather than a list because a list is the thing that drifts.
/// A new pre-session endpoint is exempt by being pre-session, and an endpoint
/// that starts carrying a session starts being checked, both without anybody
/// remembering to edit this file.
///
/// The failure direction is the safe one. Somebody holding a session but no
/// `csrf_token` - a response that set one without the other - is not exempt,
/// so they are refused rather than waved through, and that shows up as
/// `no_cookie` in the counter before it ever shows up as a hole.
fn carries_a_session(headers: &HeaderMap) -> bool {
    let Some(raw) = headers.get("cookie").and_then(|v| v.to_str().ok()) else {
        return false;
    };
    raw.split(';').map(str::trim).any(|c| {
        c.starts_with("access_token=")
            || c.starts_with("admin_access_token=")
            || c.starts_with("refresh_token=")
    })
}

/// The endpoints that establish a session rather than spend one.
///
/// Each of these authenticates on a credential carried in the request itself -
/// a password, a token in the body, the rotating refresh cookie - and not on
/// the session cookie. Forging one does not spend the victim's session, which
/// is what CSRF is about.
///
/// They have to be named, because "carries no session" is not enough on its
/// own. `refresh_token` lasts a week and `csrf_token` fifteen minutes, so
/// somebody who comes back the next day arrives holding a session cookie and
/// no CSRF cookie - and would be refused at the login form. A test caught
/// that; the rule alone looked right and was not.
///
/// `/auth/refresh` is here for a second reason: the client sends it outside
/// the wrapper that attaches the header, because by the time it runs the
/// access token it would have paired with has usually expired. Refusing it
/// would break silent refresh everywhere - failing closed, and looking exactly
/// like the signed-out symptom this session spent two days on. What stands in
/// for the check is the refresh token itself: it rotates on every use and a
/// replayed one revokes the whole session tree (see
/// `auth_test::test_refresh_reuse_detection_revokes_all_sessions`), so a
/// forged refresh costs an attacker the session rather than winning one.
///
/// Both mount paths, because this middleware sits under `/api` and the routes
/// are declared without it.
fn establishes_a_session(path: &str) -> bool {
    let path = path.strip_prefix("/api").unwrap_or(path);
    matches!(
        path,
        "/auth/register"
            | "/auth/login"
            | "/auth/refresh"
            | "/auth/forgot-password"
            | "/auth/reset-password"
            | "/auth/email-2fa/verify"
    )
}

pub async fn require_csrf(req: Request, next: Next) -> Result<Response, AppError> {
    match *req.method() {
        Method::GET | Method::HEAD | Method::OPTIONS => Ok(next.run(req).await),
        _ if !carries_a_session(req.headers()) => Ok(next.run(req).await),
        _ if establishes_a_session(req.uri().path()) => Ok(next.run(req).await),
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers_with(cookie: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        if !cookie.is_empty() {
            h.insert("cookie", HeaderValue::from_str(cookie).unwrap());
        }
        h
    }

    /// The whole reason the exemption is a rule and not a path list.
    ///
    /// At `POST /auth/register` there is no `csrf_token` cookie, because the
    /// response to that request is what creates it. Demanding one would refuse
    /// every registration and every sign-in the moment `CSRF_ENFORCE` was
    /// turned on - a total outage produced by the defence, which is the exact
    /// failure this module's header says the default-off exists to avoid.
    #[test]
    fn a_request_with_no_session_is_not_asked_for_a_token() {
        assert!(!carries_a_session(&headers_with("")));
        assert!(!carries_a_session(&headers_with("theme=dark; locale=fr")));
        // The cookie the browser holds on the way to signing in.
        assert!(!carries_a_session(&headers_with("csrf_token=abc")));
    }

    #[test]
    fn a_request_that_spends_a_session_is_checked() {
        assert!(carries_a_session(&headers_with("access_token=x")));
        assert!(carries_a_session(&headers_with("admin_access_token=x")));
        assert!(carries_a_session(&headers_with("refresh_token=s:t")));
        assert!(carries_a_session(&headers_with(
            "theme=dark; access_token=x; csrf_token=y"
        )));
    }

    /// A name that merely ends in one of ours is not one of ours.
    ///
    /// `my_access_token=` would have matched a substring search, and the cost
    /// of that mistake is an exemption rather than a refusal - the direction
    /// that fails open.
    #[test]
    fn a_lookalike_cookie_name_does_not_count_as_a_session() {
        assert!(!carries_a_session(&headers_with("my_access_token=x")));
        assert!(!carries_a_session(&headers_with("not_refresh_token=x")));
    }

    /// Every endpoint that hands out a session, under both mount paths.
    ///
    /// The one that matters most is `/auth/login`. `refresh_token` lasts a
    /// week and `csrf_token` fifteen minutes, so somebody returning the next
    /// day arrives holding a session cookie and no CSRF cookie - and without
    /// this, enforcement would refuse them at the login form.
    #[test]
    fn session_establishing_endpoints_are_exempt_under_both_mount_paths() {
        for p in [
            "/auth/register",
            "/auth/login",
            "/auth/refresh",
            "/auth/forgot-password",
            "/auth/reset-password",
            "/auth/email-2fa/verify",
        ] {
            assert!(establishes_a_session(p), "{p}");
            assert!(establishes_a_session(&format!("/api{p}")), "/api{p}");
        }
    }

    /// Everything that spends a session is still checked.
    #[test]
    fn an_authenticated_write_is_not_exempt() {
        for p in [
            "/api/auth/logout",
            "/api/auth/change-password",
            "/api/auth/account",
            "/api/auth/sessions/revoke-all",
            "/api/users/me/orientations",
            "/api/auth/refresh/extra",
        ] {
            assert!(!establishes_a_session(p), "{p} must still be checked");
        }
    }

    #[test]
    fn the_token_comparison_is_length_and_content() {
        assert!(constant_time_eq("abc", "abc"));
        assert!(!constant_time_eq("abc", "abd"));
        assert!(!constant_time_eq("abc", "abcd"));
        assert!(!constant_time_eq("", "a"));
    }

    /// Admin first, public second - the same order the auth cookie parser
    /// uses. Both live in the jar once somebody has signed into the admin app,
    /// and reading the wrong one refuses every admin write.
    #[test]
    fn the_admin_token_wins_when_both_are_present() {
        let h = headers_with("csrf_token=public; admin_csrf_token=admin");
        assert_eq!(extract_csrf_cookie(&h).as_deref(), Some("admin"));
        let h = headers_with("csrf_token=public");
        assert_eq!(extract_csrf_cookie(&h).as_deref(), Some("public"));
    }

    /// The cookie the SPA has to read must be readable from the app's origin.
    ///
    /// Issued host-only from `api.skill-uv.com`, `document.cookie` on
    /// `skill-uv.com` cannot see it, and the frontend could never echo a value
    /// it was never able to learn.
    #[test]
    fn the_csrf_cookie_is_lax_and_not_http_only() {
        let c = build_csrf_cookie("v", "/api", 900);
        assert!(c.contains("SameSite=Lax"), "{c}");
        assert!(
            !c.to_lowercase().contains("httponly"),
            "the SPA has to read this one: {c}"
        );
    }
}
