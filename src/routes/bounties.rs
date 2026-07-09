//! OSS Bounties — Phase 5.6.
//!
//! Flow :
//!   1. Entreprise POST /api/bounties (crédits débités séquestre)
//!   2. Talent POST /api/bounties/{id}/claim
//!   3. Talent POST /api/bounties/{id}/pr {url}
//!   4. Webhook GitHub `pull_request.closed` merged=true → payout auto
//!      (crédits séquestrés → transférés au talent + bonus fragments)

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use bigdecimal::BigDecimal;
use hmac::{Hmac, Mac};
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
        SELECT b.id, b.title, b.description, b.repo_owner, b.repo_name, b.issue_number,
               b.issue_url, b.reward_credits::TEXT AS reward_credits,
               b.fragments_bonus, b.required_skills, b.difficulty, b.tags, b.status,
               b.expires_at, b.created_at,
               e.company_name
        FROM oss_bounties b
        JOIN enterprises e ON e.id = b.enterprise_id
        WHERE b.status = $1
          AND ($2::TEXT IS NULL OR $2 = ANY(b.required_skills))
          AND ($3::TEXT IS NULL OR $3 = ANY(b.tags))
        ORDER BY b.created_at DESC
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
            json!({
                "id": r.get::<Uuid, _>("id"),
                "title": r.get::<String, _>("title"),
                "description": r.get::<String, _>("description"),
                "repo": format!("{}/{}", r.get::<String, _>("repo_owner"), r.get::<String, _>("repo_name")),
                "issue_url": r.get::<String, _>("issue_url"),
                "issue_number": r.get::<i32, _>("issue_number"),
                "reward_credits": r.get::<String, _>("reward_credits"),
                "fragments_bonus": r.get::<i32, _>("fragments_bonus"),
                "required_skills": r.get::<Vec<String>, _>("required_skills"),
                "tags": r.get::<Vec<String>, _>("tags"),
                "difficulty": r.get::<i32, _>("difficulty"),
                "status": r.get::<String, _>("status"),
                "expires_at": r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("expires_at"),
                "created_at": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                "company_name": r.get::<String, _>("company_name"),
            })
        })
        .collect();

    Ok(Json(build_response(json!({ "bounties": items, "page": page, "per_page": per_page }))))
}

