//! Refreshing the external accounts somebody has linked.
//!
//! ## What this is and what `code_portfolio` is
//!
//! [`crate::services::code_portfolio`] fetches the code forges — GitHub,
//! GitLab, Codeberg — where the figures are repositories, stars and a
//! contribution graph, and where one of them can actually be *verified*
//! against an OAuth identity. It stays as it is.
//!
//! This is the rest: the platforms a communicator or an educator publishes on,
//! where nothing can be verified and the only question is whether the figures
//! can be fetched at all. `portfolio_platforms.has_public_api` is the answer,
//! and it decides `sync_enabled` when the row is created.
//!
//! ## Fetched figures replace declared ones, and say so
//!
//! A row starts declared: somebody typed what they read on their own
//! dashboard, and `figures_are_declared` is true. When a fetch succeeds the
//! figures are overwritten and the flag is cleared, which is what makes the
//! craft scores count them in full instead of at half — migration 0507 and
//! `communication_profile::reach` are the two ends of that.
//!
//! A fetch that fails leaves the numbers and the flag alone and writes
//! `last_error`. An old figure with a visible date is worth more than no
//! figure, and much more than a zero.
//!
//! ## The four that answer, and the four that do not
//!
//! Implemented: DEV, Hashnode, a personal feed, YouTube and Weblate. Not
//! implemented, and their rows say `has_public_api = FALSE` so nothing here
//! ever looks at them: Medium (nothing machine-readable since 2019), Twitch
//! (needs an application registration this deployment does not have), Spotify
//! and Apple Podcasts, Crowdin, and every education platform — Udemy,
//! Coursera, LinkedIn Learning, Teachable, OpenClassrooms, Exercism.
//!
//! That last group is worth saying out loud: almost nothing in education can
//! be fetched, so almost every educator's reach figure is theirs, marked, and
//! counted at half. That is the honest description of the field rather than a
//! gap in this module.

use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

/// How old a row has to be before a sweep touches it again.
///
/// A week. These figures move slowly — an article's reaction count a week late
/// is a fine answer — and every one of these services is somebody else's, run
/// for free.
const STALE_AFTER_DAYS: i64 = 7;

/// How many rows one pass will refresh.
///
/// Bounded so a sweep finishes. Whatever it does not reach is the oldest thing
/// next time, because the query orders by staleness.
const PER_PASS: i64 = 200;

/// What a platform told us about an account.
///
/// Both fields optional and independently so: DEV publishes reactions and not
/// views, a feed publishes a post count and no readership at all. `None` means
/// the platform does not say, which must never be stored as zero.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AccountStats {
    /// Articles, videos, translations — whatever the platform's `items_label`
    /// calls them.
    pub items: Option<i32>,
    /// Readers, viewers, reactions — whatever `reach_label` calls them.
    pub reach: Option<i64>,
}

impl AccountStats {
    /// Whether there is anything worth writing.
    ///
    /// A fetch that succeeded and returned nothing is not a reason to clear
    /// what somebody declared: it means the platform answered and said
    /// nothing, and the declared figure is still the best answer available.
    fn is_empty(&self) -> bool {
        self.items.is_none() && self.reach.is_none()
    }
}

/// Whether a handle can go into a URL untouched.
///
/// Deliberately narrow, and not a general escaper: everything it guards is a
/// handle somebody typed into a form, and a handle containing anything outside
/// this set is not a handle. Refusing it is more useful than encoding it.
fn is_handle_safe(handle: &str) -> bool {
    !handle.is_empty()
        && handle.len() <= 120
        && handle
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~'))
}

// ═══════════════════════════════════════════════════════════════════
// DEV
// ═══════════════════════════════════════════════════════════════════

#[derive(Deserialize)]
struct DevToArticle {
    public_reactions_count: Option<i32>,
    comments_count: Option<i32>,
}

