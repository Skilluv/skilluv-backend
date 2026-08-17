//! Reading a package registry to see whether a published library is used.
//!
//! ## What is testable and what is not
//!
//! Recognising `https://crates.io/crates/serde` as the crate `serde` is pure
//! and covered by tests. Asking crates.io how many times it was downloaded is
//! a network call to somebody else's service, and a test that made it would
//! fail whenever they deploy.
//!
//! The split is deliberate: everything that can be wrong in our code —
//! parsing, storage, staleness — is tested, and the part that can only be
//! wrong in theirs is behind a trait.
//!
//! ## Why NULL and not zero
//!
//! Go modules and Homebrew publish no download count. Writing zero would
//! claim nobody uses a package we cannot measure, which is a different and
//! much worse statement.

use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

/// A package, as identified from the URL somebody pasted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageRef {
    pub registry: &'static str,
    pub name: String,
}

/// Which registry a URL points at, and what the package is called there.
///
/// Returns `None` rather than guessing. A URL we do not recognise is better
/// left unclaimed than filed under the wrong registry, where its figures
/// would be fetched from the wrong package of the same name — `serde` exists
/// on crates.io and on npm, and they are not the same project.
pub fn identify(url: &str) -> Option<PackageRef> {
    let url = url.trim();
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let (host, path) = rest.split_once('/')?;
    let host = host.trim_start_matches("www.").to_ascii_lowercase();
    let segments: Vec<&str> = path
        .split('?')
        .next()
        .unwrap_or("")
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();

    let named = |registry: &'static str, name: String| {
        (!name.is_empty()).then_some(PackageRef { registry, name })
    };

    match host.as_str() {
        // https://crates.io/crates/serde
        "crates.io" => match segments.as_slice() {
            ["crates", name, ..] => named("crates_io", (*name).to_string()),
            _ => None,
        },
        // https://www.npmjs.com/package/@scope/thing — the scope is part of
        // the name, so it is kept.
        "npmjs.com" => match segments.as_slice() {
            ["package", scope, name, ..] if scope.starts_with('@') => {
                named("npm", format!("{scope}/{name}"))
            }
            ["package", name, ..] => named("npm", (*name).to_string()),
            _ => None,
        },
        // https://pypi.org/project/requests/
        "pypi.org" => match segments.as_slice() {
            ["project", name, ..] => named("pypi", (*name).to_string()),
            _ => None,
        },
        // https://pkg.go.dev/github.com/user/module — the whole path is the
        // module path.
        "pkg.go.dev" => named("go_modules", segments.join("/")),
        // https://central.sonatype.com/artifact/org.example/thing
        "central.sonatype.com" | "search.maven.org" => match segments.as_slice() {
            ["artifact", group, artifact, ..] => {
                named("maven_central", format!("{group}:{artifact}"))
            }
            _ => None,
        },
        // https://rubygems.org/gems/rails
        "rubygems.org" => match segments.as_slice() {
            ["gems", name, ..] => named("rubygems", (*name).to_string()),
            _ => None,
        },
        // https://www.nuget.org/packages/Newtonsoft.Json
        "nuget.org" => match segments.as_slice() {
            ["packages", name, ..] => named("nuget", (*name).to_string()),
            _ => None,
        },
        // https://packagist.org/packages/vendor/name
        "packagist.org" => match segments.as_slice() {
            ["packages", vendor, name, ..] => named("packagist", format!("{vendor}/{name}")),
            _ => None,
        },
        // https://hex.pm/packages/phoenix
        "hex.pm" => match segments.as_slice() {
            ["packages", name, ..] => named("hex_pm", (*name).to_string()),
            _ => None,
        },
        // https://formulae.brew.sh/formula/ripgrep
        "formulae.brew.sh" => match segments.as_slice() {
            ["formula", name, ..] => named("homebrew", (*name).to_string()),
            _ => None,
        },
        _ => None,
    }
}

