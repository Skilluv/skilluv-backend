//! Service `deliverables` — création, vérification, propagation des skills.
//!
//! Phase P2.1 : implémente le workflow G.1 "PR mergée → deliverable auto-vérifié".
//! Voir docs/challenges-target-model-and-roadmap.md partie G.1 pour les 14 étapes.
//!
//! Toutes les opérations qui modifient l'état (create, revoke) sont transactionnelles
//! pour garantir la cohérence entre `deliverables`, `project_slices`, `user_skills`,
//! `users.total_fragments`, `credit_transactions`.

use bigdecimal::BigDecimal;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::errors::AppError;
use crate::models::{Deliverable, UserSkill};

/// Fenêtre après vérification pendant laquelle le user peut déclarer son
/// niveau d'assistance IA (aligné workflow G.1 étape 12).
pub const AI_DISCLOSURE_WINDOW_DAYS: i64 = 7;

pub struct DeliverablesService;

/// P10.4 — Un contributeur d'une team submission avec son rôle et sa part de fragments.
///
/// Utilisé pour matérialiser dans `deliverables.artifact_metadata.contributors`
/// qui a participé et comment les fragments ont été répartis (proportionnel au
/// nombre de slots occupés, ou équirépartition si pas de slots).
#[derive(Debug, Clone, serde::Serialize)]
pub struct TeamContributor {
    pub user_id: Uuid,
    pub role_slug: Option<String>,
    pub fragments_awarded: i32,
}

/// Payload extrait d'un webhook GitHub `pull_request.closed` avec merged=true.
///
/// Utilisé par [`DeliverablesService::create_from_pr_merged`].
#[derive(Debug, Clone)]
pub struct PrMergedParams {
    pub project_id: Uuid,
    pub repo_owner: String,
    pub repo_name: String,
    pub pr_number: i32,
    pub pr_url: String,
    pub pr_body: String,
    pub merge_commit_sha: String,
    pub github_login: String,
    pub commits_count: Option<i32>,
    pub additions: Option<i32>,
    pub deletions: Option<i32>,
    pub files_changed: Option<i32>,
    pub base_branch: Option<String>,
}

/// Résultat de la tentative de création à partir d'un PR mergé.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum PrMergedOutcome {
    /// Deliverable créé et vérifié automatiquement.
    Verified { deliverable_id: Uuid },
    /// PR authored by ≠ claimed_by — deliverable créé en pending_manual_review.
    PendingManualReview { deliverable_id: Uuid },
    /// Aucune slice matchée pour cette PR. Rien créé, la PR reste "libre".
    NoMatchingSlice,
    /// Idempotence : ce commit hash a déjà produit un deliverable pour ce user.
    AlreadyProcessed { deliverable_id: Uuid },
    /// La slice matchée n'est pas dans un statut compatible avec merge (déjà fermée, etc.).
    SliceNotActionable { slice_id: Uuid, slice_status: String },
}

impl DeliverablesService {
    // ═══════════════════════════════════════════════════════════════════
    // Point d'entrée principal : workflow G.1
    // ═══════════════════════════════════════════════════════════════════