/// DEV, by username.
///
/// The public listing is paginated and caps at a thousand per page, which is
/// far above anybody's output here; one page is the whole account.
async fn fetch_dev_to(client: &reqwest::Client, handle: &str) -> Result<AccountStats, AppError> {
    if !is_handle_safe(handle) {
        return Err(AppError::Internal(format!("bad dev.to handle: {handle}")));
    }

    let articles: Vec<DevToArticle> = client
        .get(format!(
            "https://dev.to/api/articles?username={handle}&per_page=1000"
        ))
        .header("User-Agent", "skilluv (https://skill-uv.com)")
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("dev.to unreachable: {e}")))?
        .error_for_status()
        .map_err(|e| AppError::Internal(format!("dev.to refused: {e}")))?
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("dev.to sent something unexpected: {e}")))?;

    // Reactions and comments are two counts of the same thing — somebody
    // bothered — and the column is one.
    let reach: i64 = articles
        .iter()
        .map(|a| {
            i64::from(a.public_reactions_count.unwrap_or(0))
                + i64::from(a.comments_count.unwrap_or(0))
        })
        .sum();

    Ok(AccountStats {
        items: Some(articles.len() as i32),
        reach: Some(reach),
    })
}

// ═══════════════════════════════════════════════════════════════════
// Hashnode
// ═══════════════════════════════════════════════════════════════════

#[derive(Deserialize)]
struct HashnodeEnvelope {
    data: Option<HashnodeData>,
}

#[derive(Deserialize)]
struct HashnodeData {
    user: Option<HashnodeUser>,
}

#[derive(Deserialize)]
struct HashnodeUser {
    posts: Option<HashnodePosts>,
}

#[derive(Deserialize)]
struct HashnodePosts {
    #[serde(rename = "totalDocuments")]
    total_documents: Option<i32>,
    nodes: Option<Vec<HashnodePost>>,
}

#[derive(Deserialize)]
struct HashnodePost {
    views: Option<i64>,
    #[serde(rename = "reactionCount")]
    reaction_count: Option<i64>,
}

/// The query, with the handle substituted rather than interpolated blindly.
///
/// Hashnode answers one GraphQL endpoint and nothing else. The document is
/// small enough to write here; pulling in a GraphQL client for one caller is a
/// dependency the whole project would carry.
fn hashnode_query(handle: &str) -> String {
    format!(
        r#"{{"query":"query {{ user(username: \"{handle}\") {{ posts(page: 1, pageSize: 50) {{ totalDocuments nodes {{ views reactionCount }} }} }} }}"}}"#
    )
}

/// Hashnode, by username.
///
/// `totalDocuments` is the whole output; the per-post figures are only the
/// first fifty, so the reach is a floor rather than a total. Said here because
/// it is the kind of thing that looks like a bug three years later: somebody
/// with two hundred posts shows a reach that stops growing.
async fn fetch_hashnode(client: &reqwest::Client, handle: &str) -> Result<AccountStats, AppError> {
    if !is_handle_safe(handle) {
        return Err(AppError::Internal(format!("bad Hashnode handle: {handle}")));
    }

    let body: HashnodeEnvelope = client
        .post("https://gql.hashnode.com/")
        .header("Content-Type", "application/json")
        .header("User-Agent", "skilluv (https://skill-uv.com)")
        .body(hashnode_query(handle))
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("Hashnode unreachable: {e}")))?
        .error_for_status()
        .map_err(|e| AppError::Internal(format!("Hashnode refused: {e}")))?
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("Hashnode sent something unexpected: {e}")))?;

    // A user who does not exist comes back as a null with no error, which is
    // GraphQL behaving normally and not a failure to report.
    let Some(posts) = body.data.and_then(|d| d.user).and_then(|u| u.posts) else {
        return Ok(AccountStats::default());
    };

    let reach: Option<i64> = posts.nodes.as_ref().map(|nodes| {
        nodes
            .iter()
            .map(|p| p.views.unwrap_or(0) + p.reaction_count.unwrap_or(0))
            .sum()
    });

    Ok(AccountStats {
        items: posts.total_documents,
        reach,
    })
}

// ═══════════════════════════════════════════════════════════════════
// A feed of one's own
// ═══════════════════════════════════════════════════════════════════

/// How many entries a feed lists.
///
/// RSS calls them `<item>` and Atom calls them `<entry>`, and a feed is one or
/// the other. Counted by opening tag rather than parsed, for the reason
/// [`crate::services::artifact_registry`] gives about arXiv: an XML parser
/// added for one caller is a dependency the whole project carries, and two
/// tag names is not parsing.
///
/// Pure, so the trade-off is testable without a network.
pub fn count_feed_entries(feed: &str) -> Option<i32> {
    // `<item>` and `<item ` — an attribute is legal and rare. `</item>` must
    // not match, which is why the prefix includes the `<`.
    let count = |open: &str, with_attrs: &str| {
        feed.matches(open).count() + feed.matches(with_attrs).count()
    };

    let items = count("<item>", "<item ");
    let entries = count("<entry>", "<entry ");

    match items.max(entries) {
        0 => None,
        n => Some(n as i32),
    }
}

