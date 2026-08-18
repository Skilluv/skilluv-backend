//! SKI-42 (Post-MVP T2-03) — external reputation signals.
//!
//! See migration 0145 for the isolation guarantee. This module deliberately
//! imports nothing from `ranks`, `badge_engine`, `capabilities_engine` or
//! `skills`: the absence of those dependencies is what makes "external
//! signals never affect Skilluv proofs" true by construction rather than by
//! discipline.
//!
//! ## URL validation
//!
//! URLs are user-supplied and end up rendered as links on a public profile,
//! so they are checked against a per-provider host allowlist. That serves
//! two purposes:
//!
//!   * it stops a `medium` signal from pointing at an unrelated domain,
//!     which is what makes the provider label meaningful at all;
//!   * it keeps the stored value inside a known set of hosts, so nothing
//!     downstream is tempted to fetch an arbitrary URL later.
//!
//! `conf_ref` is the one open provider — conference talks live wherever the
//! conference lives — so it only gets the generic scheme and shape checks.

use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

pub const PROVIDER_GITHUB: &str = "github";
pub const PROVIDER_MEDIUM: &str = "medium";
pub const PROVIDER_DEV_TO: &str = "dev_to";
pub const PROVIDER_CONF_REF: &str = "conf_ref";

// Design portfolios (migration 0415). Declared and reviewed, never imported:
// Behance's public API was withdrawn in 2020 and Dribbble's needs a
// partnership, so an "import" would mean fetching arbitrary user-supplied
// URLs from the backend — and an imported portfolio must not count for
// anything anyway.
pub const PROVIDER_BEHANCE: &str = "behance";
pub const PROVIDER_DRIBBBLE: &str = "dribbble";
pub const PROVIDER_ARTSTATION: &str = "artstation";
pub const PROVIDER_VIMEO: &str = "vimeo";
/// A type foundry. Open-hosted, because a family can be published anywhere.
pub const PROVIDER_FOUNDRY: &str = "foundry";

pub const PROVIDERS: &[&str] = &[
    PROVIDER_GITHUB,
    PROVIDER_MEDIUM,
    PROVIDER_DEV_TO,
    PROVIDER_CONF_REF,
    PROVIDER_BEHANCE,
    PROVIDER_DRIBBBLE,
    PROVIDER_ARTSTATION,
    PROVIDER_VIMEO,
    PROVIDER_FOUNDRY,
];

/// Cap on signals per user. External context is a sidebar, not a second
/// portfolio, and an unbounded list would drown the actual proofs.
pub const MAX_SIGNALS_PER_USER: i64 = 20;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ExternalSignal {
    pub id: Uuid,
    pub user_id: Uuid,
    pub provider: String,
    pub url: String,
    pub title: String,
    pub verified_at: Option<chrono::DateTime<chrono::Utc>>,
    pub verification_method: Option<String>,
    pub verified_by: Option<Uuid>,
    pub meta: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub struct CreateParams<'a> {
    pub provider: &'a str,
    pub url: &'a str,
    pub title: &'a str,
    pub meta: Option<serde_json::Value>,
}

/// Hosts accepted per provider. `None` means "any host" (`conf_ref`).
fn allowed_hosts(provider: &str) -> Option<&'static [&'static str]> {
    match provider {
        PROVIDER_GITHUB => Some(&["github.com", "www.github.com", "gist.github.com"]),
        PROVIDER_MEDIUM => Some(&["medium.com"]),
        PROVIDER_DEV_TO => Some(&["dev.to"]),
        // Pinned hosts, so a link labelled "Behance" cannot point somewhere
        // else. A moderator confirming ownership should not also have to
        // notice the domain.
        PROVIDER_BEHANCE => Some(&["behance.net", "www.behance.net"]),
        PROVIDER_DRIBBBLE => Some(&["dribbble.com", "www.dribbble.com"]),
        PROVIDER_ARTSTATION => Some(&["artstation.com", "www.artstation.com"]),
        PROVIDER_VIMEO => Some(&["vimeo.com", "player.vimeo.com"]),
        // `foundry` and `conf_ref` accept any host: a typeface and a talk
        // both live wherever their author put them.
        _ => None,
    }
}

