//! Importing what somebody has already built, from where it already lives.
//!
//! ## The split, again
//!
//! Recognising a platform from a URL, deciding whether an account is
//! countable, and knowing which forges answer which questions — all pure, all
//! tested here. Asking GitHub how many stars somebody has is a call to
//! somebody else's service and sits behind [`fetch`], like the registry
//! statistics it sits next to.
//!
//! ## Claimed against verified
//!
//! Anybody can type `torvalds`. Only an OAuth flow proves it. Both are stored
//! — a Codeberg profile is worth showing on a page even unproved — and only
//! the proved one is countable. That rule lives in [`is_countable`] and in a
//! partial unique index, so neither the API nor a direct INSERT can go round
//! it.
//!
//! ## Why the alternative forges are worth the trouble
//!
//! GitLab, Codeberg and SourceHut exist because some people will not put
//! their work on GitHub. A platform whose whole argument is "prove what you
//! have done" cannot then require one company's account to do it.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

/// Every platform the column allows.
pub const PLATFORMS: &[&str] = &[
    "github",
    "gitlab",
    "codeberg",
    "sourcehut",
    "crates_io",
    "npm",
    "pypi",
    "go_modules",
    "rubygems",
    "maven_central",
    "nuget",
    "packagist",
    "hex",
    "homebrew",
];

/// Forges — places code is written. The rest are registries, where it is
/// published.
pub const FORGES: &[&str] = &["github", "gitlab", "codeberg", "sourcehut"];

/// Platforms whose ownership Skilluv can prove today.
///
/// One, and that is the honest number. GitLab has OAuth and Skilluv has not
/// registered an application for it yet; Codeberg and SourceHut are read-only
/// public APIs. Listing aspirations here would mean the countability rule
/// silently accepts unproved accounts the day somebody adds a string.
pub const VERIFIABLE_PLATFORMS: &[&str] = &["github"];

/// Whether a portfolio row may feed anything that counts.
///
/// The whole rule in one place: proved, and on a platform where proving means
/// something. A claimed handle is a link on a page and nothing more.
pub fn is_countable(platform: &str, verified: bool) -> bool {
    verified && VERIFIABLE_PLATFORMS.contains(&platform)
}

/// Which platform a profile URL belongs to, and the handle on it.
///
/// Registries are handed to the package parser, which already knows all ten
/// of them — two parsers for the same URLs would drift within a month.
pub fn identify_profile(url: &str) -> Option<(&'static str, String)> {
    let trimmed = url.trim();
    let rest = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))?;
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let host = host.trim_start_matches("www.").to_ascii_lowercase();
    let segments: Vec<&str> = path
        .split(['?', '#'])
        .next()
        .unwrap_or("")
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();

    let first = segments.first().copied();

    match host.as_str() {
        // A profile is the first path segment and nothing after it:
        // github.com/torvalds is a person, github.com/torvalds/linux is a
        // repository, and filing the second as a profile would claim somebody
        // owns an account called `linux`.
        "github.com" => match segments.as_slice() {
            [handle] => Some(("github", (*handle).to_string())),
            _ => None,
        },
        "gitlab.com" => match segments.as_slice() {
            [handle] => Some(("gitlab", (*handle).to_string())),
            _ => None,
        },
        "codeberg.org" => match segments.as_slice() {
            [handle] => Some(("codeberg", (*handle).to_string())),
            _ => None,
        },
        // SourceHut puts the tilde in the path: sr.ht/~sircmpwn
        "sr.ht" | "git.sr.ht" => first
            .filter(|h| h.starts_with('~'))
            .map(|h| ("sourcehut", h.trim_start_matches('~').to_string())),
        _ => None,
    }
}

