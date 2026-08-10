//! Service `project_slices` — unité de travail réelle sur un projet curated.
//!
//! Phase P1 du refactor challenges (voir `docs/challenges-target-model-and-roadmap.md`
//! partie C phase 1 et partie G.1 pour le workflow "PR mergée → deliverable").
//!
//! Ce service généralise le pattern éprouvé de `oss_bounties`/`oss_bounty_claims`.
//! Une slice est claimable exclusivement par un user (soft-lock 7 jours), et son
//! statut suit le lifecycle : `draft` → `open` → `claimed` → `in_review` → `merged`
//! (ou `expired` si claim non honorée).

use chrono::{DateTime, Duration, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::models::ProjectSlice;

/// Durée pendant laquelle un claim est exclusif (7 jours, aligné pattern bounties).
pub const CLAIM_DURATION_DAYS: i64 = 7;

/// P26 v2 SKI-78 — total order over the 5 P17 ranks. Any unknown value
/// returns 0 (apprenti) to fail closed on user data corruption rather than
/// spuriously grant access.
fn rank_ordinal(rank: &str) -> u8 {
    match rank {
        "ranger" => 1,
        "artisan" => 2,
        "maitre" => 3,
        "doyen" => 4,
        _ => 0, // apprenti + unknown
    }
}

/// Re-export of `rank_ordinal` for use by other services that need the
/// same total order (e.g. validator eligibility, SKI-81).
pub fn rank_ordinal_public(rank: &str) -> u8 {
    rank_ordinal(rank)
}

/// P26 v2 SKI-113 — guard against submitting someone else's PR.
///
/// Fetches the PR from GitHub with our bot token and checks that:
///   1. `pr.user.login` matches the challenger's `github_login`.
///   2. `pr.head.repo.full_name` starts with the challenger's login
///      (i.e. the PR comes from *their* fork, not from a co-authored
///      branch on the base repo).
///
/// Silent no-op when either:
///   - The user has no `github_connections` row → pre-P26 v2 flow.
///     A warning is logged so operators can spot users skipping
///     verification, but the flow proceeds.
///   - `SKILLUV_BOT_GITHUB_TOKEN` is unset → dev/staging.
///
/// Returns `AppError::Forbidden` on mismatch — deliberately the same
/// discriminant as the rank/orientation gates, so the API response
/// shape is consistent across all pre-flight refusals.
async fn assert_pr_authored_by_user(
    db: &PgPool,
    user_id: Uuid,
    pr_url: &str,
) -> Result<(), AppError> {
    let gh_login: Option<(String,)> =
        sqlx::query_as("SELECT github_login FROM github_connections WHERE user_id = $1")
            .bind(user_id)
            .fetch_optional(db)
            .await?;
    let Some((login,)) = gh_login else {
        tracing::warn!(
            user_id = %user_id,
            "SKI-113: user has no github_connections; skipping PR-author check"
        );
        return Ok(());
    };

    let Ok(bot_token) = std::env::var("SKILLUV_BOT_GITHUB_TOKEN") else {
        tracing::info!("SKI-113 skipped: SKILLUV_BOT_GITHUB_TOKEN unset (dev mode)");
        return Ok(());
    };

    let Some((owner, repo, number)) = parse_github_pr_url_parts(pr_url) else {
        // Already validated by is_valid_github_pr_url, but be defensive.
        return Ok(());
    };

    #[derive(serde::Deserialize)]
    struct GhPrLite {
        user: GhUser,
        head: GhHead,
    }
    #[derive(serde::Deserialize)]
    struct GhUser {
        login: String,
    }
    #[derive(serde::Deserialize)]
    struct GhHead {
        repo: Option<GhHeadRepo>,
    }
    #[derive(serde::Deserialize)]
    struct GhHeadRepo {
        full_name: String,
    }

    let url = format!("https://api.github.com/repos/{owner}/{repo}/pulls/{number}");
    let pr: GhPrLite = reqwest::Client::new()
        .get(&url)
        .bearer_auth(&bot_token)
        .header("User-Agent", "skilluv-backend/ski-113")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("PR author check fetch failed: {e}")))?
        .error_for_status()
        .map_err(|e| AppError::Internal(format!("PR author check status: {e}")))?
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("PR author check decode: {e}")))?;

    if !login.eq_ignore_ascii_case(&pr.user.login) {
        tracing::warn!(
            user_id = %user_id,
            expected = %login,
            actual = %pr.user.login,
            "SKI-113 refused: PR author mismatch"
        );
        return Err(AppError::Forbidden);
    }

    // Head repo check: the PR must come from a fork owned by the
    // challenger. Some maintainer flows push a branch to the base repo
    // directly; that's not the Skilluv path — external contributors work
    // on their own fork.
    if let Some(head_repo) = pr.head.repo {
        let expected_prefix = format!("{}/", login.to_lowercase());
        if !head_repo
            .full_name
            .to_lowercase()
            .starts_with(&expected_prefix)
        {
            tracing::warn!(
                user_id = %user_id,
                head_repo = %head_repo.full_name,
                "SKI-113 refused: PR head is not the challenger's fork"
            );
            return Err(AppError::Forbidden);
        }
    }
    Ok(())
}