    /// Traite un événement webhook GitHub `pull_request.closed` avec merged=true.
    ///
    /// Workflow (voir G.1) :
    /// 1. Résolution slice (marker body / Closes #N / best-effort)
    /// 2. Vérification legitimité (author PR match claimed_by)
    /// 3. Insertion deliverable (idempotent via UNIQUE user_id + artifact_hash)
    /// 4. Update slice → merged (si vérifié)
    /// 5. Award fragments à l'auteur
    /// 6. Propagation skills → user_skills recompute
    ///
    /// Le tout dans une transaction unique pour cohérence.
    pub async fn create_from_pr_merged(
        db: &PgPool,
        params: PrMergedParams,
    ) -> Result<PrMergedOutcome, AppError> {
        let mut tx = db.begin().await?;

        // 1. Résolution slice
        let Some(slice) = Self::resolve_slice_for_pr(&mut tx, &params).await? else {
            return Ok(PrMergedOutcome::NoMatchingSlice);
        };

        // Slice doit être active pour être finalisée par cette PR
        if !matches!(slice.status.as_str(), "claimed" | "in_review" | "open") {
            return Ok(PrMergedOutcome::SliceNotActionable {
                slice_id: slice.id,
                slice_status: slice.status,
            });
        }

        // 2. Résolution de l'auteur Skilluv depuis son github_login
        let Some(author_user_id) = Self::resolve_github_login(&mut tx, &params.github_login).await? else {
            // L'auteur PR n'est pas un user Skilluv connecté.
            // Best-effort : si la slice a un claimed_by, on suppose que la PR est
            // pour ce user et on marque pending_manual_review.
            let Some(claimed_by) = slice.claimed_by_user_id else {
                return Ok(PrMergedOutcome::NoMatchingSlice);
            };
            return Self::insert_deliverable_pending_manual_review(&mut tx, &slice, &params, claimed_by).await;
        };

        // 3. Vérification legitimité
        let author_match = slice
            .claimed_by_user_id
            .is_some_and(|claimed_by| claimed_by == author_user_id);

        let outcome = if author_match {
            Self::insert_deliverable_verified(&mut tx, &slice, &params, author_user_id).await?
        } else {
            // Mismatch : la slice était claimed par quelqu'un d'autre.
            // Peut être un co-auteur, une escalade, ou un abus → review humaine.
            let assign_to = slice.claimed_by_user_id.unwrap_or(author_user_id);
            Self::insert_deliverable_pending_manual_review(&mut tx, &slice, &params, assign_to)
                .await?
        };

        tx.commit().await?;

        // P19.2 — Best-effort recompute proof engines si le deliverable est
        // devenu verified via cet event (author_match). Le hook est async pour
        // ne pas bloquer le webhook GitHub.
        if matches!(outcome, PrMergedOutcome::Verified { .. }) {
            let db_clone = db.clone();
            tokio::spawn(async move {
                let _ = crate::services::proof_hooks::recompute_all_for_user(
                    &db_clone,
                    author_user_id,
                )
                .await;
            });
        }

        Ok(outcome)
    }

    // ═══════════════════════════════════════════════════════════════════
    // Insertion + side-effects (transactionnels)
    // ═══════════════════════════════════════════════════════════════════

    async fn insert_deliverable_verified(
        tx: &mut Transaction<'_, Postgres>,
        slice: &crate::models::ProjectSlice,
        params: &PrMergedParams,
        user_id: Uuid,
    ) -> Result<PrMergedOutcome, AppError> {
        let fragments_awarded = slice.fragments_reward;
        let credits_awarded = slice.credits_reward.clone();

        let now = Utc::now();
        let ai_deadline = now + chrono::Duration::days(AI_DISCLOSURE_WINDOW_DAYS);

        // Insertion idempotente via UNIQUE(user_id, artifact_hash)
        let inserted: Option<Uuid> = sqlx::query_scalar(
            r#"
            INSERT INTO deliverables (
                slice_id, user_id,
                artifact_type, artifact_url, artifact_hash, artifact_metadata,
                verifiable_by, verification_status, verified_at, verification_signal,
                fragments_awarded, credits_awarded,
                ai_disclosure_prompted_at, ai_disclosure_deadline_at,
                public, submitted_at, created_at
            )
            VALUES (
                $1, $2,
                'pr_merged', $3, $4, $5,
                'github_webhook', 'verified', NOW(), $6,
                $7, $8,
                NOW(), $9,
                TRUE, NOW(), NOW()
            )
            ON CONFLICT (user_id, artifact_hash) WHERE artifact_hash IS NOT NULL DO NOTHING
            RETURNING id
            "#,
        )
        .bind(slice.id)
        .bind(user_id)
        .bind(&params.pr_url)
        .bind(&params.merge_commit_sha)
        .bind(Self::build_pr_metadata(params))
        .bind(Self::build_verification_signal(params))
        .bind(fragments_awarded)
        .bind(&credits_awarded)
        .bind(ai_deadline)
        .fetch_optional(&mut **tx)
        .await?;

        let deliverable_id = match inserted {
            Some(id) => id,
            None => {
                // Idempotence hit : le deliverable existe déjà.
                let existing_id: Uuid = sqlx::query_scalar(
                    "SELECT id FROM deliverables
                     WHERE user_id = $1 AND artifact_hash = $2 LIMIT 1",
                )
                .bind(user_id)
                .bind(&params.merge_commit_sha)
                .fetch_one(&mut **tx)
                .await?;
                return Ok(PrMergedOutcome::AlreadyProcessed {
                    deliverable_id: existing_id,
                });
            }
        };

        // Slice → merged
        sqlx::query(
            "UPDATE project_slices
             SET status = 'merged', closed_at = NOW(), updated_at = NOW()
             WHERE id = $1",
        )
        .bind(slice.id)
        .execute(&mut **tx)
        .await?;

        // Fragments à l'auteur
        if fragments_awarded > 0 {
            sqlx::query(
                "UPDATE users
                 SET total_fragments = total_fragments + $1, updated_at = NOW()
                 WHERE id = $2",
            )
            .bind(fragments_awarded)
            .bind(user_id)
            .execute(&mut **tx)
            .await?;
        }

        // Propagation skills
        Self::propagate_skills(tx, slice.id, user_id, deliverable_id).await?;

        Ok(PrMergedOutcome::Verified { deliverable_id })
    }