/// The public API a forge answers profile questions on.
pub fn forge_api(platform: &str, handle: &str) -> Option<String> {
    match platform {
        "github" => Some(format!("https://api.github.com/users/{handle}")),
        "gitlab" => Some(format!("https://gitlab.com/api/v4/users?username={handle}")),
        // Codeberg runs Forgejo, whose API is Gitea's.
        "codeberg" => Some(format!("https://codeberg.org/api/v1/users/{handle}")),
        // SourceHut's meta API needs a token even to read a public profile,
        // so there is nothing to call anonymously. Recognised, listed, not
        // measured — the same answer the registries with no download figures
        // get.
        "sourcehut" => None,
        _ => None,
    }
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Portfolio {
    pub id: Uuid,
    pub platform: String,
    pub handle: String,
    pub profile_url: String,
    pub verified_at: Option<chrono::DateTime<chrono::Utc>>,
    pub repos_count: Option<i32>,
    pub stars_received: Option<i32>,
    pub followers_count: Option<i32>,
    pub contributions_last_year: Option<i32>,
    pub packages_count: Option<i32>,
    pub downloads_total: Option<i64>,
    pub metadata: serde_json::Value,
    pub last_synced_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_error: Option<String>,
}

/// Record an account somebody says is theirs.
///
/// Claimed, not verified: this is the door for the forges Skilluv cannot
/// prove. Passing `verified` here is refused for anything not in
/// [`VERIFIABLE_PLATFORMS`], so the only way to a verified row is the OAuth
/// callback.
pub async fn claim(db: &PgPool, user_id: Uuid, profile_url: &str) -> Result<Portfolio, AppError> {
    let (platform, handle) = identify_profile(profile_url).ok_or_else(|| {
        AppError::Validation(
            "that URL is not a profile on a forge Skilluv knows — GitHub, GitLab, Codeberg \
             or SourceHut, and the profile page itself rather than one of its repositories"
                .into(),
        )
    })?;

    // Somebody else has proved this account. Two people cannot both be it,
    // and the one who proved it wins.
    let taken: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM user_external_portfolios
              WHERE platform = $1 AND lower(handle) = lower($2)
                AND verified_at IS NOT NULL AND user_id <> $3)",
    )
    .bind(platform)
    .bind(&handle)
    .bind(user_id)
    .fetch_one(db)
    .await?;
    if taken {
        return Err(AppError::Validation(format!(
            "{handle} on {platform} has already been proved by somebody else"
        )));
    }

    let row: Portfolio = sqlx::query_as(
        r#"
        INSERT INTO user_external_portfolios (user_id, platform, handle, profile_url)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (user_id, platform, handle) DO UPDATE
            SET profile_url = EXCLUDED.profile_url, sync_enabled = TRUE
        RETURNING id, platform, handle, profile_url, verified_at,
                  repos_count, stars_received, followers_count,
                  contributions_last_year, packages_count, downloads_total,
                  metadata, last_synced_at, last_error
        "#,
    )
    .bind(user_id)
    .bind(platform)
    .bind(&handle)
    .bind(profile_url.trim())
    .fetch_one(db)
    .await?;

    Ok(row)
}

/// Record an account whose ownership was proved.
///
/// Called from the OAuth callback and from nowhere else that should exist.
pub async fn record_verified(
    db: &PgPool,
    user_id: Uuid,
    platform: &str,
    handle: &str,
    profile_url: &str,
    method: &str,
) -> Result<(), AppError> {
    if !VERIFIABLE_PLATFORMS.contains(&platform) {
        return Err(AppError::Internal(format!(
            "{platform} has no way of proving ownership — recording one as verified would \
             make the distinction meaningless"
        )));
    }

    sqlx::query(
        r#"
        INSERT INTO user_external_portfolios
            (user_id, platform, handle, profile_url, verified_at, verification_method)
        VALUES ($1, $2, $3, $4, NOW(), $5)
        ON CONFLICT (user_id, platform, handle) DO UPDATE
            SET profile_url = EXCLUDED.profile_url,
                verified_at = COALESCE(user_external_portfolios.verified_at, NOW()),
                verification_method = EXCLUDED.verification_method,
                sync_enabled = TRUE
        "#,
    )
    .bind(user_id)
    .bind(platform)
    .bind(handle)
    .bind(profile_url)
    .bind(method)
    .execute(db)
    .await?;

    Ok(())
}

