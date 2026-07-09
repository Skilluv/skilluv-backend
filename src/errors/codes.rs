use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Resource not found: {0}")]
    NotFound(String),

    #[error("Invalid credentials")]
    InvalidCredentials,

    #[error("Unauthorized")]
    Unauthorized,

    #[error("Forbidden")]
    Forbidden,

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("2FA code required")]
    TotpRequired,

    #[error("TOTP setup required for this account")]
    TotpSetupRequired,

    #[error("SSO login is required for this account")]
    SsoRequired { start_url: String },

    #[error("Email verification required")]
    EmailVerificationRequired,

    #[error("Invalid 2FA code")]
    TotpInvalid,

    #[error("Invalid email 2FA code")]
    Email2faInvalid,

    #[error("Profile onboarding required")]
    ProfileIncomplete,

    #[error("Challenge prerequisite not met")]
    ChallengePrerequisiteNotMet,

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Redis error: {0}")]
    Redis(#[from] redis::RedisError),

    #[error("Too many requests. Retry in {0} seconds")]
    RateLimited(i64),

    #[error("Cooldown active until {0}")]
    CooldownActive(String),

    #[error("Interest request already pending")]
    AlreadyRequested,

    #[error("Blocked by user")]
    Blocked,

    #[error("Conversation is closed")]
    ConversationClosed,

    #[error("Internal server error: {0}")]
    Internal(String),
}

impl AppError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::InvalidCredentials => StatusCode::UNAUTHORIZED,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::Validation(_) => StatusCode::BAD_REQUEST,
            Self::TotpRequired => StatusCode::FORBIDDEN,
            Self::TotpSetupRequired => StatusCode::FORBIDDEN,
            Self::SsoRequired { .. } => StatusCode::FORBIDDEN,
            Self::EmailVerificationRequired => StatusCode::FORBIDDEN,
            Self::TotpInvalid => StatusCode::UNAUTHORIZED,
            Self::Email2faInvalid => StatusCode::UNAUTHORIZED,
            Self::ProfileIncomplete => StatusCode::FORBIDDEN,
            Self::ChallengePrerequisiteNotMet => StatusCode::FORBIDDEN,
            Self::RateLimited(_) => StatusCode::TOO_MANY_REQUESTS,
            Self::CooldownActive(_) => StatusCode::TOO_MANY_REQUESTS,
            Self::AlreadyRequested => StatusCode::CONFLICT,
            Self::Blocked => StatusCode::FORBIDDEN,
            Self::ConversationClosed => StatusCode::FORBIDDEN,
            Self::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Redis(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_code(&self) -> &'static str {
        match self {
            Self::NotFound(_) => "RESOURCE_NOT_FOUND",
            Self::InvalidCredentials => "AUTH_INVALID_CREDENTIALS",
            Self::Unauthorized => "AUTH_UNAUTHORIZED",
            Self::Forbidden => "AUTH_FORBIDDEN",
            Self::Validation(_) => "VALIDATION_ERROR",
            Self::TotpRequired => "AUTH_TOTP_REQUIRED",
            Self::TotpSetupRequired => "AUTH_TOTP_SETUP_REQUIRED",
            Self::SsoRequired { .. } => "AUTH_SSO_REQUIRED",
            Self::EmailVerificationRequired => "AUTH_EMAIL_VERIFY_REQUIRED",
            Self::TotpInvalid => "AUTH_TOTP_INVALID",
            Self::Email2faInvalid => "AUTH_EMAIL_2FA_INVALID",
            Self::ProfileIncomplete => "AUTH_PROFILE_INCOMPLETE",
            Self::ChallengePrerequisiteNotMet => "CHALLENGE_PREREQUISITE_NOT_MET",
            Self::RateLimited(_) => "RATE_LIMITED",
            Self::CooldownActive(_) => "CONTACT_COOLDOWN_ACTIVE",
            Self::AlreadyRequested => "CONTACT_ALREADY_REQUESTED",
            Self::Blocked => "CONTACT_BLOCKED",
            Self::ConversationClosed => "CONVERSATION_CLOSED",
            Self::Database(_) => "DATABASE_ERROR",
            Self::Redis(_) => "CACHE_ERROR",
            Self::Internal(_) => "INTERNAL_ERROR",
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let request_id = Uuid::new_v4().to_string();
        let timestamp = chrono::Utc::now().to_rfc3339();

        let extra: Option<serde_json::Value> = match &self {
            Self::SsoRequired { start_url } => Some(json!({ "start_url": start_url })),
            _ => None,
        };
        let mut error_obj = json!({
            "code": self.error_code(),
            "message": self.to_string(),
        });
        if let Some(ex) = extra {
            if let Some(obj) = error_obj.as_object_mut() {
                if let Some(m) = ex.as_object() {
                    for (k, v) in m {
                        obj.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        let body = json!({
            "error": error_obj,
            "meta": {
                "request_id": request_id,
                "timestamp": timestamp,
            }
        });

        (self.status_code(), axum::Json(body)).into_response()
    }
}