    async fn insert_deliverable_pending_manual_review(
        tx: &mut Transaction<'_, Postgres>,
        slice: &crate::models::ProjectSlice,
        params: &PrMergedParams,
        assign_to_user_id: Uuid,
    ) -> Result<PrMergedOutcome, AppError> {
        let inserted: Option<Uuid> = sqlx::query_scalar(
            r#"
            INSERT INTO deliverables (
                slice_id, user_id,
                artifact_type, artifact_url, artifact_hash, artifact_metadata,
                verifiable_by, verification_status, verification_signal,
                public, submitted_at, created_at
            )
            VALUES (
                $1, $2,
                'pr_merged', $3, $4, $5,
                'github_webhook', 'pending_manual_review', $6,
                TRUE, NOW(), NOW()
            )
            ON CONFLICT (user_id, artifact_hash) WHERE artifact_hash IS NOT NULL DO NOTHING
            RETURNING id
            "#,
        )
        .bind(slice.id)
        .bind(assign_to_user_id)
        .bind(&params.pr_url)
        .bind(&params.merge_commit_sha)
        .bind(Self::build_pr_metadata(params))
        .bind(Self::build_verification_signal(params))
        .fetch_optional(&mut **tx)
        .await?;

        let Some(deliverable_id) = inserted else {
            let existing_id: Uuid = sqlx::query_scalar(
                "SELECT id FROM deliverables
                 WHERE user_id = $1 AND artifact_hash = $2 LIMIT 1",
            )
            .bind(assign_to_user_id)
            .bind(&params.merge_commit_sha)
            .fetch_one(&mut **tx)
            .await?;
            return Ok(PrMergedOutcome::AlreadyProcessed {
                deliverable_id: existing_id,
            });
        };

        // Slice → in_review (pas encore merged, on attend le verdict humain)
        sqlx::query(
            "UPDATE project_slices
             SET status = 'in_review', updated_at = NOW()
             WHERE id = $1",
        )
        .bind(slice.id)
        .execute(&mut **tx)
        .await?;

        // Auto-create review_task pour le workflow H.2
        crate::services::ReviewQueueService::create_task_for_slice_claim(
            tx,
            deliverable_id,
            slice.id,
            &slice.primary_domain,
        )
        .await?;

        Ok(PrMergedOutcome::PendingManualReview { deliverable_id })
    }

    // ═══════════════════════════════════════════════════════════════════
    // Résolution slice (marker body / Closes #N)
    // ═══════════════════════════════════════════════════════════════════

