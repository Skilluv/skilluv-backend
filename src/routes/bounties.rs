//! OSS Bounties — flow (P9.2 : entirely backed by `project_slices`).
//!
//! Historique : originellement 2 tables dédiées `oss_bounties` + `oss_bounty_claims`
//! (Phase 5.6). Depuis P9.2 les bounties sont un cas particulier de `project_slice`
//! avec `funder_enterprise_id NOT NULL` et `credits_reward > 0` — les colonnes
//! `pr_url/pr_number/pr_submitted_at/merged_at/paid_at` sur project_slices
//! remplacent l'ancien oss_bounty_claims.
//!
//! Flow inchangé côté API publique :
//!   1. Entreprise POST /api/bounties (crédits débités séquestre)
//!   2. Talent POST /api/bounties/{id}/claim
//!   3. Talent POST /api/bounties/{id}/pr {url, number}
//!   4. Webhook GitHub `pull_request.closed` merged=true → payout auto
//!      (crédits séquestrés → transférés au talent + bonus fragments)

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use bigdecimal::BigDecimal;
use hmac::{Hmac, KeyInit, Mac};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::Sha256;
use sqlx::Row;
use std::str::FromStr;
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::AuthUser;

type HmacSha256 = Hmac<Sha256>;

pub fn bounty_routes() -> Router<AppState> {
    Router::new()
        .route("/bounties", get(list_bounties).post(create_bounty))
        .route("/bounties/{id}", get(get_bounty))
        .route("/bounties/{id}/claim", post(claim_bounty))
        .route("/bounties/{id}/pr", post(submit_pr))
        .route("/bounties/{id}/cancel", post(cancel_bounty))
        .route("/webhooks/github", post(github_webhook))
}

fn build_response(data: Value) -> Value {
    json!({
        "data": data,
        "meta": {
            "request_id": Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    })
}

async fn current_enterprise_for(db: &sqlx::PgPool, user_id: Uuid) -> Result<Uuid, AppError> {
    let row: Option<(Uuid,)> = sqlx::query_as(
        "SELECT enterprise_id FROM enterprise_members WHERE user_id = $1 AND status = 'active' LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(db)
    .await?;
    row.map(|(id,)| id).ok_or(AppError::Forbidden)
}

/// Résout ou crée un project miroir du repo GitHub. Si un project existe déjà
/// avec le même (owner, name), on le réutilise. Sinon on en crée un attaché au
/// user posteur (owner_type='user'), qui pourra être re-attribué à un guild par
/// un steward plus tard. Simplifie la création B2B : plus besoin de créer un
/// project au préalable via l'admin UI.
async fn resolve_or_create_project(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    repo_owner: &str,
    repo_name: &str,
    posted_by: Uuid,
) -> Result<Uuid, AppError> {
    if let Some(project_id) = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM projects
         WHERE github_repo_owner = $1 AND github_repo_name = $2
         LIMIT 1",
    )
    .bind(repo_owner)
    .bind(repo_name)
    .fetch_optional(&mut **tx)
    .await?
    {
        return Ok(project_id);
    }

    // Slug basé sur repo + suffixe court pour éviter collisions si plusieurs
    // enterprises créent une bounty sur des forks du même owner/name — le slug
    // est UNIQUE.
    let slug = format!(
        "{}-{}-{}",
        repo_owner.to_lowercase(),
        repo_name.to_lowercase(),
        &Uuid::new_v4().to_string()[..8]
    );
    let project_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO projects
            (slug, name, description, github_repo_owner, github_repo_name,
             owner_type, owner_id)
        VALUES ($1, $2, $3, $4, $5, 'user', $6)
        RETURNING id
        "#,
    )
    .bind(&slug)
    .bind(format!("{repo_owner}/{repo_name}"))
    .bind(format!(
        "Auto-created for bounty on {repo_owner}/{repo_name}"
    ))
    .bind(repo_owner)
    .bind(repo_name)
    .bind(posted_by)
    .fetch_one(&mut **tx)
    .await?;

    metrics::counter!("skilluv_bounty_projects_auto_created_total").increment(1);
    Ok(project_id)
}

