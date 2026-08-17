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

/// Enforce `#[param(max_length = N)]` cote handler pour un Option<String>
/// de Query DTO. axum Query n'enforce pas les contraintes utoipa
/// post-deserialisation, donc chaque handler qui declare une contrainte
/// de longueur dans son DTO DOIT appeler ce helper pour la garantir cote
/// serveur (schema OpenAPI = contrat opposable, pas fiction).
/// Same rule for a value that is always present. Counts characters, not
/// bytes: a limit expressed in bytes rejects a shorter message in French than
/// in English, for no reason the person writing it could guess.
pub fn check_max_len(value: &str, field: &str, max: usize) -> Result<(), AppError> {
    if value.chars().count() > max {
        return Err(AppError::Validation(format!(
            "{field} must be at most {max} characters"
        )));
    }
    Ok(())
}

pub fn check_max_len_opt(value: &Option<String>, field: &str, max: usize) -> Result<(), AppError> {
    if let Some(s) = value
        && s.chars().count() > max
    {
        return Err(AppError::Validation(format!(
            "{field} must be at most {max} characters"
        )));
    }
    Ok(())
}

/// Enforce `#[param(minimum = A, maximum = B)]` cote handler pour un
/// Option<i64>. Meme raisonnement que check_max_len_opt.
pub fn check_range_opt(
    value: Option<i64>,
    field: &str,
    min: i64,
    max: i64,
) -> Result<(), AppError> {
    if let Some(v) = value
        && (v < min || v > max)
    {
        return Err(AppError::Validation(format!(
            "{field} must be between {min} and {max}"
        )));
    }
    Ok(())
}

/// Enforce the OpenAPI schema pattern `^\S(.*\S)?$` + min/max character
/// bounds on a required text field. Rules:
///   - no `\n` / `\r` (`.` in the schema pattern doesn't span lines);
///   - no leading / trailing whitespace (the `\S` anchors at both ends);
///   - character count (not byte length) between `min` and `max`.
///
/// Every field whose OpenAPI schema declares that pattern MUST route
/// through this helper — otherwise `negative_data_rejection` will catch
/// the drift (server accepts what the schema rejects) and
/// `positive_data_acceptance` will catch over-strict server rules.
pub fn validate_bounded_line(
    value: &str,
    field: &str,
    min: usize,
    max: usize,
) -> Result<(), AppError> {
    if value.contains(['\n', '\r']) {
        return Err(AppError::Validation(format!(
            "{field} must not contain line breaks"
        )));
    }
    let starts_ws = value.chars().next().is_some_and(|c| c.is_whitespace());
    let ends_ws = value.chars().next_back().is_some_and(|c| c.is_whitespace());
    if starts_ws || ends_ws {
        return Err(AppError::Validation(format!(
            "{field} must not have leading or trailing whitespace"
        )));
    }
    let n = value.chars().count();
    if n < min || n > max {
        return Err(AppError::Validation(format!(
            "{field} must be between {min} and {max} characters"
        )));
    }
    Ok(())
}