    async fn resolve_slice_for_pr(
        tx: &mut Transaction<'_, Postgres>,
        params: &PrMergedParams,
    ) -> Result<Option<crate::models::ProjectSlice>, AppError> {
        // Méthode (a) : `Skilluv-Slice: <uuid>` explicite dans le body
        if let Some(slice_id) = Self::parse_body_marker(&params.pr_body) {
            let slice = sqlx::query_as::<_, crate::models::ProjectSlice>(
                "SELECT * FROM project_slices WHERE id = $1 AND project_id = $2",
            )
            .bind(slice_id)
            .bind(params.project_id)
            .fetch_optional(&mut **tx)
            .await?;
            if slice.is_some() {
                return Ok(slice);
            }
        }

        // Méthode (b) : `Closes #N`, `Fixes #N`, `Resolves #N` dans le body
        for issue_num in Self::parse_closing_keywords(&params.pr_body) {
            let slice = sqlx::query_as::<_, crate::models::ProjectSlice>(
                "SELECT * FROM project_slices
                 WHERE project_id = $1
                   AND slice_type = 'github_issue'
                   AND external_ref = $2
                 LIMIT 1",
            )
            .bind(params.project_id)
            .bind(issue_num.to_string())
            .fetch_optional(&mut **tx)
            .await?;
            if slice.is_some() {
                return Ok(slice);
            }
        }

        // Méthode (c) : best-effort — une seule slice claimed par l'auteur sur ce projet
        let candidate: Option<crate::models::ProjectSlice> = sqlx::query_as(
            "SELECT ps.* FROM project_slices ps
             JOIN github_connections gc ON gc.user_id = ps.claimed_by_user_id
             WHERE ps.project_id = $1
               AND ps.status = 'claimed'
               AND gc.github_login = $2",
        )
        .bind(params.project_id)
        .bind(&params.github_login)
        .fetch_optional(&mut **tx)
        .await?;

        Ok(candidate)
    }

    /// Extrait un UUID du body PR via le marker `Skilluv-Slice: <uuid>` sur sa
    /// propre ligne. Case-insensitive sur le nom du marker.
    fn parse_body_marker(body: &str) -> Option<Uuid> {
        for line in body.lines() {
            let trimmed = line.trim();
            let lower = trimmed.to_lowercase();
            if let Some(rest) = lower.strip_prefix("skilluv-slice:") {
                let candidate = rest.trim();
                if let Ok(uuid) = Uuid::parse_str(candidate) {
                    return Some(uuid);
                }
            }
        }
        None
    }

    /// Extrait tous les numéros d'issue mentionnés via `Closes #N`, `Fixes #N`,
    /// `Resolves #N` (case-insensitive). Retourne un Vec dans l'ordre du body.
    fn parse_closing_keywords(body: &str) -> Vec<i32> {
        const KEYWORDS: &[&str] = &["closes", "fixes", "resolves", "close", "fix", "resolve"];
        let lower = body.to_lowercase();
        let mut result = Vec::new();
        for kw in KEYWORDS {
            let pattern = format!("{kw} #");
            let mut cursor = 0usize;
            while let Some(idx) = lower[cursor..].find(&pattern) {
                let start = cursor + idx + pattern.len();
                let end = lower[start..]
                    .find(|c: char| !c.is_ascii_digit())
                    .map(|off| start + off)
                    .unwrap_or(lower.len());
                if end > start {
                    if let Ok(n) = lower[start..end].parse::<i32>() {
                        if !result.contains(&n) {
                            result.push(n);
                        }
                    }
                }
                cursor = end;
            }
        }
        result
    }

    async fn resolve_github_login(
        tx: &mut Transaction<'_, Postgres>,
        github_login: &str,
    ) -> Result<Option<Uuid>, AppError> {
        let user_id: Option<Uuid> = sqlx::query_scalar(
            "SELECT user_id FROM github_connections WHERE github_login = $1 LIMIT 1",
        )
        .bind(github_login)
        .fetch_optional(&mut **tx)
        .await?;
        Ok(user_id)
    }

    // ═══════════════════════════════════════════════════════════════════
    // Propagation skills (workflow G.2)
    // ═══════════════════════════════════════════════════════════════════