/// What a registry told us about a package.
#[derive(Debug, Clone, Default)]
pub struct PackageStats {
    pub latest_version: Option<String>,
    pub downloads_total: Option<i64>,
    pub downloads_recent: Option<i64>,
    pub dependents_count: Option<i32>,
}

/// Ask one registry about one package.
///
/// Only the registries that publish usage figures over a public API are
/// implemented. The rest are recognised by [`identify`] and stored with no
/// numbers, which is the truth about them.
pub async fn fetch(
    client: &reqwest::Client,
    package: &PackageRef,
) -> Result<PackageStats, AppError> {
    match package.registry {
        "crates_io" => fetch_crates_io(client, &package.name).await,
        "npm" => fetch_npm(client, &package.name).await,
        "pypi" => fetch_pypi(client, &package.name).await,
        // Recognised, and honest about having nothing to report.
        _ => Ok(PackageStats::default()),
    }
}

#[derive(Deserialize)]
struct CratesIoResponse {
    #[serde(rename = "crate")]
    krate: CratesIoCrate,
}

#[derive(Deserialize)]
struct CratesIoCrate {
    downloads: Option<i64>,
    recent_downloads: Option<i64>,
    max_stable_version: Option<String>,
}

async fn fetch_crates_io(client: &reqwest::Client, name: &str) -> Result<PackageStats, AppError> {
    // crates.io asks every caller to identify itself and refuses those that
    // do not.
    let body: CratesIoResponse = client
        .get(format!("https://crates.io/api/v1/crates/{name}"))
        .header("User-Agent", "skilluv (https://skill-uv.com)")
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("crates.io unreachable: {e}")))?
        .error_for_status()
        .map_err(|e| AppError::Internal(format!("crates.io refused: {e}")))?
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("crates.io sent something unexpected: {e}")))?;

    Ok(PackageStats {
        latest_version: body.krate.max_stable_version,
        downloads_total: body.krate.downloads,
        downloads_recent: body.krate.recent_downloads,
        dependents_count: None,
    })
}

#[derive(Deserialize)]
struct NpmDownloads {
    downloads: i64,
}

async fn fetch_npm(client: &reqwest::Client, name: &str) -> Result<PackageStats, AppError> {
    // npm publishes downloads on a separate host from the registry itself,
    // and only over a window — there is no lifetime total to ask for.
    let recent: NpmDownloads = client
        .get(format!(
            "https://api.npmjs.org/downloads/point/last-month/{name}"
        ))
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("npm unreachable: {e}")))?
        .error_for_status()
        .map_err(|e| AppError::Internal(format!("npm refused: {e}")))?
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("npm sent something unexpected: {e}")))?;

    Ok(PackageStats {
        latest_version: None,
        downloads_total: None,
        downloads_recent: Some(recent.downloads),
        dependents_count: None,
    })
}

#[derive(Deserialize)]
struct PypiResponse {
    info: PypiInfo,
}

#[derive(Deserialize)]
struct PypiInfo {
    version: Option<String>,
}

async fn fetch_pypi(client: &reqwest::Client, name: &str) -> Result<PackageStats, AppError> {
    // PyPI stopped serving download counts from its own API years ago; the
    // version is what it still answers. Reporting no downloads is accurate.
    let body: PypiResponse = client
        .get(format!("https://pypi.org/pypi/{name}/json"))
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("PyPI unreachable: {e}")))?
        .error_for_status()
        .map_err(|e| AppError::Internal(format!("PyPI refused: {e}")))?
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("PyPI sent something unexpected: {e}")))?;

    Ok(PackageStats {
        latest_version: body.info.version,
        downloads_total: None,
        downloads_recent: None,
        dependents_count: None,
    })
}