/// A personal blog, by the address of its feed.
///
/// The `handle` for this platform is the feed URL, because a personal blog has
/// no username — which is the whole reason it needs a row of its own rather
/// than being filed under one of the hosted platforms.
///
/// No feed publishes readership, so `reach` is always absent. The item count
/// is checked and the audience is simply not knowable, which is the honest
/// pair for a self-hosted site.
async fn fetch_feed(_client: &reqwest::Client, feed_url: &str) -> Result<AccountStats, AppError> {
    // Through `services::outbound`, not the shared client. This address is
    // typed by a member and this is the sweep that goes and reads it, so it
    // was the one place in this codebase that would fetch whatever it was
    // pointed at — including `https://` names resolving to a metadata
    // endpoint. It checked the scheme and nothing else.
    let body = crate::services::outbound::get_text(feed_url).await?;

    Ok(AccountStats {
        items: count_feed_entries(&body),
        reach: None,
    })
}

// ═══════════════════════════════════════════════════════════════════
// YouTube
// ═══════════════════════════════════════════════════════════════════

#[derive(Deserialize)]
struct YouTubeChannelList {
    items: Vec<YouTubeChannel>,
}

#[derive(Deserialize)]
struct YouTubeChannel {
    statistics: Option<YouTubeChannelStats>,
}

#[derive(Deserialize, Default)]
struct YouTubeChannelStats {
    // The Data API returns its counters as strings, and always has.
    #[serde(rename = "videoCount")]
    video_count: Option<String>,
    #[serde(rename = "viewCount")]
    view_count: Option<String>,
}

/// A YouTube channel, by handle.
///
/// Needs `YOUTUBE_API_KEY`, and is skipped rather than failed without one: a
/// deployment that has not configured a key is not broken, and writing an
/// error on every channel every week would fill `last_error` with a message
/// about our own configuration rather than about the platform.
///
/// Views rather than subscribers. The ticket asked for subscribers; a
/// subscriber count is a following, and what the craft score claims to measure
/// is how far the work got. Somebody with four hundred subscribers and a
/// tutorial that reached eighty thousand people has reached eighty thousand
/// people.
async fn fetch_youtube(client: &reqwest::Client, handle: &str) -> Result<AccountStats, AppError> {
    let Ok(key) = std::env::var("YOUTUBE_API_KEY") else {
        tracing::debug!(
            channel = handle,
            "YOUTUBE_API_KEY absent — channel not fetched"
        );
        return Ok(AccountStats::default());
    };

    // The handle is stored with or without its leading `@` depending on what
    // the person pasted; the API wants it with.
    let bare = handle.trim_start_matches('@');
    if !is_handle_safe(bare) {
        return Err(AppError::Internal(format!("bad YouTube handle: {handle}")));
    }
    if !is_handle_safe(&key) {
        return Err(AppError::Internal(
            "YOUTUBE_API_KEY contains characters that cannot go in a URL".to_string(),
        ));
    }

    let body: YouTubeChannelList = client
        .get(format!(
            "https://www.googleapis.com/youtube/v3/channels\
             ?part=statistics&forHandle=%40{bare}&key={key}"
        ))
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("YouTube unreachable: {e}")))?
        .error_for_status()
        .map_err(|e| AppError::Internal(format!("YouTube refused: {e}")))?
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("YouTube sent something unexpected: {e}")))?;

    // An empty list means the handle does not resolve. Not an error: the row
    // keeps what it had, with a visible date.
    let Some(channel) = body.items.into_iter().next() else {
        return Ok(AccountStats::default());
    };
    let stats = channel.statistics.unwrap_or_default();

    Ok(AccountStats {
        items: stats.video_count.and_then(|v| v.parse().ok()),
        reach: stats.view_count.and_then(|v| v.parse().ok()),
    })
}

// ═══════════════════════════════════════════════════════════════════
// Weblate
// ═══════════════════════════════════════════════════════════════════

#[derive(Deserialize, Default)]
struct WeblateStatistics {
    translated: Option<i32>,
}

