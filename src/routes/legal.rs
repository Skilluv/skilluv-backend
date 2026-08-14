//! Legal & consent endpoints (Phase 1.9 + 1.10).

use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

use crate::AppState;
use crate::api_response::ApiResponse;
use crate::errors::AppError;
use crate::middleware::{OptionalAuth, extract_ip};

/// Current legal consent version. Bump this when CGU / Privacy text changes — the front
/// will re-prompt users to accept again. Keep history of versions in `docs/legal/`.
pub const CURRENT_CONSENT_VERSION: i32 = 1;

pub fn legal_routes() -> Router<AppState> {
    Router::new()
        .route("/legal/consent-version", get(consent_version))
        .route("/legal/consent", post(record_consent))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LegalPages {
    #[schema(example = "https://skill-uv.com/legal/terms")]
    pub terms: &'static str,
    #[schema(example = "https://skill-uv.com/legal/privacy")]
    pub privacy: &'static str,
    #[schema(example = "https://skill-uv.com/legal/cookies")]
    pub cookies: &'static str,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ConsentVersionResponse {
    /// Monotonically-increasing consent version. Front re-prompts when
    /// this exceeds `users.consent_version_accepted`.
    pub version: i32,
    pub pages: LegalPages,
}

/// Return the current consent-banner version + canonical URLs to the
/// legal pages. Public — used by the cookie banner on first visit.
#[utoipa::path(
    get,
    path = "/api/legal/consent-version",
    tag = "profile",
    responses(
        (status = 200, description = "Current consent version + legal URLs", body = ApiResponse<ConsentVersionResponse>),
    ),
)]
pub async fn consent_version() -> Json<ApiResponse<ConsentVersionResponse>> {
    Json(ApiResponse::new(ConsentVersionResponse {
        version: CURRENT_CONSENT_VERSION,
        pages: LegalPages {
            terms: "https://skill-uv.com/legal/terms",
            privacy: "https://skill-uv.com/legal/privacy",
            cookies: "https://skill-uv.com/legal/cookies",
        },
    }))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ConsentBody {
    /// True → PostHog + Sentry may capture non-essential events.
    pub analytics: bool,
    /// True → marketing emails / product updates allowed.
    pub marketing: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ConsentRecordedResponse {
    pub version: i32,
    pub analytics: bool,
    pub marketing: bool,
    /// Always `true` — essential cookies are outside the consent gate.
    pub essential: bool,
    pub stored: bool,
}

/// Record the user's consent decision. Called whenever the banner is
/// dismissed (front), with the categories the user accepted. Works
/// both anonymous (writes to `consent_log` only) and authenticated
/// (also mirrors the decision on the `users` row).
#[utoipa::path(
    post,
    path = "/api/legal/consent",
    tag = "profile",
    request_body = ConsentBody,
    responses(
        (status = 200, description = "Consent stored", body = ApiResponse<ConsentRecordedResponse>),
    ),
)]
pub async fn record_consent(
    State(state): State<AppState>,
    OptionalAuth(auth): OptionalAuth,
    headers: HeaderMap,
    Json(body): Json<ConsentBody>,
) -> Result<Json<ApiResponse<ConsentRecordedResponse>>, AppError> {
    let user_id = auth.as_ref().map(|a| a.user_id);
    let ip = extract_ip(&headers);
    let user_agent = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    sqlx::query(
        r#"
        INSERT INTO consent_log (user_id, version, analytics, marketing, ip, user_agent)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(user_id)
    .bind(CURRENT_CONSENT_VERSION)
    .bind(body.analytics)
    .bind(body.marketing)
    .bind(&ip)
    .bind(&user_agent)
    .execute(&state.db)
    .await?;

    if let Some(ref auth) = auth {
        sqlx::query(
            r#"
            UPDATE users SET
                consent_version_accepted = $2,
                consent_analytics = $3,
                consent_marketing = $4,
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(auth.user_id)
        .bind(CURRENT_CONSENT_VERSION)
        .bind(body.analytics)
        .bind(body.marketing)
        .execute(&state.db)
        .await?;
    }

    Ok(Json(ApiResponse::new(ConsentRecordedResponse {
        version: CURRENT_CONSENT_VERSION,
        analytics: body.analytics,
        marketing: body.marketing,
        essential: true,
        stored: true,
    })))
}

/// Parse the `cookie_consent` cookie and return whether the user has given analytics consent.
/// Used by handlers to gate PostHog event emission for sensitive (non-essential) events.
pub fn analytics_consent(headers: &HeaderMap) -> bool {
    parse_consent_cookie(headers)
        .and_then(|v| v.get("analytics").and_then(Value::as_bool))
        .unwrap_or(false)
}

pub fn marketing_consent(headers: &HeaderMap) -> bool {
    parse_consent_cookie(headers)
        .and_then(|v| v.get("marketing").and_then(Value::as_bool))
        .unwrap_or(false)
}

fn parse_consent_cookie(headers: &HeaderMap) -> Option<Value> {
    let cookie_header = headers.get("cookie")?.to_str().ok()?;
    let raw = cookie_header
        .split(';')
        .map(str::trim)
        .find(|c| c.starts_with("cookie_consent="))?
        .strip_prefix("cookie_consent=")?;
    // URL-decoded JSON
    let decoded = urlencoding_decode(raw)?;
    serde_json::from_str::<Value>(&decoded).ok()
}

/// Minimal URL-decode for percent-encoded JSON. Avoids adding a new dependency.
fn urlencoding_decode(s: &str) -> Option<String> {
    let mut out = Vec::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?;
            let v = u8::from_str_radix(hex, 16).ok()?;
            out.push(v);
            i += 3;
        } else if b == b'+' {
            out.push(b' ');
            i += 1;
        } else {
            out.push(b);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers_with_cookie(value: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert("cookie", HeaderValue::from_str(value).unwrap());
        h
    }

    #[test]
    fn no_cookie_means_no_consent() {
        let h = HeaderMap::new();
        assert!(!analytics_consent(&h));
        assert!(!marketing_consent(&h));
    }

    #[test]
    fn parses_analytics_true() {
        let json = "%7B%22analytics%22%3Atrue%2C%22marketing%22%3Afalse%7D";
        let h = headers_with_cookie(&format!("cookie_consent={json}; other=x"));
        assert!(analytics_consent(&h));
        assert!(!marketing_consent(&h));
    }

    #[test]
    fn parses_marketing_true() {
        let json = "%7B%22analytics%22%3Atrue%2C%22marketing%22%3Atrue%7D";
        let h = headers_with_cookie(&format!("cookie_consent={json}"));
        assert!(marketing_consent(&h));
    }

    #[test]
    fn malformed_cookie_falls_back_to_false() {
        let h = headers_with_cookie("cookie_consent=not-json");
        assert!(!analytics_consent(&h));
    }
}
