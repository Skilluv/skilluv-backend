//! Shared response envelope types for OpenAPI contract testing (BE-P1-CONTRACT).
//!
//! Every JSON handler in the codebase wraps its payload in `{ data, meta }`
//! via a local `build_response()` helper. To make that envelope machine
//! -documentable (schemathesis needs the *exact* response shape), we expose
//! two generic wrappers here — `ApiResponse<T>` for 2xx and `ErrorResponse`
//! for non-2xx — both deriving `utoipa::ToSchema`.
//!
//! Handlers should return `Json<ApiResponse<MyPayload>>` (or refactored to
//! typed payloads); their `#[utoipa::path]` annotation then references
//! `body = ApiResponse<MyPayload>` and `body = ErrorResponse` on failure
//! branches. See docs/BE-P1-CONTRACT-brief.md §5.
//!
//! Design choice: **Option A** from the brief — one generic envelope reused
//! everywhere, so the schema stays DRY and drift-proof.

use serde::Serialize;
use utoipa::ToSchema;

/// Envelope wrapper for every successful (2xx) JSON response.
///
/// Frontends read `data` for the payload and `meta.request_id` for support
/// correlation. Both fields are always present.
#[derive(Debug, Serialize, ToSchema)]
pub struct ApiResponse<T: ToSchema> {
    pub data: T,
    pub meta: MetaInfo,
}

impl<T: ToSchema + Serialize> ApiResponse<T> {
    /// Wraps a payload with a freshly generated `request_id` and current UTC
    /// timestamp. Matches the shape produced by the per-file `build_response`
    /// helpers so handlers can migrate transparently.
    pub fn new(data: T) -> Self {
        Self {
            data,
            meta: MetaInfo::now(),
        }
    }
}

/// Envelope wrapper for every non-2xx JSON response emitted by `AppError`.
///
/// Kept in sync with the shape produced by
/// `impl IntoResponse for AppError` in `src/errors/codes.rs` — any change
/// there must be mirrored here (contract tests will catch drift immediately).
#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorResponse {
    pub error: ErrorObject,
    pub meta: MetaInfo,
}

/// Machine-stable error payload. `code` is the canonical string documented
/// in `docs/errors.md` (`AUTH_INVALID_CREDENTIALS`, `RATE_LIMITED`, …) —
/// frontends key their i18n and UX branching off it, so it must never
/// break silently.
#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorObject {
    /// Stable business error code (see `AppError::error_code`).
    #[schema(example = "AUTH_INVALID_CREDENTIALS")]
    pub code: String,
    /// Human-readable message. NOT to be shown verbatim to end users —
    /// front performs i18n keyed on `code`.
    #[schema(example = "Invalid credentials")]
    pub message: String,
}

/// Metadata attached to every response (success or error) for support and
/// tracing. `request_id` shows up in logs & Sentry; front stores it to help
/// bug reports.
#[derive(Debug, Serialize, ToSchema)]
pub struct MetaInfo {
    /// UUID v4 identifying the request. Present in Sentry events + logs.
    #[schema(example = "b1a2c3d4-e5f6-7890-abcd-ef1234567890")]
    pub request_id: String,
    /// RFC 3339 UTC timestamp of response emission.
    #[schema(example = "2026-07-28T14:32:11.472Z")]
    pub timestamp: String,
}

impl MetaInfo {
    /// Fresh `request_id` + `now()` timestamp. Used by `ApiResponse::new`.
    pub fn now() -> Self {
        Self {
            request_id: uuid::Uuid::new_v4().to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        }
    }
}

/// Reusable payload for endpoints whose only success signal is a
/// human-readable confirmation string (email sent, action queued, …).
/// Wrap it in `ApiResponse<SimpleMessage>` and reference from
/// `#[utoipa::path]` as `body = ApiResponse<SimpleMessage>`.
#[derive(Debug, Serialize, ToSchema)]
pub struct SimpleMessage {
    #[schema(example = "Email verified successfully")]
    pub message: String,
}

impl SimpleMessage {
    pub fn new(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
        }
    }
}