/// Renvoie une valeur JSON depuis l'external_metadata d'une slice, avec un
/// fallback vide. Sert à extraire les tags/required_skills/issue_url historiques.
fn meta_str(meta: &Value, key: &str) -> String {
    meta.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

fn meta_array(meta: &Value, key: &str) -> Vec<String> {
    meta.get(key)
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

// ─── Listing ─────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ListQuery {
    status: Option<String>,
    skill: Option<String>,
    tag: Option<String>,
    page: Option<i64>,
    per_page: Option<i64>,
}

async fn list_bounties(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Value>, AppError> {
    let per_page = q.per_page.unwrap_or(20).clamp(1, 100);
    let page = q.page.unwrap_or(1).max(1);
    let offset = (page - 1) * per_page;
    let status = q.status.unwrap_or_else(|| "open".into());

    let rows = sqlx::query(
        r#"
        SELECT ps.id, ps.title, ps.description,
               p.github_repo_owner, p.github_repo_name,
               ps.external_ref, ps.external_metadata,
               ps.credits_reward::TEXT AS credits_reward,
               ps.fragments_reward, ps.difficulty, ps.status,
               ps.claim_expires_at, ps.created_at,
               e.company_name
        FROM project_slices ps
        JOIN projects p ON p.id = ps.project_id
        LEFT JOIN enterprises e ON e.id = ps.funder_enterprise_id
        WHERE ps.status = $1
          AND ps.funder_enterprise_id IS NOT NULL
          AND ($2::TEXT IS NULL OR ps.external_metadata->'required_skills' ? $2)
          AND ($3::TEXT IS NULL OR ps.external_metadata->'tags' ? $3)
        ORDER BY ps.created_at DESC
        LIMIT $4 OFFSET $5
        "#,
    )
    .bind(&status)
    .bind(&q.skill)
    .bind(&q.tag)
    .bind(per_page)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            let meta: Value = r
                .try_get::<Value, _>("external_metadata")
                .unwrap_or(Value::Null);
            let repo_owner: String = r.get("github_repo_owner");
            let repo_name: String = r.get("github_repo_name");
            let issue_number = r
                .try_get::<Option<String>, _>("external_ref")
                .ok()
                .flatten()
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(0);
            json!({
                "id": r.get::<Uuid, _>("id"),
                "title": r.get::<String, _>("title"),
                "description": r.get::<String, _>("description"),
                "repo": format!("{repo_owner}/{repo_name}"),
                "issue_url": meta_str(&meta, "issue_url"),
                "issue_number": issue_number,
                "reward_credits": r.get::<String, _>("credits_reward"),
                "fragments_bonus": r.get::<i32, _>("fragments_reward"),
                "required_skills": meta_array(&meta, "required_skills"),
                "tags": meta_array(&meta, "tags"),
                "difficulty": r.get::<i16, _>("difficulty") as i32,
                "status": bounty_status_from_slice(&r.get::<String, _>("status")),
                "expires_at": r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("claim_expires_at"),
                "created_at": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                "company_name": r
                    .try_get::<Option<String>, _>("company_name")
                    .ok()
                    .flatten()
                    .unwrap_or_default(),
            })
        })
        .collect();

    Ok(Json(build_response(
        json!({ "bounties": items, "page": page, "per_page": per_page }),
    )))
}

/// Mapping status project_slice → status bounty exposé côté API.
/// Le vocabulaire historique bounty est plus détaillé (pr_submitted, paid) que
/// project_slice ; on utilise les champs pr_url/paid_at pour raffiner.
fn bounty_status_from_slice(slice_status: &str) -> String {
    match slice_status {
        "open" => "open".into(),
        "claimed" => "claimed".into(),
        "in_review" => "in_review".into(),
        "merged" => "paid".into(),
        "closed" => "cancelled".into(),
        "expired" => "expired".into(),
        other => other.to_string(),
    }
}

