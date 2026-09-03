//! Whether an entrant actually owns what they handed in.
//!
//! A contest entry points at a URL, and until now nothing looked at whose URL
//! it was. Somebody could enter a well-known project, or a rival's entry, and
//! win a prize pool with it. The same hole was closed on challenge submissions
//! by making an attachment a reference to a row the platform holds; a contest
//! artifact cannot work that way, because what it points at lives on GitHub
//! and the platform does not host it.
//!
//! What is checkable is narrower and worth doing exactly: a `github.com` URL
//! names its owner in the first path segment, and the platform already knows
//! every account's GitHub login. So a github.com URL in an entry can be
//! required to be the entrant's own — which covers the artifact types a code
//! contest actually uses.
//!
//! A deployed demo, a hosted design file or a video cannot be attributed from
//! a URL at all. Those are accepted and recorded as unchecked. Saying so is
//! better than a check that only looks like one.

use uuid::Uuid;

use crate::errors::AppError;

/// The hosts whose URLs name an owner we can compare against an account.
const GITHUB_HOSTS: &[&str] = &["github.com", "www.github.com", "gist.github.com"];

/// The owner segment of a github.com URL, lowercased.
///
/// `https://github.com/octocat/hello` and `https://gist.github.com/octocat/1`
/// both give `octocat`. `None` for any other host, and for a github.com URL
/// with nothing after the host — which is not an artifact anybody submitted.
pub fn github_owner(url: &str) -> Option<String> {
    let rest = url
        .trim()
        .strip_prefix("https://")
        .or_else(|| url.trim().strip_prefix("http://"))?;
    let (host, path) = rest.split_once('/')?;
    if !GITHUB_HOSTS.contains(&host.to_ascii_lowercase().as_str()) {
        return None;
    }
    let owner = path.split('/').next()?.trim();
    if owner.is_empty() {
        return None;
    }
    Some(owner.to_ascii_lowercase())
}

/// Check every URL in an entry, and say whether anybody vouched for it.
///
/// Returns `true` when at least one URL was checked and all the checkable ones
/// belong to `user_id`; `false` when nothing in the entry could be checked.
/// Refuses outright when a github.com URL names somebody else — that is not an
/// unverifiable entry, it is a wrong one.
pub async fn verify_entry_urls(
    db: &sqlx::PgPool,
    user_id: Uuid,
    urls: &[&str],
) -> Result<bool, AppError> {
    let claimed: Vec<String> = urls.iter().filter_map(|u| github_owner(u)).collect();
    if claimed.is_empty() {
        // Nothing on a host we can attribute. Accepted, and recorded as
        // nobody having checked it.
        return Ok(false);
    }

    let login: Option<String> =
        sqlx::query_scalar("SELECT github_login FROM github_connections WHERE user_id = $1")
            .bind(user_id)
            .fetch_optional(db)
            .await?;

    let Some(login) = login else {
        return Err(AppError::Validation(
            "This entry points at GitHub, and no GitHub account is connected to \
             yours — so nobody can tell it is your work. Connect one via \
             /api/github/oauth, or hand in something else."
                .into(),
        ));
    };
    let login = login.to_ascii_lowercase();

    for owner in &claimed {
        if owner != &login {
            return Err(AppError::Validation(format!(
                "This entry points at github.com/{owner}, and your connected \
                 GitHub account is {login}. An entry has to be your own work."
            )));
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_owner_is_the_first_path_segment() {
        assert_eq!(
            github_owner("https://github.com/octocat/hello"),
            Some("octocat".to_string())
        );
        assert_eq!(
            github_owner("https://gist.github.com/octocat/1abc"),
            Some("octocat".to_string())
        );
        assert_eq!(
            github_owner("https://github.com/octocat/hello/pull/3"),
            Some("octocat".to_string())
        );
    }

    /// GitHub logins are case-insensitive, so the comparison has to be too —
    /// otherwise `OctoCat` entering their own repository is refused.
    #[test]
    fn the_comparison_ignores_case() {
        assert_eq!(
            github_owner("https://GitHub.com/OctoCat/Hello"),
            Some("octocat".to_string())
        );
    }

    /// Anything else is unattributable rather than wrong, and this function
    /// says so by declining to name an owner.
    #[test]
    fn a_host_we_cannot_attribute_names_nobody() {
        for url in [
            "https://example.com/octocat/hello",
            "https://gitlab.com/octocat/hello",
            "https://github.com",
            "https://github.com/",
            "not a url at all",
            // The trap worth naming: a host that merely ends in github.com.
            "https://github.com.evil.test/octocat/hello",
        ] {
            assert_eq!(github_owner(url), None, "{url} was attributed to somebody");
        }
    }
}