/// Extract the lowercase host from an `https://` URL.
///
/// Hand-rolled rather than pulling in a URL parser: the accepted shape is
/// narrow (scheme is already pinned to `https://` by the caller) and the
/// result is only ever compared against an allowlist.
fn host_of(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://")?;
    let host = rest
        .split(['/', '?', '#'])
        .next()?
        // Strip userinfo: `https://evil.com@github.com/x` must not read as
        // host `evil.com`, and `https://github.com@evil.com/x` must not read
        // as `github.com`. The host is what follows the last '@'.
        .rsplit('@')
        .next()?;
    // Drop an explicit port before comparing.
    let host = host.split(':').next()?;
    if host.is_empty() {
        return None;
    }
    Some(host.to_ascii_lowercase())
}

/// Validate the provider, URL shape and host.
pub fn validate(provider: &str, url: &str, title: &str) -> Result<(), AppError> {
    if !PROVIDERS.contains(&provider) {
        return Err(AppError::Validation(format!(
            "provider must be one of: {}",
            PROVIDERS.join(", ")
        )));
    }
    if !url.starts_with("https://") {
        return Err(AppError::Validation("url must start with https://".into()));
    }
    if !(12..=500).contains(&url.len()) {
        return Err(AppError::Validation(
            "url must be 12..500 characters".into(),
        ));
    }
    let host =
        host_of(url).ok_or_else(|| AppError::Validation("url has no readable host".into()))?;

    // Reject literal IPs outright, for every provider including conf_ref.
    // A talk reference pointing at a bare address is never legitimate and
    // is exactly the shape an internal-network probe takes.
    if host.parse::<std::net::IpAddr>().is_ok() {
        return Err(AppError::Validation(
            "url must use a hostname, not an IP address".into(),
        ));
    }
    if !host.contains('.') {
        return Err(AppError::Validation(
            "url host must be a fully-qualified domain".into(),
        ));
    }

    if let Some(allowed) = allowed_hosts(provider)
        && !allowed.contains(&host.as_str())
    {
        return Err(AppError::Validation(format!(
            "a '{provider}' signal must link to one of: {}",
            allowed.join(", ")
        )));
    }

    let title_len = title.trim().chars().count();
    if !(3..=200).contains(&title_len) {
        return Err(AppError::Validation(
            "title must be 3..200 characters".into(),
        ));
    }
    Ok(())
}

/// The GitHub login the URL claims, if it looks like a user-owned URL.
///
/// `https://github.com/<login>/...` and `https://gist.github.com/<login>/...`
/// both put the owner in the first path segment.
fn github_login_in(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://")?;
    let path = rest.split_once('/')?.1;
    let first = path.split(['/', '?', '#']).next()?;
    if first.is_empty() {
        return None;
    }
    Some(first.to_ascii_lowercase())
}

/// Create a signal, self-verifying it when we already hold proof of
/// ownership.
///
/// The only zero-cost proof available is the GitHub OAuth link the user
/// completed earlier: if the URL's owner segment matches the login stored
/// in `github_connections`, the claim is confirmed without any outbound
/// request. Everything else is recorded unverified.
pub async fn create(
    db: &PgPool,
    user_id: Uuid,
    params: CreateParams<'_>,
) -> Result<ExternalSignal, AppError> {
    validate(params.provider, params.url, params.title)?;

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM external_signals WHERE user_id = $1")
        .bind(user_id)
        .fetch_one(db)
        .await?;
    if count >= MAX_SIGNALS_PER_USER {
        return Err(AppError::Validation(format!(
            "at most {MAX_SIGNALS_PER_USER} external signals — remove one first"
        )));
    }

    let mut verified = false;
    let mut meta = params.meta.unwrap_or_else(|| serde_json::json!({}));
    if !meta.is_object() {
        return Err(AppError::Validation("meta must be a JSON object".into()));
    }

    if params.provider == PROVIDER_GITHUB
        && let Some(claimed) = github_login_in(params.url)
    {
        let linked: Option<String> =
            sqlx::query_scalar("SELECT github_login FROM github_connections WHERE user_id = $1")
                .bind(user_id)
                .fetch_optional(db)
                .await?;
        if let Some(login) = linked
            && login.to_ascii_lowercase() == claimed
        {
            verified = true;
            if let Some(obj) = meta.as_object_mut() {
                obj.insert("github_login".to_string(), serde_json::Value::String(login));
            }
        }
    }

    let inserted: Result<ExternalSignal, sqlx::Error> = sqlx::query_as(
        r#"
        INSERT INTO external_signals
            (user_id, provider, url, title, verified_at, verification_method, meta)
        VALUES ($1, $2, $3, $4,
                CASE WHEN $5 THEN NOW() ELSE NULL END,
                CASE WHEN $5 THEN 'oauth_github' ELSE NULL END,
                $6)
        RETURNING *
        "#,
    )
    .bind(user_id)
    .bind(params.provider)
    .bind(params.url)
    .bind(params.title.trim())
    .bind(verified)
    .bind(&meta)
    .fetch_one(db)
    .await;

    match inserted {
        Ok(s) => Ok(s),
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => Err(AppError::Conflict(
            "this URL is already linked to your profile".into(),
        )),
        Err(e) => Err(e.into()),
    }
}