/// Optional variant of `validate_bounded_line` — skips checks when the
/// field is `None`, applies them when `Some`.
pub fn validate_bounded_line_opt(
    value: Option<&str>,
    field: &str,
    min: usize,
    max: usize,
) -> Result<(), AppError> {
    match value {
        Some(v) => validate_bounded_line(v, field, min, max),
        None => Ok(()),
    }
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

/// The platform's skill domains, and the only list of them.
///
/// A domain is the widest unit of craft: it decides which validators may judge
/// your work, which craft-score weights apply, which leaderboard you appear on
/// and which rank ladder you climb. Migrations 0056 and 0088 have known seven
/// since 2024.
///
/// This constant exists because there were six copies of this list and three
/// of them had been left at the four domains of migration 0002 — so somebody
/// could be granted `challenge_validator:ai`, be seeded AI challenges and
/// still be refused `ai` at signup. A list that decides who may do what has to
/// have one home; `skill_domains_match_the_database` below keeps it honest
/// against the CHECK constraint that enforces it.
///
/// Ordered oldest-first: `code`, `design`, `game` and `security` shipped in
/// 0002, `ops`, `ai` and `soft_skills` arrived with the orientation work.
pub const SKILL_DOMAINS: &[&str] = &[
    "code",
    "design",
    "game",
    "security",
    "ops",
    "ai",
    "soft_skills",
];

/// Reject anything that is not one of [`SKILL_DOMAINS`].
///
/// `field` names the caller's parameter, because the same list is checked at
/// signup (`skill_domain`), on a validator application (`domain`) and on a
/// project (`skill_domains[]`), and a message naming the wrong one wastes an
/// afternoon.
pub fn validate_skill_domain(domain: &str, field: &str) -> Result<(), AppError> {
    if !SKILL_DOMAINS.contains(&domain) {
        return Err(AppError::Validation(format!(
            "{field} must be one of: {}",
            SKILL_DOMAINS.join(", ")
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The list above and the CHECK constraint that enforces it have to say
    /// the same thing, and they live in different languages in different
    /// files. This reads migration 0243 at compile time and compares them, so
    /// adding a domain to one and forgetting the other fails here rather than
    /// as a 500 at signup.
    #[test]
    fn skill_domains_match_the_database() {
        const MIGRATION: &str = include_str!("../migrations/0243_skill_domains_everywhere.sql");

        // The constraint body, as written: `'code', 'design', ...`. Both
        // tables get the identical list, so finding it once is enough.
        let in_clause = MIGRATION
            .split("skill_domain IN (")
            .nth(1)
            .and_then(|rest| rest.split(')').next())
            .expect("migration 0243 no longer spells its domain list as `skill_domain IN (...)`");

        let from_sql: Vec<&str> = in_clause
            .split(',')
            .map(|token| token.trim().trim_matches('\'').trim())
            .filter(|token| !token.is_empty())
            .collect();

        assert_eq!(
            from_sql, SKILL_DOMAINS,
            "SKILL_DOMAINS and migration 0243 disagree — a domain was added to one of them only"
        );
    }

    #[test]
    fn an_unknown_domain_is_named_in_the_error() {
        let err = validate_skill_domain("crypto", "skill_domain").unwrap_err();
        let message = format!("{err:?}");
        assert!(message.contains("skill_domain"), "{message}");
        assert!(message.contains("soft_skills"), "{message}");
    }

    #[test]
    fn the_three_domains_that_used_to_be_refused_are_accepted() {
        // The whole point of migration 0243: these were seeded, validated and
        // ranked long before anybody could declare them.
        for domain in ["ai", "ops", "soft_skills"] {
            assert!(
                validate_skill_domain(domain, "skill_domain").is_ok(),
                "{domain} should be a declarable domain"
            );
        }
    }

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

    #[test]
    fn bounded_line_accepts_plain() {
        assert!(validate_bounded_line("Alice", "name", 1, 50).is_ok());
        assert!(validate_bounded_line("Ada Lovelace", "name", 1, 50).is_ok());
    }

    #[test]
    fn bounded_line_accepts_control_chars_non_line_break() {
        assert!(validate_bounded_line("a\u{0089}b", "name", 1, 50).is_ok());
    }

    #[test]
    fn bounded_line_rejects_line_breaks() {
        assert!(validate_bounded_line("a\nb", "name", 1, 50).is_err());
        assert!(validate_bounded_line("a\rb", "name", 1, 50).is_err());
    }

    #[test]
    fn bounded_line_rejects_edge_whitespace() {
        assert!(validate_bounded_line(" Alice", "name", 1, 50).is_err());
        assert!(validate_bounded_line("Alice ", "name", 1, 50).is_err());
        assert!(validate_bounded_line("\tAlice", "name", 1, 50).is_err());
    }

    #[test]
    fn bounded_line_counts_chars_not_bytes() {
        let s: String = "é".repeat(10);
        assert!(validate_bounded_line(&s, "name", 1, 10).is_ok());
        assert!(validate_bounded_line(&s, "name", 1, 9).is_err());
    }

    #[test]
    fn bounded_line_rejects_below_min() {
        assert!(validate_bounded_line("", "name", 1, 50).is_err());
    }

    #[test]
    fn bounded_line_opt_skips_when_none() {
        assert!(validate_bounded_line_opt(None, "name", 1, 50).is_ok());
        assert!(validate_bounded_line_opt(Some("ok"), "name", 1, 50).is_ok());
        assert!(validate_bounded_line_opt(Some(" bad"), "name", 1, 50).is_err());
    }
}
