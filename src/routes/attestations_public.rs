//! P26 v2 SKI-115 — public attestation verification endpoint.
//!
//! Route `GET /verify/{attestation_hash}` — **unauthenticated**, mounted
//! OUTSIDE `/api` (same convention as `/webhooks/*`). Anyone with an
//! attestation hash can verify it's real, without a Skilluv account.
//!
//! This is what makes SKI-90's `attestation_hash` genuinely opposable:
//! a recruiter reading a candidate's CV can paste the hash and get a
//! signed answer from us.
//!
//! ─── Response shape ───────────────────────────────────────────────
//!
//! 200 OK — { valid: true, ...metadata }
//! 404    — { valid: false, reason: "unknown attestation hash" }
//!
//! We deliberately return **200 with `valid:false`** in ambiguous cases
//! (bad shape, unknown hash) so the client can render a stable UI. Only
//! internal errors bubble as 5xx.
//!
//! ─── Rate limiting ────────────────────────────────────────────────
//!
//! The hash is HMAC-SHA256 (2^256 space) so brute-force enumeration is
//! not credible. We rely on the reverse-proxy's global rate limit rather
//! than adding per-endpoint machinery; a rate-limit layer can be added
//! later if we see abuse.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;

use crate::AppState;
use crate::errors::AppError;

pub fn attestations_public_routes() -> Router<AppState> {
    Router::new()
        .route("/verify/{hash}", get(verify))
        // SKI-118 — same route, `.pdf` suffix. Distinct handler because
        // the response contract is completely different (bytes vs JSON).
        .route("/verify/{hash}.pdf", get(verify_pdf))
}

/// Shape check — must be 64 lowercase hex chars (SHA-256 output).
/// Rejecting bad shape as `valid:false` (not 400) keeps the endpoint
/// friendly to human copy-paste with trailing whitespace already trimmed
/// on the caller side; internal errors are the only 5xx source.
fn is_valid_hash_shape(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase())
}

type AttestationRow = (
    uuid::Uuid,
    Option<chrono::DateTime<chrono::Utc>>,
    Option<String>,
    Option<String>,
    i16,
    Option<uuid::Uuid>,
    // challenger
    Option<String>, // username
    Option<String>, // display_name
    Option<String>, // avatar_url
    // validator
    Option<String>, // username
    Option<String>, // display_name
);

async fn verify(
    State(state): State<AppState>,
    Path(hash): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    if !is_valid_hash_shape(&hash) {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(json!({ "valid": false, "reason": "malformed attestation hash" })),
        ));
    }

    let row: Option<AttestationRow> = sqlx::query_as(
        r#"
        SELECT s.id,
               s.validated_at,
               s.submitted_pr_url,
               s.primary_domain,
               s.difficulty,
               s.claimed_by_user_id,
               cu.username, cu.display_name, cu.avatar_url,
               vu.username, vu.display_name
          FROM project_slices s
          LEFT JOIN users cu ON cu.id = s.claimed_by_user_id
          LEFT JOIN users vu ON vu.id = s.validated_by_user_id
         WHERE s.attestation_hash = $1
        "#,
    )
    .bind(&hash)
    .fetch_optional(&state.db)
    .await?;

    let Some((
        _slice_id,
        validated_at,
        pr_url,
        domain,
        difficulty,
        _claimer_id,
        c_username,
        c_display,
        c_avatar,
        v_username,
        v_display,
    )) = row
    else {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(json!({ "valid": false, "reason": "unknown attestation hash" })),
        ));
    };

    // Defensive: an attestation without validated_at means an inconsistency
    // (hash written outside the `approve` path). Treat as unknown to avoid
    // returning half-signed data.
    let Some(validated_at) = validated_at else {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(json!({ "valid": false, "reason": "unknown attestation hash" })),
        ));
    };

    let body = json!({
        "valid": true,
        "challenger": {
            "username": c_username,
            "display_name": c_display,
            "avatar_url": c_avatar,
        },
        "validator": {
            "username": v_username,
            "display_name": v_display,
        },
        "pr_url": pr_url,
        "domain": domain,
        "difficulty": difficulty,
        "validated_at": validated_at.to_rfc3339(),
        // Not the hash itself in the response — client already has it.
    });
    Ok((StatusCode::OK, Json(body)))
}

// ─── SKI-118 PDF handler ─────────────────────────────────────────

