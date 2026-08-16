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
    /// Total issues fetched from the source (post-filter, incl. PRs skipped).
    /// SKI-110 exposes this on the manual-trigger endpoint so an admin can
    /// tell "config wrong, 0 issues match" apart from "config right, everything
    /// already ingested".
    pub issues_seen: u32,
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
///
/// P26 v2 SKI-101: `skill_domains` is the ordered list of the project's
/// primary domains (e.g. `['code','ops']` for skilluv-backend). The first
/// entry is used as the fallback `primary_domain` when an incoming issue
/// carries no explicit `domain:*` label.
#[derive(Debug, sqlx::FromRow)]
struct ProjectIngestRow {
    github_repo_owner: Option<String>,
    github_repo_name: Option<String>,
    curated_labels: Vec<String>,
    slice_ingestion_mode: String,
    #[allow(dead_code)] // used indirectly via `default_domain` picking below.
    skill_domains: Vec<String>,
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
            SELECT github_repo_owner, github_repo_name, curated_labels,
                   slice_ingestion_mode, skill_domains
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
        report.issues_seen = issues.len() as u32;

        // P26 v2 SKI-101: pick the project's first skill_domain as the
        // fallback domain for issues that don't carry a `domain:*` label.
        // Empty → "code" (matches pre-SKI-101 hardcoded behaviour so
        // legacy projects without skill_domains still ingest identically).
        let default_domain = project
            .skill_domains
            .first()
            .cloned()
            .unwrap_or_else(|| "code".to_string());