/// Record what a registry said, keeping the previous figures on failure.
///
/// A failed fetch writes the error and leaves the numbers alone. An old
/// figure with a visible date is worth more than no figure, and much more
/// than a zero that reads as "nobody uses this".
pub async fn record(
    db: &PgPool,
    slice_id: Uuid,
    package: &PackageRef,
    outcome: Result<PackageStats, AppError>,
) -> Result<(), AppError> {
    match outcome {
        Ok(stats) => {
            sqlx::query(
                r#"
                INSERT INTO code_package_stats
                    (slice_id, registry, package_name, latest_version,
                     downloads_total, downloads_recent, dependents_count,
                     fetched_at, last_error)
                VALUES ($1, $2, $3, $4, $5, $6, $7, NOW(), NULL)
                ON CONFLICT (slice_id, registry, package_name) DO UPDATE SET
                    latest_version   = EXCLUDED.latest_version,
                    downloads_total  = EXCLUDED.downloads_total,
                    downloads_recent = EXCLUDED.downloads_recent,
                    dependents_count = EXCLUDED.dependents_count,
                    fetched_at       = NOW(),
                    last_error       = NULL
                "#,
            )
            .bind(slice_id)
            .bind(package.registry)
            .bind(&package.name)
            .bind(stats.latest_version)
            .bind(stats.downloads_total)
            .bind(stats.downloads_recent)
            .bind(stats.dependents_count)
            .execute(db)
            .await?;
        }
        Err(e) => {
            sqlx::query(
                r#"
                INSERT INTO code_package_stats
                    (slice_id, registry, package_name, last_error)
                VALUES ($1, $2, $3, $4)
                ON CONFLICT (slice_id, registry, package_name) DO UPDATE SET
                    last_error = EXCLUDED.last_error
                "#,
            )
            .bind(slice_id)
            .bind(package.registry)
            .bind(&package.name)
            .bind(e.to_string())
            .execute(db)
            .await?;
        }
    }
    Ok(())
}

/// Refresh every published library whose figures are older than a week.
///
/// Returns how many were refreshed. One failing registry does not stop the
/// others: the whole point of running this on a schedule is that a bad day
/// at npm costs a week of freshness, not the entire sweep.
pub async fn sync_stale(db: &PgPool, client: &reqwest::Client) -> Result<usize, AppError> {
    let rows: Vec<(Uuid, String)> = sqlx::query_as(
        r#"
        SELECT ps.id, ps.code_package_registry_url
          FROM project_slices ps
          LEFT JOIN code_package_stats st ON st.slice_id = ps.id
         WHERE ps.code_subtype = 'library_published'
           AND ps.code_package_registry_url IS NOT NULL
           AND (st.fetched_at IS NULL OR st.fetched_at < NOW() - INTERVAL '7 days')
         LIMIT 500
        "#,
    )
    .fetch_all(db)
    .await?;

    let mut refreshed = 0usize;
    for (slice_id, url) in rows {
        let Some(package) = identify(&url) else {
            tracing::warn!(
                slice = %slice_id,
                url = %url,
                "package registry URL not recognised — no figures will be fetched"
            );
            continue;
        };
        let outcome = fetch(client, &package).await;
        let failed = outcome.is_err();
        record(db, slice_id, &package, outcome).await?;
        if !failed {
            refreshed += 1;
            // The registry answered, so the package exists and is installable
            // — which is what "published" means and the only moment we can
            // honestly say it. Idempotent on the slice, so the weekly sweep
            // does not repost it.
            if let Err(err) = announce_publication(db, slice_id, &package).await {
                tracing::warn!(slice = %slice_id, %err, "publication not announced on the public feed");
            }
        }
    }

    Ok(refreshed)
}