/// P26 v2 SKI-119 — post a Skilluv attribution comment on the PR as the
/// challenger. Uses the user's own OAuth token (scope `public_repo`
/// already grants `issues:write` on public repos — no re-consent needed).
///
/// Idempotent: won't re-post if `announced_at` is already set (checked
/// atomically as part of the UPDATE). Silent no-op when the user hasn't
/// connected GitHub OAuth — no announcement is better than none.
async fn try_announce_on_pr(
    db: &PgPool,
    jwt_secret: &str,
    slice_id: Uuid,
    user_id: Uuid,
    pr_url: &str,
) -> Result<(), AppError> {
    // Load user's decrypted token via the existing github service helper.
    let token = match crate::services::github::load_token(db, jwt_secret, user_id).await {
        Ok(Some(t)) => t,
        _ => {
            tracing::info!(
                user_id = %user_id,
                "SKI-119 skipped: user has no GitHub OAuth connection"
            );
            return Ok(());
        }
    };

    let Some((owner, repo, number)) = parse_github_pr_url_parts(pr_url) else {
        return Ok(()); // parsed already in submit_pr; defensive.
    };

    let body = " This PR is part of a Skilluv community challenge. \
                Learn more: https://skill-uv.com";
    let url = format!("https://api.github.com/repos/{owner}/{repo}/issues/{number}/comments");
    let resp = reqwest::Client::new()
        .post(&url)
        .bearer_auth(&token)
        .header("User-Agent", "skilluv-backend/ski-119")
        .header("Accept", "application/vnd.github+json")
        .json(&serde_json::json!({ "body": body }))
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("SKI-119 POST failed: {e}")))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(AppError::Internal(format!(
            "SKI-119 POST returned {status}: {text}"
        )));
    }

    // Stamp announced_at — this UPDATE races safely with any concurrent
    // duplicate call because the condition `announced_at IS NULL` filters
    // the second one out.
    sqlx::query(
        r#"
        UPDATE project_slices
           SET announced_at = NOW(), updated_at = NOW()
         WHERE id = $1 AND announced_at IS NULL
        "#,
    )
    .bind(slice_id)
    .execute(db)
    .await?;
    Ok(())
}

fn parse_github_pr_url_parts(url: &str) -> Option<(String, String, i32)> {
    let rest = url.strip_prefix("https://github.com/")?;
    let parts: Vec<&str> = rest.split('/').collect();
    if parts.len() != 4 || parts[2] != "pull" {
        return None;
    }
    let n: i32 = parts[3].parse().ok()?;
    Some((parts[0].to_string(), parts[1].to_string(), n))
}

/// P26 v2 SKI-76 — accept only the canonical `https://github.com/{o}/{r}/pull/{n}`
/// shape. Stricter than URL parsing on purpose: gh.io / api.github.com /
/// enterprise hosts are rejected until we explicitly support them, so a
/// typo can never silently associate a challenge with the wrong repo.
fn is_valid_github_pr_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://github.com/") else {
        return false;
    };
    let parts: Vec<&str> = rest.split('/').collect();
    if parts.len() != 4 || parts[2] != "pull" {
        return false;
    }
    !parts[0].is_empty() && !parts[1].is_empty() && parts[3].parse::<u64>().is_ok_and(|n| n > 0)
}

#[cfg(test)]
mod rank_tests {
    use super::rank_ordinal;