async fn get_bounty(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let row = sqlx::query(
        r#"
        SELECT ps.*, p.github_repo_owner, p.github_repo_name,
               e.company_name,
               CASE WHEN ps.status IN ('claimed', 'in_review') THEN 1 ELSE 0 END::BIGINT AS active_claims
        FROM project_slices ps
        JOIN projects p ON p.id = ps.project_id
        LEFT JOIN enterprises e ON e.id = ps.funder_enterprise_id
        WHERE ps.id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound("bounty not found".into()))?;

    let meta: Value = row
        .try_get::<Value, _>("external_metadata")
        .unwrap_or(Value::Null);
    let repo_owner: String = row.get("github_repo_owner");
    let repo_name: String = row.get("github_repo_name");

    Ok(Json(build_response(json!({
        "id": row.get::<Uuid, _>("id"),
        "title": row.get::<String, _>("title"),
        "description": row.get::<String, _>("description"),
        "repo": format!("{repo_owner}/{repo_name}"),
        "issue_url": meta_str(&meta, "issue_url"),
        "reward_credits": row.get::<BigDecimal, _>("credits_reward").to_string(),
        "fragments_bonus": row.get::<i32, _>("fragments_reward"),
        "required_skills": meta_array(&meta, "required_skills"),
        "tags": meta_array(&meta, "tags"),
        "difficulty": row.get::<i16, _>("difficulty") as i32,
        "status": bounty_status_from_slice(&row.get::<String, _>("status")),
        "company_name": row
            .try_get::<Option<String>, _>("company_name")
            .ok()
            .flatten()
            .unwrap_or_default(),
        "active_claims": row.get::<i64, _>("active_claims"),
    }))))
}

// ─── Création (enterprise) ───────────────────────────────────────

#[derive(Deserialize)]
struct CreateBountyBody {
    repo_owner: String,
    repo_name: String,
    issue_number: i32,
    issue_url: String,
    title: String,
    description: String,
    reward_credits: String,
    fragments_bonus: Option<i32>,
    required_skills: Option<Vec<String>>,
    difficulty: Option<i32>,
    tags: Option<Vec<String>>,
    expires_in_days: Option<i32>,
}

async fn create_bounty(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateBountyBody>,
) -> Result<Json<Value>, AppError> {
    let enterprise_id = current_enterprise_for(&state.db, auth.user_id).await?;
    let reward = BigDecimal::from_str(&body.reward_credits)
        .map_err(|_| AppError::Validation("invalid reward_credits amount".into()))?;
    if reward <= 0 {
        return Err(AppError::Validation("reward_credits must be > 0".into()));
    }

    let escrow_notes = format!("bounty:{}#{}", body.repo_name, body.issue_number);
    crate::services::credits::spend(
        &state.db,
        crate::services::credits::SpendInput {
            enterprise_id,
            amount: &reward,
            reason: "spend_bounty_escrow",
            related_interest_request_id: None,
            actor_user_id: Some(auth.user_id),
            notes: Some(&escrow_notes),
        },
    )
    .await?;

    let expires_at = body
        .expires_in_days
        .map(|d| chrono::Utc::now() + chrono::Duration::days(d as i64));

    let mut tx = state.db.begin().await?;

    // Résout (ou crée) le project miroir du repo GitHub — évite d'exiger que
    // l'admin ait pré-créé le project côté enterprise.
    let project_id =
        resolve_or_create_project(&mut tx, &body.repo_owner, &body.repo_name, auth.user_id).await?;

    let metadata = json!({
        "source": "bounty_create",
        "issue_url": body.issue_url,
        "tags": body.tags.clone().unwrap_or_default(),
        "required_skills": body.required_skills.clone().unwrap_or_default(),
    });

    let slice_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO project_slices
            (project_id, slice_type, external_ref, external_metadata,
             title, description,
             primary_domain, difficulty, fragments_reward, credits_reward,
             status, claim_expires_at,
             created_by_user_id, funded_by_user_id, funder_enterprise_id, ingested_from)
        VALUES ($1, 'github_issue', $2, $3,
                $4, $5,
                'code', $6, $7, $8,
                'open', $9,
                $10, $10, $11, 'manual')
        RETURNING id
        "#,
    )
    .bind(project_id)
    .bind(body.issue_number.to_string())
    .bind(&metadata)
    .bind(&body.title)
    .bind(&body.description)
    .bind(body.difficulty.unwrap_or(3) as i16)
    .bind(body.fragments_bonus.unwrap_or(100))
    .bind(&reward)
    .bind(expires_at)
    .bind(auth.user_id)
    .bind(enterprise_id)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;

    metrics::counter!("skilluv_bounties_posted_total").increment(1);

    Ok(Json(build_response(json!({
        "bounty_id": slice_id,
        "slice_id": slice_id,
    }))))
}

// ─── Claim + submit PR (talent) ──────────────────────────────────