    /// Pour chaque `slice_skills` de la slice donnée, upsert `user_skills` :
    /// - `proven_count` += 1
    /// - `weighted_proven_count` += weight
    /// - Recompute `proficiency_level` via formule log2 (voir models::UserSkill)
    /// - Update `last_proven_at`, ensure `first_proven_at`
    async fn propagate_skills(
        tx: &mut Transaction<'_, Postgres>,
        slice_id: Uuid,
        user_id: Uuid,
        _deliverable_id: Uuid,
    ) -> Result<(), AppError> {
        let slice_skills: Vec<(Uuid, i16)> = sqlx::query_as(
            "SELECT skill_id, weight FROM slice_skills WHERE slice_id = $1",
        )
        .bind(slice_id)
        .fetch_all(&mut **tx)
        .await?;

        for (skill_id, weight) in slice_skills {
            // Upsert user_skills row
            sqlx::query(
                r#"
                INSERT INTO user_skills (
                    user_id, skill_id, proven_count, weighted_proven_count,
                    proficiency_level, first_proven_at, last_proven_at
                )
                VALUES ($1, $2, 1, $3, 1, NOW(), NOW())
                ON CONFLICT (user_id, skill_id) DO UPDATE SET
                    proven_count = user_skills.proven_count + 1,
                    weighted_proven_count = user_skills.weighted_proven_count + $3,
                    last_proven_at = NOW(),
                    first_proven_at = COALESCE(user_skills.first_proven_at, NOW())
                "#,
            )
            .bind(user_id)
            .bind(skill_id)
            .bind(weight as i32)
            .execute(&mut **tx)
            .await?;

            // Recompute proficiency_level en Rust (formule log2 dans UserSkill).
            // On re-lit le WPC pour avoir la valeur post-upsert.
            let wpc: i32 = sqlx::query_scalar(
                "SELECT weighted_proven_count FROM user_skills
                 WHERE user_id = $1 AND skill_id = $2",
            )
            .bind(user_id)
            .bind(skill_id)
            .fetch_one(&mut **tx)
            .await?;

            let new_level = UserSkill::proficiency_level_for(wpc);

            sqlx::query(
                "UPDATE user_skills SET proficiency_level = $1
                 WHERE user_id = $2 AND skill_id = $3",
            )
            .bind(new_level)
            .bind(user_id)
            .bind(skill_id)
            .execute(&mut **tx)
            .await?;

            // P5 auto-issue : le UNIQUE index sur attestations garantit qu'une
            // seconde émission active ne se fera pas, donc c'est safe d'appeler
            // même quand aucun level-up réel n'a eu lieu.
            let _issued = crate::services::AttestationsService::check_and_issue_for_skill_levelup(
                tx, user_id, skill_id, new_level,
            )
            .await?;
        }

        Ok(())
    }

    // ═══════════════════════════════════════════════════════════════════
    // Helpers : construction des JSONB
    // ═══════════════════════════════════════════════════════════════════

    fn build_pr_metadata(params: &PrMergedParams) -> serde_json::Value {
        serde_json::json!({
            "pr_number": params.pr_number,
            "commits_count": params.commits_count,
            "additions": params.additions,
            "deletions": params.deletions,
            "files_changed": params.files_changed,
            "base_branch": params.base_branch,
            "github_login": params.github_login,
        })
    }

    fn build_verification_signal(params: &PrMergedParams) -> serde_json::Value {
        serde_json::json!({
            "source": "github_webhook",
            "event": "pull_request.closed",
            "merged": true,
            "repo": format!("{}/{}", params.repo_owner, params.repo_name),
            "pr_number": params.pr_number,
            "merge_commit_sha": params.merge_commit_sha,
        })
    }

    // ═══════════════════════════════════════════════════════════════════
    // P8.5a : dual-write challenge_submissions → deliverables
    // ═══════════════════════════════════════════════════════════════════