pub async fn for_user(db: &PgPool, user_id: Uuid) -> Result<Vec<Portfolio>, AppError> {
    let rows = sqlx::query_as::<_, Portfolio>(
        "SELECT id, platform, handle, profile_url, verified_at,
                repos_count, stars_received, followers_count,
                contributions_last_year, packages_count, downloads_total,
                metadata, last_synced_at, last_error
           FROM user_external_portfolios
          WHERE user_id = $1
          ORDER BY verified_at NULLS LAST, platform, handle",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

// ═══════════════════════════════════════════════════════════════════
// Fetching
// ═══════════════════════════════════════════════════════════════════

/// What a forge answered about an account.
#[derive(Debug, Clone, Default)]
pub struct ForgeProfile {
    pub repos_count: Option<i32>,
    pub followers_count: Option<i32>,
    pub stars_received: Option<i32>,
    pub metadata: serde_json::Value,
}

#[derive(Deserialize)]
struct GithubUser {
    public_repos: Option<i32>,
    followers: Option<i32>,
    name: Option<String>,
    bio: Option<String>,
    company: Option<String>,
    blog: Option<String>,
    created_at: Option<String>,
}

#[derive(Deserialize)]
struct GiteaUser {
    followers_count: Option<i32>,
    #[serde(default)]
    full_name: Option<String>,
    #[serde(default)]
    website: Option<String>,
    #[serde(default)]
    created: Option<String>,
}

#[derive(Deserialize)]
struct GitlabUser {
    name: Option<String>,
    bio: Option<String>,
    web_url: Option<String>,
    created_at: Option<String>,
}

/// Ask a forge about an account.
///
/// Anonymous. Every one of these endpoints answers without a token for a
/// public profile, which matters: an import that only works for people who
/// have connected an account is not an import, it is the OAuth flow with
/// extra steps.
pub async fn fetch(
    client: &reqwest::Client,
    platform: &str,
    handle: &str,
) -> Result<ForgeProfile, AppError> {
    let Some(url) = forge_api(platform, handle) else {
        // Recognised and honest about having nothing to report, like a
        // registry that publishes no downloads.
        return Ok(ForgeProfile::default());
    };

    let response = client
        .get(&url)
        .header("User-Agent", "skilluv (https://skill-uv.com)")
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("{platform} unreachable: {e}")))?;

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(AppError::NotFound(format!(
            "{platform} has no account called {handle}"
        )));
    }
    let response = response
        .error_for_status()
        .map_err(|e| AppError::Internal(format!("{platform} refused: {e}")))?;

    match platform {
        "github" => {
            let user: GithubUser = response.json().await.map_err(|e| {
                AppError::Internal(format!("github sent something unexpected: {e}"))
            })?;
            Ok(ForgeProfile {
                repos_count: user.public_repos,
                followers_count: user.followers,
                // Stars received is a sum across repositories, not a field on
                // the profile. `github_repos` already holds them for anybody
                // who connected, and `stars_from_repos` reads that.
                stars_received: None,
                metadata: serde_json::json!({
                    "name": user.name,
                    "bio": user.bio,
                    "company": user.company,
                    "blog": user.blog,
                    "since": user.created_at,
                }),
            })
        }
        "codeberg" => {
            let user: GiteaUser = response.json().await.map_err(|e| {
                AppError::Internal(format!("codeberg sent something unexpected: {e}"))
            })?;
            Ok(ForgeProfile {
                repos_count: None,
                followers_count: user.followers_count,
                stars_received: None,
                metadata: serde_json::json!({
                    "name": user.full_name,
                    "blog": user.website,
                    "since": user.created,
                }),
            })
        }
        "gitlab" => {
            // The users endpoint answers a list, because a username filter
            // could in principle match more than one. It cannot, but the
            // shape is a list either way.
            let users: Vec<GitlabUser> = response.json().await.map_err(|e| {
                AppError::Internal(format!("gitlab sent something unexpected: {e}"))
            })?;
            let Some(user) = users.into_iter().next() else {
                return Err(AppError::NotFound(format!(
                    "gitlab has no account called {handle}"
                )));
            };
            Ok(ForgeProfile {
                repos_count: None,
                followers_count: None,
                stars_received: None,
                metadata: serde_json::json!({
                    "name": user.name,
                    "bio": user.bio,
                    "profile": user.web_url,
                    "since": user.created_at,
                }),
            })
        }
        _ => Ok(ForgeProfile::default()),
    }
}

// ═══════════════════════════════════════════════════════════════════
// The contribution graph
// ═══════════════════════════════════════════════════════════════════

/// A year of daily contribution counts.
///
/// The one figure on a GitHub profile that reads as effort rather than
/// outcome, and the reason it is worth the trouble: a graph shows somebody
/// who turned up on Tuesdays for a year, which no count of merged pull
/// requests does.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContributionGraph {
    pub total: i32,
    /// `[["2026-01-14", 3], …]`, oldest first. A flat array of pairs rather
    /// than a map: it is rendered in order and JSON objects have none.
    pub days: Vec<(String, i32)>,
    /// The longest run of consecutive days with at least one contribution.
    pub longest_streak: i32,
}

#[derive(Deserialize)]
struct GraphqlEnvelope {
    data: Option<GraphqlData>,
}

#[derive(Deserialize)]
struct GraphqlData {
    user: Option<GraphqlUser>,
}