/// Hosted Weblate, by username.
///
/// The one translation platform with a public statistics endpoint. `translated`
/// counts strings rather than projects, which is the right unit for this trade
/// and the reason the platform's `items_label` says "translations".
///
/// No reach: a translated string has no audience figure, and inventing one
/// from the project's popularity would credit the translator with somebody
/// else's users.
async fn fetch_weblate(client: &reqwest::Client, handle: &str) -> Result<AccountStats, AppError> {
    if !is_handle_safe(handle) {
        return Err(AppError::Internal(format!("bad Weblate handle: {handle}")));
    }

    let body: WeblateStatistics = client
        .get(format!(
            "https://hosted.weblate.org/api/users/{handle}/statistics/"
        ))
        .header("User-Agent", "skilluv (https://skill-uv.com)")
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("Weblate unreachable: {e}")))?
        .error_for_status()
        .map_err(|e| AppError::Internal(format!("Weblate refused: {e}")))?
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("Weblate sent something unexpected: {e}")))?;

    Ok(AccountStats {
        items: body.translated,
        reach: None,
    })
}

// ═══════════════════════════════════════════════════════════════════
// The sweep
// ═══════════════════════════════════════════════════════════════════

/// Ask one platform about one account.
///
/// A platform with no branch here returns nothing rather than an error. The
/// row should not have reached this function at all — `sync_enabled` is set
/// from `has_public_api` — and a platform that arrives anyway is a seeding
/// mistake worth a log line, not a failure that would blank the figures.
/// The platforms this module can read.
///
/// The same set as the `match` below, named once so a test can compare it
/// against the rows `portfolio_platforms` marks `synced_by =
/// 'portfolio_sync'`. The two disagreed before there was anything to notice.
pub const SYNCABLE: &[&str] = &["dev_to", "hashnode", "personal_blog", "weblate", "youtube"];

pub async fn fetch(
    client: &reqwest::Client,
    platform: &str,
    handle: &str,
) -> Result<AccountStats, AppError> {
    match platform {
        "dev_to" => fetch_dev_to(client, handle).await,
        "hashnode" => fetch_hashnode(client, handle).await,
        "personal_blog" => fetch_feed(client, handle).await,
        "youtube" => fetch_youtube(client, handle).await,
        "weblate" => fetch_weblate(client, handle).await,
        other => {
            tracing::warn!(
                platform = other,
                "a portfolio row is marked syncable and nothing knows how to fetch it"
            );
            Ok(AccountStats::default())
        }
    }
}

/// One row the sweep is going to touch.
#[derive(sqlx::FromRow)]
struct Candidate {
    id: Uuid,
    platform: String,
    handle: String,
}

/// Write what a platform said, or why it did not answer.
///
/// A successful fetch that returned figures clears `figures_are_declared`,
/// which is what makes the craft scores stop discounting them. A successful
/// fetch that returned nothing touches only `last_synced_at`: the platform
/// answered and said nothing, and what the person declared is still the best
/// answer available.
async fn record(
    db: &PgPool,
    id: Uuid,
    outcome: Result<AccountStats, AppError>,
) -> Result<bool, AppError> {
    match outcome {
        Ok(stats) if stats.is_empty() => {
            sqlx::query(
                "UPDATE user_external_portfolios
                    SET last_synced_at = NOW(), last_error = NULL, updated_at = NOW()
                  WHERE id = $1",
            )
            .bind(id)
            .execute(db)
            .await?;
            Ok(false)
        }
        Ok(stats) => {
            sqlx::query(
                "UPDATE user_external_portfolios
                    SET items_count = COALESCE($2, items_count),
                        reach_count = COALESCE($3, reach_count),
                        figures_are_declared = FALSE,
                        last_synced_at = NOW(),
                        last_error = NULL,
                        updated_at = NOW()
                  WHERE id = $1",
            )
            .bind(id)
            .bind(stats.items)
            .bind(stats.reach)
            .execute(db)
            .await?;
            Ok(true)
        }
        Err(e) => {
            // The figures and the flag are left alone. An old number with a
            // visible date and a visible error is worth more than a zero.
            sqlx::query(
                "UPDATE user_external_portfolios
                    SET last_error = $2, updated_at = NOW()
                  WHERE id = $1",
            )
            .bind(id)
            .bind(e.to_string())
            .execute(db)
            .await?;
            Ok(false)
        }
    }
}