        for issue in issues {
            if issue.pull_request.is_some() {
                continue; // GitHub renvoie les PR via /issues, on skip.
            }
            match insert_slice_from_issue(db, project_id, default_status, &default_domain, &issue)
                .await
            {
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

/// Which trade an ingested issue belongs to, read from the labels the
/// maintainers put on it.
///
/// `trigger` is the label that caused the ingestion, when there is one — a
/// webhook knows which label was just added, a polling cycle does not. It
/// wins over the rest, because it is the most recent statement anybody made
/// about that issue.
///
/// Otherwise the issue is typed only when the mapped labels agree. Two curated
/// labels pointing at two different trades is a contradiction on the upstream
/// repository, and guessing between them would credit somebody with a
/// speciality they may never have worked in. An untyped slice is honest;
/// a wrongly typed one is not.
pub async fn orientation_for_labels(
    db: &PgPool,
    project_id: Uuid,
    labels: &[String],
    trigger: Option<&str>,
) -> Result<Option<Uuid>, AppError> {
    if let Some(trigger) = trigger {
        let direct: Option<Uuid> = sqlx::query_scalar(
            "SELECT orientation_id FROM project_label_orientations
              WHERE project_id = $1 AND label = $2",
        )
        .bind(project_id)
        .bind(trigger)
        .fetch_optional(db)
        .await?;
        if direct.is_some() {
            return Ok(direct);
        }
    }

    if labels.is_empty() {
        return Ok(None);
    }

    let agreed: Vec<Uuid> = sqlx::query_scalar(
        "SELECT DISTINCT orientation_id FROM project_label_orientations
          WHERE project_id = $1 AND label = ANY($2)",
    )
    .bind(project_id)
    .bind(labels)
    .fetch_all(db)
    .await?;

    match agreed.as_slice() {
        [single] => Ok(Some(*single)),
        _ => Ok(None),
    }
}

/// INSERT ON CONFLICT : true si nouveau, false si duplicate.
///
/// P26 v2 SKI-101 — `default_domain` is the project's fallback (usually
/// `projects.skill_domains[0]`). The enricher parses labels and body to
/// derive `primary_domain`, `difficulty` and `acceptance_criteria`; only
/// values it cannot infer fall back to the defaults.
///
/// `fragments_reward` scales with the derived difficulty: 30/40/50/70/100
/// for difficulties 1..5. Rationale: harder challenges deserve more
/// reward without being disproportionate to the mid-tier baseline (kept
/// at 50 for difficulty=3, matching pre-SKI-101 behaviour).
async fn insert_slice_from_issue(
    db: &PgPool,
    project_id: Uuid,
    default_status: &str,
    default_domain: &str,
    issue: &GithubIssue,
) -> Result<bool, AppError> {
    let title = truncate(&issue.title, 300);
    let description = truncate(issue.body.as_deref().unwrap_or("(no description)"), 4000);
    let labels: Vec<String> = issue.labels.iter().map(|l| l.name.clone()).collect();

    let enriched = crate::services::slice_enrichment::enrich_from_issue(
        &labels,
        issue.body.as_deref(),
        default_domain,
    );
    let fragments_reward: i32 = match enriched.difficulty {
        1 => 30,
        2 => 40,
        3 => 50,
        4 => 70,
        5 => 100,
        _ => 50,
    };

    let metadata = serde_json::json!({
        "source": "github_polling",
        "issue_url": issue.html_url,
        "issue_number": issue.number,
        "labels": labels,
        "enrichment": {
            "domain_source": if labels
                .iter()
                .any(|l| l.to_lowercase().starts_with("domain:"))
            {
                "label"
            } else {
                "project_default"
            },
        },
    });

    let orientation_id = orientation_for_labels(db, project_id, &labels, None).await?;

    let inserted: Option<Uuid> = sqlx::query_scalar(
        r#"
        INSERT INTO project_slices
            (project_id, slice_type, external_ref, external_metadata,
             title, description, acceptance_criteria,
             primary_domain, difficulty, fragments_reward,
             status, ingested_from, orientation_id)
        VALUES ($1, 'github_issue', $2, $3,
                $4, $5, $6,
                $7, $8, $9,
                $10, 'github_webhook', $11)
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
    .bind(&enriched.acceptance_criteria)
    .bind(&enriched.primary_domain)
    .bind(enriched.difficulty)
    .bind(fragments_reward)
    .bind(default_status)
    .bind(orientation_id)
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
// P11.3 — Stubs pour futures sources (Figma, Notion, partenaires)
// ═══════════════════════════════════════════════════════════════════

/// Stub Figma : futur ingestor qui lira les nouveaux frames d'un projet Figma
/// et les matérialisera comme `project_slices` de type `figma_frame`. En
/// attente de l'API Figma OAuth (post-P13). No-op actuellement.
pub struct FigmaIngestor;

#[async_trait]
impl SliceIngestor for FigmaIngestor {
    fn name(&self) -> &'static str {
        "figma"
    }

    async fn ingest_for_project(
        &self,
        _db: &PgPool,
        project_id: Uuid,
    ) -> Result<IngestReport, AppError> {
        tracing::debug!(project_id = %project_id, "FigmaIngestor is a stub — no-op");
        Ok(IngestReport {
            project_id,
            ..Default::default()
        })
    }
}

/// Dispatcher générique : appelle chaque ingestor pour un projet donné.
///
/// Le worker `bin/github_ingest.rs` utilise uniquement `GitHubIngestor` en P11 ;
/// cette fn est là pour prouver que le pattern scale à N sources sans coupler
/// tout au GitHub. Un futur worker pourra faire :
///
///   let ingestors: Vec<Box<dyn SliceIngestor>> = vec![
///       Box::new(GitHubIngestor),
///       Box::new(FigmaIngestor),
///   ];
///   let reports = dispatch_ingestors(&ingestors, &db, project_id).await;
pub async fn dispatch_ingestors(
    ingestors: &[Box<dyn SliceIngestor>],
    db: &PgPool,
    project_id: Uuid,
) -> Vec<Result<IngestReport, AppError>> {
    let mut out = Vec::with_capacity(ingestors.len());
    for ing in ingestors {
        out.push(ing.ingest_for_project(db, project_id).await);
    }
    out
}

// ═══════════════════════════════════════════════════════════════════
// Fonction utilitaire — parcourt tous les projets éligibles
// ═══════════════════════════════════════════════════════════════════

/// Poll tous les projets en mode `auto` ou `curator_review` qui ont un repo
/// GitHub configuré et au moins un curated_label. Retourne le rapport agrégé.
pub async fn poll_all_github_projects(db: &PgPool) -> Result<Vec<IngestReport>, AppError> {
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
