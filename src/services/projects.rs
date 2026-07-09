//! Projects service (Phase 2 Sprint 5).
//!
//! Owners are either users or guilds (via owner_type + owner_id discriminator).
//! Discussions reuse the polymorphic `comments` table with `target_type='project'`.

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
        return Err(AppError::Validation("owner_type must be user or guild".into()));
    }
    if input.name.trim().len() < 2 {
        return Err(AppError::Validation("name too short".into()));
    }
    let slug = input.slug.trim().to_lowercase();
    if slug.len() < 2 || slug.len() > 80 || !slug.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
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
    sqlx::query(
        "UPDATE projects SET curated_by_admin = $1, updated_at = NOW() WHERE id = $2",
    )
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
            if matches!(role.map(|(r,)| r).as_deref(), Some("founder") | Some("officer")) {
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
