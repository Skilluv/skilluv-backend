//! Reading a public registry or hub to see whether published work is used.
//!
//! Covers package registries — crates.io, npm, PyPI — the model and dataset
//! hubs, HuggingFace and Kaggle, and the infrastructure registries a
//! published ops artefact lives on: Terraform, Ansible Galaxy, ArtifactHub,
//! Docker Hub. The question is the same in all three cases and so is the
//! answer: which registry, what it is called there, how many downloads, and
//! when we last asked.
//!
//! ## What each one is willing to say
//!
//! Not all of them answer "how many". The Terraform registry and Docker Hub
//! publish a lifetime count; ArtifactHub publishes stars and no count at all;
//! Galaxy publishes a count whose field name has moved twice. So a figure is
//! stored where it belongs — stars in `likes_count`, never in a downloads
//! column — and absent where it is genuinely absent. Filling a downloads
//! column with an approval count would claim use the hub never measured.
//!
//! ## What is testable and what is not
//!
//! Recognising `https://huggingface.co/mistralai/Mistral-7B-v0.1` as the
//! model `mistralai/Mistral-7B-v0.1` is pure and covered by tests. Asking
//! HuggingFace how many times it was downloaded is a network call to somebody
//! else's service, and a test that made it would fail whenever they deploy.
//!
//! The split is deliberate: everything that can be wrong in our code —
//! parsing, storage, staleness — is tested, and the part that can only be
//! wrong in theirs is not.
//!
//! ## Why NULL and not zero
//!
//! Go modules and Homebrew publish no download count, and Kaggle asks for
//! credentials before it publishes anything. Writing zero would claim nobody
//! uses work we cannot measure, which is a different and much worse
//! statement.

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
        // HuggingFace puts models at the root and datasets one level down:
        //   https://huggingface.co/mistralai/Mistral-7B-v0.1
        //   https://huggingface.co/datasets/masakhane/masakhaner
        // The owner is part of the name — two people can publish `bert-base`
        // and they are not the same weights.
        "huggingface.co" | "hf.co" => match segments.as_slice() {
            ["datasets", owner, name, ..] => {
                named("huggingface_datasets", format!("{owner}/{name}"))
            }
            // `spaces` and `docs` are neither a model nor a dataset. Left
            // unrecognised rather than filed as one.
            [kind, ..] if matches!(*kind, "datasets" | "spaces" | "docs" | "blog") => None,
            [owner, name, ..] => named("huggingface_models", format!("{owner}/{name}")),
            _ => None,
        },
        // https://www.kaggle.com/datasets/owner/name
        "kaggle.com" => match segments.as_slice() {
            ["datasets", owner, name, ..] => named("kaggle_datasets", format!("{owner}/{name}")),
            _ => None,
        },
        // The Terraform registry holds two different things at two paths, and
        // they are not interchangeable: a module is code somebody calls, a
        // provider is a plugin somebody configures. Both are kept, prefixed,
        // because the API path differs and the name alone would not say which.
        //   https://registry.terraform.io/modules/terraform-aws-modules/vpc/aws
        //   https://registry.terraform.io/providers/hashicorp/aws
        "registry.terraform.io" | "registry.opentofu.org" => match segments.as_slice() {
            ["modules", namespace, name, provider, ..] => named(
                "terraform_registry",
                format!("modules/{namespace}/{name}/{provider}"),
            ),
            ["providers", namespace, name, ..] => named(
                "terraform_registry",
                format!("providers/{namespace}/{name}"),
            ),
            _ => None,
        },
        // Galaxy moved its collections behind a `/ui/repo/published/` prefix
        // and kept the old two-segment form working. Both arrive from real
        // people, so both are read.
        //   https://galaxy.ansible.com/ui/repo/published/community/general/
        //   https://galaxy.ansible.com/community/general
        "galaxy.ansible.com" => match segments.as_slice() {
            ["ui", "repo", _repo, namespace, name, ..] => {
                named("ansible_galaxy", format!("{namespace}/{name}"))
            }
            [namespace, name, ..] if *namespace != "ui" => {
                named("ansible_galaxy", format!("{namespace}/{name}"))
            }
            _ => None,
        },
        // https://artifacthub.io/packages/helm/bitnami/postgresql
        // The kind is part of the identity: a Helm chart and an OLM operator
        // called `postgresql` are different things from different publishers.
        "artifacthub.io" => match segments.as_slice() {
            ["packages", kind, repo, name, ..] => {
                named("artifacthub", format!("{kind}/{repo}/{name}"))
            }
            _ => None,
        },
        // https://hub.docker.com/r/grafana/grafana  (a user or organisation)
        // https://hub.docker.com/_/postgres         (an official image)
        // Official images live under the `library` namespace in the API, and
        // storing them that way means one fetch path instead of two.
        "hub.docker.com" => match segments.as_slice() {
            ["r", owner, name, ..] => named("docker_hub", format!("{owner}/{name}")),
            ["_", name, ..] => named("docker_hub", format!("library/{name}")),
            _ => None,
        },
        _ => None,
    }
}

