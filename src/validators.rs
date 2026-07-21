//! Centralised input validators (Phase 1.20).
//!
//! Each handler should validate user input through these helpers before any DB write.
//! Existing per-handler validators (`auth.rs::validate_email`, etc.) will be migrated here
//! over time; new code should use this module from day one.

use crate::errors::AppError;

/// 100 KB max code submission size.
pub const MAX_CODE_BYTES: usize = 100 * 1024;
/// 2 MB max for avatar uploads (cf. `user_profile.rs::MAX_AVATAR_SIZE`).
pub const MAX_AVATAR_BYTES: usize = 2 * 1024 * 1024;

/// Reject strings containing ASCII control characters (other than common whitespace).
/// Useful for display names, titles, slugs.
pub fn no_control_chars(value: &str, field: &str) -> Result<(), AppError> {
    if value
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\r' && c != '\t')
    {
        return Err(AppError::Validation(format!(
            "{field} contains invalid control characters"
        )));
    }
    Ok(())
}

/// URL validator: must start with http:// or https://, total length capped.
pub fn validate_url(value: &str, field: &str, max_len: usize) -> Result<(), AppError> {
    if value.is_empty() {
        return Ok(());
    }
    if value.len() > max_len {
        return Err(AppError::Validation(format!(
            "{field} must be at most {max_len} characters"
        )));
    }
    if !value.starts_with("http://") && !value.starts_with("https://") {
        return Err(AppError::Validation(format!(
            "{field} must start with http:// or https://"
        )));
    }
    // No whitespace or control chars
    if value.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(AppError::Validation(format!(
            "{field} contains invalid characters"
        )));
    }
    Ok(())
}

/// GitHub username pattern (alphanumeric + dash, 1-39 chars, must not start/end with dash).
pub fn validate_github_username(value: &str) -> Result<(), AppError> {
    if value.is_empty() {
        return Ok(());
    }
    if value.len() > 39 {
        return Err(AppError::Validation("GitHub username too long".into()));
    }
    if !value.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(AppError::Validation(
            "GitHub username can only contain letters, digits, and dashes".into(),
        ));
    }
    if value.starts_with('-') || value.ends_with('-') {
        return Err(AppError::Validation(
            "GitHub username cannot start or end with a dash".into(),
        ));
    }
    Ok(())
}

/// Code submission size guard.
pub fn validate_code_size(code: &str) -> Result<(), AppError> {
    if code.len() > MAX_CODE_BYTES {
        return Err(AppError::Validation(format!(
            "Code submission too large (max {} KB)",
            MAX_CODE_BYTES / 1024
        )));
    }
    Ok(())
}

/// Bio: 0-1000 chars, no control chars. Markdown content stored as-is; the front
/// renders with sanitization (no `dangerouslyInnerHTML`).
pub fn validate_bio(bio: &str) -> Result<(), AppError> {
    if bio.len() > 1000 {
        return Err(AppError::Validation(
            "Bio must be at most 1000 characters".into(),
        ));
    }
    no_control_chars(bio, "Bio")?;
    Ok(())
}

/// Display name: 1-100 chars, trimmed, no control chars.
pub fn validate_display_name(name: &str) -> Result<(), AppError> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.len() > 100 {
        return Err(AppError::Validation(
            "Display name must be between 1 and 100 characters".into(),
        ));
    }
    no_control_chars(trimmed, "Display name")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_control_chars_accepts_whitespace() {
        assert!(no_control_chars("Hello\nWorld", "f").is_ok());
        assert!(no_control_chars("tabs\there", "f").is_ok());
    }

    #[test]
    fn no_control_chars_rejects_null() {
        assert!(no_control_chars("hi\0there", "f").is_err());
    }

    #[test]
    fn no_control_chars_rejects_escape() {
        assert!(no_control_chars("foo\x1bbar", "f").is_err());
    }

    #[test]
    fn validate_url_basics() {
        assert!(validate_url("https://example.com", "f", 200).is_ok());
        assert!(validate_url("http://localhost:3000/path", "f", 200).is_ok());
        assert!(validate_url("", "f", 200).is_ok());
        assert!(validate_url("javascript:alert(1)", "f", 200).is_err());
        assert!(validate_url("ftp://example.com", "f", 200).is_err());
        assert!(validate_url("not a url", "f", 200).is_err());
    }

    #[test]
    fn validate_url_length() {
        assert!(validate_url("https://example.com", "f", 10).is_err());
    }

    #[test]
    fn validate_github_username_accepts() {
        assert!(validate_github_username("torvalds").is_ok());
        assert!(validate_github_username("user-123").is_ok());
        assert!(validate_github_username("").is_ok());
    }

    #[test]
    fn validate_github_username_rejects() {
        assert!(validate_github_username("-leading-dash").is_err());
        assert!(validate_github_username("trailing-").is_err());
        assert!(validate_github_username("with space").is_err());
        assert!(validate_github_username("with_underscore").is_err());
        assert!(validate_github_username(&"a".repeat(40)).is_err());
    }

    #[test]
    fn validate_code_size_ok() {
        assert!(validate_code_size("print('hi')").is_ok());
    }

    #[test]
    fn validate_code_size_too_big() {
        let huge = "x".repeat(MAX_CODE_BYTES + 1);
        assert!(validate_code_size(&huge).is_err());
    }

    #[test]
    fn validate_bio_xss_chars_allowed_but_stored_raw() {
        // We don't try to strip <script>; the front sanitises render-side.
        // We only block control chars that would break terminals or DB.
        assert!(validate_bio("<script>alert(1)</script>").is_ok());
        assert!(validate_bio("oof\0null").is_err());
    }

    #[test]
    fn validate_display_name_trims() {
        assert!(validate_display_name("  Foo  ").is_ok());
        assert!(validate_display_name("   ").is_err());
    }
}