async fn claim_bounty(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    // Récupère la slice + verrouille jusqu'à la fin du claim
    let mut tx = state.db.begin().await?;
    let row: Option<(String, Option<Uuid>)> = sqlx::query_as(
        "SELECT status, claimed_by_user_id FROM project_slices WHERE id = $1 FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let (status, current_claimant) = row.ok_or(AppError::NotFound("bounty not found".into()))?;
    if status != "open" && (status != "claimed" || current_claimant != Some(auth.user_id)) {
        return Err(AppError::Validation(format!(
            "bounty status '{status}' does not accept new claims"
        )));
    }
    let default_ttl_days: i64 = 7;
    let expires = chrono::Utc::now() + chrono::Duration::days(default_ttl_days);
    sqlx::query(
        r#"
        UPDATE project_slices
        SET status = 'claimed',
            claimed_by_user_id = $1,
            claimed_at = COALESCE(claimed_at, NOW()),
            claim_expires_at = COALESCE(claim_expires_at, $2)
        WHERE id = $3
        "#,
    )
    .bind(auth.user_id)
    .bind(expires)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    metrics::counter!("skilluv_bounties_claimed_total").increment(1);
    Ok(Json(build_response(json!({ "claim_id": id }))))
}

#[derive(Deserialize)]
struct SubmitPrBody {
    pull_request_url: String,
    pull_request_number: i32,
}

async fn submit_pr(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<SubmitPrBody>,
) -> Result<Json<Value>, AppError> {
    let res = sqlx::query(
        r#"
        UPDATE project_slices
        SET pr_url = $1, pr_number = $2,
            pr_submitted_at = NOW(),
            status = 'in_review'
        WHERE id = $3 AND claimed_by_user_id = $4
          AND status IN ('claimed', 'in_review')
        "#,
    )
    .bind(&body.pull_request_url)
    .bind(body.pull_request_number)
    .bind(id)
    .bind(auth.user_id)
    .execute(&state.db)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::Validation("no active claim to attach PR".into()));
    }
    Ok(Json(build_response(json!({ "attached": true }))))
}

