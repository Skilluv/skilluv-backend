//! Priorite haute #2 strategy doc §15 : mirroring des entrees Hello Wall vers
//! le repo GitHub `skilluv-community/hello-wall`.
//!
//! Flux :
//!   1. Le webhook Bonjour Skilluv (`routes/onboarding::handle_bonjour_skilluv_pr_event`)
//!      insere une ligne dans `hello_wall_entries` quand un user ouvre sa PR.
//!   2. `mirrored_at` est NULL a ce moment — le fichier
//!      `entries/{username}.md` n'existe pas encore sur `skilluv-community/hello-wall`.
//!   3. Ce service, appele soit inline par le webhook (best-effort), soit
//!      par un cron/worker, prend les entrees WHERE mirrored_at IS NULL et
//!      PUT le contenu sur GitHub via un service account token.
//!   4. Sur succes : UPDATE mirrored_at = NOW(), mirror_error = NULL.
//!   5. Sur echec : UPDATE mirror_error = ..., mirror_attempt_count += 1.
//!
//! Le token GitHub utilise est un service account (env `SKILLUV_BOT_GITHUB_TOKEN`)
//! avec droit ecriture sur `skilluv-community/hello-wall`. Different du token
//! OAuth de l'user (qui ne peut pas ecrire dans l'organisation skilluv-community).

use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::github;

const MIRROR_TARGET_REPO: &str = "skilluv-community/hello-wall";
const MAX_ATTEMPTS: i32 = 5;
const BATCH_SIZE: i64 = 20;

#[derive(Debug, Clone, sqlx::FromRow)]
struct PendingEntry {
    id: Uuid,
    user_id: Uuid,
    hello_markdown: String,
    source_pr_url: String,
    source_starter_repo: String,
    mirror_attempt_count: i32,
}

#[derive(Debug, Clone)]
pub struct MirrorReport {
    pub mirrored: Vec<Uuid>, // hello_wall_entries.id
    pub failed: Vec<(Uuid, String)>,
    pub skipped: usize,
}

/// Recupere les entrees pending et pousse chacune sur GitHub.
///
/// - `bot_token` : Personal Access Token (ou fine-grained) avec droit
///   `contents:write` sur `skilluv-community/hello-wall`.
/// - Idempotent : retenter est safe (create_or_update_file gere le sha existant).
///
/// Batch limite a `BATCH_SIZE` par appel pour eviter de saturer GitHub API
/// rate limits (5000 req/h par token).
pub async fn mirror_pending_entries(
    db: &PgPool,
    bot_token: &str,
) -> Result<MirrorReport, AppError> {
    let pending: Vec<PendingEntry> = sqlx::query_as(
        r#"
        SELECT hwe.id, hwe.user_id, hwe.hello_markdown,
               hwe.source_pr_url, hwe.source_starter_repo,
               hwe.mirror_attempt_count
        FROM hello_wall_entries hwe
        WHERE hwe.deleted_at IS NULL
          AND hwe.mirrored_at IS NULL
          AND hwe.mirror_attempt_count < $1
        ORDER BY hwe.archived_at ASC
        LIMIT $2
        "#,
    )
    .bind(MAX_ATTEMPTS)
    .bind(BATCH_SIZE)
    .fetch_all(db)
    .await?;

    let mut mirrored = Vec::new();
    let mut failed = Vec::new();

    for entry in pending {
        // Recupere le username depuis la table users pour construire le path.
        let username: Option<String> =
            sqlx::query_scalar("SELECT username FROM users WHERE id = $1")
                .bind(entry.user_id)
                .fetch_optional(db)
                .await?;
        let Some(username) = username else {
            tracing::warn!(entry_id = %entry.id, "user vanished, skipping");
            continue;
        };

        let path = format!("entries/{username}.md");
        let content = format_entry_markdown(&username, &entry);
        let commit_message = format!("chore(hello-wall): archive {username}'s first commit");

        match github::create_or_update_file(
            bot_token,
            MIRROR_TARGET_REPO,
            &path,
            &content,
            &commit_message,
        )
        .await
        {
            Ok(html_url) => {
                sqlx::query(
                    "UPDATE hello_wall_entries
                     SET mirrored_at = NOW(),
                         mirror_error = NULL,
                         mirror_attempt_count = mirror_attempt_count + 1,
                         github_entry_url = $2
                     WHERE id = $1",
                )
                .bind(entry.id)
                .bind(&html_url)
                .execute(db)
                .await?;
                mirrored.push(entry.id);
                tracing::info!(
                    entry_id = %entry.id,
                    username,
                    github_url = html_url,
                    "hello_wall entry mirrored"
                );
            }
            Err(e) => {
                let err_str = e.to_string();
                sqlx::query(
                    "UPDATE hello_wall_entries
                     SET mirror_error = $2,
                         mirror_attempt_count = mirror_attempt_count + 1
                     WHERE id = $1",
                )
                .bind(entry.id)
                .bind(&err_str)
                .execute(db)
                .await?;
                failed.push((entry.id, err_str.clone()));
                tracing::error!(
                    entry_id = %entry.id,
                    username,
                    attempt = entry.mirror_attempt_count + 1,
                    error = err_str,
                    "hello_wall mirror failed"
                );
            }
        }
    }

    let skipped = 0; // reserve pour les cas ou on skip (rate limit auto-detect, etc.)

    Ok(MirrorReport {
        mirrored,
        failed,
        skipped,
    })
}

fn format_entry_markdown(username: &str, entry: &PendingEntry) -> String {
    format!(
        r#"# Hello {username}

**Joined**: {archived_date}
**Starter template**: `{starter}`
**Source PR**: {pr_url}

## Their introduction

{content}
"#,
        username = username,
        archived_date = chrono::Utc::now().format("%Y-%m-%d"),
        starter = entry.source_starter_repo,
        pr_url = entry.source_pr_url,
        content = entry.hello_markdown,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_markdown_has_key_fields() {
        let entry = PendingEntry {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            hello_markdown: "Hello world.".to_string(),
            source_pr_url: "https://github.com/testuser/starter-fullstack-rust/pull/1".to_string(),
            source_starter_repo: "starter-fullstack-rust".to_string(),
            mirror_attempt_count: 0,
        };
        let md = format_entry_markdown("testuser", &entry);
        assert!(md.contains("# Hello testuser"));
        assert!(md.contains("`starter-fullstack-rust`"));
        assert!(md.contains("Hello world."));
        assert!(md.contains("pull/1"));
    }
}
