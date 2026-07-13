//! P11 — Ingestion automatique de `project_slices` depuis des sources externes.
//!
//! Rôle : abstraire "un projet Skilluv veut détecter automatiquement les
//! nouvelles unités de travail à claimer" (issues GitHub curées, frames Figma,
//! etc.) et matérialiser ça en `project_slices`.
//!
//! Design :
//! - Un `trait SliceIngestor` normalise l'interface.
//! - `GitHubIngestor` implémente le pattern pour les issues GitHub avec labels
//!   curés — la seule impl live en P11. Les autres (Figma, Notion) sont des
//!   stubs futurs.
//! - Le worker `bin/github_ingest.rs` boucle sur tous les projets éligibles
//!   et appelle l'ingestor correspondant. Idempotent via
//!   `uniq_slices_github_issue_per_project` UNIQUE index.

use async_trait::async_trait;
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

const GITHUB_API: &str = "https://api.github.com";
const USER_AGENT: &str = "skilluv-backend/1.0";

/// Rapport d'ingestion pour un projet donné.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct IngestReport {
    pub project_id: Uuid,
    pub slices_created: u32,
    pub slices_skipped_duplicate: u32,
    pub errors: u32,
}

/// Trait générique : chaque source (GitHub, Figma…) implémente sa logique
/// d'ingestion pour un projet donné.
#[async_trait]
pub trait SliceIngestor: Send + Sync {
    async fn ingest_for_project(
        &self,
        db: &PgPool,
        project_id: Uuid,
    ) -> Result<IngestReport, AppError>;

    /// Nom court pour logs + metrics.
    fn name(&self) -> &'static str;
}

// ═══════════════════════════════════════════════════════════════════
// Implémentation GitHub — issues avec labels curés
// ═══════════════════════════════════════════════════════════════════

pub struct GitHubIngestor;

#[derive(Debug, Deserialize)]
struct GithubIssue {
    number: i32,
    title: String,
    body: Option<String>,
    html_url: String,
    #[serde(default)]
    labels: Vec<GithubLabel>,
    #[serde(default)]
    pull_request: Option<serde_json::Value>, // Present si issue est un PR — on skip.
}

#[derive(Debug, Deserialize)]
struct GithubLabel {
    name: String,
}

/// Charge les colonnes projet nécessaires à l'ingestion GitHub.
#[derive(Debug, sqlx::FromRow)]
struct ProjectIngestRow {
    github_repo_owner: Option<String>,
    github_repo_name: Option<String>,
    curated_labels: Vec<String>,
    slice_ingestion_mode: String,
}

#[async_trait]
impl SliceIngestor for GitHubIngestor {
    fn name(&self) -> &'static str {
        "github"
    }

    async fn ingest_for_project(
        &self,
        db: &PgPool,
        project_id: Uuid,
    ) -> Result<IngestReport, AppError> {
        let mut report = IngestReport {
            project_id,
            ..Default::default()
        };

        let project: ProjectIngestRow = sqlx::query_as(
            r#"
            SELECT github_repo_owner, github_repo_name, curated_labels, slice_ingestion_mode
            FROM projects
            WHERE id = $1
            "#,
        )
        .bind(project_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NotFound("Project not found".into()))?;

        let (Some(owner), Some(name)) = (
            project.github_repo_owner.as_deref(),
            project.github_repo_name.as_deref(),
        ) else {
            return Ok(report); // Pas de repo GitHub configuré : no-op.
        };

        if project.slice_ingestion_mode == "manual_only" {
            return Ok(report);
        }
        if project.curated_labels.is_empty() {
            return Ok(report); // Rien à écouter.
        }

        // Le status d'insertion dépend du mode. `auto` = publie directement.
        // `curator_review` = draft, steward valide via P11.4.
        let default_status = if project.slice_ingestion_mode == "auto" {
            "open"
        } else {
            "draft"
        };

        let issues = fetch_open_issues(owner, name, &project.curated_labels).await?;

        for issue in issues {
            if issue.pull_request.is_some() {
                continue; // GitHub renvoie les PR via /issues, on skip.
            }
            match insert_slice_from_issue(db, project_id, default_status, &issue).await {
                Ok(true) => report.slices_created += 1,
                Ok(false) => report.slices_skipped_duplicate += 1,
                Err(e) => {
                    tracing::warn!(
                        project_id = %project_id, issue = issue.number, error = %e,
                        "slice ingest insert failed"
                    );
                    report.errors += 1;
                }
            }
        }

        Ok(report)
    }
}

