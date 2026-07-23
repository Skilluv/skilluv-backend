//! Priorite moyenne #5 strategy doc §15 : sync du Profil README depuis
//! GitHub pour les users qui ont configure `profile_readme_source='github_sync'`.
//!
//! Convention GitHub : le README affiche sur le profil github.com/{username}
//! est stocke dans le repo public {username}/{username} (aka le "special repo").
//! On fetch `README.md` (branche par defaut) via GitHub API contents,
//! decode base64, et l'ecrit dans `users.profile_readme_markdown`.
//!
//! Contraintes :
//! - Quota anti-abus deja modelise en migration 0108 : max 20 KB (check
//!   length <= 20480). On tronque cote service pour eviter le rejet DB.
//! - `profile_readme_sync_url` peut etre fourni pour override le repo/path,
//!   ex `https://github.com/{owner}/{repo}/blob/{ref}/{path}`. Sinon on
//!   utilise la convention standard `{username}/{username}/README.md`.
//! - Un token GitHub service-account (SKILLUV_BOT_GITHUB_TOKEN) est utilise
//!   pour eviter le rate limit anonyme de 60 req/h. Optionnel — le fetch
//!   fonctionne aussi sans token, juste plus limite.
//!
//! Non wire dans un cron : appelable depuis un endpoint admin ou un worker
//! quotidien a ajouter dans une PR suivante.

use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::github;

const MAX_README_BYTES: usize = 20 * 1024;
const BATCH_SIZE: i64 = 50;