    #[test]
    fn ordinal_matches_progression() {
        assert!(rank_ordinal("apprenti") < rank_ordinal("ranger"));
        assert!(rank_ordinal("ranger") < rank_ordinal("artisan"));
        assert!(rank_ordinal("artisan") < rank_ordinal("maitre"));
        assert!(rank_ordinal("maitre") < rank_ordinal("doyen"));
    }

    #[test]
    fn unknown_rank_treated_as_lowest() {
        // Fail-closed: junk data must not silently grant elevated access.
        assert_eq!(rank_ordinal("god-mode"), 0);
        assert_eq!(rank_ordinal(""), 0);
    }
}

#[cfg(test)]
mod pr_url_tests {
    use super::is_valid_github_pr_url;

    #[test]
    fn accepts_canonical_shape() {
        assert!(is_valid_github_pr_url(
            "https://github.com/skilluv/skilluv-backend/pull/42"
        ));
    }

    #[test]
    fn rejects_non_github_hosts() {
        assert!(!is_valid_github_pr_url(
            "https://gitlab.com/skilluv/skilluv-backend/pull/42"
        ));
        assert!(!is_valid_github_pr_url(
            "https://api.github.com/repos/skilluv/x/pulls/42"
        ));
    }

    #[test]
    fn rejects_missing_segments() {
        assert!(!is_valid_github_pr_url(
            "https://github.com/skilluv/pull/42"
        ));
        assert!(!is_valid_github_pr_url(
            "https://github.com/skilluv/x/issues/42"
        ));
        assert!(!is_valid_github_pr_url(
            "https://github.com/skilluv/x/pull/not-a-number"
        ));
        assert!(!is_valid_github_pr_url(
            "https://github.com/skilluv/x/pull/0"
        ));
    }
}

/// Service métier pour les slices.
///
/// N'a pas d'état côté Rust — c'est un namespace de fonctions qui opèrent sur
/// le PgPool. Suit la convention des autres services du projet.
pub struct SlicesService;

/// Filtres pour lister les slices ouvertes.
#[derive(Debug, Clone, Default)]
pub struct ListFilter {
    pub domain: Option<String>,
    pub difficulty: Option<i16>,
    pub project_id: Option<Uuid>,
    pub page: i64,
    pub per_page: i64,
}

impl SlicesService {
    // ═══════════════════════════════════════════════════════════════════
    // Lectures (list, get)
    // ═══════════════════════════════════════════════════════════════════

    /// Liste les slices `status='open'` avec filtres.
    ///
    /// Ordre : difficulty ASC puis created_at DESC (les plus faciles d'abord,
    /// puis les plus récentes) — cohérent avec l'expérience d'entrée d'un
    /// nouveau contributeur qui cherche des tâches accessibles.
    pub async fn list_open(
        db: &PgPool,
        filter: &ListFilter,
    ) -> Result<(Vec<ProjectSlice>, i64), AppError> {
        let per_page = filter.per_page.clamp(1, 100);
        let page = filter.page.max(1);
        let offset = (page - 1) * per_page;

        let slices = sqlx::query_as::<_, ProjectSlice>(
            r#"
            SELECT * FROM project_slices
            WHERE status = 'open'
              AND ($1::text IS NULL OR primary_domain = $1)
              AND ($2::smallint IS NULL OR difficulty = $2)
              AND ($3::uuid IS NULL OR project_id = $3)
            ORDER BY difficulty ASC, created_at DESC
            LIMIT $4 OFFSET $5
            "#,
        )
        .bind(&filter.domain)
        .bind(filter.difficulty)
        .bind(filter.project_id)
        .bind(per_page)
        .bind(offset)
        .fetch_all(db)
        .await?;

        let total: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM project_slices
            WHERE status = 'open'
              AND ($1::text IS NULL OR primary_domain = $1)
              AND ($2::smallint IS NULL OR difficulty = $2)
              AND ($3::uuid IS NULL OR project_id = $3)
            "#,
        )
        .bind(&filter.domain)
        .bind(filter.difficulty)
        .bind(filter.project_id)
        .fetch_one(db)
        .await?;

