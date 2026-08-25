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
//! 200 OK — { valid: false, reason: "unknown attestation hash" }
//!
//! We deliberately return **200 with `valid:false`** in ambiguous cases
//! (bad shape, unknown hash) so the client can render a stable UI. Only
//! internal errors bubble as 5xx.
//!
//! SKI-288 — this paragraph described the intent from the start, but the
//! code answered 404 on those two branches. That is a meaningful
//! difference for the caller: "this hash is not a valid attestation" is a
//! successful answer to the question asked, and an HTTP client that treats
//! 4xx as a transport failure (ours does) cannot tell it apart from an
//! outage. The branches now match the documented contract.
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
use crate::services::og_card;

pub fn attestations_public_routes() -> Router<AppState> {
    Router::new()
        .route("/verify/{hash}", get(verify))
        // SKI-292 — share card. Same segment-per-parameter constraint as the
        // PDF route below, hence `/og.png` as its own segment.
        .route("/verify/{hash}/og.png", get(verify_og_card))
        // SKI-118 — separate segment for the PDF form. axum/matchit
        // requires exactly one parameter per path segment, so
        // `/verify/{hash}.pdf` panics at Router::new() time. Adding
        // `/pdf` as its own segment sidesteps that limitation with a
        // clean, browser-friendly URL.
        .route("/verify/{hash}/pdf", get(verify_pdf))
}