async fn cancel_bounty(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let enterprise_id = current_enterprise_for(&state.db, auth.user_id).await?;
    let row: Option<(String, BigDecimal, Option<Uuid>)> = sqlx::query_as(
        "SELECT status, credits_reward, funder_enterprise_id
         FROM project_slices WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?;
    let (status, reward, funder) = row.ok_or(AppError::NotFound("bounty not found".into()))?;
    if funder != Some(enterprise_id) {
        return Err(AppError::Forbidden);
    }
    if !matches!(status.as_str(), "open" | "claimed") {
        return Err(AppError::Validation(format!(
            "cannot cancel bounty in status '{status}'"
        )));
    }
    crate::services::credits::grant(
        &state.db,
        crate::services::credits::GrantInput {
            enterprise_id,
            amount: &reward,
            reason: "refund_bounty_cancelled",
            related_payment_id: None,
            related_promo_code_id: None,
            notes: Some(&format!("bounty:{id}")),
            actor_user_id: Some(auth.user_id),
            expires_at: None,
        },
    )
    .await?;
    sqlx::query("UPDATE project_slices SET status = 'closed', closed_at = NOW() WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;
    Ok(Json(build_response(json!({ "cancelled": true }))))
}

// ─── Webhook GitHub (payout automatique) ─────────────────────────

async fn github_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, AppError> {
    let secret = std::env::var("GITHUB_WEBHOOK_SECRET")
        .ok()
        .filter(|s| !s.is_empty())
        .ok_or(AppError::Internal("GITHUB_WEBHOOK_SECRET not set".into()))?;
    let signature = headers
        .get("x-hub-signature-256")
        .and_then(|v| v.to_str().ok())
        .ok_or(AppError::Unauthorized)?;
    let expected = format!("sha256={}", {
        let mut mac = <HmacSha256 as KeyInit>::new_from_slice(secret.as_bytes())
            .map_err(|_| AppError::Internal("hmac init".into()))?;
        mac.update(&body);
        hex::encode(mac.finalize().into_bytes())
    });
    if !constant_time_eq(signature.as_bytes(), expected.as_bytes()) {
        return Err(AppError::Unauthorized);
    }

    let delivery_id = headers
        .get("x-github-delivery")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let event_type = headers
        .get("x-github-event")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let already: Option<(String,)> =
        sqlx::query_as("SELECT delivery_id FROM github_webhook_events WHERE delivery_id = $1")
            .bind(&delivery_id)
            .fetch_optional(&state.db)
            .await?;
    if already.is_some() {
        return Ok(Json(build_response(json!({ "duplicate": true }))));
    }
    let payload: Value = serde_json::from_slice(&body)
        .map_err(|e| AppError::Validation(format!("github payload decode: {e}")))?;

    sqlx::query(
        "INSERT INTO github_webhook_events (delivery_id, event_type, payload) VALUES ($1, $2, $3)",
    )
    .bind(&delivery_id)
    .bind(&event_type)
    .bind(&payload)
    .execute(&state.db)
    .await?;

    if event_type == "pull_request" {
        handle_pull_request_event(&state, &payload).await?;
    }
    // P11.2 : issues.labeled → ingestion real-time d'une nouvelle slice
    // pour tout projet Skilluv qui a le label ajouté dans curated_labels.
    if event_type == "issues" {
        handle_issues_event(&state, &payload).await?;
    }
    Ok(Json(build_response(json!({ "processed": true }))))
}

/// P11.2 — Handler `issues.labeled`. Quand un mainteneur GitHub ajoute un
/// label curé (ex: 'good-first-issue') à une issue, on crée immédiatement une
/// slice draft (curator_review) ou open (auto) sans attendre le prochain cycle
/// de polling.
async fn handle_issues_event(state: &AppState, payload: &Value) -> Result<(), AppError> {
    let action = payload.get("action").and_then(|v| v.as_str()).unwrap_or("");
    if action != "labeled" {
        return Ok(());
    }
    let added_label = payload
        .get("label")
        .and_then(|l| l.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or("");
    if added_label.is_empty() {
        return Ok(());
    }

    let issue = payload.get("issue").cloned().unwrap_or(Value::Null);
    // Skip si l'issue est en réalité un PR
    if issue.get("pull_request").is_some() {
        return Ok(());
    }

    let issue_number = issue.get("number").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
    let issue_url = issue
        .get("html_url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let title = issue
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("(untitled)")
        .to_string();
    let body_text = issue
        .get("body")
        .and_then(|v| v.as_str())
        .unwrap_or("(no description)")
        .to_string();
    if issue_number == 0 {
        return Ok(());
    }

    let repository = payload.get("repository").cloned().unwrap_or(Value::Null);
    let full_name = repository
        .get("full_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let (owner, name) = match full_name.split_once('/') {
        Some(pair) => pair,
        None => return Ok(()),
    };

    // Trouver un project Skilluv qui match le repo ET qui liste ce label.
    let project_row: Option<(Uuid, String)> = sqlx::query_as(
        r#"
        SELECT id, slice_ingestion_mode
        FROM projects
        WHERE github_repo_owner = $1
          AND github_repo_name = $2
          AND slice_ingestion_mode IN ('auto', 'curator_review')
          AND $3 = ANY(curated_labels)
          AND archived_at IS NULL
        LIMIT 1
        "#,
    )
    .bind(owner)
    .bind(name)
    .bind(added_label)
    .fetch_optional(&state.db)
    .await?;

    let Some((project_id, mode)) = project_row else {
        return Ok(()); // Ce label n'est pas curé pour ce projet.
    };

    let default_status = if mode == "auto" { "open" } else { "draft" };
    let existing_labels: Vec<String> = issue
        .get("labels")
        .and_then(|l| l.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.get("name").and_then(|n| n.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let metadata = json!({
        "source": "github_webhook_issues_labeled",
        "issue_url": issue_url,
        "issue_number": issue_number,
        "labels": existing_labels,
        "trigger_label": added_label,
    });

    let inserted: Option<Uuid> = sqlx::query_scalar(
        r#"
        INSERT INTO project_slices
            (project_id, slice_type, external_ref, external_metadata,
             title, description, primary_domain, difficulty, fragments_reward,
             status, ingested_from)
        VALUES ($1, 'github_issue', $2, $3,
                $4, $5, 'code', 3, 50,
                $6, 'github_webhook')
        ON CONFLICT (project_id, external_ref)
            WHERE slice_type = 'github_issue' AND external_ref IS NOT NULL
            DO NOTHING
        RETURNING id
        "#,
    )
    .bind(project_id)
    .bind(issue_number.to_string())
    .bind(&metadata)
    .bind(truncate(&title, 300))
    .bind(truncate(&body_text, 4000))
    .bind(default_status)
    .fetch_optional(&state.db)
    .await?;

    if inserted.is_some() {
        metrics::counter!(
            "skilluv_github_slices_ingested_total",
            "source" => "webhook"
        )
        .increment(1);
    }
    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        let cut = s
            .char_indices()
            .take_while(|(i, _)| *i < max.saturating_sub(1))
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(max);
        format!("{}…", &s[..cut])
    }
}

async fn handle_pull_request_event(state: &AppState, payload: &Value) -> Result<(), AppError> {
    let action = payload.get("action").and_then(|v| v.as_str()).unwrap_or("");
    if action != "closed" {
        return Ok(());
    }
    let pr = payload.get("pull_request").cloned().unwrap_or(Value::Null);
    let merged = pr.get("merged").and_then(|v| v.as_bool()).unwrap_or(false);
    if !merged {
        return Ok(());
    }
    let pr_url = pr.get("html_url").and_then(|v| v.as_str()).unwrap_or("");
    let pr_number = pr.get("number").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
    if pr_url.is_empty() || pr_number == 0 {
        return Ok(());
    }
    let repo_full_name = pr
        .get("base")
        .and_then(|b| b.get("repo"))
        .and_then(|r| r.get("full_name"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // Trouve la slice ciblée : pr_url exact OU (repo + pr_number).
    let row: Option<(Uuid, Uuid, BigDecimal, i32, Uuid, String)> = sqlx::query_as(
        r#"
        SELECT ps.id, ps.claimed_by_user_id, ps.credits_reward,
               ps.fragments_reward, ps.funder_enterprise_id, ps.status
        FROM project_slices ps
        JOIN projects p ON p.id = ps.project_id
        WHERE ps.status = 'in_review'
          AND ps.claimed_by_user_id IS NOT NULL
          AND ps.funder_enterprise_id IS NOT NULL
          AND (
              ps.pr_url = $1
              OR (
                  ps.pr_number = $2
                  AND (p.github_repo_owner || '/' || p.github_repo_name) = $3
              )
          )
        LIMIT 1
        "#,
    )
    .bind(pr_url)
    .bind(pr_number)
    .bind(repo_full_name)
    .fetch_optional(&state.db)
    .await?;

    let Some((slice_id, talent_user_id, reward, fragments_bonus, enterprise_id, slice_status)) =
        row
    else {
        return Ok(());
    };
    if slice_status == "merged" {
        return Ok(());
    }

    // BE-P26 — Split fee Skilluv 8% (env override SKILLUV_BOUNTY_FEE_BPS,
    // default 800 basis points = 8%). Sur bounty 500 crédits :
    //   platform_share = 40, talent_share = 460.
    // Le talent voit talent_share × credit_to_frag en fragments et un montant
    // pré-fee (transparence UI côté enterprise ET talent doit indiquer le fee).
    let fee_bps: i64 = std::env::var("SKILLUV_BOUNTY_FEE_BPS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(800);
    let reward_i64 = num_traits::ToPrimitive::to_i64(&reward).unwrap_or(0);
    let platform_share_i64 = (reward_i64 * fee_bps) / 10_000;
    let talent_share_i64 = reward_i64 - platform_share_i64;
    let platform_share_bd = BigDecimal::from(platform_share_i64);
    let talent_share_bd = BigDecimal::from(talent_share_i64);

    // Conversion crédits séquestrés → fragments talent (les crédits sont B2B).
    let credit_to_frag: i64 = std::env::var("BOUNTY_CREDIT_TO_FRAGMENTS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(500);
    let fragments_from_credits = (talent_share_i64 * credit_to_frag) as i32;
    let total_fragments_award = fragments_from_credits + fragments_bonus;

    sqlx::query(
        r#"
        INSERT INTO credit_transactions
            (enterprise_id, delta, reason, related_talent_id, notes, actor_user_id)
        VALUES ($1, 0, 'spend_bounty_payout', $2, $3, NULL)
        "#,
    )
    .bind(enterprise_id)
    .bind(talent_user_id)
    .bind(format!(
        "bounty:{slice_id} total_credits={reward} platform_fee={platform_share_bd} talent_share={talent_share_bd} fragments={total_fragments_award} pr={pr_url}"
    ))
    .execute(&state.db)
    .await?;

    // BE-P26 — Insert platform_revenues ligne pour la marge Skilluv.
    if platform_share_i64 > 0 {
        sqlx::query(
            r#"
            INSERT INTO platform_revenues
                (source, source_slice_id, related_talent_id, related_enterprise_id,
                 amount_credits, fee_rate_bps, notes)
            VALUES ('bounty', $1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(slice_id)
        .bind(talent_user_id)
        .bind(enterprise_id)
        .bind(&platform_share_bd)
        .bind(fee_bps as i32)
        .bind(format!(
            "bounty payout fee {fee_bps}bps on {reward} credits"
        ))
        .execute(&state.db)
        .await?;

        metrics::counter!("skilluv_bounty_platform_fees_total").increment(1);
    }

    sqlx::query("UPDATE users SET total_fragments = total_fragments + $1 WHERE id = $2")
        .bind(total_fragments_award)
        .bind(talent_user_id)
        .execute(&state.db)
        .await?;

    // P13.4 : dual payout — en plus des fragments, crédite le talent_wallet
    // en devise réelle (EUR ou XOF selon residency_country). Best-effort :
    // si le wallet n'existe pas ou le taux est 0, on skip silencieusement.
    let residency: Option<String> =
        sqlx::query_scalar("SELECT residency_country FROM talent_wallets WHERE user_id = $1")
            .bind(talent_user_id)
            .fetch_optional(&state.db)
            .await?
            .flatten();
    let is_xof_country = matches!(
        residency.as_deref(),
        Some("CI" | "SN" | "BJ" | "TG" | "ML" | "BF" | "NE" | "GW")
    );
    let (wallet_currency, rate_env) = if is_xof_country {
        (
            crate::services::talent_wallet::Currency::Xof,
            "BOUNTY_CREDIT_TO_XOF",
        )
    } else {
        (
            crate::services::talent_wallet::Currency::Eur,
            "BOUNTY_CREDIT_TO_EUR",
        )
    };
    let rate: f64 = std::env::var(rate_env)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0);
    if rate > 0.0 {
        // BE-P26 : le wallet talent reçoit talent_share (92% après fee 8%),
        // pas le reward brut. Ex bounty 500€ → wallet 460€ EUR.
        let rate_bd = BigDecimal::try_from(rate).unwrap_or(BigDecimal::from(0));
        let fiat_amount = &talent_share_bd * &rate_bd;
        if fiat_amount > 0 {
            let entry = crate::services::talent_wallet::LedgerEntry {
                user_id: talent_user_id,
                delta: &fiat_amount,
                currency: wallet_currency,
                reason: "bounty_payout",
                related_slice_id: Some(slice_id),
                related_provider_txn_id: None,
                notes: Some("automatic bounty merge payout"),
            };
            match crate::services::talent_wallet::credit(&state.db, entry).await {
                Ok(txn) => {
                    metrics::counter!(
                        "skilluv_bounty_wallet_payouts_total",
                        "currency" => wallet_currency.as_str().to_string()
                    )
                    .increment(1);
                    tracing::info!(
                        talent = %talent_user_id, slice_id = %slice_id,
                        currency = wallet_currency.as_str(), amount = %fiat_amount,
                        tx_id = %txn.id,
                        "bounty wallet payout credited"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e, talent = %talent_user_id, slice_id = %slice_id,
                        "P13.4 dual payout to wallet failed (best-effort, fragments still awarded)"
                    );
                }
            }
        }
    }

    sqlx::query(
        r#"
        UPDATE project_slices
        SET status = 'merged',
            merged_at = NOW(),
            paid_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(slice_id)
    .execute(&state.db)
    .await?;

    metrics::counter!("skilluv_bounties_paid_total").increment(1);
    tracing::info!(
        slice_id = %slice_id,
        talent = %talent_user_id,
        reward = %reward,
        "bounty payout completed"
    );
    Ok(())
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut d = 0u8;
    for (x, y) in a.iter().zip(b) {
        d |= x ^ y;
    }
    d == 0
}
