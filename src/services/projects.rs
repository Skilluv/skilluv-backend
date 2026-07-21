//! Projects service (Phase 2 Sprint 5).
//!
//! Owners are either users or guilds (via owner_type + owner_id discriminator).
//! Discussions reuse the polymorphic `comments` table with `target_type='project'`.

use bigdecimal::BigDecimal;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

pub const VALID_OWNER_TYPES: &[&str] = &["user", "guild"];

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Project {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub repo_url: Option<String>,
    pub demo_url: Option<String>,
    pub tech_stack: Vec<String>,
    pub is_oss: bool,
    pub looking_for_contributors: bool,
    pub owner_type: String,
    pub owner_id: Uuid,
    pub curated_by_admin: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub archived_at: Option<DateTime<Utc>>,
    /// P12.1 : agrégat de domaines exposé pour le matching skills ↔ project.
    /// Rempli par la migration 0055.
    #[serde(default)]
    pub skill_domains: Vec<String>,
    /// P12.1 : score de santé projet (0.0-1.0), pondération du match reco.
    #[serde(default)]
    pub health_score: Option<BigDecimal>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateProjectInput {
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub repo_url: Option<String>,
    pub demo_url: Option<String>,
    pub tech_stack: Vec<String>,
    pub is_oss: bool,
    pub looking_for_contributors: bool,
    pub owner_type: String,
    pub owner_id: Uuid,
}

pub async fn create(
    db: &PgPool,
    requester_id: Uuid,
    requester_role: &str,
    input: CreateProjectInput,
) -> Result<Project, AppError> {
    if !VALID_OWNER_TYPES.contains(&input.owner_type.as_str()) {
        return Err(AppError::Validation(
            "owner_type must be user or guild".into(),
        ));
    }
    if input.name.trim().len() < 2 {
        return Err(AppError::Validation("name too short".into()));
    }
    let slug = input.slug.trim().to_lowercase();
    if slug.len() < 2
        || slug.len() > 80
        || !slug.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err(AppError::Validation(
            "slug must be 2-80 lowercase alphanumeric/dash".into(),
        ));
    }
    // Authorization: user can only create projects they own ; guild projects require
    // the requester to be an officer of that guild.
    match input.owner_type.as_str() {
        "user" => {
            if input.owner_id != requester_id && requester_role != "admin" {
                return Err(AppError::Forbidden);
            }
        }
        "guild" => {
            let role: Option<(String,)> = sqlx::query_as(
                "SELECT role FROM guild_members WHERE guild_id = $1 AND user_id = $2",
            )
            .bind(input.owner_id)
            .bind(requester_id)
            .fetch_optional(db)
            .await?;
            let role = role.map(|(r,)| r);
            let is_officer = matches!(role.as_deref(), Some("founder") | Some("officer"));
            if !is_officer && requester_role != "admin" {
                return Err(AppError::Forbidden);
            }
        }
        _ => unreachable!(),
    }
    let project: Project = sqlx::query_as(
        r#"
        INSERT INTO projects
            (slug, name, description, repo_url, demo_url, tech_stack, is_oss, looking_for_contributors, owner_type, owner_id)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        RETURNING *
        "#,
    )
    .bind(&slug)
    .bind(input.name.trim())
    .bind(input.description.as_deref().map(str::trim))
    .bind(input.repo_url.as_deref().map(str::trim))
    .bind(input.demo_url.as_deref().map(str::trim))
    .bind(&input.tech_stack)
    .bind(input.is_oss)
    .bind(input.looking_for_contributors)
    .bind(&input.owner_type)
    .bind(input.owner_id)
    .fetch_one(db)
    .await?;
    // The owner (when type=user) is auto-listed as a maintainer.
    if project.owner_type == "user" {
        let _ = sqlx::query(
            "INSERT INTO project_contributors (project_id, user_id, role) VALUES ($1, $2, 'maintainer') ON CONFLICT DO NOTHING",
        )
        .bind(project.id)
        .bind(project.owner_id)
        .execute(db)
        .await;
    }
    Ok(project)
}