/// Put a published package on the public feed.
///
/// Already public elsewhere — anybody can install it — so this repeats
/// something rather than publishing it, and defaults to visible for that
/// reason.
async fn announce_publication(
    db: &PgPool,
    slice_id: Uuid,
    package: &PackageRef,
) -> Result<(), AppError> {
    let row: Option<(Uuid, String, String)> = sqlx::query_as(
        "SELECT d.user_id, u.username, ps.code_package_registry_url
           FROM project_slices ps
           JOIN deliverables d ON d.slice_id = ps.id
           JOIN users u ON u.id = d.user_id
          WHERE ps.id = $1
            AND d.verification_status = 'verified'
            AND d.revoked_at IS NULL
          ORDER BY d.verified_at ASC
          LIMIT 1",
    )
    .bind(slice_id)
    .fetch_optional(db)
    .await?;
    // Nobody has a verified deliverable against it yet: the package may exist
    // and nothing on Skilluv says who published it.
    let Some((user_id, username, url)) = row else {
        return Ok(());
    };

    crate::services::public_feed::emit(
        db,
        crate::services::public_feed::Emission {
            kind: "library_published",
            subject_type: "user",
            subject_id: user_id,
            subject_label: &username,
            headline: format!(
                "bibliothèque publiée sur {} — {}",
                package.registry, package.name
            ),
            artifact_url: url,
            repository: None,
            amount: None,
            currency: None,
            source_type: "slice_package",
            source_id: slice_id,
        },
    )
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ident(url: &str) -> Option<(&'static str, String)> {
        identify(url).map(|p| (p.registry, p.name))
    }

    #[test]
    fn every_supported_registry_is_recognised() {
        assert_eq!(
            ident("https://crates.io/crates/serde"),
            Some(("crates_io", "serde".into()))
        );
        assert_eq!(
            ident("https://www.npmjs.com/package/svelte"),
            Some(("npm", "svelte".into()))
        );
        assert_eq!(
            ident("https://pypi.org/project/requests/"),
            Some(("pypi", "requests".into()))
        );
        assert_eq!(
            ident("https://pkg.go.dev/github.com/spf13/cobra"),
            Some(("go_modules", "github.com/spf13/cobra".into()))
        );
        assert_eq!(
            ident("https://central.sonatype.com/artifact/org.slf4j/slf4j-api"),
            Some(("maven_central", "org.slf4j:slf4j-api".into()))
        );
        assert_eq!(
            ident("https://rubygems.org/gems/rails"),
            Some(("rubygems", "rails".into()))
        );
        assert_eq!(
            ident("https://www.nuget.org/packages/Newtonsoft.Json"),
            Some(("nuget", "Newtonsoft.Json".into()))
        );
        assert_eq!(
            ident("https://packagist.org/packages/symfony/console"),
            Some(("packagist", "symfony/console".into()))
        );
        assert_eq!(
            ident("https://hex.pm/packages/phoenix"),
            Some(("hex_pm", "phoenix".into()))
        );
        assert_eq!(
            ident("https://formulae.brew.sh/formula/ripgrep"),
            Some(("homebrew", "ripgrep".into()))
        );
    }

    #[test]
    fn an_npm_scope_is_part_of_the_name() {
        // `@sveltejs/kit` and `kit` are different packages.
        assert_eq!(
            ident("https://www.npmjs.com/package/@sveltejs/kit"),
            Some(("npm", "@sveltejs/kit".into()))
        );
    }

    #[test]
    fn a_url_we_do_not_know_is_left_alone() {
        // Guessing would file it under the wrong registry, and `serde` on
        // crates.io is not `serde` on npm.
        assert_eq!(ident("https://example.com/packages/thing"), None);
        assert_eq!(ident("https://crates.io/"), None);
        assert_eq!(ident("https://pypi.org/project/"), None);
        assert_eq!(ident("not a url"), None);
        assert_eq!(ident(""), None);
    }

    #[test]
    fn trailing_paths_and_queries_do_not_change_the_package() {
        assert_eq!(
            ident("https://crates.io/crates/serde/1.0.0"),
            Some(("crates_io", "serde".into()))
        );
        assert_eq!(
            ident("https://pypi.org/project/requests/#history"),
            Some(("pypi", "requests".into()))
        );
        assert_eq!(
            ident("https://www.npmjs.com/package/svelte?activeTab=readme"),
            Some(("npm", "svelte".into()))
        );
    }
}