/// A user's signals, verified first.
pub async fn list_for_user(db: &PgPool, user_id: Uuid) -> Result<Vec<ExternalSignal>, AppError> {
    let signals: Vec<ExternalSignal> = sqlx::query_as(
        "SELECT * FROM external_signals
          WHERE user_id = $1
          ORDER BY verified_at DESC NULLS LAST, created_at DESC",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;
    Ok(signals)
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn host_extraction_is_not_fooled_by_userinfo_or_ports() {
        assert_eq!(
            host_of("https://github.com/alice"),
            Some("github.com".into())
        );
        assert_eq!(
            host_of("https://GitHub.com/alice"),
            Some("github.com".into()),
            "host comparison is case-insensitive"
        );
        assert_eq!(
            host_of("https://github.com:443/alice"),
            Some("github.com".into())
        );
        // The classic allowlist bypass: the real host is after the '@'.
        assert_eq!(
            host_of("https://github.com@evil.test/alice"),
            Some("evil.test".into())
        );
        assert_eq!(
            host_of("https://evil.test@github.com/alice"),
            Some("github.com".into())
        );
        assert_eq!(host_of("http://github.com/alice"), None, "https only");
    }

    #[test]
    fn provider_host_allowlist_is_enforced() {
        assert!(validate(PROVIDER_GITHUB, "https://github.com/alice/repo", "My repo").is_ok());
        assert!(validate(PROVIDER_MEDIUM, "https://medium.com/@alice/post", "My post").is_ok());
        assert!(validate(PROVIDER_DEV_TO, "https://dev.to/alice/post", "My post").is_ok());
        // conf_ref accepts any well-formed host.
        assert!(
            validate(
                PROVIDER_CONF_REF,
                "https://rustconf.test/talks/1",
                "My talk"
            )
            .is_ok()
        );

        assert!(
            validate(PROVIDER_GITHUB, "https://evil.test/alice/repo", "Fake").is_err(),
            "a github signal must actually point at github"
        );
        assert!(
            validate(
                PROVIDER_MEDIUM,
                "https://github.com/alice",
                "Wrong provider"
            )
            .is_err()
        );
        assert!(validate("linkedin", "https://linkedin.test/in/a", "x").is_err());
    }

    #[test]
    fn urls_that_are_not_public_hostnames_are_refused() {
        // Internal-network shapes, refused for every provider.
        assert!(validate(PROVIDER_CONF_REF, "https://127.0.0.1/talk", "x").is_err());
        assert!(validate(PROVIDER_CONF_REF, "https://192.168.1.10/talk", "x").is_err());
        assert!(validate(PROVIDER_CONF_REF, "https://localhost/talk", "x").is_err());
        assert!(
            validate(PROVIDER_CONF_REF, "https://internal-wiki/talk", "x").is_err(),
            "a host with no dot is not a public domain"
        );
        assert!(validate(PROVIDER_CONF_REF, "http://rustconf.test/t/1", "x").is_err());
    }

    #[test]
    fn title_bounds_are_enforced() {
        let url = "https://dev.to/alice/post";
        assert!(validate(PROVIDER_DEV_TO, url, "ab").is_err());
        assert!(validate(PROVIDER_DEV_TO, url, &"x".repeat(201)).is_err());
        assert!(validate(PROVIDER_DEV_TO, url, &"x".repeat(200)).is_ok());
    }

    #[test]
    fn github_owner_segment_is_extracted() {
        assert_eq!(
            github_login_in("https://github.com/Alice/repo"),
            Some("alice".into())
        );
        assert_eq!(
            github_login_in("https://gist.github.com/alice/deadbeef"),
            Some("alice".into())
        );
        assert_eq!(github_login_in("https://github.com/"), None);
    }
}