pub async fn list_for_owner(
    db: &PgPool,
    owner_type: &str,
    owner_id: Uuid,
) -> Result<Vec<Project>, AppError> {
    if !VALID_OWNER_TYPES.contains(&owner_type) {
        return Err(AppError::Validation("invalid owner_type".into()));
    }
    let rows = sqlx::query_as(
        r#"
        SELECT * FROM projects
        WHERE owner_type = $1 AND owner_id = $2 AND archived_at IS NULL
        ORDER BY created_at DESC
        "#,
    )
    .bind(owner_type)
    .bind(owner_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

pub async fn list_looking_for_contributors(
    db: &PgPool,
    limit: i64,
) -> Result<Vec<Project>, AppError> {
    let rows = sqlx::query_as(
        r#"
        SELECT * FROM projects
        WHERE archived_at IS NULL AND looking_for_contributors = TRUE
        ORDER BY created_at DESC
        LIMIT $1
        "#,
    )
    .bind(limit.clamp(1, 100))
    .fetch_all(db)
    .await?;
    Ok(rows)
}

pub async fn list_curated(db: &PgPool, limit: i64) -> Result<Vec<Project>, AppError> {
    let rows = sqlx::query_as(
        r#"
        SELECT * FROM projects
        WHERE archived_at IS NULL AND curated_by_admin = TRUE
        ORDER BY created_at DESC
        LIMIT $1
        "#,
    )
    .bind(limit.clamp(1, 100))
    .fetch_all(db)
    .await?;
    Ok(rows)
}

// ═══════════════════════════════════════════════════════════════════
// P12.1 — Recommandations projets pour un user
// ═══════════════════════════════════════════════════════════════════

/// Recommendation avec le score de match et les domaines qui matchent.
///
/// Le score = somme des WPC du user sur les domaines matchés, pondérée par
/// `health_score` (défaut 0.5 si NULL), + bonus 50% si `looking_for_contributors`.
#[derive(Debug, Clone, Serialize)]
pub struct ProjectRecommendation {
    pub project: Project,
    pub match_score: f64,
    pub matched_domains: Vec<String>,
    pub user_wpc_on_matched_domains: i64,
}

/// Recommande des projets à un user en fonction de ses skills prouvés.
///
/// Algorithme :
/// 1. Aggréger `user_skills.weighted_proven_count` par domaine (via `skill_nodes.domain`).
/// 2. Trouver les projets dont `skill_domains && user_top_domains`.
/// 3. Exclure les projets où le user a déjà un deliverable verified (déjà exploré).
/// 4. Scorer : `sum(user_wpc_on_domain) × coalesce(health_score, 0.5) ×
///             (1.5 si looking_for_contributors else 1.0)`.
/// 5. Retourner top `limit`.
///
/// Retourne un vec vide si le user n'a aucune skill prouvée.
pub async fn recommend_for_user(
    db: &PgPool,
    user_id: Uuid,
    limit: i64,
) -> Result<Vec<ProjectRecommendation>, AppError> {
    let limit = limit.clamp(1, 50);

    // 1. Top domaines du user via WPC agrégé.
    let user_domains: Vec<(String, i64)> = sqlx::query_as(
        r#"
        SELECT sn.domain, SUM(us.weighted_proven_count)::BIGINT AS total_wpc
        FROM user_skills us
        JOIN skill_nodes sn ON sn.id = us.skill_id
        WHERE us.user_id = $1 AND us.weighted_proven_count > 0
        GROUP BY sn.domain
        ORDER BY total_wpc DESC
        "#,
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    if user_domains.is_empty() {
        return Ok(Vec::new());
    }
    let domain_names: Vec<String> = user_domains.iter().map(|(d, _)| d.clone()).collect();
    let domain_wpc: std::collections::HashMap<String, i64> = user_domains.iter().cloned().collect();

    // 2 + 3 : projets candidats, sans ceux où le user a déjà un deliverable verified.
    let projects: Vec<Project> = sqlx::query_as(
        r#"
        SELECT * FROM projects p
        WHERE p.archived_at IS NULL
          AND p.skill_domains && $1::TEXT[]
          AND NOT EXISTS (
              SELECT 1 FROM project_slices ps
              JOIN deliverables d ON d.slice_id = ps.id
              WHERE ps.project_id = p.id
                AND d.user_id = $2
                AND d.verification_status = 'verified'
          )
        "#,
    )
    .bind(&domain_names)
    .bind(user_id)
    .fetch_all(db)
    .await?;

    // 4. Scorer + garder les infos matchées.
    let mut recos: Vec<ProjectRecommendation> = projects
        .into_iter()
        .map(|project| {
            let matched_domains: Vec<String> = project
                .skill_domains
                .iter()
                .filter(|d| domain_wpc.contains_key(*d))
                .cloned()
                .collect();
            let user_wpc_on_matched: i64 = matched_domains
                .iter()
                .filter_map(|d| domain_wpc.get(d).copied())
                .sum();
            let health_f: f64 = project
                .health_score
                .as_ref()
                .and_then(|b| {
                    use num_traits::ToPrimitive;
                    b.to_f64()
                })
                .unwrap_or(0.5);
            let contributor_boost = if project.looking_for_contributors {
                1.5
            } else {
                1.0
            };
            let match_score = (user_wpc_on_matched as f64) * health_f * contributor_boost;
            ProjectRecommendation {
                project,
                match_score,
                matched_domains,
                user_wpc_on_matched_domains: user_wpc_on_matched,
            }
        })
        .collect();

    // 5. Tri + truncate.
    recos.sort_by(|a, b| {
        b.match_score
            .partial_cmp(&a.match_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    recos.truncate(limit as usize);
    Ok(recos)
}

// ═══════════════════════════════════════════════════════════════════
// P12.2 — Marque d'intérêt user → project (onboarding + feed for-you)
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct UserProjectInterest {
    pub user_id: Uuid,
    pub project_id: Uuid,
    pub interest_score: i16,
    pub decided_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Marque un projet comme intéressant pour le user (onboarding step).
/// Score par défaut 50 ; upsert idempotent — un click répété = même score.
pub async fn mark_interested(
    db: &PgPool,
    user_id: Uuid,
    project_id: Uuid,
    score: i16,
) -> Result<UserProjectInterest, AppError> {
    let score = score.clamp(0, 100);
    let row = sqlx::query_as::<_, UserProjectInterest>(
        r#"
        INSERT INTO user_project_interests (user_id, project_id, interest_score)
        VALUES ($1, $2, $3)
        ON CONFLICT (user_id, project_id) DO UPDATE SET
            interest_score = EXCLUDED.interest_score,
            updated_at = NOW()
        RETURNING *
        "#,
    )
    .bind(user_id)
    .bind(project_id)
    .bind(score)
    .fetch_one(db)
    .await?;
    Ok(row)
}

/// Retire l'intérêt (score → 0). Utilisé pour "je n'aime plus ce projet".
pub async fn unmark_interested(
    db: &PgPool,
    user_id: Uuid,
    project_id: Uuid,
) -> Result<u64, AppError> {
    let res = sqlx::query(
        "UPDATE user_project_interests SET interest_score = 0, updated_at = NOW()
         WHERE user_id = $1 AND project_id = $2",
    )
    .bind(user_id)
    .bind(project_id)
    .execute(db)
    .await?;
    Ok(res.rows_affected())
}

/// Batch onboarding : marque plusieurs projets d'un coup.
pub async fn mark_interested_batch(
    db: &PgPool,
    user_id: Uuid,
    project_ids: &[Uuid],
) -> Result<u32, AppError> {
    if project_ids.is_empty() {
        return Ok(0);
    }
    let mut tx = db.begin().await?;
    let mut count: u32 = 0;
    for pid in project_ids {
        sqlx::query(
            r#"
            INSERT INTO user_project_interests (user_id, project_id, interest_score)
            VALUES ($1, $2, 50)
            ON CONFLICT (user_id, project_id) DO UPDATE SET
                interest_score = GREATEST(user_project_interests.interest_score, 50),
                updated_at = NOW()
            "#,
        )
        .bind(user_id)
        .bind(pid)
        .execute(&mut *tx)
        .await?;
        count += 1;
    }
    tx.commit().await?;
    Ok(count)
}

/// Un projet enrichi de l'interest_score du user courant.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ProjectWithInterest {
    #[sqlx(flatten)]
    pub project: Project,
    pub interest_score: i16,
}

/// Liste les projets d'intérêt d'un user, triés par score DESC puis récence.
pub async fn list_interests(
    db: &PgPool,
    user_id: Uuid,
) -> Result<Vec<ProjectWithInterest>, AppError> {
    let rows = sqlx::query_as::<_, ProjectWithInterest>(
        r#"
        SELECT p.*, upi.interest_score
        FROM user_project_interests upi
        JOIN projects p ON p.id = upi.project_id
        WHERE upi.user_id = $1 AND upi.interest_score > 0
          AND p.archived_at IS NULL
        ORDER BY upi.interest_score DESC, upi.updated_at DESC
        "#,
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

pub async fn by_slug(db: &PgPool, slug: &str) -> Result<Project, AppError> {
    let row: Option<Project> =
        sqlx::query_as("SELECT * FROM projects WHERE slug = $1 AND archived_at IS NULL")
            .bind(slug)
            .fetch_optional(db)
            .await?;
    row.ok_or(AppError::NotFound("project not found".into()))
}

pub async fn add_contributor(
    db: &PgPool,
    project_id: Uuid,
    requester_id: Uuid,
    requester_role: &str,
    contributor_id: Uuid,
    role: &str,
) -> Result<(), AppError> {
    let project = sqlx::query_as::<_, Project>(
        "SELECT * FROM projects WHERE id = $1 AND archived_at IS NULL",
    )
    .bind(project_id)
    .fetch_optional(db)
    .await?
    .ok_or(AppError::NotFound("project not found".into()))?;
    check_maintainer(db, &project, requester_id, requester_role).await?;
    if !matches!(role, "maintainer" | "contributor") {
        return Err(AppError::Validation(
            "role must be maintainer or contributor".into(),
        ));
    }
    sqlx::query(
        "INSERT INTO project_contributors (project_id, user_id, role) VALUES ($1, $2, $3) ON CONFLICT (project_id, user_id) DO UPDATE SET role = EXCLUDED.role",
    )
    .bind(project_id)
    .bind(contributor_id)
    .bind(role)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn remove_contributor(
    db: &PgPool,
    project_id: Uuid,
    requester_id: Uuid,
    requester_role: &str,
    contributor_id: Uuid,
) -> Result<(), AppError> {
    let project = sqlx::query_as::<_, Project>(
        "SELECT * FROM projects WHERE id = $1 AND archived_at IS NULL",
    )
    .bind(project_id)
    .fetch_optional(db)
    .await?
    .ok_or(AppError::NotFound("project not found".into()))?;
    check_maintainer(db, &project, requester_id, requester_role).await?;
    sqlx::query("DELETE FROM project_contributors WHERE project_id = $1 AND user_id = $2")
        .bind(project_id)
        .bind(contributor_id)
        .execute(db)
        .await?;
    Ok(())
}

pub async fn list_contributors(
    db: &PgPool,
    project_id: Uuid,
) -> Result<Vec<ProjectContributor>, AppError> {
    let rows = sqlx::query_as(
        r#"
        SELECT pc.project_id, pc.user_id, pc.role, pc.commits_count, pc.added_at, u.username, u.display_name
        FROM project_contributors pc
        JOIN users u ON u.id = pc.user_id
        WHERE pc.project_id = $1
        ORDER BY pc.role, pc.added_at
        "#,
    )
    .bind(project_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ProjectContributor {
    pub project_id: Uuid,
    pub user_id: Uuid,
    pub role: String,
    pub commits_count: i32,
    pub added_at: DateTime<Utc>,
    pub username: String,
    pub display_name: String,
}

pub async fn admin_set_curated(
    db: &PgPool,
    project_id: Uuid,
    curated: bool,
) -> Result<(), AppError> {
    sqlx::query("UPDATE projects SET curated_by_admin = $1, updated_at = NOW() WHERE id = $2")
        .bind(curated)
        .bind(project_id)
        .execute(db)
        .await?;
    Ok(())
}

pub async fn archive(
    db: &PgPool,
    project_id: Uuid,
    requester_id: Uuid,
    requester_role: &str,
) -> Result<(), AppError> {
    let project = sqlx::query_as::<_, Project>(
        "SELECT * FROM projects WHERE id = $1 AND archived_at IS NULL",
    )
    .bind(project_id)
    .fetch_optional(db)
    .await?
    .ok_or(AppError::NotFound("project not found".into()))?;
    check_maintainer(db, &project, requester_id, requester_role).await?;
    sqlx::query("UPDATE projects SET archived_at = NOW() WHERE id = $1")
        .bind(project_id)
        .execute(db)
        .await?;
    Ok(())
}

async fn check_maintainer(
    db: &PgPool,
    project: &Project,
    requester_id: Uuid,
    requester_role: &str,
) -> Result<(), AppError> {
    if requester_role == "admin" {
        return Ok(());
    }
    match project.owner_type.as_str() {
        "user" => {
            if project.owner_id == requester_id {
                return Ok(());
            }
        }
        "guild" => {
            let role: Option<(String,)> = sqlx::query_as(
                "SELECT role FROM guild_members WHERE guild_id = $1 AND user_id = $2",
            )
            .bind(project.owner_id)
            .bind(requester_id)
            .fetch_optional(db)
            .await?;
            if matches!(
                role.map(|(r,)| r).as_deref(),
                Some("founder") | Some("officer")
            ) {
                return Ok(());
            }
        }
        _ => {}
    }
    let pc: Option<(String,)> = sqlx::query_as(
        "SELECT role FROM project_contributors WHERE project_id = $1 AND user_id = $2",
    )
    .bind(project.id)
    .bind(requester_id)
    .fetch_optional(db)
    .await?;
    if matches!(pc.map(|(r,)| r).as_deref(), Some("maintainer")) {
        return Ok(());
    }
    Err(AppError::Forbidden)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_types_accepted() {
        assert!(VALID_OWNER_TYPES.contains(&"user"));
        assert!(VALID_OWNER_TYPES.contains(&"guild"));
        assert_eq!(VALID_OWNER_TYPES.len(), 2);
    }
}