/// What a registry told us about a published artefact.
#[derive(Debug, Clone, Default)]
pub struct PackageStats {
    pub latest_version: Option<String>,
    pub downloads_total: Option<i64>,
    pub downloads_recent: Option<i64>,
    pub dependents_count: Option<i32>,
    /// HuggingFace likes, Kaggle votes. Approval rather than use.
    pub likes_count: Option<i32>,
}

/// Ask one registry about one artefact.
///
/// Only the registries that publish usage figures over a public API are
/// implemented. The rest are recognised by [`identify`] and stored with no
/// numbers, which is the truth about them — Kaggle among them, because its
/// API asks for credentials before it answers anything.
pub async fn fetch(
    client: &reqwest::Client,
    package: &PackageRef,
) -> Result<PackageStats, AppError> {
    match package.registry {
        "crates_io" => fetch_crates_io(client, &package.name).await,
        "npm" => fetch_npm(client, &package.name).await,
        "pypi" => fetch_pypi(client, &package.name).await,
        "huggingface_models" => fetch_huggingface(client, "models", &package.name).await,
        "huggingface_datasets" => fetch_huggingface(client, "datasets", &package.name).await,
        "terraform_registry" => fetch_terraform(client, &package.name).await,
        "ansible_galaxy" => fetch_ansible_galaxy(client, &package.name).await,
        "artifacthub" => fetch_artifacthub(client, &package.name).await,
        "docker_hub" => fetch_docker_hub(client, &package.name).await,
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
        likes_count: None,
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
        likes_count: None,
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
        likes_count: None,
    })
}

#[derive(Deserialize)]
struct HuggingFaceRepo {
    /// Downloads over the last thirty days. The hub publishes no lifetime
    /// total, so `downloads_total` stays NULL rather than being filled with
    /// a window that is not one.
    downloads: Option<i64>,
    likes: Option<i32>,
    sha: Option<String>,
}

/// Ask HuggingFace about one model or dataset.
///
/// `kind` is `models` or `datasets`: the hub keeps them in separate
/// namespaces, and the same owner/name can exist in both as different things.
async fn fetch_huggingface(
    client: &reqwest::Client,
    kind: &str,
    name: &str,
) -> Result<PackageStats, AppError> {
    let body: HuggingFaceRepo = client
        .get(format!("https://huggingface.co/api/{kind}/{name}"))
        .header("User-Agent", "skilluv (https://skill-uv.com)")
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("HuggingFace unreachable: {e}")))?
        .error_for_status()
        .map_err(|e| AppError::Internal(format!("HuggingFace refused: {e}")))?
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("HuggingFace sent something unexpected: {e}")))?;

    Ok(PackageStats {
        // The commit the repository is on. It is what "version" means on a
        // hub with no releases, and it is what lets a reviewer check that the
        // weights judged are the weights published.
        latest_version: body.sha.map(|s| s.chars().take(12).collect()),
        downloads_total: None,
        downloads_recent: body.downloads,
        dependents_count: None,
        likes_count: body.likes,
    })
}