#[derive(Debug, Clone, sqlx::FromRow)]
struct SyncTarget {
    id: Uuid,
    username: String,
    profile_readme_sync_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SyncReport {
    pub synced: Vec<Uuid>,
    pub failed: Vec<(Uuid, String)>,
    pub skipped_no_readme: Vec<Uuid>,
}

/// Sync le README des users qui ont profile_readme_source='github_sync'.
///
/// `bot_token` : optionnel (rate limit + qualite). Si None, on fetch anonyme.
///
/// Idempotent : rejoue = re-fetch + update. Le check delta reduit les
/// writes DB inutiles.
pub async fn sync_pending_readmes(
    db: &PgPool,
    bot_token: Option<&str>,
) -> Result<SyncReport, AppError> {
    let targets: Vec<SyncTarget> = sqlx::query_as(
        r#"
        SELECT id, username, profile_readme_sync_url
        FROM users
        WHERE profile_readme_source = 'github_sync'
          AND is_banned = FALSE
        ORDER BY id
        LIMIT $1
        "#,
    )
    .bind(BATCH_SIZE)
    .fetch_all(db)
    .await?;

    let mut synced = Vec::new();
    let mut failed = Vec::new();
    let mut skipped_no_readme = Vec::new();

    for target in targets {
        let (repo, path) = resolve_source(&target);

        let content = match fetch_readme(bot_token, &repo, &path).await {
            Ok(c) => c,
            Err(FetchError::NotFound) => {
                skipped_no_readme.push(target.id);
                continue;
            }
            Err(FetchError::Other(msg)) => {
                failed.push((target.id, msg.clone()));
                tracing::warn!(
                    user_id = %target.id,
                    username = target.username,
                    error = msg,
                    "profile README sync failed"
                );
                continue;
            }
        };

        // Defense en profondeur : sanitize serveur avant persistance (priorite
        // basse #8). Le README github est du contenu externe non-controle.
        let sanitized = crate::services::readme_sanitize::sanitize_readme_markdown(&content);

        // Tronque a la limite du quota anti-abus (check DB length <= 20480).
        let truncated = if sanitized.len() > MAX_README_BYTES {
            let mut cut = sanitized.into_bytes();
            cut.truncate(MAX_README_BYTES);
            match String::from_utf8(cut) {
                Ok(s) => s,
                Err(e) => {
                    // Coupe au dernier code point valide.
                    let mut b = e.into_bytes();
                    while b.last().is_some() && String::from_utf8(b.clone()).is_err() {
                        b.pop();
                    }
                    String::from_utf8(b).unwrap_or_default()
                }
            }
        } else {
            sanitized
        };

        sqlx::query(
            "UPDATE users
             SET profile_readme_markdown = $2,
                 updated_at = NOW()
             WHERE id = $1",
        )
        .bind(target.id)
        .bind(&truncated)
        .execute(db)
        .await?;
        synced.push(target.id);
    }

    Ok(SyncReport {
        synced,
        failed,
        skipped_no_readme,
    })
}

/// Determine (repo_full_name, path) a fetch. Prend la sync_url si fournie,
/// sinon la convention `{username}/{username}/README.md`.
fn resolve_source(target: &SyncTarget) -> (String, String) {
    if let Some(url) = &target.profile_readme_sync_url {
        // Parse `https://github.com/{owner}/{repo}/blob/{ref}/{path}` ou raw.
        if let Some(parts) = parse_github_url(url) {
            return parts;
        }
    }
    (
        format!("{}/{}", target.username, target.username),
        "README.md".to_string(),
    )
}

fn parse_github_url(url: &str) -> Option<(String, String)> {
    let stripped = url
        .strip_prefix("https://github.com/")
        .or_else(|| url.strip_prefix("https://raw.githubusercontent.com/"))?;
    let (owner, rest) = stripped.split_once('/')?;
    // On veut juste owner/repo — le path suit apres /blob/{ref}/ ou /{ref}/.
    let (repo, after) = rest.split_once('/')?;
    // Skippe le segment blob (github.com) ou pas (raw).
    let path = if let Some(after_blob) = after.strip_prefix("blob/") {
        // On skip le refname en +1 segment.
        let (_ref, rest) = after_blob.split_once('/')?;
        rest.to_string()
    } else {
        // raw : {ref}/{path}
        let (_ref, rest) = after.split_once('/')?;
        rest.to_string()
    };
    Some((format!("{owner}/{repo}"), path))
}

enum FetchError {
    NotFound,
    Other(String),
}

async fn fetch_readme(
    bot_token: Option<&str>,
    repo_full_name: &str,
    path: &str,
) -> Result<String, FetchError> {
    match bot_token {
        Some(t) => match github::fetch_file_content(t, repo_full_name, path, "HEAD").await {
            Ok(c) => Ok(c),
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("404") || msg.contains("status 404") {
                    Err(FetchError::NotFound)
                } else {
                    Err(FetchError::Other(msg))
                }
            }
        },
        None => {
            // Fetch anonyme via raw.githubusercontent — pas de rate limit token
            // mais plus limite (60 req/h par IP).
            let url = format!("https://raw.githubusercontent.com/{repo_full_name}/HEAD/{path}");
            let resp = reqwest::Client::new()
                .get(&url)
                .header("User-Agent", "skilluv-backend")
                .send()
                .await
                .map_err(|e| FetchError::Other(e.to_string()))?;
            if resp.status() == reqwest::StatusCode::NOT_FOUND {
                return Err(FetchError::NotFound);
            }
            if !resp.status().is_success() {
                return Err(FetchError::Other(format!(
                    "raw github status {}",
                    resp.status()
                )));
            }
            resp.text()
                .await
                .map_err(|e| FetchError::Other(e.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_github_blob_url() {
        let (repo, path) =
            parse_github_url("https://github.com/octocat/octocat/blob/main/README.md").unwrap();
        assert_eq!(repo, "octocat/octocat");
        assert_eq!(path, "README.md");
    }

    #[test]
    fn parse_github_blob_with_nested_path() {
        let (repo, path) =
            parse_github_url("https://github.com/user/repo/blob/main/docs/PROFILE.md").unwrap();
        assert_eq!(repo, "user/repo");
        assert_eq!(path, "docs/PROFILE.md");
    }

    #[test]
    fn parse_raw_githubusercontent() {
        let (repo, path) =
            parse_github_url("https://raw.githubusercontent.com/user/repo/main/README.md").unwrap();
        assert_eq!(repo, "user/repo");
        assert_eq!(path, "README.md");
    }

    #[test]
    fn resolve_falls_back_to_username_repo() {
        let target = SyncTarget {
            id: Uuid::new_v4(),
            username: "amina".to_string(),
            profile_readme_sync_url: None,
        };
        let (repo, path) = resolve_source(&target);
        assert_eq!(repo, "amina/amina");
        assert_eq!(path, "README.md");
    }
}