    /// Crée un deliverable "verified" à partir d'un `challenge_submissions.status='success'`.
    ///
    /// Appelé depuis `routes/challenges.rs::submit_challenge` en best-effort après
    /// que la submission legacy soit finalisée avec succès. Le deliverable pointe
    /// vers le challenge (pas de slice) et marque `verifiable_by='automated_diff'`
    /// pour tracer l'origine du pipeline legacy Judge0.
    ///
    /// Idempotent via UNIQUE (user_id, artifact_hash). Si le même hash de
    /// submission a déjà produit un deliverable, retourne l'existant.
    pub async fn create_from_challenge_submission(
        db: &PgPool,
        user_id: Uuid,
        challenge_id: Uuid,
        submission_id: Uuid,
        submission_code: &str,
        fragments_awarded: i32,
        language: Option<&str>,
        stdout: Option<&str>,
        stderr: Option<&str>,
    ) -> Result<Uuid, AppError> {
        use sha2::{Digest, Sha256};

        let mut hasher = Sha256::new();
        hasher.update(submission_code.as_bytes());
        let artifact_hash = hex::encode(hasher.finalize());

        // Format URI interne : la submission n'est pas exposée en HTTP public.
        // Cette convention `skilluv:submission:<uuid>` évite les collisions avec
        // les vrais artifact_url et signale clairement l'origine legacy.
        let artifact_url = format!("skilluv:submission:{submission_id}");

        // P9.1 : embarquer le contenu du code + stdout + stderr dans artifact_metadata
        // pour préserver la preuve après drop des colonnes challenge_submissions.code|*.
        let mut metadata = serde_json::json!({
            "source": "challenge_submission_dual_write",
            "submission_id": submission_id.to_string(),
            "code_content": submission_code,
        });
        if let (Some(obj), Some(lang)) = (metadata.as_object_mut(), language) {
            obj.insert("language".into(), serde_json::Value::String(lang.to_string()));
        }
        if let (Some(obj), Some(s)) = (metadata.as_object_mut(), stdout) {
            obj.insert("stdout".into(), serde_json::Value::String(s.to_string()));
        }
        if let (Some(obj), Some(s)) = (metadata.as_object_mut(), stderr) {
            obj.insert("stderr".into(), serde_json::Value::String(s.to_string()));
        }

        let inserted: Option<Uuid> = sqlx::query_scalar(
            r#"
            INSERT INTO deliverables (
                challenge_id, user_id,
                artifact_type, artifact_url, artifact_hash, artifact_metadata,
                verifiable_by, verification_status, verified_at, verification_signal,
                fragments_awarded, public, submitted_at, created_at
            )
            VALUES (
                $1, $2,
                'other', $3, $4, $5,
                'automated_diff', 'verified', NOW(), $6,
                $7, TRUE, NOW(), NOW()
            )
            ON CONFLICT (user_id, artifact_hash) WHERE artifact_hash IS NOT NULL DO NOTHING
            RETURNING id
            "#,
        )
        .bind(challenge_id)
        .bind(user_id)
        .bind(&artifact_url)
        .bind(&artifact_hash)
        .bind(&metadata)
        .bind(serde_json::json!({
            "source": "legacy_submit_pipeline",
            "submission_id": submission_id.to_string(),
        }))
        .bind(fragments_awarded)
        .fetch_optional(db)
        .await?;

        if let Some(id) = inserted {
            // P19.2 — Best-effort recompute proof engines pour ce user.
            let db_clone = db.clone();
            tokio::spawn(async move {
                let _ = crate::services::proof_hooks::recompute_all_for_user(&db_clone, user_id).await;
            });
            return Ok(id);
        }

        // Idempotence hit : la submission a déjà été convertie en deliverable
        let existing_id: Uuid = sqlx::query_scalar(
            "SELECT id FROM deliverables
             WHERE user_id = $1 AND artifact_hash = $2 LIMIT 1",
        )
        .bind(user_id)
        .bind(&artifact_hash)
        .fetch_one(db)
        .await?;
        Ok(existing_id)
    }

    // ═══════════════════════════════════════════════════════════════════
    // P10.4 : team submission → deliverable partagé
    // ═══════════════════════════════════════════════════════════════════