#[derive(Deserialize)]
struct GraphqlUser {
    #[serde(rename = "contributionsCollection")]
    contributions_collection: ContributionsCollection,
}

#[derive(Deserialize)]
struct ContributionsCollection {
    #[serde(rename = "contributionCalendar")]
    contribution_calendar: ContributionCalendar,
}

#[derive(Deserialize)]
struct ContributionCalendar {
    #[serde(rename = "totalContributions")]
    total_contributions: i32,
    weeks: Vec<ContributionWeek>,
}

#[derive(Deserialize)]
struct ContributionWeek {
    #[serde(rename = "contributionDays")]
    contribution_days: Vec<ContributionDay>,
}

#[derive(Deserialize)]
struct ContributionDay {
    date: String,
    #[serde(rename = "contributionCount")]
    contribution_count: i32,
}

/// The longest run of consecutive days with something on them.
///
/// Pure, and worth its own function: an off-by-one here reports somebody's
/// hundred-day streak as ninety-nine, which is the kind of wrong that gets
/// noticed by exactly the person it is about.
pub fn longest_streak(days: &[(String, i32)]) -> i32 {
    let mut best = 0;
    let mut current = 0;
    for (_, count) in days {
        if *count > 0 {
            current += 1;
            best = best.max(current);
        } else {
            current = 0;
        }
    }
    best
}

/// Ask GitHub for a year of contributions.
///
/// GraphQL, because this is the one thing the REST API does not expose — and
/// authenticated, because the calendar is not public over the API even for a
/// public profile. So it is only available for somebody who connected their
/// account, which is the same set of people it is countable for.
pub async fn fetch_contribution_graph(
    client: &reqwest::Client,
    token: &str,
    handle: &str,
) -> Result<ContributionGraph, AppError> {
    let query = serde_json::json!({
        "query": "query($login: String!) { \
                    user(login: $login) { \
                      contributionsCollection { \
                        contributionCalendar { \
                          totalContributions \
                          weeks { contributionDays { date contributionCount } } \
                        } \
                      } \
                    } \
                  }",
        "variables": { "login": handle },
    });

    let response = client
        .post("https://api.github.com/graphql")
        .header("User-Agent", "skilluv (https://skill-uv.com)")
        .header("Authorization", format!("Bearer {token}"))
        .json(&query)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("github graphql unreachable: {e}")))?
        .error_for_status()
        .map_err(|e| AppError::Internal(format!("github graphql refused: {e}")))?;

    let envelope: GraphqlEnvelope = response
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("github sent something unexpected: {e}")))?;

    // GraphQL answers 200 with `data: null` when the user is gone or the
    // token cannot see them. Treated as absence rather than as an error:
    // there is nothing to retry.
    let Some(user) = envelope.data.and_then(|d| d.user) else {
        return Ok(ContributionGraph::default());
    };

    let calendar = user.contributions_collection.contribution_calendar;
    let days: Vec<(String, i32)> = calendar
        .weeks
        .into_iter()
        .flat_map(|w| w.contribution_days)
        .map(|d| (d.date, d.contribution_count))
        .collect();

    Ok(ContributionGraph {
        total: calendar.total_contributions,
        longest_streak: longest_streak(&days),
        days,
    })
}

/// Refresh somebody's contribution graph, if they connected GitHub.
///
/// Stored in `metadata` rather than in a column of its own: it is three
/// hundred and sixty-five pairs read as one blob by one page, and a column
/// per figure would be a column nobody queries on.
pub async fn sync_contribution_graph(
    db: &PgPool,
    client: &reqwest::Client,
    jwt_secret: &str,
    user_id: Uuid,
) -> Result<Option<ContributionGraph>, AppError> {
    let handle: Option<String> = sqlx::query_scalar(
        "SELECT handle FROM user_external_portfolios
          WHERE user_id = $1 AND platform = 'github' AND verified_at IS NOT NULL",
    )
    .bind(user_id)
    .fetch_optional(db)
    .await?;
    let Some(handle) = handle else {
        return Ok(None);
    };

    let Some(token) = crate::services::github::load_token(db, jwt_secret, user_id).await? else {
        // Proved but the token is gone — disconnected, or revoked upstream.
        // Not an error: there is simply nothing to ask with.
        return Ok(None);
    };

    let graph = fetch_contribution_graph(client, &token, &handle).await?;

    sqlx::query(
        "UPDATE user_external_portfolios
            SET contributions_last_year = $2,
                metadata = metadata || jsonb_build_object(
                    'contribution_graph', $3::JSONB,
                    'longest_streak_days', $4::INT
                ),
                last_synced_at = NOW(),
                last_error = NULL
          WHERE user_id = $1 AND platform = 'github'",
    )
    .bind(user_id)
    .bind(graph.total)
    .bind(serde_json::to_value(&graph.days).unwrap_or_else(|_| serde_json::json!([])))
    .bind(graph.longest_streak)
    .execute(db)
    .await?;

    Ok(Some(graph))
}