        Ok((slices, total))
    }

    /// Récupère une slice par son id (peu importe le status — utile pour affichage).
    pub async fn get(db: &PgPool, slice_id: Uuid) -> Result<ProjectSlice, AppError> {
        sqlx::query_as::<_, ProjectSlice>("SELECT * FROM project_slices WHERE id = $1")
            .bind(slice_id)
            .fetch_optional(db)
            .await?
            .ok_or_else(|| AppError::NotFound("Slice not found".to_string()))
    }

    /// Liste les slices claimed par un user (`in_progress` côté user).
    pub async fn list_claimed_by(
        db: &PgPool,
        user_id: Uuid,
    ) -> Result<Vec<ProjectSlice>, AppError> {
        let slices = sqlx::query_as::<_, ProjectSlice>(
            r#"
            SELECT * FROM project_slices
            WHERE claimed_by_user_id = $1
              AND status IN ('claimed', 'in_review')
            ORDER BY claim_expires_at ASC NULLS LAST, claimed_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(db)
        .await?;

        Ok(slices)
    }

    // ═══════════════════════════════════════════════════════════════════
    // Mutations : claim, unclaim
    // ═══════════════════════════════════════════════════════════════════

    /// Claim une slice pour un user. Soft-lock exclusif pendant `CLAIM_DURATION_DAYS`.
    ///
    /// Erreurs :
    /// - `NotFound` si la slice n'existe pas ou n'est pas `open`
    /// - `Validation` si le user a déjà `max_concurrent_claims` slices actives
    ///   (Phase P1 : pas de limite. À réintroduire si besoin en Phase P3+)
    /// - `Forbidden` si la slice porte un gate (P26 v2 SKI-79 orientations)
    ///   que le user ne satisfait pas.
    ///
    /// Retourne la slice mise à jour avec `claim_expires_at` calculé.
    pub async fn claim(
        db: &PgPool,
        slice_id: Uuid,
        user_id: Uuid,
    ) -> Result<ProjectSlice, AppError> {
        Self::assert_orientation_access(db, slice_id, user_id).await?;
        Self::assert_rank_access(db, slice_id, user_id).await?;

        let expires_at = Utc::now() + Duration::days(CLAIM_DURATION_DAYS);

        let slice = sqlx::query_as::<_, ProjectSlice>(
            r#"
            UPDATE project_slices
            SET status = 'claimed',
                claimed_by_user_id = $1,
                claimed_at = NOW(),
                claim_expires_at = $2,
                updated_at = NOW()
            WHERE id = $3
              AND status = 'open'
            RETURNING *
            "#,
        )
        .bind(user_id)
        .bind(expires_at)
        .bind(slice_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| {
            AppError::Validation(
                "Slice is not available for claim (either not found, or already claimed / closed)"
                    .to_string(),
            )
        })?;

        Ok(slice)
    }

    /// P26 v2 SKI-76 — challenger declares the PR they've opened against
    /// the target repo. Advances status from `claimed`/`in_progress` to
    /// `submitted`, stores the URL, and stamps `submitted_at`.
    ///
    /// Errors:
    /// - `Validation` if the PR URL is not a well-formed GitHub PR URL
    ///   (shape `https://github.com/{owner}/{repo}/pull/{n}`).
    /// - `Validation` if the slice is not currently claimed by this user
    ///   (or is in a status that can't accept a submission).
    pub async fn submit_pr(
        db: &PgPool,
        jwt_secret: &str,
        slice_id: Uuid,
        user_id: Uuid,
        pr_url: &str,
        announce_publicly: bool,
    ) -> Result<ProjectSlice, AppError> {
        if !is_valid_github_pr_url(pr_url) {
            return Err(AppError::Validation(
                "pr_url must look like https://github.com/{owner}/{repo}/pull/{n}".into(),
            ));
        }

        // P26 v2 SKI-113 — verify the PR was actually opened by this
        // challenger. Prevents submitting someone else's PR to earn a
        // challenge. No-op silently when:
        //   - user has not connected GitHub OAuth (pre-P26 v2 flow,
        //     don't break existing tests / dev environments)
        //   - bot token is unset (dev / staging where verification isn't
        //     wired yet — we log so operators see it in prod audits)
        assert_pr_authored_by_user(db, user_id, pr_url).await?;

        let slice = sqlx::query_as::<_, ProjectSlice>(
            r#"
            UPDATE project_slices
               SET status = 'submitted',
                   submitted_pr_url = $3,
                   submitted_at = NOW(),
                   updated_at = NOW()
             WHERE id = $1
               AND claimed_by_user_id = $2
               AND status IN ('claimed', 'in_progress')
         RETURNING *
            "#,
        )
        .bind(slice_id)
        .bind(user_id)
        .bind(pr_url)
        .fetch_optional(db)
        .await
        .map_err(|e| {
            // SKI-114 — partial UNIQUE `uq_slices_submitted_pr_url_active`
            // fires when the same PR URL is submitted to a second active
            // slice. Return a stable 409 with an actionable message.
            if let sqlx::Error::Database(dbe) = &e
                && dbe
                    .constraint()
                    .is_some_and(|c| c == "uq_slices_submitted_pr_url_active")
            {
                return AppError::Conflict(
                    "This PR URL is already attached to another active challenge.".into(),
                );
            }
            AppError::Database(e)
        })?
        .ok_or_else(|| {
            AppError::Validation(
                "Slice cannot be submitted (not found, not your claim, or wrong status).".into(),
            )
        })?;

        // SKI-119 — opt-in public announcement. Fire-and-forget best-effort:
        // any failure logs a warn but does NOT roll back the submission (the
        // challenger's claim of a PR is the primary outcome). Idempotent
        // via the `announced_at IS NULL` guard inside the spawned task.
        if announce_publicly && slice.announced_at.is_none() {
            let db_clone = db.clone();
            let secret_clone = jwt_secret.to_string();
            let pr_url_clone = pr_url.to_string();
            let slice_id_clone = slice.id;
            tokio::spawn(async move {
                if let Err(e) = try_announce_on_pr(
                    &db_clone,
                    &secret_clone,
                    slice_id_clone,
                    user_id,
                    &pr_url_clone,
                )
                .await
                {
                    tracing::warn!(
                        slice_id = %slice_id_clone,
                        error = %e,
                        "SKI-119 announcement failed — slice already submitted, no rollback"
                    );
                }
            });
        }

        Ok(slice)
    }

    /// P26 v2 SKI-79 — orientation gate. Returns `Ok(())` when either:
    ///   - the slice's `required_orientation_slugs` is empty (no restriction), or
    ///   - the user holds an active (`ended_at IS NULL`) user_orientation
    ///     whose orientation.slug matches one of the required slugs.
    ///
    /// Returns `AppError::Forbidden` otherwise. The message deliberately does
    /// NOT enumerate the allowed orientations — that hint would let a curious
    /// user reverse-engineer the sensitivity policy. The slice detail view
    /// exposes the required slugs to the user as a first-class field so they
    /// can decide whether to add the orientation before retrying.
    pub async fn assert_orientation_access(
        db: &PgPool,
        slice_id: Uuid,
        user_id: Uuid,
    ) -> Result<(), AppError> {
        let gate: Option<(Vec<String>,)> =
            sqlx::query_as("SELECT required_orientation_slugs FROM project_slices WHERE id = $1")
                .bind(slice_id)
                .fetch_optional(db)
                .await?;
        let Some((required,)) = gate else {
            return Ok(()); // Slice not found → the UPDATE below will surface a clearer error.
        };
        if required.is_empty() {
            return Ok(());
        }

        let has_match: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM user_orientations uo
                JOIN orientations o ON o.id = uo.orientation_id
                WHERE uo.user_id = $1
                  AND uo.ended_at IS NULL
                  AND o.slug = ANY($2)
            )
            "#,
        )
        .bind(user_id)
        .bind(&required)
        .fetch_one(db)
        .await?;
        if !has_match {
            return Err(AppError::Forbidden);
        }
        Ok(())
    }

    /// P26 v2 SKI-78 — minimum-rank gate. Returns `Ok(())` when the slice
    /// has no `min_rank` set or when the user's current rank is at or
    /// above it. Ordering: apprenti(0) < ranger(1) < artisan(2) < maitre(3)
    /// < doyen(4). A user with no `user_ranks` row is treated as apprenti(0).
    pub async fn assert_rank_access(
        db: &PgPool,
        slice_id: Uuid,
        user_id: Uuid,
    ) -> Result<(), AppError> {
        let gate: Option<(Option<String>,)> =
            sqlx::query_as("SELECT min_rank FROM project_slices WHERE id = $1")
                .bind(slice_id)
                .fetch_optional(db)
                .await?;
        let Some((Some(required),)) = gate else {
            return Ok(()); // slice absent → the UPDATE surfaces the real error / no gate set
        };

        let current: Option<(String,)> =
            sqlx::query_as("SELECT rank FROM user_ranks WHERE user_id = $1")
                .bind(user_id)
                .fetch_optional(db)
                .await?;
        let current_rank = current.map(|(r,)| r).unwrap_or_else(|| "apprenti".into());

        if rank_ordinal(&current_rank) >= rank_ordinal(&required) {
            Ok(())
        } else {
            Err(AppError::Forbidden)
        }
    }

    /// Un user relâche sa slice. Elle retourne au pool `open`.
    ///
    /// Erreurs :
    /// - `Validation` si la slice n'est pas claimée par ce user
    pub async fn unclaim(
        db: &PgPool,
        slice_id: Uuid,
        user_id: Uuid,
    ) -> Result<ProjectSlice, AppError> {
        let slice = sqlx::query_as::<_, ProjectSlice>(
            r#"
            UPDATE project_slices
            SET status = 'open',
                claimed_by_user_id = NULL,
                claimed_at = NULL,
                claim_expires_at = NULL,
                updated_at = NOW()
            WHERE id = $1
              AND claimed_by_user_id = $2
              AND status = 'claimed'
            RETURNING *
            "#,
        )
        .bind(slice_id)
        .bind(user_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| {
            AppError::Validation("You can only unclaim your own claimed slices".to_string())
        })?;

        Ok(slice)
    }

    // ═══════════════════════════════════════════════════════════════════
    // P11.4 : steward inbox — validation des drafts ingérées auto
    // ═══════════════════════════════════════════════════════════════════

    /// Liste les slices en `status='draft'` pour un project — dashboard
    /// steward "voici ce que le poller GitHub / webhook a ingesté, à valider".
    ///
    /// L'appelant doit être steward du project (validation côté route).
    pub async fn list_drafts_for_project(
        db: &PgPool,
        project_id: Uuid,
    ) -> Result<Vec<ProjectSlice>, AppError> {
        let slices = sqlx::query_as::<_, ProjectSlice>(
            r#"
            SELECT * FROM project_slices
            WHERE project_id = $1 AND status = 'draft'
            ORDER BY created_at DESC
            "#,
        )
        .bind(project_id)
        .fetch_all(db)
        .await?;
        Ok(slices)
    }

    /// Publie une slice draft (draft → open). Autorisation à faire côté
    /// route via `StewardsService::is_steward` OU admin.
    pub async fn publish_draft(db: &PgPool, slice_id: Uuid) -> Result<ProjectSlice, AppError> {
        let slice = sqlx::query_as::<_, ProjectSlice>(
            r#"
            UPDATE project_slices
            SET status = 'open', updated_at = NOW()
            WHERE id = $1 AND status = 'draft'
            RETURNING *
            "#,
        )
        .bind(slice_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::Validation("Slice not found or not in draft".into()))?;
        Ok(slice)
    }

    /// Ferme une slice draft rejetée (draft → closed avec raison).
    /// Le steward peut refuser une slice ingérée non pertinente sans la publier.
    pub async fn reject_draft(db: &PgPool, slice_id: Uuid) -> Result<ProjectSlice, AppError> {
        let slice = sqlx::query_as::<_, ProjectSlice>(
            r#"
            UPDATE project_slices
            SET status = 'closed', closed_at = NOW(), updated_at = NOW()
            WHERE id = $1 AND status = 'draft'
            RETURNING *
            "#,
        )
        .bind(slice_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::Validation("Slice not found or not in draft".into()))?;
        Ok(slice)
    }

    // ═══════════════════════════════════════════════════════════════════
    // P10.1 : claim en team (persistent) — alternative au claim solo user
    // ═══════════════════════════════════════════════════════════════════

    /// Claim une slice pour une team persistente. XOR avec le claim solo user.
    ///
    /// L'appelant doit être membre de la team (validation faite côté route).
    /// Erreurs : `Validation` si slice pas `open` ou team déjà claim ailleurs.
    pub async fn claim_as_team(
        db: &PgPool,
        slice_id: Uuid,
        team_id: Uuid,
    ) -> Result<ProjectSlice, AppError> {
        let expires_at = Utc::now() + Duration::days(CLAIM_DURATION_DAYS);

        let slice = sqlx::query_as::<_, ProjectSlice>(
            r#"
            UPDATE project_slices
            SET status = 'claimed',
                claimed_by_team_id = $1,
                claimed_by_user_id = NULL,
                claimed_at = NOW(),
                claim_expires_at = $2,
                updated_at = NOW()
            WHERE id = $3
              AND status = 'open'
            RETURNING *
            "#,
        )
        .bind(team_id)
        .bind(expires_at)
        .bind(slice_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| {
            AppError::Validation(
                "Slice is not available for team claim (not found or already claimed / closed)"
                    .to_string(),
            )
        })?;

        Ok(slice)
    }

    /// Un membre de la team relâche le claim collectif de la slice.
    pub async fn unclaim_by_team(
        db: &PgPool,
        slice_id: Uuid,
        team_id: Uuid,
    ) -> Result<ProjectSlice, AppError> {
        let slice = sqlx::query_as::<_, ProjectSlice>(
            r#"
            UPDATE project_slices
            SET status = 'open',
                claimed_by_team_id = NULL,
                claimed_at = NULL,
                claim_expires_at = NULL,
                updated_at = NOW()
            WHERE id = $1
              AND claimed_by_team_id = $2
              AND status = 'claimed'
            RETURNING *
            "#,
        )
        .bind(slice_id)
        .bind(team_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| {
            AppError::Validation("This team does not currently claim this slice".to_string())
        })?;

        Ok(slice)
    }

    /// Slices claimed par une team (dashboard team).
    pub async fn list_claimed_by_team(
        db: &PgPool,
        team_id: Uuid,
    ) -> Result<Vec<ProjectSlice>, AppError> {
        let slices = sqlx::query_as::<_, ProjectSlice>(
            r#"
            SELECT * FROM project_slices
            WHERE claimed_by_team_id = $1
              AND status IN ('claimed', 'in_review')
            ORDER BY claim_expires_at ASC NULLS LAST, claimed_at DESC
            "#,
        )
        .bind(team_id)
        .fetch_all(db)
        .await?;

        Ok(slices)
    }

    // ═══════════════════════════════════════════════════════════════════
    // Maintenance : expire les claims dépassés (appelé par cron)
    // ═══════════════════════════════════════════════════════════════════

    /// Retourne au pool `open` les claims dont `claim_expires_at` est dépassé.
    ///
    /// Appelé par un cron (à définir en Phase P1 ou plus tard) toutes les heures.
    /// Retourne le nombre de slices remises au pool.
    ///
    /// Note (workflow W1) : la reco de la session 2026-07-09 mentionne une notif
    /// à J+5 puis prolongation manuelle par steward possible. Ce service ne gère
    /// que le hard expire à J+7 ; la notif J+5 est un cron séparé (Phase P2+).
    pub async fn expire_stale_claims(db: &PgPool) -> Result<u64, AppError> {
        let now = Utc::now();
        let result = sqlx::query(
            r#"
            UPDATE project_slices
            SET status = 'open',
                claimed_by_user_id = NULL,
                claimed_by_team_id = NULL,
                claimed_at = NULL,
                claim_expires_at = NULL,
                updated_at = NOW()
            WHERE status = 'claimed'
              AND claim_expires_at IS NOT NULL
              AND claim_expires_at < $1
            "#,
        )
        .bind(now)
        .execute(db)
        .await?;

        Ok(result.rows_affected())
    }

    /// Slices proches d'expirer (utile pour envoyer une notif J+5 au user
    /// et à son steward, workflow W1 reco session 2026-07-09).
    pub async fn find_expiring_within(
        db: &PgPool,
        within: Duration,
    ) -> Result<Vec<ProjectSlice>, AppError> {
        let deadline: DateTime<Utc> = Utc::now() + within;

        let slices = sqlx::query_as::<_, ProjectSlice>(
            r#"
            SELECT * FROM project_slices
            WHERE status = 'claimed'
              AND claim_expires_at IS NOT NULL
              AND claim_expires_at BETWEEN NOW() AND $1
            ORDER BY claim_expires_at ASC
            "#,
        )
        .bind(deadline)
        .fetch_all(db)
        .await?;

        Ok(slices)
    }
}