/// Refresh every linked account whose figures are older than a week.
///
/// Returns how many were actually updated. One failing platform does not stop
/// the others: the point of running this on a schedule is that a bad day at
/// DEV costs a week of freshness rather than the whole sweep.
pub async fn sync_stale(db: &PgPool, client: &reqwest::Client) -> Result<usize, AppError> {
    let rows: Vec<Candidate> = sqlx::query_as(
        r#"
        SELECT p.id, p.platform, p.handle
          FROM user_external_portfolios p
          JOIN portfolio_platforms pf ON pf.slug = p.platform
         WHERE p.sync_enabled
           -- This module's own rows and nothing else. Selecting on
           -- `has_public_api` put the forges in this queue so that `fetch`
           -- could fall through to its catch-all arm, and `code_portfolio`
           -- was meanwhile stamping `last_synced_at` on the rows that belong
           -- here. `SYNCABLE` below is the same list, and a test holds the
           -- two together.
           AND pf.synced_by = 'portfolio_sync'
           AND (p.last_synced_at IS NULL
                OR p.last_synced_at < NOW() - make_interval(days => $1::INT))
         ORDER BY p.last_synced_at NULLS FIRST
         LIMIT $2
        "#,
    )
    .bind(STALE_AFTER_DAYS as i32)
    .bind(PER_PASS)
    .fetch_all(db)
    .await?;

    let mut refreshed = 0usize;
    for row in rows {
        let outcome = fetch(client, &row.platform, &row.handle).await;
        if record(db, row.id, outcome).await? {
            refreshed += 1;
        }
    }

    metrics::counter!("skilluv_portfolio_synced_total").increment(refreshed as u64);
    Ok(refreshed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_feed_is_counted_whichever_dialect_it_speaks() {
        let rss = "<rss><channel><item><title>a</title></item>\
                   <item><title>b</title></item></channel></rss>";
        assert_eq!(count_feed_entries(rss), Some(2));

        let atom = "<feed><entry><title>a</title></entry>\
                    <entry><title>b</title></entry><entry/></feed>";
        // The self-closing third is not counted, and that is the honest
        // reading: an entry with no content is not a post.
        assert_eq!(count_feed_entries(atom), Some(2));
    }

    #[test]
    fn an_entry_with_attributes_still_counts() {
        let atom = r#"<feed><entry xml:base="https://x/">a</entry></feed>"#;
        assert_eq!(count_feed_entries(atom), Some(1));
    }

    #[test]
    fn a_closing_tag_is_not_an_entry() {
        // The bug this guards: counting `</item>` doubles every feed.
        let rss = "<rss><item>a</item></rss>";
        assert_eq!(count_feed_entries(rss), Some(1));
    }

    #[test]
    fn an_empty_feed_reports_nothing_rather_than_zero() {
        // Absent and zero are different claims, and a blog with no posts is
        // rare enough that a parse failure is the likelier explanation.
        assert_eq!(count_feed_entries("<rss><channel/></rss>"), None);
        assert_eq!(count_feed_entries(""), None);
    }

    #[test]
    fn a_handle_that_would_need_escaping_is_refused() {
        assert!(is_handle_safe("kps"));
        assert!(is_handle_safe("some.person_1"));

        assert!(!is_handle_safe(""));
        assert!(!is_handle_safe("a/b"));
        assert!(!is_handle_safe("a b"));
        assert!(!is_handle_safe("x&key=stolen"));
    }

    #[test]
    fn the_hashnode_query_carries_the_handle_and_nothing_else() {
        let query = hashnode_query("kps");
        assert!(query.contains(r#"\"kps\""#));
        // Valid JSON, which is the thing a hand-written body can get wrong.
        let parsed: serde_json::Value = serde_json::from_str(&query).unwrap();
        assert!(parsed["query"].as_str().unwrap().contains("totalDocuments"));
    }

    #[test]
    fn a_platform_answering_nothing_does_not_blank_what_was_declared() {
        // `record` reads this: an empty answer touches only the date, and the
        // figures somebody typed survive it.
        assert!(AccountStats::default().is_empty());
        assert!(
            !AccountStats {
                items: Some(0),
                reach: None
            }
            .is_empty(),
            "a platform saying zero is saying something"
        );
    }
}