/// Stars across the repositories already imported for this person.
///
/// Read from `github_repos` rather than from the API: the repository list is
/// already synced by the OAuth import, and summing it is one query against
/// one page-size worth of API calls.
pub async fn stars_from_repos(db: &PgPool, user_id: Uuid) -> Result<Option<i32>, AppError> {
    let row: Option<(i64, i64)> = sqlx::query_as(
        "SELECT count(*), COALESCE(sum(stargazers_count), 0)
           FROM github_repos
          WHERE user_id = $1 AND fork = FALSE",
    )
    .bind(user_id)
    .fetch_optional(db)
    .await?;

    // No repositories imported is not zero stars. The difference is between
    // "we looked and there are none" and "we have not looked".
    Ok(match row {
        Some((0, _)) | None => None,
        Some((_, stars)) => Some(stars.min(i32::MAX as i64) as i32),
    })
}

/// Refresh one portfolio row, keeping the previous figures on failure.
///
/// Same rule as the package statistics: an old figure with a visible date is
/// worth more than no figure, and much more than a zero that reads as "this
/// person has nothing".
pub async fn sync_one(
    db: &PgPool,
    client: &reqwest::Client,
    portfolio_id: Uuid,
) -> Result<(), AppError> {
    let row: Option<(Uuid, String, String)> = sqlx::query_as(
        "SELECT user_id, platform, handle FROM user_external_portfolios WHERE id = $1",
    )
    .bind(portfolio_id)
    .fetch_optional(db)
    .await?;
    let (user_id, platform, handle) =
        row.ok_or_else(|| AppError::NotFound("portfolio not found".into()))?;

    match fetch(client, &platform, &handle).await {
        Ok(profile) => {
            let stars = match platform.as_str() {
                "github" => stars_from_repos(db, user_id).await?,
                _ => profile.stars_received,
            };
            sqlx::query(
                // `items_count` and `reach_count` are the domain-neutral
                // projection every reader outside code uses (migration 0515).
                // Written in the same statement as the specific columns so the
                // two cannot disagree.
                "UPDATE user_external_portfolios
                    SET repos_count = COALESCE($2, repos_count),
                        followers_count = COALESCE($3, followers_count),
                        stars_received = COALESCE($4, stars_received),
                        items_count = COALESCE($2, items_count),
                        reach_count = COALESCE($4::BIGINT, reach_count),
                        metadata = $5,
                        last_synced_at = NOW(),
                        last_error = NULL
                  WHERE id = $1",
            )
            .bind(portfolio_id)
            .bind(profile.repos_count)
            .bind(profile.followers_count)
            .bind(stars)
            .bind(&profile.metadata)
            .execute(db)
            .await?;
        }
        Err(e) => {
            sqlx::query(
                "UPDATE user_external_portfolios
                    SET last_error = $2, last_synced_at = last_synced_at
                  WHERE id = $1",
            )
            .bind(portfolio_id)
            .bind(e.to_string())
            .execute(db)
            .await?;
        }
    }

    Ok(())
}

/// Refresh everything older than a week.
///
/// Weekly, because none of these figures move fast enough to be worth asking
/// more often, and every one of these APIs rate-limits anonymous callers.
///
/// `jwt_secret` is what decrypts a stored GitHub token, which is the only way
/// to reach the contribution calendar. Passed rather than read from the
/// environment so a caller with no secret gets everything except the graph
/// instead of a panic.
pub async fn sync_stale(
    db: &PgPool,
    client: &reqwest::Client,
    jwt_secret: Option<&str>,
) -> Result<u64, AppError> {
    if let Some(secret) = jwt_secret {
        // The graph is per person rather than per portfolio row: one call
        // covers somebody however many accounts they have listed.
        let connected: Vec<Uuid> = sqlx::query_scalar(
            "SELECT p.user_id FROM user_external_portfolios p
               JOIN github_connections g ON g.user_id = p.user_id
              WHERE p.platform = 'github' AND p.verified_at IS NOT NULL
                AND p.sync_enabled = TRUE
                AND (p.last_synced_at IS NULL
                     OR p.last_synced_at < NOW() - INTERVAL '7 days')
              LIMIT 200",
        )
        .fetch_all(db)
        .await?;

        for user_id in connected {
            // One failure must not stop the sweep.
            if let Err(err) = sync_contribution_graph(db, client, secret, user_id).await {
                tracing::warn!(%user_id, error = %err, "contribution graph refresh failed");
            }
        }
    }

    sync_stale_profiles(db, client).await
}