/// Interroge l'API GitHub public (no token) pour lister les issues open
/// avec au moins un des `curated_labels`. Sans token, rate-limit 60/h par IP —
/// suffisant pour un poll horaire de quelques dizaines de projets.
async fn fetch_open_issues(
    owner: &str,
    name: &str,
    curated_labels: &[String],
) -> Result<Vec<GithubIssue>, AppError> {
    // GitHub accepte plusieurs labels séparés par virgule → OR logique.
    let labels_csv = curated_labels.join(",");
    let url = format!(
        "{GITHUB_API}/repos/{owner}/{name}/issues?state=open&per_page=100&labels={labels_csv}"
    );

    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("github issues fetch failed: {e}")))?;

    if !resp.status().is_success() {
        return Err(AppError::Internal(format!(
            "github /issues status {} for {owner}/{name}",
            resp.status()
        )));
    }

    let issues: Vec<GithubIssue> = resp
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("github issues decode failed: {e}")))?;
    Ok(issues)
}

/// INSERT ON CONFLICT : true si nouveau, false si duplicate.
async fn insert_slice_from_issue(
    db: &PgPool,
    project_id: Uuid,
    default_status: &str,
    issue: &GithubIssue,
) -> Result<bool, AppError> {
    let title = truncate(&issue.title, 300);
    let description = truncate(
        issue.body.as_deref().unwrap_or("(no description)"),
        4000,
    );
    let labels: Vec<String> = issue.labels.iter().map(|l| l.name.clone()).collect();
    let metadata = serde_json::json!({
        "source": "github_polling",
        "issue_url": issue.html_url,
        "issue_number": issue.number,
        "labels": labels,
    });

    let inserted: Option<Uuid> = sqlx::query_scalar(
        r#"
        INSERT INTO project_slices
            (project_id, slice_type, external_ref, external_metadata,
             title, description,
             primary_domain, difficulty, fragments_reward,
             status, ingested_from)
        VALUES ($1, 'github_issue', $2, $3,
                $4, $5,
                'code', 3, 50,
                $6, 'github_webhook')
        ON CONFLICT (project_id, external_ref)
            WHERE slice_type = 'github_issue' AND external_ref IS NOT NULL
            DO NOTHING
        RETURNING id
        "#,
    )
    .bind(project_id)
    .bind(issue.number.to_string())
    .bind(&metadata)
    .bind(&title)
    .bind(&description)
    .bind(default_status)
    .fetch_optional(db)
    .await?;

    Ok(inserted.is_some())
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        // char_indices pour ne pas couper au milieu d'un char UTF-8.
        let cut = s
            .char_indices()
            .take_while(|(i, _)| *i < max.saturating_sub(1))
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(max);
        format!("{}…", &s[..cut])
    }
}

// ═══════════════════════════════════════════════════════════════════
// Fonction utilitaire — parcourt tous les projets éligibles
// ═══════════════════════════════════════════════════════════════════

/// Poll tous les projets en mode `auto` ou `curator_review` qui ont un repo
/// GitHub configuré et au moins un curated_label. Retourne le rapport agrégé.
pub async fn poll_all_github_projects(
    db: &PgPool,
) -> Result<Vec<IngestReport>, AppError> {
    let projects: Vec<(Uuid,)> = sqlx::query_as(
        r#"
        SELECT id FROM projects
        WHERE archived_at IS NULL
          AND slice_ingestion_mode IN ('auto', 'curator_review')
          AND github_repo_owner IS NOT NULL
          AND github_repo_name IS NOT NULL
          AND array_length(curated_labels, 1) > 0
        ORDER BY id
        "#,
    )
    .fetch_all(db)
    .await?;

    let ingestor = GitHubIngestor;
    let mut reports = Vec::with_capacity(projects.len());
    for (project_id,) in projects {
        match ingestor.ingest_for_project(db, project_id).await {
            Ok(report) => {
                if report.slices_created > 0 {
                    metrics::counter!(
                        "skilluv_github_slices_ingested_total",
                        "project" => project_id.to_string()
                    )
                    .increment(report.slices_created as u64);
                }
                reports.push(report);
            }
            Err(e) => {
                tracing::warn!(
                    project_id = %project_id, error = %e,
                    "poll_all_github_projects: ingest_for_project failed"
                );
                reports.push(IngestReport {
                    project_id,
                    errors: 1,
                    ..Default::default()
                });
            }
        }
    }
    Ok(reports)
}