#[derive(Deserialize)]
struct TerraformArtifact {
    /// Lifetime downloads. The registry publishes this for both modules and
    /// providers, which makes it the only infrastructure registry of the four
    /// that answers the question directly.
    downloads: Option<i64>,
    version: Option<String>,
}

/// Ask the Terraform registry about a module or a provider.
///
/// The name carries its own path — `modules/ns/name/provider` or
/// `providers/ns/name` — because the two live at different endpoints and a
/// bare name would not say which to call.
async fn fetch_terraform(client: &reqwest::Client, name: &str) -> Result<PackageStats, AppError> {
    let body: TerraformArtifact = client
        .get(format!("https://registry.terraform.io/v1/{name}"))
        .header("User-Agent", "skilluv (https://skill-uv.com)")
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("Terraform registry unreachable: {e}")))?
        .error_for_status()
        .map_err(|e| AppError::Internal(format!("Terraform registry refused: {e}")))?
        .json()
        .await
        .map_err(|e| {
            AppError::Internal(format!("Terraform registry sent something unexpected: {e}"))
        })?;

    Ok(PackageStats {
        latest_version: body.version,
        downloads_total: body.downloads,
        downloads_recent: None,
        dependents_count: None,
        likes_count: None,
    })
}

#[derive(Deserialize)]
struct GalaxyCollection {
    download_count: Option<i64>,
    highest_version: Option<GalaxyVersion>,
}

#[derive(Deserialize)]
struct GalaxyVersion {
    version: Option<String>,
}

/// Ask Ansible Galaxy about a collection.
///
/// Every field is optional on purpose. Galaxy has changed its API shape twice
/// and will again; a rename upstream should cost us a NULL, which reads as
/// "not measured", rather than a hard error that marks the artefact broken
/// when the only thing broken is our guess about a field name.
async fn fetch_ansible_galaxy(
    client: &reqwest::Client,
    name: &str,
) -> Result<PackageStats, AppError> {
    let (namespace, collection) = name
        .split_once('/')
        .ok_or_else(|| AppError::Internal(format!("'{name}' is not namespace/collection")))?;

    let body: GalaxyCollection = client
        .get(format!(
            "https://galaxy.ansible.com/api/v3/plugin/ansible/content/published/collections/index/{namespace}/{collection}/"
        ))
        .header("User-Agent", "skilluv (https://skill-uv.com)")
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("Ansible Galaxy unreachable: {e}")))?
        .error_for_status()
        .map_err(|e| AppError::Internal(format!("Ansible Galaxy refused: {e}")))?
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("Ansible Galaxy sent something unexpected: {e}")))?;

    Ok(PackageStats {
        latest_version: body.highest_version.and_then(|v| v.version),
        downloads_total: body.download_count,
        downloads_recent: None,
        dependents_count: None,
        likes_count: None,
    })
}

#[derive(Deserialize)]
struct ArtifactHubPackage {
    version: Option<String>,
    stars: Option<i32>,
}

/// Ask ArtifactHub about a chart, an operator or a policy.
///
/// It publishes stars and no download count, so the figure lands in
/// `likes_count`. Putting stars in a downloads column would claim use where
/// the hub only knows about approval, which is the distinction migration 0216
/// added the column for.
async fn fetch_artifacthub(client: &reqwest::Client, name: &str) -> Result<PackageStats, AppError> {
    let body: ArtifactHubPackage = client
        .get(format!("https://artifacthub.io/api/v1/packages/{name}"))
        .header("User-Agent", "skilluv (https://skill-uv.com)")
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("ArtifactHub unreachable: {e}")))?
        .error_for_status()
        .map_err(|e| AppError::Internal(format!("ArtifactHub refused: {e}")))?
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("ArtifactHub sent something unexpected: {e}")))?;

    Ok(PackageStats {
        latest_version: body.version,
        downloads_total: None,
        downloads_recent: None,
        dependents_count: None,
        likes_count: body.stars,
    })
}

