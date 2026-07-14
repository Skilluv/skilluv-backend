//! P18.2 — Rules engine capabilities (auto-promotion sur activité mesurable).
//!
//! Contrat : `recompute_capabilities_for_user(user_id)` évalue chaque règle
//! auto-promotable et INSERT la capability si le seuil est atteint et qu'elle
//! n'est pas déjà active. Ne révoque pas automatiquement (les capabilities
//! restent gagnées, comme le rank — décision produit alignée avec P17.4).
//!
//! Seuils par défaut (spec discussion produit, memory `project_p17_completion`) :
//!   - challenger      : tout user inscrit
//!   - mentor          : >= 5 attestations reçues OU >= 3 mentorship_sessions
//!                       en tant que mentor
//!   - pr_reviewer     : >= 10 PR reviewed approuvées (via reviews table)
//!   - issue_proposer  : >= 3 propositions communauté acceptées
//!                       (challenge_templates.is_community=TRUE, status='published',
//!                        created_by=user)
//!   - bounty_funder   : manual-only (funding actuel via project_slices.funder_enterprise_id, côté enterprise)
//!   - project_steward : owner d'au moins 1 project non-archived
//!
//! Non-automatiques (attribution manuelle uniquement) :
//!   - admin, jury_tournament, enterprise_recruiter

use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

#[derive(Debug, Clone)]
pub struct RecomputeCapReport {
    pub granted: Vec<String>,
    pub already_active: Vec<String>,
}

pub async fn recompute_capabilities_for_user(
    db: &PgPool,
    user_id: Uuid,
) -> Result<RecomputeCapReport, AppError> {
    let mut granted = Vec::new();
    let mut already = Vec::new();

    // Défaut universel : challenger.
    grant_if_missing(db, user_id, "challenger", "auto:default", &mut granted, &mut already).await?;

    // Mentor : 5 attestations reçues OU 3 sessions mentor (best-effort si
    // les tables existent).
    let attests: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM attestations WHERE user_id = $1 AND revoked_at IS NULL",
    )
    .bind(user_id).fetch_one(db).await.unwrap_or(0);
    let sessions: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM mentorship_sessions WHERE mentor_user_id = $1",
    )
    .bind(user_id).fetch_optional(db).await.unwrap_or(None).unwrap_or(0);
    if attests >= 5 || sessions >= 3 {
        grant_if_missing(db, user_id, "mentor",
            &format!("auto:threshold(attests={attests}, sessions={sessions})"),
            &mut granted, &mut already).await?;
    }

    // pr_reviewer : 10 reviews approuvées (via reviews table verdict='approved').
    let reviews: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM reviews WHERE reviewer_user_id = $1 AND verdict = 'approve'",
    )
    .bind(user_id).fetch_optional(db).await.unwrap_or(None).unwrap_or(0);
    if reviews >= 10 {
        grant_if_missing(db, user_id, "pr_reviewer",
            &format!("auto:threshold(approved_reviews={reviews})"),
            &mut granted, &mut already).await?;
    }

    // issue_proposer : 3 templates créés par ce user, is_community, published.
    let props: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM challenge_templates
         WHERE created_by = $1 AND is_community = TRUE AND status = 'published'",
    )
    .bind(user_id).fetch_optional(db).await.unwrap_or(None).unwrap_or(0);
    if props >= 3 {
        grant_if_missing(db, user_id, "issue_proposer",
            &format!("auto:threshold(published_proposals={props})"),
            &mut granted, &mut already).await?;
    }

    // bounty_funder est manual-only : le funding réel des bounties dans le
    // modèle actuel passe par project_slices.funder_enterprise_id (côté
    // enterprise, pas user). Pas de règle auto ici.

    // project_steward : owner (owner_type='user') d'au moins 1 project non archivé.
    let owned_projects: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM projects
         WHERE owner_type = 'user' AND owner_id = $1 AND archived_at IS NULL",
    )
    .bind(user_id).fetch_optional(db).await.unwrap_or(None).unwrap_or(0);
    if owned_projects >= 1 {
        grant_if_missing(db, user_id, "project_steward",
            &format!("auto:threshold(owned_projects={owned_projects})"),
            &mut granted, &mut already).await?;
    }

    Ok(RecomputeCapReport { granted, already_active: already })
}

async fn grant_if_missing(
    db: &PgPool,
    user_id: Uuid,
    capability: &str,
    reason: &str,
    granted: &mut Vec<String>,
    already: &mut Vec<String>,
) -> Result<(), AppError> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM user_capabilities
            WHERE user_id = $1 AND capability = $2 AND revoked_at IS NULL
        )",
    )
    .bind(user_id).bind(capability)
    .fetch_one(db).await?;
    if exists {
        already.push(capability.to_string());
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO user_capabilities (user_id, capability, granted_reason)
         VALUES ($1, $2, $3)
         ON CONFLICT DO NOTHING",
    )
    .bind(user_id).bind(capability).bind(reason)
    .execute(db).await?;
    granted.push(capability.to_string());
    Ok(())
}