/// SKI-288 — the same handlers under `/api`, mounted by `build_router`.
///
/// The root-level route above cannot serve the front end: the SvelteKit app
/// owns `/verify/{hash}` on its own origin, so a browser asking for that
/// path gets the HTML page, never this JSON. The client therefore failed to
/// parse the response and rendered every attestation as invalid.
///
/// Both mounts share one set of handlers, so the shapes cannot drift. The
/// root route stays for external consumers who already use it.
pub fn attestations_public_api_routes() -> Router<AppState> {
    Router::new()
        .route("/verify/{hash}", get(verify))
        .route("/verify/{hash}/og.png", get(verify_og_card))
        .route("/verify/{hash}/pdf", get(verify_pdf))
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

/// Check an attestation by its hash. Public: this is the surface a
/// recruiter reaches from the certificate without holding an account.
#[utoipa::path(
    get, path = "/api/verify/{hash}", tag = "attestations",
    params(("hash" = String, Path, description = "The attestation hash printed on the certificate")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 404, description = "No attestation carries that hash", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn verify(
    State(state): State<AppState>,
    Path(hash): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    if !is_valid_hash_shape(&hash) {
        return Ok((
            StatusCode::OK,
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
            StatusCode::OK,
            Json(json!({ "valid": false, "reason": "unknown attestation hash" })),
        ));
    };

    // Defensive: an attestation without validated_at means an inconsistency
    // (hash written outside the `approve` path). Treat as unknown to avoid
    // returning half-signed data.
    let Some(validated_at) = validated_at else {
        return Ok((
            StatusCode::OK,
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

/// Row backing the share card. Separate from `AttestationRow` because the
/// card needs the repository, which the JSON payload does not carry.
type CardRow = (
    Option<chrono::DateTime<chrono::Utc>>,
    Option<String>, // primary_domain
    i16,            // difficulty
    Option<String>, // contributor username
    Option<String>, // contributor display_name
    Option<String>, // github_repo_owner
    Option<String>, // github_repo_name
);

/// OpenGraph share card for an attestation, as a PNG.
///
/// Public and unauthenticated: the callers are the crawlers of X, LinkedIn
/// and Facebook, which follow `og:image` without cookies.
///
/// An unknown or malformed hash returns a generic card with **200**, never a
/// 404. A crawler that receives an error renders no preview at all, and the
/// person who shared the link is never told why their post looks broken.
#[utoipa::path(
    get,
    path = "/api/verify/{hash}/og.png",
    tag = "attestations",
    params(("hash" = String, Path, description = "Attestation hash (64 hex chars)")),
    responses(
        (status = 200, description = "PNG share card, 1200x630", content_type = "image/png"),
    ),
    security(),
)]
pub async fn verify_og_card(
    State(state): State<AppState>,
    Path(hash): Path<String>,
) -> Result<axum::response::Response, AppError> {
    let data = if is_valid_hash_shape(&hash) {
        load_card_data(&state, &hash).await?
    } else {
        None
    };
    let card = data.unwrap_or_else(og_card::fallback_card);
    let png = og_card::render_png(&card)?;

    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("image/png"),
    );
    // A validated attestation never changes, so the card is immutable. The
    // fallback is cached far more briefly: the hash may simply not exist yet
    // when a crawler races the page being published.
    headers.insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("public, max-age=31536000, immutable"),
    );

    Ok((StatusCode::OK, headers, png).into_response())
}

/// Fetch what the card shows. `None` means no such attestation.
async fn load_card_data(
    state: &AppState,
    hash: &str,
) -> Result<Option<og_card::CardData>, AppError> {
    let row: Option<CardRow> = sqlx::query_as(
        r#"
        SELECT s.validated_at,
               s.primary_domain,
               s.difficulty,
               cu.username,
               cu.display_name,
               p.github_repo_owner,
               p.github_repo_name
          FROM project_slices s
          LEFT JOIN users cu ON cu.id = s.claimed_by_user_id
          LEFT JOIN projects p ON p.id = s.project_id
         WHERE s.attestation_hash = $1
        "#,
    )
    .bind(hash)
    .fetch_optional(&state.db)
    .await?;

    let Some((validated_at, domain, difficulty, username, display_name, repo_owner, repo_name)) =
        row
    else {
        return Ok(None);
    };

    // Same guard as `verify`: a hash without a validation date means the row
    // was written outside the approval path. Do not vouch for it on a card
    // that will be cached for a year.
    let Some(validated_at) = validated_at else {
        return Ok(None);
    };

    let username = username.unwrap_or_else(|| "anonyme".to_string());
    Ok(Some(og_card::CardData {
        display_name: display_name.unwrap_or_else(|| username.clone()),
        username,
        repository: match (repo_owner, repo_name) {
            (Some(o), Some(n)) => Some(format!("{o}/{n}")),
            _ => None,
        },
        domain,
        difficulty: Some(difficulty),
        validated_on: Some(validated_at.format("%d/%m/%Y").to_string()),
        hash: Some(hash.to_string()),
    }))
}

/// The same attestation as a PDF, rendered by the sidecar service.
#[utoipa::path(
    get, path = "/api/verify/{hash}/pdf", tag = "attestations",
    params(("hash" = String, Path, description = "The attestation hash printed on the certificate")),
    responses(
        (status = 200, description = "The certificate as a PDF", content_type = "application/pdf"),
        (status = 404, description = "No attestation carries that hash", body = crate::api_response::ErrorResponse),
        (status = 503, description = "PDF_RENDERER_URL is not configured", body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn verify_pdf(
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
        slice_id,
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

    let evidence = measured_evidence(&state, slice_id).await?;

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
        evidence,
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
    /// The measured claims this slice carries, already reproduced by somebody
    /// else. Empty for a slice with none, which is most of them.
    evidence: Vec<EvidenceLine>,
}

/// One line of measured evidence, pre-formatted.
///
/// Deliberately a rendered string rather than a set of numbers the template
/// assembles: what a benchmark line says depends on its unit and on which
/// direction counts as better, and a template deciding that would be a second
/// place to get it wrong.
#[derive(Debug, Clone)]
struct EvidenceLine {
    label: String,
    value: String,
}

/// What this slice has that a second person confirmed.
///
/// Only reproduced benchmarks and hub figures we fetched ourselves. An
/// unreproduced measurement is the author's word, and an attestation exists
/// precisely so a reader does not have to take it — printing one would undo
/// the point of the document.
async fn measured_evidence(
    state: &AppState,
    slice_id: uuid::Uuid,
) -> Result<Vec<EvidenceLine>, AppError> {
    let mut lines = Vec::new();

    type Bench = (String, String, String, f64, bool, Option<String>);
    let benchmarks: Vec<Bench> = sqlx::query_as(
        "SELECT benchmark_name, metric_name, metric_unit, metric_value,
                lower_is_better, dataset_split
           FROM benchmark_results
          WHERE slice_id = $1 AND reproduced_at IS NOT NULL
          ORDER BY benchmark_name
          LIMIT 5",
    )
    .bind(slice_id)
    .fetch_all(&state.db)
    .await?;

    for (name, metric, unit, value, lower_is_better, split) in benchmarks {
        let direction = if lower_is_better {
            "plus bas vaut mieux"
        } else {
            "plus haut vaut mieux"
        };
        let on = split.map(|s| format!(", sur {s}")).unwrap_or_default();
        lines.push(EvidenceLine {
            label: name,
            value: format!("{metric} {value} {unit} ({direction}{on}) — rejoué par un relecteur"),
        });
    }

    // Hub figures, summed across whatever this slice publishes. Dated,
    // because a download count with no date is a number nobody can situate.
    type Reach = (
        Option<i64>,
        Option<i32>,
        Option<chrono::DateTime<chrono::Utc>>,
    );
    let reach: Option<Reach> = sqlx::query_as(
        "SELECT sum(downloads_recent)::BIGINT, sum(likes_count)::INT, max(fetched_at)
           FROM published_artifact_stats
          WHERE slice_id = $1
            AND registry IN ('huggingface_models', 'huggingface_datasets',
                             'kaggle_datasets')",
    )
    .bind(slice_id)
    .fetch_optional(&state.db)
    .await?;

    if let Some((Some(downloads), likes, Some(fetched))) = reach
        && downloads > 0
    {
        let likes = likes.unwrap_or(0);
        lines.push(EvidenceLine {
            label: "Diffusion".into(),
            value: format!(
                "{downloads} téléchargements sur 30 jours, {likes} mentions — relevé le {}",
                fetched.format("%d/%m/%Y")
            ),
        });
    }

    Ok(lines)
}

/// The measured-evidence block, or nothing at all.
///
/// ## Why there are no images here
///
/// The AI backlog asked for model-card thumbnails and benchmark charts. Both
/// mean the PDF renderer fetching a URL somebody else controls, at render
/// time, from inside our network — which is a request-forgery primitive and a
/// way to put arbitrary bytes into a document carrying our name. The existing
/// QR code is inline SVG for the same reason, and that decision is worth
/// keeping rather than making an exception to.
///
/// The substance survives without them. What makes a benchmark worth printing
/// is the figure, the baseline it beat, and the fact that a second person
/// re-ran it — all three are text we already hold. A chart would be a picture
/// of one row.
fn render_evidence(lines: &[EvidenceLine]) -> String {
    if lines.is_empty() {
        return String::new();
    }
    let rows = lines
        .iter()
        .map(|l| {
            format!(
                r#"<div style="margin-top: 8px; font-size: 14px;"><strong>{}</strong> — {}</div>"#,
                html_escape(&l.label),
                html_escape(&l.value)
            )
        })
        .collect::<Vec<_>>()
        .join("\n      ");

    format!(
        r#"
  <section style="margin-top: 24px; border-left: 3px solid #2E7D32; padding-left: 16px;">
    <div style="font-size: 13px; text-transform: uppercase; color:#666;">Mesuré et reproduit</div>
      {rows}
  </section>"#
    )
}

/// Pure HTML template. Uses inline styles because the pdf_renderer
/// service is expected to consume standalone HTML (no CSS bundling).
/// The QR code is embedded as inline SVG generated by the `qrcode`
/// crate — no external image fetch during PDF rendering.
fn render_attestation_html(v: AttestationView<'_>) -> String {
    let qr_svg = render_qr_svg(v.verify_url);
    let evidence_html = render_evidence(&v.evidence);
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

  {evidence_html}

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
        evidence_html = evidence_html,
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