#[derive(Deserialize)]
struct DockerHubRepository {
    pull_count: Option<i64>,
    star_count: Option<i32>,
}

/// Ask Docker Hub about an image.
///
/// `pull_count` is lifetime pulls, and it is worth reading with the same
/// caution as an npm figure: automated builds pull too, so a large number
/// says the image is wired into pipelines rather than that people chose it.
/// It is still the only usage figure the hub publishes.
async fn fetch_docker_hub(client: &reqwest::Client, name: &str) -> Result<PackageStats, AppError> {
    let body: DockerHubRepository = client
        .get(format!("https://hub.docker.com/v2/repositories/{name}/"))
        .header("User-Agent", "skilluv (https://skill-uv.com)")
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("Docker Hub unreachable: {e}")))?
        .error_for_status()
        .map_err(|e| AppError::Internal(format!("Docker Hub refused: {e}")))?
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("Docker Hub sent something unexpected: {e}")))?;

    Ok(PackageStats {
        // The hub versions by tag, and an image has many at once. Naming one
        // would be picking a favourite.
        latest_version: None,
        downloads_total: body.pull_count,
        downloads_recent: None,
        dependents_count: None,
        likes_count: body.star_count,
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
                INSERT INTO published_artifact_stats
                    (slice_id, registry, package_name, latest_version,
                     downloads_total, downloads_recent, dependents_count,
                     likes_count, fetched_at, last_error)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NOW(), NULL)
                ON CONFLICT (slice_id, registry, package_name) DO UPDATE SET
                    latest_version   = EXCLUDED.latest_version,
                    downloads_total  = EXCLUDED.downloads_total,
                    downloads_recent = EXCLUDED.downloads_recent,
                    dependents_count = EXCLUDED.dependents_count,
                    likes_count      = EXCLUDED.likes_count,
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
            .bind(stats.likes_count)
            .execute(db)
            .await?;
        }
        Err(e) => {
            sqlx::query(
                r#"
                INSERT INTO published_artifact_stats
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

/// Refresh every published artefact whose figures are older than a week.
///
/// Returns how many were refreshed. One failing registry does not stop the
/// others: the whole point of running this on a schedule is that a bad day
/// at npm costs a week of freshness, not the entire sweep.
///
/// Two kinds of slice qualify — a published library, which names a package
/// registry, and a published model or dataset, which names a hub. One sweep
/// for both, because the staleness rule is the same and having two would mean
/// one of them silently stops running.
pub async fn sync_stale(db: &PgPool, client: &reqwest::Client) -> Result<usize, AppError> {
    let rows: Vec<(Uuid, String)> = sqlx::query_as(
        r#"
        SELECT ps.id, ps.published_artifact_url AS url
          FROM project_slices ps
          LEFT JOIN published_artifact_stats st ON st.slice_id = ps.id
         WHERE ps.published_artifact_url IS NOT NULL
           -- Any slice that named a published artefact, whatever domain it
           -- came from. The subtype list this used to carry had to be edited
           -- every time a domain arrived, and the edit was forgotten once:
           -- ops artefacts were storable and never fetched.
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
        "SELECT d.user_id, u.username, ps.published_artifact_url
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
    fn the_model_hubs_are_recognised() {
        assert_eq!(
            ident("https://huggingface.co/mistralai/Mistral-7B-v0.1"),
            Some(("huggingface_models", "mistralai/Mistral-7B-v0.1".into()))
        );
        assert_eq!(
            ident("https://huggingface.co/datasets/masakhane/masakhaner"),
            Some(("huggingface_datasets", "masakhane/masakhaner".into()))
        );
        assert_eq!(
            ident("https://www.kaggle.com/datasets/uciml/iris"),
            Some(("kaggle_datasets", "uciml/iris".into()))
        );
        // The short host redirects to the long one and people paste both.
        assert_eq!(
            ident("https://hf.co/google/gemma-2b"),
            Some(("huggingface_models", "google/gemma-2b".into()))
        );
    }

    #[test]
    fn the_owner_is_part_of_a_model_name() {
        // Two people can publish `bert-base`, and they are not the same
        // weights. Dropping the owner would fetch figures for whichever one
        // the hub resolved first.
        assert_ne!(
            ident("https://huggingface.co/google/bert-base"),
            ident("https://huggingface.co/someone-else/bert-base")
        );
    }

    #[test]
    fn a_huggingface_page_that_is_not_an_artefact_is_left_alone() {
        // Spaces are demos, not weights. Filing one as a model would count
        // its likes towards a claim about a model that does not exist.
        assert_eq!(ident("https://huggingface.co/spaces/owner/demo"), None);
        assert_eq!(ident("https://huggingface.co/docs/transformers"), None);
        assert_eq!(ident("https://huggingface.co/datasets"), None);
        // An owner with no repository names nobody's artefact.
        assert_eq!(ident("https://huggingface.co/mistralai"), None);
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

    // ═══════════════════════════════════════════════════════════════
    // The infrastructure registries
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn a_terraform_module_and_a_provider_are_told_apart() {
        // The path is kept in the name because the API endpoint differs, and
        // `hashicorp/aws` alone would not say which of the two to call.
        assert_eq!(
            ident("https://registry.terraform.io/modules/terraform-aws-modules/vpc/aws"),
            Some((
                "terraform_registry",
                "modules/terraform-aws-modules/vpc/aws".into()
            ))
        );
        assert_eq!(
            ident("https://registry.terraform.io/providers/hashicorp/aws"),
            Some(("terraform_registry", "providers/hashicorp/aws".into()))
        );
        // OpenTofu's registry mirrors the same layout, and somebody who left
        // Terraform over the licence should not lose their figures for it.
        assert_eq!(
            ident("https://registry.opentofu.org/providers/hashicorp/aws"),
            Some(("terraform_registry", "providers/hashicorp/aws".into()))
        );
        // A module URL missing its provider segment is not a module.
        assert_eq!(
            ident("https://registry.terraform.io/modules/terraform-aws-modules"),
            None
        );
    }

    #[test]
    fn both_shapes_of_galaxy_url_reach_the_same_collection() {
        assert_eq!(
            ident("https://galaxy.ansible.com/ui/repo/published/community/general/"),
            Some(("ansible_galaxy", "community/general".into()))
        );
        assert_eq!(
            ident("https://galaxy.ansible.com/community/general"),
            Some(("ansible_galaxy", "community/general".into()))
        );
    }

    #[test]
    fn an_artifacthub_package_carries_its_kind() {
        // A Helm chart and an OLM operator can share a name and come from
        // different publishers; the kind is part of the identity.
        assert_eq!(
            ident("https://artifacthub.io/packages/helm/bitnami/postgresql"),
            Some(("artifacthub", "helm/bitnami/postgresql".into()))
        );
        assert_eq!(
            ident("https://artifacthub.io/packages/olm/community-operators/postgresql"),
            Some(("artifacthub", "olm/community-operators/postgresql".into()))
        );
    }

    #[test]
    fn an_official_docker_image_is_stored_under_library() {
        // That is where the API keeps it, so storing it that way means one
        // fetch path rather than two.
        assert_eq!(
            ident("https://hub.docker.com/_/postgres"),
            Some(("docker_hub", "library/postgres".into()))
        );
        assert_eq!(
            ident("https://hub.docker.com/r/grafana/grafana"),
            Some(("docker_hub", "grafana/grafana".into()))
        );
        // The search page is not an image.
        assert_eq!(ident("https://hub.docker.com/search?q=postgres"), None);
    }
}