    /// Crée un deliverable "verified" à partir d'une team submission.
    ///
    /// Sémantique identique à `create_from_challenge_submission` (SHA-256 + idempotence)
    /// avec en plus la matérialisation des contributeurs dans artifact_metadata.
    ///
    /// Le deliverable est rattaché au *team leader* (créateur de la team) comme
    /// `user_id` primaire — les autres contributeurs vivent dans les metadata.
    /// Cette convention permet de garder l'invariant `deliverables.user_id NOT NULL`
    /// tout en traçant l'auteur collectif.
    #[allow(clippy::too_many_arguments)]
    pub async fn create_from_team_submission(
        db: &PgPool,
        team_id: Uuid,
        team_leader_id: Uuid,
        challenge_id: Uuid,
        submission_id: Uuid,
        submission_code: &str,
        contributors: &[TeamContributor],
        language: Option<&str>,
        stdout: Option<&str>,
        stderr: Option<&str>,
    ) -> Result<Uuid, AppError> {
        use sha2::{Digest, Sha256};

        let mut hasher = Sha256::new();
        hasher.update(submission_code.as_bytes());
        // Hash inclut aussi les contributeurs pour éviter que 2 teams différentes
        // avec le même code partagent le hash.
        hasher.update(team_id.as_bytes());
        let artifact_hash = hex::encode(hasher.finalize());

        let artifact_url = format!("skilluv:team_submission:{team_id}:{submission_id}");

        let total_fragments: i32 = contributors.iter().map(|c| c.fragments_awarded).sum();

        let mut metadata = serde_json::json!({
            "source": "team_challenge_submission",
            "team_id": team_id.to_string(),
            "submission_id": submission_id.to_string(),
            "code_content": submission_code,
            "contributors": contributors,
        });
        if let (Some(obj), Some(lang)) = (metadata.as_object_mut(), language) {
            obj.insert("language".into(), serde_json::Value::String(lang.to_string()));
        }
        if let (Some(obj), Some(s)) = (metadata.as_object_mut(), stdout) {
            obj.insert("stdout".into(), serde_json::Value::String(s.to_string()));
        }
        if let (Some(obj), Some(s)) = (metadata.as_object_mut(), stderr) {
            obj.insert("stderr".into(), serde_json::Value::String(s.to_string()));
        }

        let inserted: Option<Uuid> = sqlx::query_scalar(
            r#"
            INSERT INTO deliverables (
                challenge_id, user_id,
                artifact_type, artifact_url, artifact_hash, artifact_metadata,
                verifiable_by, verification_status, verified_at, verification_signal,
                fragments_awarded, public, submitted_at, created_at
            )
            VALUES (
                $1, $2,
                'other', $3, $4, $5,
                'automated_diff', 'verified', NOW(), $6,
                $7, TRUE, NOW(), NOW()
            )
            ON CONFLICT (user_id, artifact_hash) WHERE artifact_hash IS NOT NULL DO NOTHING
            RETURNING id
            "#,
        )
        .bind(challenge_id)
        .bind(team_leader_id)
        .bind(&artifact_url)
        .bind(&artifact_hash)
        .bind(&metadata)
        .bind(serde_json::json!({
            "source": "team_challenge_submission",
            "submission_id": submission_id.to_string(),
            "team_id": team_id.to_string(),
        }))
        .bind(total_fragments)
        .fetch_optional(db)
        .await?;

        if let Some(id) = inserted {
            return Ok(id);
        }

        // Idempotence hit : même code + même team → même deliverable.
        let existing_id: Uuid = sqlx::query_scalar(
            "SELECT id FROM deliverables
             WHERE user_id = $1 AND artifact_hash = $2 LIMIT 1",
        )
        .bind(team_leader_id)
        .bind(&artifact_hash)
        .fetch_one(db)
        .await?;
        Ok(existing_id)
    }

    // ═══════════════════════════════════════════════════════════════════
    // Lectures
    // ═══════════════════════════════════════════════════════════════════

    /// Récupère un deliverable par id.
    pub async fn get(db: &PgPool, id: Uuid) -> Result<Deliverable, AppError> {
        sqlx::query_as::<_, Deliverable>(
            "SELECT * FROM deliverables WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NotFound("Deliverable not found".to_string()))
    }

    /// Liste les deliverables publics vérifiés d'un user (profil public).
    ///
    /// Utilisé pour "voir le portfolio de ce user". Ne retourne PAS les deliverables
    /// pending, rejected, revoked, ou marqués `public=FALSE`.
    pub async fn list_public_by_user(
        db: &PgPool,
        user_id: Uuid,
        limit: i64,
    ) -> Result<Vec<Deliverable>, AppError> {
        let limit = limit.clamp(1, 100);
        let deliverables = sqlx::query_as::<_, Deliverable>(
            r#"
            SELECT * FROM deliverables
            WHERE user_id = $1
              AND public = TRUE
              AND revoked_at IS NULL
              AND verification_status = 'verified'
            ORDER BY submitted_at DESC
            LIMIT $2
            "#,
        )
        .bind(user_id)
        .bind(limit)
        .fetch_all(db)
        .await?;
        Ok(deliverables)
    }
}

// Silence unused imports (BigDecimal + DateTime are used in signature/types only)
#[allow(dead_code)]
fn _unused_marker() {
    let _ = std::mem::size_of::<BigDecimal>();
    let _ = std::mem::size_of::<DateTime<Utc>>();
}