async fn get_bounty(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let row = sqlx::query(
        r#"
        SELECT b.*, e.company_name,
               (SELECT COUNT(*)::BIGINT FROM oss_bounty_claims c WHERE c.bounty_id = b.id AND c.status IN ('claimed', 'pr_submitted')) AS active_claims
        FROM oss_bounties b
        JOIN enterprises e ON e.id = b.enterprise_id
        WHERE b.id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound("bounty not found".into()))?;

    Ok(Json(build_response(json!({
        "id": row.get::<Uuid, _>("id"),
        "title": row.get::<String, _>("title"),
        "description": row.get::<String, _>("description"),
        "repo": format!("{}/{}", row.get::<String, _>("repo_owner"), row.get::<String, _>("repo_name")),
        "issue_url": row.get::<String, _>("issue_url"),
        "reward_credits": row.get::<BigDecimal, _>("reward_credits").to_string(),
        "fragments_bonus": row.get::<i32, _>("fragments_bonus"),
        "required_skills": row.get::<Vec<String>, _>("required_skills"),
        "tags": row.get::<Vec<String>, _>("tags"),
        "difficulty": row.get::<i32, _>("difficulty"),
        "status": row.get::<String, _>("status"),
        "company_name": row.get::<String, _>("company_name"),
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
    if reward <= BigDecimal::from(0) {
        return Err(AppError::Validation("reward_credits must be > 0".into()));
    }
    // Séquestre : on débite immédiatement l'entreprise (spend_bounty_escrow) —
    // le payout final ré-attribuera au talent lors du merge.
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
    let inserted: (Uuid,) = sqlx::query_as(
        r#"
        INSERT INTO oss_bounties
            (enterprise_id, posted_by_user_id, repo_owner, repo_name, issue_number, issue_url,
             title, description, reward_credits, fragments_bonus, required_skills, difficulty, tags, expires_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
        RETURNING id
        "#,
    )
    .bind(enterprise_id)
    .bind(auth.user_id)
    .bind(&body.repo_owner)
    .bind(&body.repo_name)
    .bind(body.issue_number)
    .bind(&body.issue_url)
    .bind(&body.title)
    .bind(&body.description)
    .bind(&reward)
    .bind(body.fragments_bonus.unwrap_or(100))
    .bind(body.required_skills.unwrap_or_default())
    .bind(body.difficulty.unwrap_or(3))
    .bind(body.tags.unwrap_or_default())
    .bind(expires_at)
    .fetch_one(&state.db)
    .await?;

    metrics::counter!("skilluv_bounties_posted_total").increment(1);
    Ok(Json(build_response(json!({ "bounty_id": inserted.0 }))))
}

// ─── Claim + submit PR (talent) ──────────────────────────────────

async fn claim_bounty(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let bounty_status: Option<(String,)> =
        sqlx::query_as("SELECT status FROM oss_bounties WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await?;
    let status = bounty_status
        .map(|(s,)| s)
        .ok_or(AppError::NotFound("bounty not found".into()))?;
    if status != "open" && status != "claimed" {
        return Err(AppError::Validation(format!(
            "bounty status '{status}' does not accept new claims"
        )));
    }
    let inserted: (Uuid,) = sqlx::query_as(
        r#"
        INSERT INTO oss_bounty_claims (bounty_id, user_id)
        VALUES ($1, $2)
        ON CONFLICT (bounty_id, user_id) DO UPDATE SET status = 'claimed', claimed_at = NOW()
        RETURNING id
        "#,
    )
    .bind(id)
    .bind(auth.user_id)
    .fetch_one(&state.db)
    .await?;
    // Passe la bounty en "claimed" si elle était open
    sqlx::query("UPDATE oss_bounties SET status = 'claimed', updated_at = NOW() WHERE id = $1 AND status = 'open'")
        .bind(id)
        .execute(&state.db)
        .await?;
    metrics::counter!("skilluv_bounties_claimed_total").increment(1);
    Ok(Json(build_response(json!({ "claim_id": inserted.0 }))))
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
        UPDATE oss_bounty_claims
        SET pull_request_url = $1, pull_request_number = $2,
            status = 'pr_submitted', pr_submitted_at = NOW()
        WHERE bounty_id = $3 AND user_id = $4 AND status IN ('claimed', 'pr_submitted')
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
    sqlx::query("UPDATE oss_bounties SET status = 'in_review', updated_at = NOW() WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;
    Ok(Json(build_response(json!({ "attached": true }))))
}

async fn cancel_bounty(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let enterprise_id = current_enterprise_for(&state.db, auth.user_id).await?;
    // Récupérer la bounty et le montant à rembourser
    let row: Option<(String, BigDecimal, Uuid)> = sqlx::query_as(
        "SELECT status, reward_credits, enterprise_id FROM oss_bounties WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?;
    let (status, reward, ent) = row.ok_or(AppError::NotFound("bounty not found".into()))?;
    if ent != enterprise_id {
        return Err(AppError::Forbidden);
    }
    if !matches!(status.as_str(), "open" | "claimed") {
        return Err(AppError::Validation(format!(
            "cannot cancel bounty in status '{status}'"
        )));
    }
    // Remboursement crédits (grant reason refund_bounty_cancelled)
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
    sqlx::query(
        "UPDATE oss_bounties SET status = 'cancelled', updated_at = NOW() WHERE id = $1",
    )
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
        let mut mac = <HmacSha256 as Mac>::new_from_slice(secret.as_bytes())
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

    // Idempotence
    let already: Option<(String,)> = sqlx::query_as(
        "SELECT delivery_id FROM github_webhook_events WHERE delivery_id = $1",
    )
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
    Ok(Json(build_response(json!({ "processed": true }))))
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

    // Match sur PR URL ou (repo + PR number)
    let claim_row: Option<(Uuid, Uuid, Uuid)> = sqlx::query_as(
        r#"
        SELECT c.id, c.bounty_id, c.user_id
        FROM oss_bounty_claims c
        JOIN oss_bounties b ON b.id = c.bounty_id
        WHERE c.status = 'pr_submitted'
          AND (c.pull_request_url = $1 OR (c.pull_request_number = $2 AND b.repo_owner || '/' || b.repo_name = $3))
        LIMIT 1
        "#,
    )
    .bind(pr_url)
    .bind(pr_number)
    .bind(
        pr.get("base")
            .and_then(|b| b.get("repo"))
            .and_then(|r| r.get("full_name"))
            .and_then(|v| v.as_str())
            .unwrap_or(""),
    )
    .fetch_optional(&state.db)
    .await?;

    let Some((claim_id, bounty_id, talent_user_id)) = claim_row else {
        return Ok(());
    };
    // Récupérer la bounty (pool, fragments_bonus, entreprise)
    let brow = sqlx::query(
        r#"
        SELECT enterprise_id, reward_credits::TEXT AS reward_credits, fragments_bonus, status
        FROM oss_bounties WHERE id = $1 FOR UPDATE
        "#,
    )
    .bind(bounty_id)
    .fetch_one(&state.db)
    .await?;
    let bounty_status: String = brow.get("status");
    if bounty_status == "paid" {
        return Ok(());
    }
    let enterprise_id: Uuid = brow.get("enterprise_id");
    let reward = BigDecimal::from_str(&brow.get::<String, _>("reward_credits"))
        .map_err(|_| AppError::Internal("bad reward decimal".into()))?;
    let fragments_bonus: i32 = brow.get("fragments_bonus");

    // Payout : les crédits ont été séquestrés au create. Le talent n'a pas
    // de wallet crédits (les crédits Skilluv sont B2B), donc :
    //   - trace le payout côté enterprise (delta=0, juste une écriture d'audit)
    //   - convertit les crédits séquestrés en fragments pour le talent
    //     (1 crédit = 500 fragments par défaut, override via env
    //     BOUNTY_CREDIT_TO_FRAGMENTS)
    let credit_to_frag: i64 = std::env::var("BOUNTY_CREDIT_TO_FRAGMENTS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(500);
    let fragments_from_credits = num_traits::ToPrimitive::to_i64(&reward)
        .map(|c| c * credit_to_frag)
        .unwrap_or(0) as i32;
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
        "bounty:{bounty_id} payout_credits={reward} fragments={total_fragments_award} pr={pr_url}"
    ))
    .execute(&state.db)
    .await?;

    // Attribution fragments au talent
    sqlx::query("UPDATE users SET total_fragments = total_fragments + $1 WHERE id = $2")
        .bind(total_fragments_award)
        .bind(talent_user_id)
        .execute(&state.db)
        .await?;

    sqlx::query(
        "UPDATE oss_bounty_claims SET status = 'merged', merged_at = NOW() WHERE id = $1",
    )
    .bind(claim_id)
    .execute(&state.db)
    .await?;
    sqlx::query(
        "UPDATE oss_bounties SET status = 'paid', updated_at = NOW() WHERE id = $1",
    )
    .bind(bounty_id)
    .execute(&state.db)
    .await?;

    metrics::counter!("skilluv_bounties_paid_total").increment(1);
    tracing::info!(
        bounty_id = %bounty_id,
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