async fn sync_stale_profiles(db: &PgPool, client: &reqwest::Client) -> Result<u64, AppError> {
    let stale: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM user_external_portfolios
          WHERE sync_enabled = TRUE
            AND (last_synced_at IS NULL OR last_synced_at < NOW() - INTERVAL '7 days')
          ORDER BY last_synced_at NULLS FIRST
          LIMIT 200",
    )
    .fetch_all(db)
    .await?;

    let mut done = 0u64;
    for id in stale {
        // One failure must not stop the sweep; `sync_one` already records
        // its own error on the row.
        if sync_one(db, client, id).await.is_ok() {
            done += 1;
        }
    }
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_profile_is_the_account_not_one_of_its_repositories() {
        assert_eq!(
            identify_profile("https://github.com/torvalds"),
            Some(("github", "torvalds".to_string()))
        );
        // Filing this as a profile would claim somebody owns an account
        // called `linux`.
        assert_eq!(identify_profile("https://github.com/torvalds/linux"), None);
    }

    #[test]
    fn the_three_alternatives_are_recognised() {
        assert_eq!(
            identify_profile("https://gitlab.com/someone"),
            Some(("gitlab", "someone".to_string()))
        );
        assert_eq!(
            identify_profile("https://codeberg.org/someone"),
            Some(("codeberg", "someone".to_string()))
        );
        // SourceHut carries the tilde in the path and not in the handle.
        assert_eq!(
            identify_profile("https://sr.ht/~sircmpwn"),
            Some(("sourcehut", "sircmpwn".to_string()))
        );
        assert_eq!(identify_profile("https://sr.ht/sircmpwn"), None);
    }

    #[test]
    fn something_that_is_not_a_forge_is_not_one() {
        assert_eq!(identify_profile("https://example.test/someone"), None);
        assert_eq!(identify_profile("torvalds"), None);
        assert_eq!(identify_profile(""), None);
    }

    #[test]
    fn only_a_proved_account_counts() {
        assert!(is_countable("github", true));
        // Typed, not proved.
        assert!(!is_countable("github", false));
        // Proved by what? Nothing on Codeberg can prove it today, so a row
        // marked verified there must still not count.
        assert!(!is_countable("codeberg", true));
        assert!(!is_countable("sourcehut", true));
    }

    #[test]
    fn every_verifiable_platform_is_a_platform() {
        for platform in VERIFIABLE_PLATFORMS {
            assert!(PLATFORMS.contains(platform));
            assert!(FORGES.contains(platform));
        }
    }

    fn day(date: &str, count: i32) -> (String, i32) {
        (date.to_string(), count)
    }

    #[test]
    fn a_streak_is_consecutive_days_with_something_on_them() {
        let days = vec![
            day("2026-01-01", 3),
            day("2026-01-02", 1),
            day("2026-01-03", 0),
            day("2026-01-04", 2),
            day("2026-01-05", 2),
            day("2026-01-06", 2),
        ];
        assert_eq!(longest_streak(&days), 3);
    }

    #[test]
    fn a_streak_that_runs_to_the_end_still_counts() {
        // An off-by-one here reports somebody's hundred-day streak as
        // ninety-nine, which is noticed by exactly the person it is about.
        let days = vec![
            day("2026-01-01", 0),
            day("2026-01-02", 1),
            day("2026-01-03", 1),
        ];
        assert_eq!(longest_streak(&days), 2);
    }

    #[test]
    fn a_year_of_nothing_is_a_streak_of_nothing() {
        assert_eq!(longest_streak(&[]), 0);
        assert_eq!(longest_streak(&[day("2026-01-01", 0)]), 0);
    }

    #[test]
    fn a_forge_with_no_anonymous_api_says_so_rather_than_inventing_one() {
        assert!(forge_api("github", "x").is_some());
        assert!(forge_api("gitlab", "x").is_some());
        assert!(forge_api("codeberg", "x").is_some());
        assert!(forge_api("sourcehut", "x").is_none());
        assert!(forge_api("crates_io", "x").is_none());
    }
}