async fn verify_pdf(
    State(state): State<AppState>,
    Path(hash): Path<String>,
) -> Result<axum::response::Response, AppError> {
    // pdf_renderer_url is soft-required; if unset the endpoint returns
    // a clear 503 (same pattern as /invoices/{id}/pdf).
    let Some(renderer_url) = state.config.pdf_renderer_url.as_deref() else {
        return Err(AppError::ServiceUnavailable(
            "pdf renderer not configured (set PDF_RENDERER_URL)".into(),
        ));
    };

    if !is_valid_hash_shape(&hash) {
        return Err(AppError::NotFound("malformed attestation hash".into()));
    }

    let row: Option<AttestationRow> = sqlx::query_as(
        r#"
        SELECT s.id,
               s.validated_at,
               s.submitted_pr_url,
               s.primary_domain,
               s.difficulty,
               s.claimed_by_user_id,
               cu.username, cu.display_name, cu.avatar_url,
               vu.username, vu.display_name
          FROM project_slices s
          LEFT JOIN users cu ON cu.id = s.claimed_by_user_id
          LEFT JOIN users vu ON vu.id = s.validated_by_user_id
         WHERE s.attestation_hash = $1
        "#,
    )
    .bind(&hash)
    .fetch_optional(&state.db)
    .await?;

    let Some((
        _slice_id,
        Some(validated_at),
        pr_url,
        domain,
        difficulty,
        _claimer_id,
        c_username,
        c_display,
        c_avatar,
        v_username,
        v_display,
    )) = row
    else {
        return Err(AppError::NotFound("unknown attestation hash".into()));
    };

    let verify_url = format!(
        "{}/verify/{}",
        state.config.base_url.trim_end_matches('/'),
        hash
    );

    let html = render_attestation_html(AttestationView {
        challenger_username: c_username.as_deref().unwrap_or("(unknown)"),
        challenger_display: c_display.as_deref().unwrap_or("(unknown)"),
        challenger_avatar: c_avatar.as_deref(),
        validator_username: v_username.as_deref().unwrap_or("(unknown)"),
        validator_display: v_display.as_deref().unwrap_or("(unknown)"),
        pr_url: pr_url.as_deref().unwrap_or(""),
        domain: domain.as_deref().unwrap_or(""),
        difficulty,
        validated_at: &validated_at.to_rfc3339(),
        verify_url: &verify_url,
        hash: &hash,
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| AppError::Internal(format!("pdf client build: {e}")))?;
    let endpoint = format!("{}/render", renderer_url.trim_end_matches('/'));
    let resp = client
        .post(&endpoint)
        .header("Content-Type", "text/html; charset=utf-8")
        .body(html)
        .send()
        .await
        .map_err(|e| AppError::ServiceUnavailable(format!("pdf renderer unreachable: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let snippet = resp
            .text()
            .await
            .unwrap_or_default()
            .chars()
            .take(200)
            .collect::<String>();
        return Err(AppError::Internal(format!(
            "pdf renderer returned {status}: {snippet}"
        )));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| AppError::Internal(format!("pdf body read: {e}")))?;

    let filename = format!("attachment; filename=\"skilluv-attestation-{hash}.pdf\"");
    let response = axum::response::Response::builder()
        .status(StatusCode::OK)
        .header(axum::http::header::CONTENT_TYPE, "application/pdf")
        .header(axum::http::header::CONTENT_DISPOSITION, filename)
        .body(axum::body::Body::from(bytes))
        .map_err(|e| AppError::Internal(format!("pdf response build: {e}")))?;
    Ok(response)
}

struct AttestationView<'a> {
    challenger_username: &'a str,
    challenger_display: &'a str,
    challenger_avatar: Option<&'a str>,
    validator_username: &'a str,
    validator_display: &'a str,
    pr_url: &'a str,
    domain: &'a str,
    difficulty: i16,
    validated_at: &'a str,
    verify_url: &'a str,
    hash: &'a str,
}

/// Pure HTML template. Uses inline styles because the pdf_renderer
/// service is expected to consume standalone HTML (no CSS bundling).
/// The QR code is embedded as inline SVG generated by the `qrcode`
/// crate — no external image fetch during PDF rendering.
fn render_attestation_html(v: AttestationView<'_>) -> String {
    let qr_svg = render_qr_svg(v.verify_url);
    let avatar_html = v
        .challenger_avatar
        .map(|a| format!(
            r#"<img src="{}" alt="" style="width:64px;height:64px;border-radius:50%;vertical-align:middle;margin-right:12px;">"#,
            html_escape(a)
        ))
        .unwrap_or_default();
    format!(
        r#"<!doctype html>
<html><head><meta charset="utf-8"><title>Skilluv Attestation</title></head>
<body style="font-family: -apple-system, BlinkMacSystemFont, sans-serif; color:#222; max-width: 760px; margin: 40px auto; padding: 24px;">
  <header style="display:flex; justify-content:space-between; align-items:center; border-bottom: 2px solid #2E7D32; padding-bottom: 12px;">
    <h1 style="margin:0; color:#2E7D32; font-size: 28px;">Skilluv Attestation</h1>
    <span style="color:#666; font-size: 12px;">Verified via skill-uv.com</span>
  </header>

  <section style="margin-top: 32px;">
    <div style="display:flex; align-items:center;">
      {avatar_html}
      <div>
        <div style="font-size: 22px; font-weight: 600;">{challenger_display}</div>
        <div style="color:#666; font-size: 14px;">@{challenger_username}</div>
      </div>
    </div>
  </section>

  <section style="margin-top: 32px; background: #F5F7F5; padding: 20px; border-radius: 8px;">
    <div style="font-size: 13px; text-transform: uppercase; color:#666;">Contribution</div>
    <div style="margin-top: 8px; word-break: break-all;">
      <a href="{pr_url}" style="color:#2E7D32;">{pr_url}</a>
    </div>
    <div style="margin-top: 16px; display:flex; gap: 24px; font-size: 14px;">
      <div><strong>Domain:</strong> {domain}</div>
      <div><strong>Difficulty:</strong> {difficulty} / 5</div>
    </div>
  </section>

  <section style="margin-top: 24px;">
    <div style="font-size: 13px; text-transform: uppercase; color:#666;">Validated by</div>
    <div style="margin-top: 4px;">{validator_display} · @{validator_username}</div>
    <div style="margin-top: 4px; font-size: 12px; color:#666;">On {validated_at}</div>
  </section>

  <footer style="margin-top: 40px; display:flex; justify-content:space-between; align-items:flex-end; border-top: 1px solid #ddd; padding-top: 16px;">
    <div style="font-size: 11px; color:#666; word-break: break-all; max-width: 60%;">
      <div>Attestation hash:</div>
      <div style="font-family: 'SFMono-Regular', Consolas, monospace; margin-top: 4px;">{hash}</div>
      <div style="margin-top: 12px;">Verify: <a href="{verify_url}">{verify_url}</a></div>
    </div>
    <div style="width: 128px; height: 128px;">{qr_svg}</div>
  </footer>
</body></html>"#,
        avatar_html = avatar_html,
        challenger_display = html_escape(v.challenger_display),
        challenger_username = html_escape(v.challenger_username),
        pr_url = html_escape(v.pr_url),
        domain = html_escape(v.domain),
        difficulty = v.difficulty,
        validator_display = html_escape(v.validator_display),
        validator_username = html_escape(v.validator_username),
        validated_at = html_escape(v.validated_at),
        hash = html_escape(v.hash),
        verify_url = html_escape(v.verify_url),
        qr_svg = qr_svg,
    )
}

fn render_qr_svg(payload: &str) -> String {
    match qrcode::QrCode::new(payload) {
        Ok(qr) => qr
            .render::<qrcode::render::svg::Color>()
            .min_dimensions(120, 120)
            .quiet_zone(true)
            .build(),
        // On failure (payload too long for max version), fall back to a
        // text placeholder rather than 500 the whole PDF.
        Err(_) => "<div style='font-size:10px;color:#999'>(QR unavailable)</div>".into(),
    }
}

fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::is_valid_hash_shape;

    #[test]
    fn accepts_64_lowercase_hex() {
        let h = "a".repeat(64);
        assert!(is_valid_hash_shape(&h));
        assert!(is_valid_hash_shape(
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        ));
    }

    #[test]
    fn rejects_wrong_length() {
        assert!(!is_valid_hash_shape(""));
        assert!(!is_valid_hash_shape(&"a".repeat(63)));
        assert!(!is_valid_hash_shape(&"a".repeat(65)));
    }

    #[test]
    fn rejects_uppercase_or_non_hex() {
        // Fixed shape simplifies client caching / SRE: no ambiguity.
        assert!(!is_valid_hash_shape(&"A".repeat(64)));
        assert!(!is_valid_hash_shape(&"g".repeat(64)));
        assert!(!is_valid_hash_shape(&" ".repeat(64)));
    }
}
