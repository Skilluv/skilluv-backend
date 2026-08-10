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
    Router::new().route("/verify/{hash}", get(verify))
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
